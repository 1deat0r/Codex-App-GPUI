//! JSON-RPC over stdio adapter for the official Codex app-server.
//!
//! The transport is deliberately small and value-oriented. The app-server
//! protocol evolves quickly, so the UI-facing boundary keeps unknown fields in
//! `serde_json::Value` while enforcing the lifecycle that matters to a native
//! client: initialize, initialized, thread operations, turn operations, and
//! safe handling of server-initiated approvals.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, SyncSender},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CLIENT_NAME: &str = "codex_app_gpui";
const CLIENT_TITLE: &str = "Codex App GPUI";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcRequest {
    #[serde(default, skip_serializing)]
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcNotification {
    #[serde(default, skip_serializing)]
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerThread {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub status: String,
    pub model: String,
    pub updated_at: String,
    pub archived: bool,
}

impl ServerThread {
    pub fn from_value(value: &Value) -> Option<Self> {
        let id = value.get("id")?.as_str()?.to_string();
        let title = value
            .get("name")
            .or_else(|| value.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Untitled task")
            .to_string();
        let cwd = value
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let status = value
            .get("status")
            .map(status_text)
            .unwrap_or_else(|| "notLoaded".into());
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let updated_at = value
            .get("updatedAt")
            .or_else(|| value.get("updated_at"))
            .map(value_text)
            .unwrap_or_default();
        let archived = value
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(Self {
            id,
            title,
            cwd,
            status,
            model,
            updated_at,
            archived,
        })
    }
}

fn status_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.get("type").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| value_text(value))
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => value.to_string(),
    }
}

fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("bmp" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "webp")
    )
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

struct ResponseState {
    pending: HashMap<u64, SyncSender<Result<Value>>>,
    backlog: HashMap<u64, Result<Value>>,
}

struct EventInbox {
    queue: VecDeque<Value>,
    closed: bool,
    close_reason: Option<String>,
}

pub struct AppServerClient {
    writer: Writer,
    responses: Arc<Mutex<ResponseState>>,
    inbox: Arc<(Mutex<EventInbox>, Condvar)>,
    next_id: AtomicU64,
    child: Option<Arc<Mutex<Child>>>,
}

impl AppServerClient {
    pub fn spawn(command_line: &str) -> Result<Self> {
        let mut parts = command_line.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow!("empty app-server command"))?;
        let mut command = Command::new(program);
        command
            .args(parts)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn app-server `{command_line}`"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("app-server stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("app-server stdout unavailable"))?;
        Ok(Self::from_transport(
            BufReader::new(stdout),
            stdin,
            Some(Arc::new(Mutex::new(child))),
        ))
    }

    pub fn from_parts<R, W>(reader: R, writer: W) -> Self
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        Self::from_transport(reader, writer, None)
    }

    fn from_transport<R, W>(reader: R, writer: W, child: Option<Arc<Mutex<Child>>>) -> Self
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let writer: Writer = Arc::new(Mutex::new(Box::new(writer)));
        let responses = Arc::new(Mutex::new(ResponseState {
            pending: HashMap::new(),
            backlog: HashMap::new(),
        }));
        let inbox = Arc::new((
            Mutex::new(EventInbox {
                queue: VecDeque::new(),
                closed: false,
                close_reason: None,
            }),
            Condvar::new(),
        ));
        let reader_responses = responses.clone();
        let reader_inbox = inbox.clone();
        thread::Builder::new()
            .name("codex-app-gpui-app-server-reader".into())
            .spawn(move || read_messages(reader, reader_responses, reader_inbox))
            .expect("spawn app-server reader");
        Self {
            writer,
            responses,
            inbox,
            next_id: AtomicU64::new(1),
            child,
        }
    }

    pub fn is_live(&self) -> bool {
        self.child
            .as_ref()
            .and_then(|child| child.lock().ok())
            .map(|mut child| child.try_wait().ok().flatten().is_none())
            .unwrap_or(false)
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let backlog_result;
        {
            let mut responses = self
                .responses
                .lock()
                .map_err(|_| anyhow!("app-server pending requests poisoned"))?;
            backlog_result = responses.backlog.remove(&id);
            if backlog_result.is_none() {
                responses.pending.insert(id, sender);
            }
        }
        let write_result = self
            .writer
            .lock()
            .map_err(|_| anyhow!("app-server writer poisoned"))
            .and_then(|mut writer| write_json_line(&mut **writer, &request));
        if let Err(error) = write_result {
            if backlog_result.is_none() {
                let _ = self
                    .responses
                    .lock()
                    .map(|mut responses| responses.pending.remove(&id));
            }
            return Err(error);
        }
        if let Some(result) = backlog_result {
            return result;
        }
        receiver.recv().context("receive app-server response")?
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        let notification = RpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        };
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("app-server writer poisoned"))?;
        write_json_line(&mut **writer, &notification)
    }

    pub fn respond(&self, id: Value, result: Value) -> Result<()> {
        let response = json!({ "id": id, "result": result });
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("app-server writer poisoned"))?;
        write_json_line(&mut **writer, &response)
    }

    pub fn next_event(&self, timeout: Duration) -> Option<Value> {
        let (lock, wake) = &*self.inbox;
        let mut inbox = lock.lock().ok()?;
        loop {
            if let Some(event) = inbox.queue.pop_front() {
                return Some(event);
            }
            if inbox.closed {
                return None;
            }
            let (next, result) = wake.wait_timeout(inbox, timeout).ok()?;
            inbox = next;
            if result.timed_out() {
                return None;
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.inbox
            .0
            .lock()
            .map(|inbox| inbox.closed)
            .unwrap_or(true)
    }

    pub fn close_reason(&self) -> Option<String> {
        self.inbox
            .0
            .lock()
            .ok()
            .and_then(|inbox| inbox.close_reason.clone())
    }

    pub fn initialize(&self) -> Result<Value> {
        let result = self.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": CLIENT_NAME,
                    "title": CLIENT_TITLE,
                    "version": CLIENT_VERSION
                },
                "capabilities": {
                    "experimentalApi": true,
                    "extensions": {
                        "openai/form": {},
                        "io.modelcontextprotocol/ui": { "mimeTypes": ["text/html;profile=mcp-app"] }
                    }
                }
            }),
        )?;
        self.notify("initialized", json!({}))?;
        Ok(result)
    }

    pub fn thread_list(&self, search_term: Option<&str>) -> Result<Vec<ServerThread>> {
        self.thread_list_with_options(search_term, None)
    }

    pub fn thread_list_with_options(
        &self,
        search_term: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Vec<ServerThread>> {
        let mut cursor: Option<String> = None;
        let mut threads = Vec::new();
        for _ in 0..100 {
            let mut params = json!({ "limit": 100 });
            if let Some(search_term) = search_term.filter(|term| !term.is_empty()) {
                params["searchTerm"] = Value::String(search_term.into());
            }
            if let Some(archived) = archived {
                params["archived"] = Value::Bool(archived);
            }
            if let Some(cursor) = cursor.as_deref() {
                params["cursor"] = Value::String(cursor.to_owned());
            }
            let value = self.request("thread/list", params)?;
            let array = value
                .get("data")
                .or_else(|| value.get("threads"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            threads.extend(array.iter().filter_map(|thread| {
                let mut thread = ServerThread::from_value(thread)?;
                if archived == Some(true) {
                    thread.archived = true;
                }
                Some(thread)
            }));
            cursor = value
                .get("nextCursor")
                .or_else(|| value.get("next_cursor"))
                .and_then(Value::as_str)
                .filter(|cursor| !cursor.is_empty())
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(threads)
    }

    pub fn thread_start(&self, cwd: Option<&str>) -> Result<Value> {
        let params = cwd
            .filter(|cwd| !cwd.is_empty())
            .map(|cwd| json!({ "cwd": cwd }))
            .unwrap_or_else(|| json!({}));
        self.request("thread/start", params)
    }

    pub fn thread_fork(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/fork", json!({ "threadId": thread_id }))
    }

    pub fn thread_resume(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/resume", json!({ "threadId": thread_id }))
    }

    pub fn thread_read(&self, thread_id: &str) -> Result<Value> {
        self.request(
            "thread/read",
            json!({ "threadId": thread_id, "includeTurns": true }),
        )
    }

    pub fn thread_read_summary(&self, thread_id: &str) -> Result<Value> {
        self.request(
            "thread/read",
            json!({ "threadId": thread_id, "includeTurns": false }),
        )
    }

    pub fn thread_items_list(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id });
        if let Some(turn_id) = turn_id.filter(|value| !value.is_empty()) {
            params["turnId"] = Value::String(turn_id.into());
        }
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        self.request("thread/items/list", params)
    }

    pub fn thread_turns_list(
        &self,
        thread_id: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
        items_view: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id });
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        if let Some(items_view) = items_view.filter(|value| !value.is_empty()) {
            params["itemsView"] = Value::String(items_view.into());
        }
        self.request("thread/turns/list", params)
    }

    pub fn thread_search(&self, search_term: &str, archived: Option<bool>) -> Result<Value> {
        let mut params = json!({ "searchTerm": search_term, "limit": 100 });
        if let Some(archived) = archived {
            params["archived"] = Value::Bool(archived);
        }
        self.request("thread/search", params)
    }

    pub fn thread_search_occurrences(
        &self,
        thread_id: &str,
        search_term: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id, "searchTerm": search_term });
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        self.request("thread/searchOccurrences", params)
    }

    pub fn thread_goal_get(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/goal/get", json!({ "threadId": thread_id }))
    }

    pub fn thread_goal_set(
        &self,
        thread_id: &str,
        objective: Option<&str>,
        status: Option<&str>,
        token_budget: Option<i64>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id });
        if let Some(objective) = objective {
            params["objective"] = Value::String(objective.into());
        }
        if let Some(status) = status {
            params["status"] = Value::String(status.into());
        }
        if let Some(token_budget) = token_budget {
            params["tokenBudget"] = json!(token_budget);
        }
        self.request("thread/goal/set", params)
    }

    pub fn thread_goal_clear(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/goal/clear", json!({ "threadId": thread_id }))
    }

    pub fn thread_settings_update(&self, thread_id: &str, settings: Value) -> Result<Value> {
        let mut params = settings.as_object().cloned().unwrap_or_default();
        params.insert("threadId".into(), Value::String(thread_id.into()));
        self.request("thread/settings/update", Value::Object(params))
    }

    pub fn thread_metadata_update(&self, thread_id: &str, metadata: Value) -> Result<Value> {
        let mut params = metadata.as_object().cloned().unwrap_or_default();
        params.insert("threadId".into(), Value::String(thread_id.into()));
        self.request("thread/metadata/update", Value::Object(params))
    }

    pub fn thread_section_move(
        &self,
        thread_id: &str,
        section_id: Option<&str>,
        before_thread_id: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({
            "threadId": thread_id,
            "sectionId": section_id,
        });
        if let Some(before_thread_id) = before_thread_id.filter(|value| !value.is_empty()) {
            params["beforeThreadId"] = Value::String(before_thread_id.into());
        }
        self.request("thread/section/move", params)
    }

    pub fn thread_inject_items(&self, thread_id: &str, items: Value) -> Result<Value> {
        self.request(
            "thread/inject_items",
            json!({ "threadId": thread_id, "items": items }),
        )
    }

    pub fn thread_queue_add(
        &self,
        thread_id: &str,
        client_user_message_id: &str,
        input: Value,
    ) -> Result<Value> {
        self.request(
            "thread/queue/add",
            json!({
                "threadId": thread_id,
                "clientUserMessageId": client_user_message_id,
                "input": input,
            }),
        )
    }

    pub fn thread_queue_list(
        &self,
        thread_id: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id });
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        self.request("thread/queue/list", params)
    }

    pub fn thread_queue_update(
        &self,
        thread_id: &str,
        queued_submission_id: &str,
        input: Value,
    ) -> Result<Value> {
        self.request(
            "thread/queue/update",
            json!({
                "threadId": thread_id,
                "queuedSubmissionId": queued_submission_id,
                "input": input,
            }),
        )
    }

    pub fn thread_queue_delete(
        &self,
        thread_id: &str,
        queued_submission_id: &str,
    ) -> Result<Value> {
        self.request(
            "thread/queue/delete",
            json!({
                "threadId": thread_id,
                "queuedSubmissionId": queued_submission_id,
            }),
        )
    }

    pub fn thread_queue_reorder(
        &self,
        thread_id: &str,
        queued_submission_ids: &[String],
    ) -> Result<Value> {
        self.request(
            "thread/queue/reorder",
            json!({
                "threadId": thread_id,
                "queuedSubmissionIds": queued_submission_ids,
            }),
        )
    }

    pub fn thread_queue_start(
        &self,
        thread_id: &str,
        queued_submission_id: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({ "threadId": thread_id });
        if let Some(queued_submission_id) = queued_submission_id.filter(|value| !value.is_empty()) {
            params["queuedSubmissionId"] = Value::String(queued_submission_id.into());
        }
        self.request("thread/queue/start", params)
    }

    pub fn thread_memory_mode_set(&self, thread_id: &str, mode: &str) -> Result<Value> {
        self.request(
            "thread/memoryMode/set",
            json!({ "threadId": thread_id, "mode": mode }),
        )
    }

    pub fn thread_revert(&self, thread_id: &str, before_turn_id: &str) -> Result<Value> {
        self.request(
            "thread/revert",
            json!({ "threadId": thread_id, "beforeTurnId": before_turn_id }),
        )
    }

    pub fn thread_rollback(&self, thread_id: &str, num_turns: u32) -> Result<Value> {
        self.request(
            "thread/rollback",
            json!({ "threadId": thread_id, "numTurns": num_turns }),
        )
    }

    pub fn turn_start(&self, thread_id: &str, text: &str) -> Result<Value> {
        self.turn_start_with_options(thread_id, text, None, None, None, None)
    }

    pub fn turn_start_with_options(
        &self,
        thread_id: &str,
        text: &str,
        model: Option<&str>,
        effort: Option<&str>,
        cwd: Option<&str>,
        approval_policy: Option<&str>,
    ) -> Result<Value> {
        self.turn_start_with_options_and_attachments(
            thread_id,
            text,
            model,
            effort,
            cwd,
            approval_policy,
            &[],
        )
    }

    pub fn turn_start_with_options_and_attachments(
        &self,
        thread_id: &str,
        text: &str,
        model: Option<&str>,
        effort: Option<&str>,
        cwd: Option<&str>,
        approval_policy: Option<&str>,
        attachments: &[String],
    ) -> Result<Value> {
        self.turn_start_with_full_options_and_attachments(
            thread_id,
            text,
            model,
            effort,
            cwd,
            approval_policy,
            None,
            None,
            None,
            attachments,
        )
    }

    pub fn turn_start_with_full_options_and_attachments(
        &self,
        thread_id: &str,
        text: &str,
        model: Option<&str>,
        effort: Option<&str>,
        cwd: Option<&str>,
        approval_policy: Option<&str>,
        sandbox_policy: Option<&str>,
        model_provider: Option<&str>,
        personality: Option<&str>,
        attachments: &[String],
    ) -> Result<Value> {
        let mut input = Vec::with_capacity(attachments.len() + 1);
        if !text.is_empty() || attachments.is_empty() {
            input.push(json!({ "type": "text", "text": text }));
        }
        input.extend(attachments.iter().filter_map(|path| {
            let path = Path::new(path);
            if !path.is_absolute() {
                return None;
            }
            let path_text = path.to_str()?;
            if is_image_path(path) {
                Some(json!({ "type": "localImage", "path": path_text }))
            } else {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path_text);
                Some(json!({ "type": "mention", "name": name, "path": path_text }))
            }
        }));
        let mut params = json!({
            "threadId": thread_id,
            "input": input,
        });
        if let Some(model) = model.filter(|model| !model.is_empty()) {
            params["model"] = Value::String(model.into());
        }
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            params["effort"] = Value::String(effort.into());
        }
        if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) {
            params["cwd"] = Value::String(cwd.into());
        }
        if let Some(approval_policy) = approval_policy.filter(|policy| !policy.is_empty()) {
            params["approvalPolicy"] = Value::String(approval_policy.into());
        }
        if let Some(sandbox_policy) = sandbox_policy.filter(|policy| !policy.is_empty()) {
            params["sandboxPolicy"] = Value::String(sandbox_policy.into());
        }
        if let Some(model_provider) = model_provider.filter(|provider| !provider.is_empty()) {
            params["modelProvider"] = Value::String(model_provider.into());
        }
        if let Some(personality) = personality.filter(|personality| !personality.is_empty()) {
            params["personality"] = Value::String(personality.into());
        }
        self.request("turn/start", params)
    }

    pub fn turn_steer(&self, thread_id: &str, expected_turn_id: &str, text: &str) -> Result<Value> {
        self.request(
            "turn/steer",
            json!({
                "threadId": thread_id,
                "expectedTurnId": expected_turn_id,
                "input": [{ "type": "text", "text": text }]
            }),
        )
    }

    pub fn model_list(&self) -> Result<Value> {
        self.request("model/list", json!({ "limit": 100 }))
    }

    pub fn permission_profile_list(&self, cwd: Option<&str>) -> Result<Value> {
        let mut params = json!({});
        if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) {
            params["cwd"] = Value::String(cwd.into());
        }
        self.request("permissionProfile/list", params)
    }

    pub fn collaboration_mode_list(&self) -> Result<Value> {
        self.request("collaborationMode/list", json!({}))
    }

    pub fn apps_list(&self, thread_id: Option<&str>) -> Result<Value> {
        let mut params = json!({ "limit": 100 });
        if let Some(thread_id) = thread_id.filter(|thread_id| !thread_id.is_empty()) {
            params["threadId"] = Value::String(thread_id.into());
        }
        self.request("app/list", params)
    }

    pub fn apps_installed(&self, thread_id: Option<&str>) -> Result<Value> {
        let mut params = json!({});
        if let Some(thread_id) = thread_id.filter(|thread_id| !thread_id.is_empty()) {
            params["threadId"] = Value::String(thread_id.into());
        }
        self.request("app/installed", params)
    }

    pub fn apps_read(&self, app_ids: &[String], thread_id: Option<&str>) -> Result<Value> {
        let mut params = json!({ "appIds": app_ids, "includeTools": true });
        if let Some(thread_id) = thread_id.filter(|value| !value.is_empty()) {
            params["threadId"] = Value::String(thread_id.into());
        }
        self.request("app/read", params)
    }

    pub fn skills_list(&self, cwds: &[String]) -> Result<Value> {
        self.request("skills/list", json!({ "cwds": cwds }))
    }

    pub fn plugin_list(&self) -> Result<Value> {
        self.request("plugin/list", json!({}))
    }

    pub fn mcp_server_status_list(&self, thread_id: Option<&str>) -> Result<Value> {
        let mut params = json!({});
        if let Some(thread_id) = thread_id.filter(|thread_id| !thread_id.is_empty()) {
            params["threadId"] = Value::String(thread_id.into());
        }
        self.request("mcpServerStatus/list", params)
    }

    pub fn config_read(&self, cwd: Option<&str>, include_layers: bool) -> Result<Value> {
        let mut params = json!({ "includeLayers": include_layers });
        if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) {
            params["cwd"] = Value::String(cwd.into());
        }
        self.request("config/read", params)
    }

    pub fn account_read(&self) -> Result<Value> {
        self.request("account/read", json!({}))
    }

    pub fn account_rate_limits_read(&self) -> Result<Value> {
        self.request("account/rateLimits/read", json!({}))
    }

    pub fn account_usage_read(&self) -> Result<Value> {
        self.request("account/usage/read", json!({}))
    }

    pub fn realtime_start(&self, thread_id: &str, output_modality: &str) -> Result<Value> {
        self.request(
            "thread/realtime/start",
            json!({
                "threadId": thread_id,
                "outputModality": output_modality,
                "transport": { "type": "websocket" }
            }),
        )
    }

    pub fn realtime_append_text(&self, thread_id: &str, text: &str) -> Result<Value> {
        self.request(
            "thread/realtime/appendText",
            json!({ "threadId": thread_id, "text": text }),
        )
    }

    pub fn realtime_append_speech(&self, thread_id: &str, text: &str) -> Result<Value> {
        self.request(
            "thread/realtime/appendSpeech",
            json!({ "threadId": thread_id, "text": text }),
        )
    }

    pub fn realtime_append_audio(&self, thread_id: &str, audio: Value) -> Result<Value> {
        self.request(
            "thread/realtime/appendAudio",
            json!({ "threadId": thread_id, "audio": audio }),
        )
    }

    pub fn realtime_list_voices(&self) -> Result<Value> {
        self.request("thread/realtime/listVoices", json!({}))
    }

    pub fn realtime_stop(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/realtime/stop", json!({ "threadId": thread_id }))
    }

    pub fn turn_interrupt(&self, thread_id: &str, turn_id: &str) -> Result<Value> {
        self.request(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
        )
    }

    pub fn thread_archive(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/archive", json!({ "threadId": thread_id }))
    }

    pub fn thread_delete(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/delete", json!({ "threadId": thread_id }))
    }

    pub fn thread_unarchive(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/unarchive", json!({ "threadId": thread_id }))
    }

    pub fn thread_unsubscribe(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/unsubscribe", json!({ "threadId": thread_id }))
    }

    pub fn thread_compact(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/compact/start", json!({ "threadId": thread_id }))
    }

    pub fn thread_shell_command(&self, thread_id: &str, command: &str) -> Result<Value> {
        self.request(
            "thread/shellCommand",
            json!({ "threadId": thread_id, "command": command }),
        )
    }

    pub fn thread_name_set(&self, thread_id: &str, name: &str) -> Result<Value> {
        self.request(
            "thread/name/set",
            json!({ "threadId": thread_id, "name": name }),
        )
    }

    pub fn review_start(&self, thread_id: &str) -> Result<Value> {
        self.review_start_with_options(
            thread_id,
            json!({ "type": "uncommittedChanges" }),
            Some("inline"),
        )
    }

    pub fn review_start_with_options(
        &self,
        thread_id: &str,
        target: Value,
        delivery: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({
            "threadId": thread_id,
            "target": target,
        });
        if let Some(delivery) = delivery.filter(|value| !value.is_empty()) {
            params["delivery"] = Value::String(delivery.into());
        }
        self.request("review/start", params)
    }

    pub fn project_list(&self, limit: Option<u32>, cursor: Option<&str>) -> Result<Value> {
        let mut params = json!({});
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        self.request("project/list", params)
    }

    pub fn project_read(&self, project_id: &str) -> Result<Value> {
        self.request("project/read", json!({ "projectId": project_id }))
    }

    pub fn project_create(&self, idempotency_key: &str, name: &str, roots: Value) -> Result<Value> {
        self.request(
            "project/create",
            json!({ "idempotencyKey": idempotency_key, "name": name, "roots": roots }),
        )
    }

    pub fn project_update(&self, project_id: &str, update: Value) -> Result<Value> {
        let mut params = update.as_object().cloned().unwrap_or_default();
        params.insert("projectId".into(), Value::String(project_id.into()));
        self.request("project/update", Value::Object(params))
    }

    pub fn project_import(
        &self,
        idempotency_key: &str,
        name: &str,
        roots: Value,
        threads: Option<&[String]>,
        metadata: Option<Value>,
    ) -> Result<Value> {
        let mut params = json!({
            "idempotencyKey": idempotency_key,
            "name": name,
            "roots": roots,
        });
        if let Some(threads) = threads {
            params["threads"] = json!(threads);
        }
        if let Some(metadata) = metadata {
            params["metadata"] = metadata;
        }
        self.request("project/import", params)
    }

    pub fn project_move(&self, project_id: &str, before_project_id: Option<&str>) -> Result<Value> {
        let mut params = json!({ "projectId": project_id });
        if let Some(before_project_id) = before_project_id.filter(|value| !value.is_empty()) {
            params["beforeProjectId"] = Value::String(before_project_id.into());
        }
        self.request("project/move", params)
    }

    pub fn project_delete(&self, project_id: &str) -> Result<Value> {
        self.request("project/delete", json!({ "projectId": project_id }))
    }

    pub fn config_value_write(
        &self,
        key_path: &str,
        merge_strategy: &str,
        value: Value,
    ) -> Result<Value> {
        self.request(
            "config/value/write",
            json!({
                "keyPath": key_path,
                "mergeStrategy": merge_strategy,
                "value": value,
            }),
        )
    }

    pub fn config_batch_write(
        &self,
        edits: Value,
        expected_version: Option<&str>,
        file_path: Option<&str>,
        reload_user_config: bool,
    ) -> Result<Value> {
        let mut params = json!({
            "edits": edits,
            "reloadUserConfig": reload_user_config,
        });
        if let Some(expected_version) = expected_version.filter(|value| !value.is_empty()) {
            params["expectedVersion"] = Value::String(expected_version.into());
        }
        if let Some(file_path) = file_path.filter(|value| !value.is_empty()) {
            params["filePath"] = Value::String(file_path.into());
        }
        self.request("config/batchWrite", params)
    }

    pub fn config_mcp_server_reload(&self) -> Result<Value> {
        self.request("config/mcpServer/reload", json!({}))
    }

    pub fn marketplace_add(
        &self,
        source: &str,
        ref_name: Option<&str>,
        sparse_paths: &[String],
    ) -> Result<Value> {
        let mut params = json!({ "source": source });
        if let Some(ref_name) = ref_name.filter(|value| !value.is_empty()) {
            params["refName"] = Value::String(ref_name.into());
        }
        if !sparse_paths.is_empty() {
            params["sparsePaths"] = json!(sparse_paths);
        }
        self.request("marketplace/add", params)
    }

    pub fn marketplace_remove(&self, marketplace_name: &str) -> Result<Value> {
        self.request(
            "marketplace/remove",
            json!({ "marketplaceName": marketplace_name }),
        )
    }

    pub fn marketplace_upgrade(&self, marketplace_name: Option<&str>) -> Result<Value> {
        let mut params = json!({});
        if let Some(marketplace_name) = marketplace_name.filter(|value| !value.is_empty()) {
            params["marketplaceName"] = Value::String(marketplace_name.into());
        }
        self.request("marketplace/upgrade", params)
    }

    pub fn plugin_search(
        &self,
        search_term: &str,
        cursor: Option<&str>,
        cwds: Option<&[String]>,
        limit: Option<u32>,
        scope: Option<Value>,
    ) -> Result<Value> {
        let mut params = json!({ "searchTerm": search_term });
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            params["cursor"] = Value::String(cursor.into());
        }
        if let Some(cwds) = cwds {
            params["cwds"] = json!(cwds);
        }
        if let Some(limit) = limit {
            params["limit"] = json!(limit);
        }
        if let Some(scope) = scope {
            params["scope"] = scope;
        }
        self.request("plugin/search", params)
    }

    pub fn plugin_read(
        &self,
        plugin_name: &str,
        marketplace_path: Option<&str>,
        remote_marketplace_name: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({ "pluginName": plugin_name });
        if let Some(marketplace_path) = marketplace_path.filter(|value| !value.is_empty()) {
            params["marketplacePath"] = Value::String(marketplace_path.into());
        }
        if let Some(remote_marketplace_name) =
            remote_marketplace_name.filter(|value| !value.is_empty())
        {
            params["remoteMarketplaceName"] = Value::String(remote_marketplace_name.into());
        }
        self.request("plugin/read", params)
    }

    pub fn plugin_installed(
        &self,
        cwds: Option<&[String]>,
        install_suggestion_plugin_names: Option<&[String]>,
    ) -> Result<Value> {
        let mut params = json!({});
        if let Some(cwds) = cwds {
            params["cwds"] = json!(cwds);
        }
        if let Some(names) = install_suggestion_plugin_names {
            params["installSuggestionPluginNames"] = json!(names);
        }
        self.request("plugin/installed", params)
    }

    pub fn skills_config_write(
        &self,
        enabled: bool,
        name: Option<&str>,
        path: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({ "enabled": enabled });
        if let Some(name) = name.filter(|value| !value.is_empty()) {
            params["name"] = Value::String(name.into());
        }
        if let Some(path) = path.filter(|value| !value.is_empty()) {
            params["path"] = Value::String(path.into());
        }
        self.request("skills/config/write", params)
    }

    pub fn skills_extra_roots_set(&self, extra_roots: &[String]) -> Result<Value> {
        self.request(
            "skills/extraRoots/set",
            json!({ "extraRoots": extra_roots }),
        )
    }

    pub fn mcp_server_oauth_login(
        &self,
        name: &str,
        scopes: Option<&[String]>,
        thread_id: Option<&str>,
        timeout_secs: Option<i64>,
    ) -> Result<Value> {
        let mut params = json!({ "name": name });
        if let Some(scopes) = scopes {
            params["scopes"] = json!(scopes);
        }
        if let Some(thread_id) = thread_id.filter(|value| !value.is_empty()) {
            params["threadId"] = Value::String(thread_id.into());
        }
        if let Some(timeout_secs) = timeout_secs {
            params["timeoutSecs"] = json!(timeout_secs);
        }
        self.request("mcpServer/oauth/login", params)
    }

    pub fn mcp_server_tool_call(
        &self,
        server: &str,
        thread_id: &str,
        tool: &str,
        arguments: Option<Value>,
        meta: Option<Value>,
    ) -> Result<Value> {
        let mut params = json!({
            "server": server,
            "threadId": thread_id,
            "tool": tool,
        });
        if let Some(arguments) = arguments {
            params["arguments"] = arguments;
        }
        if let Some(meta) = meta {
            params["_meta"] = meta;
        }
        self.request("mcpServer/tool/call", params)
    }

    pub fn account_login_start(&self, params: Value) -> Result<Value> {
        self.request("account/login/start", params)
    }

    pub fn account_login_cancel(&self, login_id: &str) -> Result<Value> {
        self.request("account/login/cancel", json!({ "loginId": login_id }))
    }

    pub fn account_logout(&self) -> Result<Value> {
        self.request("account/logout", Value::Null)
    }

    pub fn command_exec(&self, command: &[String], options: Option<Value>) -> Result<Value> {
        let mut params = json!({ "command": command });
        if let Some(options) = options.and_then(|value| value.as_object().cloned()) {
            params
                .as_object_mut()
                .expect("command params is an object")
                .extend(options);
        }
        self.request("command/exec", params)
    }

    pub fn command_exec_write(
        &self,
        process_id: &str,
        delta_base64: Option<&str>,
        close_stdin: bool,
    ) -> Result<Value> {
        let mut params = json!({
            "processId": process_id,
            "closeStdin": close_stdin,
        });
        if let Some(delta_base64) = delta_base64 {
            params["deltaBase64"] = Value::String(delta_base64.into());
        }
        self.request("command/exec/write", params)
    }

    pub fn command_exec_terminate(&self, process_id: &str) -> Result<Value> {
        self.request("command/exec/terminate", json!({ "processId": process_id }))
    }

    pub fn command_exec_resize(&self, process_id: &str, rows: u16, cols: u16) -> Result<Value> {
        self.request(
            "command/exec/resize",
            json!({ "processId": process_id, "size": { "rows": rows, "cols": cols } }),
        )
    }

    pub fn process_spawn(
        &self,
        process_handle: &str,
        command: &[String],
        cwd: &str,
        options: Option<Value>,
    ) -> Result<Value> {
        let mut params = json!({
            "processHandle": process_handle,
            "command": command,
            "cwd": cwd,
        });
        if let Some(options) = options.and_then(|value| value.as_object().cloned()) {
            params
                .as_object_mut()
                .expect("process params is an object")
                .extend(options);
        }
        self.request("process/spawn", params)
    }

    pub fn process_write_stdin(
        &self,
        process_handle: &str,
        delta_base64: Option<&str>,
        close_stdin: bool,
    ) -> Result<Value> {
        let mut params = json!({
            "processHandle": process_handle,
            "closeStdin": close_stdin,
        });
        if let Some(delta_base64) = delta_base64 {
            params["deltaBase64"] = Value::String(delta_base64.into());
        }
        self.request("process/writeStdin", params)
    }

    pub fn process_kill(&self, process_handle: &str) -> Result<Value> {
        self.request("process/kill", json!({ "processHandle": process_handle }))
    }

    pub fn process_resize_pty(&self, process_handle: &str, rows: u16, cols: u16) -> Result<Value> {
        self.request(
            "process/resizePty",
            json!({ "processHandle": process_handle, "size": { "rows": rows, "cols": cols } }),
        )
    }

    pub fn plugin_install(
        &self,
        plugin_name: &str,
        marketplace_path: Option<&str>,
        remote_marketplace_name: Option<&str>,
    ) -> Result<Value> {
        let mut params = json!({ "pluginName": plugin_name });
        if let Some(marketplace_path) = marketplace_path.filter(|value| !value.is_empty()) {
            params["marketplacePath"] = Value::String(marketplace_path.into());
        }
        if let Some(remote_marketplace_name) =
            remote_marketplace_name.filter(|value| !value.is_empty())
        {
            params["remoteMarketplaceName"] = Value::String(remote_marketplace_name.into());
        }
        self.request("plugin/install", params)
    }

    pub fn plugin_uninstall(&self, plugin_id: &str) -> Result<Value> {
        self.request("plugin/uninstall", json!({ "pluginId": plugin_id }))
    }
}

impl Drop for AppServerClient {
    fn drop(&mut self) {
        if let Some(child) = &self.child {
            if let Ok(mut child) = child.lock() {
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}

fn read_messages<R>(
    mut reader: R,
    responses: Arc<Mutex<ResponseState>>,
    inbox: Arc<(Mutex<EventInbox>, Condvar)>,
) where
    R: BufRead,
{
    let mut line = String::new();
    let close_reason = loop {
        line.clear();
        let bytes = match reader.read_line(&mut line) {
            Ok(bytes) => bytes,
            Err(error) => break Some(format!("read app-server response: {error}")),
        };
        if bytes == 0 {
            break Some("app-server closed the JSONL stream".into());
        }
        let wire_line = line.trim();
        if wire_line.is_empty() || !wire_line.starts_with('{') {
            continue;
        }
        let message: Value = match serde_json::from_str(wire_line) {
            Ok(message) => message,
            Err(error) => break Some(format!("decode app-server JSONL message: {error}")),
        };
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            if message.get("method").is_none() {
                let result = if let Some(error) = message.get("error") {
                    Err(anyhow!("app-server request failed: {error}"))
                } else {
                    Ok(message.get("result").cloned().unwrap_or(Value::Null))
                };
                if let Ok(mut responses) = responses.lock() {
                    if let Some(sender) = responses.pending.remove(&id) {
                        let _ = sender.send(result);
                    } else {
                        responses.backlog.insert(id, result);
                    }
                }
                continue;
            }
        }
        if message.get("method").is_some() {
            let (lock, wake) = &*inbox;
            if let Ok(mut inbox) = lock.lock() {
                inbox.queue.push_back(message);
                wake.notify_all();
            }
        }
    };

    if let Ok(mut responses) = responses.lock() {
        let reason = close_reason
            .clone()
            .unwrap_or_else(|| "app-server reader stopped".into());
        for (_, sender) in responses.pending.drain() {
            let _ = sender.send(Err(anyhow!(reason.clone())));
        }
    }
    let (lock, wake) = &*inbox;
    if let Ok(mut inbox) = lock.lock() {
        inbox.closed = true;
        inbox.close_reason = close_reason;
        wake.notify_all();
    }
}

fn write_json_line(writer: &mut dyn Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *writer, value).context("encode app-server JSONL message")?;
    writer
        .write_all(b"\n")
        .context("write app-server JSONL message")?;
    writer.flush().context("flush app-server JSONL message")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct RecordingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn offline_fixture_round_trip_covers_lifecycle_and_turn_contract() {
        let fixture = [
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "userAgent": "fixture" } }),
            json!({ "jsonrpc": "2.0", "method": "thread/started", "params": { "thread": { "id": "t-1" } } }),
            json!({ "jsonrpc": "2.0", "id": 2, "result": { "data": [{ "id": "t-1", "name": "Fixture task", "status": "idle", "cwd": "/tmp" }] } }),
            json!({ "jsonrpc": "2.0", "id": 3, "result": { "thread": { "id": "t-2" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "result": { "turn": { "id": "turn-1" } } }),
            json!({ "jsonrpc": "2.0", "id": 5, "result": {} }),
            json!({ "jsonrpc": "2.0", "id": 6, "result": {} }),
            json!({ "jsonrpc": "2.0", "id": 7, "result": {} }),
            json!({ "jsonrpc": "2.0", "id": 8, "result": { "thread": { "id": "t-2", "status": "idle" } } }),
            json!({ "jsonrpc": "2.0", "id": 9, "result": {} }),
        ];
        let input = fixture
            .iter()
            .map(|value| format!("{value}\n"))
            .collect::<String>();
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);

        assert_eq!(client.initialize().unwrap()["userAgent"], "fixture");
        let threads = client.thread_list(None).unwrap();
        assert_eq!(threads[0].title, "Fixture task");
        assert_eq!(
            client.thread_start(Some("/tmp")).unwrap()["thread"]["id"],
            "t-2"
        );
        assert_eq!(
            client.turn_start("t-2", "hello").unwrap()["turn"]["id"],
            "turn-1"
        );
        client.turn_interrupt("t-2", "turn-1").unwrap();
        client.thread_archive("t-2").unwrap();
        client.thread_unarchive("t-2").unwrap();
        assert_eq!(client.thread_resume("t-2").unwrap()["thread"]["id"], "t-2");
        client.review_start("t-2").unwrap();

        let recorded_output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let lines = recorded_output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 10);
        assert!(lines[0].contains("initialize"));
        assert!(lines[1].contains("initialized"));
        assert!(lines[4].contains("turn/start"));
        assert!(lines[7].contains("thread/unarchive"));
        assert!(lines[9].contains("review/start"));
    }

    #[test]
    fn server_initiated_approval_gets_safe_decline_response() {
        let fixture = [
            json!({ "jsonrpc": "2.0", "id": 2, "method": "item/commandExecution/requestApproval", "params": { "command": "rm -i" } }),
            json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
        ];
        let input = fixture
            .iter()
            .map(|value| format!("{value}\n"))
            .collect::<String>();
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);
        client.request("initialize", json!({})).unwrap();
        let request = client.next_event(Duration::from_millis(100)).unwrap();
        client
            .respond(request["id"].clone(), json!({ "decision": "decline" }))
            .unwrap();
        let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(output.contains("\"id\":2"));
        assert!(output.contains("decline"));
    }

    #[test]
    fn thread_summary_accepts_name_and_title_variants() {
        let named = ServerThread::from_value(&json!({ "id": "a", "name": "Named" })).unwrap();
        let titled = ServerThread::from_value(&json!({ "id": "b", "title": "Titled" })).unwrap();
        assert_eq!(named.title, "Named");
        assert_eq!(titled.title, "Titled");
    }

    #[test]
    fn turn_input_preserves_absolute_image_and_file_attachments() {
        let input = b"{\"id\":1,\"result\":{\"turn\":{\"id\":\"turn-1\"}}}\n";
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input), recorded);
        let attachments = vec![
            "/tmp/screenshot.png".to_string(),
            "/tmp/notes.md".to_string(),
            "attachment".to_string(),
        ];

        client
            .turn_start_with_options_and_attachments(
                "thread-1",
                "Review these files",
                None,
                None,
                None,
                None,
                &attachments,
            )
            .unwrap();

        let line = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let request: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(request["params"]["input"][1]["type"], "localImage");
        assert_eq!(request["params"]["input"][1]["path"], "/tmp/screenshot.png");
        assert_eq!(request["params"]["input"][2]["type"], "mention");
        assert_eq!(request["params"]["input"][2]["name"], "notes.md");
        assert_eq!(request["params"]["input"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn thread_list_follows_next_cursor_without_losing_rows() {
        let input = b"{\"id\":1,\"result\":{\"data\":[{\"id\":\"t-1\"}],\"nextCursor\":\"page-2\"}}\n{\"id\":2,\"result\":{\"data\":[{\"id\":\"t-2\"}]}}\n";
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input), recorded);
        let threads = client.thread_list(None).unwrap();
        assert_eq!(
            threads
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            ["t-1", "t-2"]
        );
        let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(output.contains("page-2"));
    }

    #[test]
    fn full_turn_and_realtime_methods_emit_official_parameter_shapes() {
        let input = b"{\"id\":1,\"result\":{\"turn\":{\"id\":\"turn-1\"}}}\n{\"id\":2,\"result\":{}}\n{\"id\":3,\"result\":{}}\n{\"id\":4,\"result\":{}}\n{\"id\":5,\"result\":{}}\n";
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input), recorded);
        client
            .turn_start_with_full_options_and_attachments(
                "thread-1",
                "hello",
                Some("gpt-test"),
                Some("high"),
                Some("/tmp"),
                Some("on-request"),
                Some("workspaceWrite"),
                Some("openai"),
                Some("friendly"),
                &[],
            )
            .unwrap();
        client.turn_steer("thread-1", "turn-1", "steer").unwrap();
        client.realtime_start("thread-1", "text").unwrap();
        client.realtime_append_text("thread-1", "voice").unwrap();
        client.realtime_stop("thread-1").unwrap();
        let requests = String::from_utf8(bytes.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(requests[0]["params"]["sandboxPolicy"], "workspaceWrite");
        assert_eq!(requests[0]["params"]["modelProvider"], "openai");
        assert_eq!(requests[1]["params"]["expectedTurnId"], "turn-1");
        assert_eq!(requests[2]["method"], "thread/realtime/start");
        assert_eq!(requests[3]["params"]["text"], "voice");
        assert_eq!(requests[4]["method"], "thread/realtime/stop");
    }

    #[test]
    fn extended_reference_methods_emit_required_v2_wire_fields() {
        let input = (1..=64)
            .map(|id| format!(r#"{{"id":{id},"result":{{}}}}"#))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);
        client
            .thread_items_list("thread-1", Some("turn-1"), Some("cursor"), Some(20))
            .unwrap();
        client
            .thread_turns_list("thread-1", Some("cursor"), Some(20), Some("full"))
            .unwrap();
        client.thread_search("needle", Some(true)).unwrap();
        client
            .thread_search_occurrences("thread-1", "needle", Some("cursor"), Some(10))
            .unwrap();
        client.thread_goal_get("thread-1").unwrap();
        client
            .thread_goal_set("thread-1", Some("ship"), Some("inProgress"), Some(1000))
            .unwrap();
        client.thread_goal_clear("thread-1").unwrap();
        client
            .thread_settings_update("thread-1", json!({ "model": "gpt-test" }))
            .unwrap();
        client
            .thread_metadata_update("thread-1", json!({ "projectId": "project-1" }))
            .unwrap();
        client
            .thread_memory_mode_set("thread-1", "enabled")
            .unwrap();
        client.thread_revert("thread-1", "turn-1").unwrap();
        client.thread_rollback("thread-1", 1).unwrap();
        client
            .apps_read(&[String::from("app-1")], Some("thread-1"))
            .unwrap();
        client.account_rate_limits_read().unwrap();
        client.account_usage_read().unwrap();
        client.realtime_append_speech("thread-1", "hello").unwrap();
        client
            .realtime_append_audio("thread-1", json!({ "data": "AA==" }))
            .unwrap();
        client.realtime_list_voices().unwrap();
        client.project_list(Some(10), Some("cursor")).unwrap();
        client.project_read("project-1").unwrap();
        client
            .project_create("idempotency-1", "Project", json!([]))
            .unwrap();
        client
            .project_update("project-1", json!({ "name": "Updated" }))
            .unwrap();
        client
            .config_value_write("approval_policy", "replace", json!("never"))
            .unwrap();
        client.config_mcp_server_reload().unwrap();
        client
            .marketplace_add("https://example.invalid/marketplace", Some("main"), &[])
            .unwrap();
        client.marketplace_remove("main").unwrap();
        client.marketplace_upgrade(Some("main")).unwrap();
        client
            .plugin_install("plugin-name", Some("/tmp/marketplace"), Some("main"))
            .unwrap();
        client.plugin_uninstall("plugin-1").unwrap();

        let requests = String::from_utf8(bytes.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 29);
        assert_eq!(requests[0]["method"], "thread/items/list");
        assert_eq!(requests[0]["params"]["turnId"], "turn-1");
        assert_eq!(requests[1]["method"], "thread/turns/list");
        assert_eq!(requests[2]["method"], "thread/search");
        assert_eq!(requests[2]["params"]["archived"], true);
        assert_eq!(requests[6]["method"], "thread/goal/clear");
        assert_eq!(requests[7]["params"]["threadId"], "thread-1");
        assert_eq!(requests[12]["method"], "app/read");
        assert_eq!(requests[22]["params"]["mergeStrategy"], "replace");
        assert_eq!(requests[25]["params"]["marketplaceName"], "main");
        assert_eq!(requests[27]["params"]["pluginName"], "plugin-name");
    }

    #[test]
    fn extended_inventory_methods_emit_schema_required_fields() {
        let input = (1..=31)
            .map(|id| format!(r#"{{"id":{id},"result":{{}}}}"#))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);
        let ids = vec![String::from("queued-1"), String::from("queued-2")];
        let roots = json!([{ "path": "/tmp" }]);
        client
            .thread_section_move("thread-1", Some("section-1"), Some("thread-2"))
            .unwrap();
        client
            .thread_inject_items("thread-1", json!([{ "type": "message" }]))
            .unwrap();
        client
            .thread_queue_add(
                "thread-1",
                "client-message-1",
                json!([{ "type": "text", "text": "queued" }]),
            )
            .unwrap();
        client
            .thread_queue_list("thread-1", Some("cursor"), Some(10))
            .unwrap();
        client
            .thread_queue_update(
                "thread-1",
                "queued-1",
                json!([{ "type": "text", "text": "updated" }]),
            )
            .unwrap();
        client.thread_queue_delete("thread-1", "queued-1").unwrap();
        client.thread_queue_reorder("thread-1", &ids).unwrap();
        client
            .thread_queue_start("thread-1", Some("queued-2"))
            .unwrap();
        client
            .review_start_with_options(
                "thread-1",
                json!({ "type": "commit", "sha": "abc" }),
                Some("detached"),
            )
            .unwrap();
        client
            .project_import(
                "import-1",
                "Imported",
                roots,
                Some(&[String::from("thread-1")]),
                Some(json!({ "source": "fixture" })),
            )
            .unwrap();
        client.project_move("project-1", Some("project-2")).unwrap();
        client.project_delete("project-2").unwrap();
        client
            .config_batch_write(
                json!([{ "keyPath": "model", "value": "gpt-test" }]),
                Some("version-1"),
                Some("/tmp/config.toml"),
                true,
            )
            .unwrap();
        client
            .plugin_search(
                "lint",
                Some("cursor"),
                Some(&[String::from("/tmp")]),
                Some(5),
                None,
            )
            .unwrap();
        client
            .plugin_read("plugin-name", Some("/tmp/marketplace"), Some("main"))
            .unwrap();
        client
            .plugin_installed(
                Some(&[String::from("/tmp")]),
                Some(&[String::from("plugin-name")]),
            )
            .unwrap();
        client
            .skills_config_write(false, Some("skill-name"), Some("/tmp/SKILL.md"))
            .unwrap();
        client
            .skills_extra_roots_set(&[String::from("/tmp/skills")])
            .unwrap();
        client
            .mcp_server_oauth_login(
                "fixture",
                Some(&[String::from("read")]),
                Some("thread-1"),
                Some(30),
            )
            .unwrap();
        client
            .mcp_server_tool_call(
                "fixture",
                "thread-1",
                "search",
                Some(json!({ "query": "needle" })),
                Some(json!({})),
            )
            .unwrap();
        client
            .account_login_start(json!({ "type": "chatgptDeviceCode" }))
            .unwrap();
        client.account_login_cancel("login-1").unwrap();
        client.account_logout().unwrap();
        client
            .command_exec(
                &[String::from("printf"), String::from("ok")],
                Some(json!({ "processId": "process-1", "streamStdoutStderr": true })),
            )
            .unwrap();
        client
            .command_exec_write("process-1", Some("b2s="), true)
            .unwrap();
        client.command_exec_terminate("process-1").unwrap();
        client.command_exec_resize("process-1", 24, 80).unwrap();
        client
            .process_spawn(
                "handle-1",
                &[String::from("printf"), String::from("ok")],
                "/tmp",
                Some(json!({ "streamStdoutStderr": true })),
            )
            .unwrap();
        client
            .process_write_stdin("handle-1", Some("b2s="), true)
            .unwrap();
        client.process_kill("handle-1").unwrap();
        client.process_resize_pty("handle-1", 24, 80).unwrap();

        let requests = String::from_utf8(bytes.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 31);
        assert_eq!(requests[0]["method"], "thread/section/move");
        assert_eq!(requests[0]["params"]["beforeThreadId"], "thread-2");
        assert_eq!(
            requests[2]["params"]["clientUserMessageId"],
            "client-message-1"
        );
        assert_eq!(requests[7]["params"]["queuedSubmissionId"], "queued-2");
        assert_eq!(requests[8]["params"]["delivery"], "detached");
        assert_eq!(requests[9]["params"]["idempotencyKey"], "import-1");
        assert_eq!(requests[12]["params"]["reloadUserConfig"], true);
        assert_eq!(requests[13]["params"]["searchTerm"], "lint");
        assert_eq!(requests[18]["params"]["name"], "fixture");
        assert_eq!(requests[19]["params"]["arguments"]["query"], "needle");
        assert_eq!(requests[20]["params"]["type"], "chatgptDeviceCode");
        assert_eq!(requests[23]["params"]["command"][0], "printf");
        assert_eq!(requests[24]["params"]["deltaBase64"], "b2s=");
        assert_eq!(requests[27]["params"]["processHandle"], "handle-1");
        assert_eq!(requests[30]["params"]["size"]["cols"], 80);
    }

    #[test]
    fn launcher_noise_before_jsonl_does_not_break_initialize() {
        let input = b"mise launcher notice\n{\"id\":1,\"result\":{\"ok\":true}}\n";
        let client = AppServerClient::from_parts(Cursor::new(input), RecordingWriter::default());
        assert_eq!(client.request("initialize", json!({})).unwrap()["ok"], true);
    }
}
