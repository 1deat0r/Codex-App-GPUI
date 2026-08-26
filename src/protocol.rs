//! JSON-RPC over stdio adapter for the official Codex app-server.
//!
//! The transport is deliberately small and value-oriented. The app-server
//! protocol evolves quickly, so the UI-facing boundary keeps unknown fields in
//! `serde_json::Value` while enforcing the lifecycle that matters to a native
//! client: initialize, initialized, thread operations, turn operations, and
//! safe handling of server-initiated approvals.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CLIENT_NAME: &str = "codex_app_gpui";
const CLIENT_TITLE: &str = "Codex App GPUI";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcNotification {
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
            .and_then(Value::as_str)
            .unwrap_or("notLoaded")
            .to_string();
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let updated_at = value
            .get("updatedAt")
            .or_else(|| value.get("updated_at"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Some(Self { id, title, cwd, status, model, updated_at })
    }
}

struct Transport {
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
}

pub struct AppServerClient {
    transport: Arc<Mutex<Transport>>,
    next_id: AtomicU64,
    child: Option<Arc<Mutex<Child>>>,
}

impl AppServerClient {
    pub fn spawn(command_line: &str) -> Result<Self> {
        let mut parts = command_line.split_whitespace();
        let program = parts.next().ok_or_else(|| anyhow!("empty app-server command"))?;
        let mut command = Command::new(program);
        command.args(parts).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = command.spawn().with_context(|| format!("spawn app-server `{command_line}`"))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("app-server stdin unavailable"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("app-server stdout unavailable"))?;
        Ok(Self {
            transport: Arc::new(Mutex::new(Transport {
                reader: Box::new(BufReader::new(stdout)),
                writer: Box::new(stdin),
            })),
            next_id: AtomicU64::new(1),
            child: Some(Arc::new(Mutex::new(child))),
        })
    }

    pub fn from_parts<R, W>(reader: R, writer: W) -> Self
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        Self {
            transport: Arc::new(Mutex::new(Transport {
                reader: Box::new(reader),
                writer: Box::new(writer),
            })),
            next_id: AtomicU64::new(1),
            child: None,
        }
    }

    pub fn is_live(&self) -> bool {
        self.child.is_some()
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };
        let mut transport = self.transport.lock().map_err(|_| anyhow!("app-server transport poisoned"))?;
        write_json_line(&mut transport.writer, &request)?;

        let mut line = String::new();
        loop {
            line.clear();
            let bytes = transport.reader.read_line(&mut line).context("read app-server response")?;
            if bytes == 0 {
                return Err(anyhow!("app-server closed the JSONL stream"));
            }
            let message: Value = serde_json::from_str(line.trim()).context("decode app-server JSONL message")?;
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(anyhow!("app-server `{method}` failed: {error}"));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                let request_id = message.get("id").cloned().unwrap_or(Value::Null);
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": { "decision": "decline" }
                });
                write_json_line(&mut transport.writer, &response)?;
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        let notification = RpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        };
        let mut transport = self.transport.lock().map_err(|_| anyhow!("app-server transport poisoned"))?;
        write_json_line(&mut transport.writer, &notification)
    }

    pub fn respond(&self, id: Value, result: Value) -> Result<()> {
        let response = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let mut transport = self.transport.lock().map_err(|_| anyhow!("app-server transport poisoned"))?;
        write_json_line(&mut transport.writer, &response)
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

    pub fn thread_resume(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/resume", json!({ "threadId": thread_id }))
    }

    pub fn thread_read(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/read", json!({ "threadId": thread_id, "includeTurns": true }))
    }

    pub fn turn_start(&self, thread_id: &str, text: &str) -> Result<Value> {
        self.request(
            "turn/start",
            json!({ "threadId": thread_id, "input": [{ "type": "text", "text": text }] }),
        )
    }

    pub fn turn_interrupt(&self, thread_id: &str, turn_id: &str) -> Result<Value> {
        self.request("turn/interrupt", json!({ "threadId": thread_id, "turnId": turn_id }))
    }

    pub fn thread_archive(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/archive", json!({ "threadId": thread_id }))
    }

    pub fn thread_delete(&self, thread_id: &str) -> Result<Value> {
        self.request("thread/delete", json!({ "threadId": thread_id }))
    }

    pub fn thread_name_set(&self, thread_id: &str, name: &str) -> Result<Value> {
        self.request("thread/name/set", json!({ "threadId": thread_id, "name": name }))
    }
}

fn write_json_line(writer: &mut dyn Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *writer, value).context("encode app-server JSONL message")?;
    writer.write_all(b"\n").context("write app-server JSONL message")?;
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
        ];
        let input = fixture.iter().map(|value| format!("{value}\n")).collect::<String>();
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);

        assert_eq!(client.initialize().unwrap()["userAgent"], "fixture");
        let threads = client.thread_list(None).unwrap();
        assert_eq!(threads[0].title, "Fixture task");
        assert_eq!(client.thread_start(Some("/tmp" )).unwrap()["thread"]["id"], "t-2");
        assert_eq!(client.turn_start("t-2", "hello").unwrap()["turn"]["id"], "turn-1");
        client.turn_interrupt("t-2", "turn-1").unwrap();

        let recorded_output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let lines = recorded_output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 6);
        assert!(lines[0].contains("initialize"));
        assert!(lines[1].contains("initialized"));
        assert!(lines[4].contains("turn/start"));
    }

    #[test]
    fn server_initiated_approval_gets_safe_decline_response() {
        let fixture = [
            json!({ "jsonrpc": "2.0", "id": 2, "method": "item/commandExecution/requestApproval", "params": { "command": "rm -i" } }),
            json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
        ];
        let input = fixture.iter().map(|value| format!("{value}\n")).collect::<String>();
        let recorded = RecordingWriter::default();
        let bytes = recorded.0.clone();
        let client = AppServerClient::from_parts(Cursor::new(input.into_bytes()), recorded);
        client.request("initialize", json!({})).unwrap();
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
}
