//! `agents.*` handlers: Settings → Agents (MCP). The server is this app's own executable started with `--mcp`
//! (see `crate::mcp`), so assistants on this computer need no token and no web address.
//!
//! | Action | Args → result |
//! | --- | --- |
//! | `agents.info` | → `{ command, args, config, cli, tools: [{ name, title, description }] }` |
//! | `agents.test` | → starts `<exe> --mcp`, runs initialize + tools/list: `{ ok, tools, protocol }` |

use crate::api::Ctx;
use crate::error::{ApiError, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    let _ = args;
    Some(match name {
        "agents.info" => info(ctx),
        "agents.test" => test(),
        _ => return None,
    })
}

fn exe() -> Result<String> {
    let p = std::env::current_exe().map_err(|e| ApiError::msg(format!("Could not find the app: {e}")))?;
    let p = std::fs::canonicalize(&p).unwrap_or(p);
    Ok(plain_path(&p.to_string_lossy()))
}

/// Windows: `canonicalize` returns a verbatim path (`\\?\C:\...`), which assistants' configs do not expect.
fn plain_path(p: &str) -> String {
    match p.strip_prefix(r"\\?\") {
        Some(rest) if rest.starts_with(r"UNC\") => format!(r"\\{}", &rest[4..]),
        Some(rest) => rest.to_string(),
        None => p.to_string(),
    }
}

/// `escapeshellarg` for the user's shell (POSIX single quotes; double quotes on Windows).
fn shell_quote(s: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn config_json(exe: &str) -> String {
    let v = json!({ "mcpServers": { "manuscript-tracker": { "command": exe, "args": [crate::mcp::FLAG] } } });
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    let _ = serde::Serialize::serialize(&v, &mut ser);
    String::from_utf8(buf).unwrap_or_default()
}

fn info(ctx: &Ctx) -> Result<Value> {
    let _ = ctx;
    let exe = exe()?;
    let tools: Vec<Value> = crate::mcp::tool_list()
        .iter()
        .map(|t| json!({ "name": t["name"], "title": t["title"], "description": t["description"], "read_only": t["annotations"]["readOnlyHint"] == json!(true) }))
        .collect();
    Ok(json!({
        "command": exe,
        "args": [crate::mcp::FLAG],
        "config": config_json(&exe),
        "cli": format!("claude mcp add manuscript-tracker -- {} {}", shell_quote(&exe), crate::mcp::FLAG),
        "tools": tools,
    }))
}

/// Start the server exactly as an assistant would and check that it answers.
fn test() -> Result<Value> {
    let exe = exe()?;
    let mut child = Command::new(&exe)
        .arg(crate::mcp::FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ApiError::msg(format!("Could not start the server: {e}")))?;
    let mut stdin = child.stdin.take().ok_or_else(|| ApiError::msg("Could not talk to the server."))?;
    let stdout = child.stdout.take().ok_or_else(|| ApiError::msg("Could not talk to the server."))?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines();
        let mut got = Vec::new();
        while got.len() < 2 {
            match lines.next() {
                Some(Ok(l)) => got.push(l),
                _ => break,
            }
        }
        let _ = tx.send(got);
    });
    let msgs = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": crate::mcp::DEFAULT_PROTOCOL, "capabilities": {}, "clientInfo": { "name": "settings-test", "version": "1" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ];
    for m in msgs {
        let _ = writeln!(stdin, "{m}");
    }
    let _ = stdin.flush();
    let got = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_default();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let parsed: Vec<Value> = got.iter().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let protocol = parsed.first().and_then(|v| v["result"]["protocolVersion"].as_str()).map(str::to_string);
    let tools = parsed.get(1).and_then(|v| v["result"]["tools"].as_array()).map_or(0, Vec::len);
    if protocol.is_none() || tools == 0 {
        return Err(ApiError::msg("The server did not answer. Try again, or restart the app."));
    }
    Ok(json!({ "ok": true, "tools": tools, "protocol": protocol }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_windows_paths() {
        assert_eq!(plain_path(r"\\?\C:\Program Files\Manuscript Tracker\app.exe"), r"C:\Program Files\Manuscript Tracker\app.exe");
        assert_eq!(plain_path(r"\\?\UNC\server\share\app.exe"), r"\\server\share\app.exe");
        assert_eq!(plain_path("/usr/bin/manuscript-tracker"), "/usr/bin/manuscript-tracker");
    }

    #[test]
    fn config_and_quoting() {
        let c = config_json("/Applications/Manuscript Tracker.app/Contents/MacOS/manuscript-tracker");
        let v: Value = serde_json::from_str(&c).unwrap();
        assert_eq!(v["mcpServers"]["manuscript-tracker"]["args"], json!(["--mcp"]));
        assert!(c.contains("/Applications/Manuscript Tracker.app"));
        if !cfg!(windows) {
            assert_eq!(shell_quote("/a b/it's"), "'/a b/it'\\''s'");
        }
    }
}
