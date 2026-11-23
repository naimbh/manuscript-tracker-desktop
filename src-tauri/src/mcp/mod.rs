//! Local MCP server (port of app/Mcp: TrackerServer and its tools).
//!
//! Started as `<app executable> --mcp` by an assistant (Claude Desktop, Claude Code, Cursor…). It speaks
//! JSON-RPC 2.0 over stdin/stdout, one message per line (MCP stdio transport), against the app's own SQLite
//! database. No window opens and nothing but protocol messages is written to stdout. It runs whether or not
//! the app is open: both use the same database file (WAL mode, busy timeout).

pub mod tools;

use crate::db::Db;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Command-line flag that starts the server instead of the app.
pub const FLAG: &str = "--mcp";
pub const SERVER_NAME: &str = "Manuscript Tracker";
pub const SERVER_VERSION: &str = "1.0.0";
pub const INSTRUCTIONS: &str = "Tools for a researcher’s manuscript tracker: papers and their submissions, editor decisions, deadlines, the venue library, calls for papers and published work. Use list-papers or get-paper before changing anything, and confirm writes with the user.";
/// Protocol versions we answer with (newest first); others get [`DEFAULT_PROTOCOL`].
pub const PROTOCOLS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const DEFAULT_PROTOCOL: &str = "2025-06-18";
/// Bundle identifier: the folder name Tauri uses for app data.
pub const IDENTIFIER: &str = "com.naimbh.manuscripttracker";

/// The tool list (names, titles, descriptions, input schemas, annotations), identical to the web app's.
pub const TOOLS_JSON: &str = include_str!("tools.json");

/// `MT_DATA_DIR` overrides the data folder in debug builds and tests only (development and testing).
pub fn data_dir_override() -> Option<PathBuf> {
    if cfg!(debug_assertions) || cfg!(test) {
        std::env::var_os("MT_DATA_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)
    } else {
        None
    }
}

/// The app data folder without a running app (same place as Tauri's `app_data_dir()`).
pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = data_dir_override() {
        return Some(d);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library").join("Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local").join("share")))
    };
    base.map(|b| b.join(IDENTIFIER))
}

/// One server: the database and the folder paper files are stored in.
pub struct Server {
    pub db: Db,
    pub files_root: PathBuf,
}

impl Server {
    pub fn new(db: Db, files_root: &Path) -> Server {
        Server { db, files_root: files_root.to_path_buf() }
    }

    /// Handle one decoded message (or a batch). `None` for notifications and responses.
    pub fn handle(&self, msg: &Value) -> Option<Value> {
        if let Some(batch) = msg.as_array() {
            if batch.is_empty() {
                return Some(error(Value::Null, -32600, "Invalid Request"));
            }
            let out: Vec<Value> = batch.iter().filter_map(|m| self.handle_one(m)).collect();
            return (!out.is_empty()).then_some(Value::Array(out));
        }
        self.handle_one(msg)
    }

    fn handle_one(&self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        };
        let id = obj.get("id").cloned();
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // A response from the client (we never send requests) or garbage.
            return match id {
                Some(id) if obj.get("result").is_none() && obj.get("error").is_none() => Some(error(id, -32600, "Invalid Request")),
                _ => None,
            };
        };
        let Some(id) = id.filter(|i| !i.is_null()) else {
            // Notifications (initialized, cancelled, roots/list_changed…) need no answer.
            return None;
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(id, -32600, "Invalid Request"));
        }
        let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
        Some(match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
                let version = if PROTOCOLS.contains(&requested) { requested } else { DEFAULT_PROTOCOL };
                result(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {
                            "tools": { "listChanged": false },
                            "resources": { "listChanged": false },
                            "prompts": { "listChanged": false },
                        },
                        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                        "instructions": INSTRUCTIONS,
                    }),
                )
            }
            "ping" => result(id, json!({})),
            "tools/list" => result(id, json!({ "tools": tool_list() })),
            "tools/call" => self.call(id, &params),
            "resources/list" => result(id, json!({ "resources": [] })),
            "resources/templates/list" => result(id, json!({ "resourceTemplates": [] })),
            "prompts/list" => result(id, json!({ "prompts": [] })),
            _ => error(id, -32601, "Method not found"),
        })
    }

    fn call(&self, id: Value, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error(id, -32602, "Missing [name] parameter.");
        };
        if !tool_list().iter().any(|t| t["name"] == name) {
            return error(id, -32602, &format!("Tool [{name}] not found."));
        }
        let empty = json!({});
        let args = match params.get("arguments") {
            Some(a) if a.is_object() => a,
            _ => &empty,
        };
        let (text, is_error) = match tools::call(self, name, args) {
            Ok(tools::Out::Text(t)) => (t, false),
            Ok(tools::Out::Json(v)) => (serde_json::to_string(&v).unwrap_or_default(), false),
            Err(e) => (e, true),
        };
        result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
    }
}

/// The parsed tool list.
pub fn tool_list() -> &'static Vec<Value> {
    static LIST: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str(TOOLS_JSON).unwrap_or_default())
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Serve newline-delimited JSON-RPC from `input` to `output` until EOF.
pub fn serve<R: BufRead, W: Write>(server: &Server, input: R, mut output: W) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => server.handle(&msg),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(reply) = reply {
            output.write_all(serde_json::to_string(&reply).unwrap_or_default().as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Entry point for `--mcp`: returns the process exit code.
pub fn main() -> i32 {
    let Some(dir) = data_dir() else {
        eprintln!("Manuscript Tracker: could not find the app data folder.");
        return 1;
    };
    if let Err(e) = std::fs::create_dir_all(dir.join("files")) {
        eprintln!("Manuscript Tracker: could not create {}: {e}", dir.display());
        return 1;
    }
    let db = match Db::open(&dir.join("tracker.sqlite")) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Manuscript Tracker: could not open the database: {e}");
            return 1;
        }
    };
    let server = Server::new(db, &dir.join("files"));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve(&server, stdin.lock(), stdout.lock()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Manuscript Tracker: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    fn run(server: &Server, input: &str) -> Vec<Value> {
        let mut out = Vec::new();
        serve(server, input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn text(v: &Value) -> &str {
        v["result"]["content"][0]["text"].as_str().unwrap()
    }

    #[test]
    fn protocol_over_a_pipe() {
        let tmp = std::env::temp_dir().join(format!("mt-mcp-test-{}", crate::util::random_token(8)));
        std::fs::create_dir_all(&tmp).unwrap();
        let server = Server::new(test_db(), &tmp);
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add-paper","arguments":{"title":"Pipes and Filters","tags":["Thesis"]}}}"#,
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list-papers","arguments":{"search":"pipes"}}}"#,
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get-paper","arguments":{"paper_id":999}}}"#,
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nope"}}"#,
            r#"{"jsonrpc":"2.0","id":8,"method":"nope"}"#,
            "not json",
        ]
        .join("\n");
        let out = run(&server, &input);
        assert_eq!(out.len(), 9);
        assert_eq!(out[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(out[0]["result"]["serverInfo"]["name"], "Manuscript Tracker");
        let tools = out[1]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 13);
        assert_eq!(tools[0]["name"], "list-papers");
        assert_eq!(tools[2]["inputSchema"]["required"], json!(["title"]));
        assert_eq!(out[2]["result"], json!({}));
        assert_eq!(text(&out[3]), "Added paper #1: “Pipes and Filters” (drafting).");
        let list: Value = serde_json::from_str(text(&out[4])).unwrap();
        assert_eq!(list[0]["title"], "Pipes and Filters");
        assert_eq!(list[0]["tags"], json!(["Thesis"]));
        assert_eq!(out[5]["result"]["isError"], json!(true));
        assert_eq!(text(&out[5]), "No paper with that id.");
        assert_eq!(out[6]["error"]["code"], -32602);
        assert_eq!(out[7]["error"]["code"], -32601);
        assert_eq!(out[8]["error"]["code"], -32700);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
