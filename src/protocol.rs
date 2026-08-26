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
        Some(Self {
            id,
            title,
            cwd,
            status,
            model,
            updated_at,
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
        let mut params = json!({ "limit": 100 });
        if let Some(search_term) = search_term.filter(|term| !term.is_empty()) {
            params["searchTerm"] = Value::String(search_term.into());
        }
        let value = self.request("thread/list", params)?;
        let array = value
            .get("data")
            .or_else(|| value.get("threads"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(array.iter().filter_map(ServerThread::from_value).collect())
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
        self.request("turn/start", params)
    }

    pub fn turn_steer(&self, thread_id: &str, text: &str) -> Result<Value> {
        self.request(
            "turn/steer",
            json!({ "threadId": thread_id, "input": [{ "type": "text", "text": text }] }),
        )
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
        self.request(
            "review/start",
            json!({
                "threadId": thread_id,
                "target": { "type": "uncommittedChanges" },
                "delivery": "inline"
            }),
        )
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
    fn launcher_noise_before_jsonl_does_not_break_initialize() {
        let input = b"mise launcher notice\n{\"id\":1,\"result\":{\"ok\":true}}\n";
        let client = AppServerClient::from_parts(Cursor::new(input), RecordingWriter::default());
        assert_eq!(client.request("initialize", json!({})).unwrap()["ok"], true);
    }
}
