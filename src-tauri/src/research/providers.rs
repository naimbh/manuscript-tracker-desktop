//! AI providers (port of VenueResearcher's drivers): Claude Code CLI, Anthropic API, OpenAI Responses,
//! Google Gemini and any OpenAI-compatible endpoint (OpenRouter, Groq, Ollama, LM Studio...).
//!
//! Every call asks for one JSON object matching a schema. `web: true` lets the provider search the web
//! itself (used only as a fallback when fetched pages lack the answer). Calls are cancellable and report
//! token usage. Keys come from the OS keychain through `models::setting`; they are never logged.

use crate::db::Db;
use crate::error::{ApiError, Result};
use crate::models::setting;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Providers people can choose in Settings → AI.
pub const PROVIDERS: &[(&str, &str)] = &[
    ("claude-code", "Claude Code"),
    ("api", "Anthropic API"),
    ("openai", "OpenAI"),
    ("gemini", "Google Gemini"),
    ("compatible", "OpenAI-compatible"),
];

/// Per provider: secret key, model key, help text, model placeholder.
pub const DETAILS: &[(&str, &str, &str, &str, &str)] = &[
    ("claude-code", "ai.claude_token", "ai.claude_model",
     "Uses your Claude subscription through the Claude Code CLI on this computer. If you are signed in to Claude Code, nothing else is needed. Web search is built in.", "opus"),
    ("api", "anthropic.key", "anthropic.model",
     "Anthropic API key from console.anthropic.com, billed per use. Uses Claude’s web search tool.", "claude-opus-5-5"),
    ("openai", "openai.key", "openai.model",
     "OpenAI API key from platform.openai.com. Uses the Responses API with web search.", "gpt-5"),
    ("gemini", "gemini.key", "gemini.model",
     "Google AI Studio key from aistudio.google.com. Uses Google Search grounding.", "gemini-2.5-flash"),
    ("compatible", "compatible.key", "compatible.model",
     "Any OpenAI-compatible endpoint: OpenRouter, Groq, Perplexity, Together, Ollama or LM Studio. Local models work too: pages are fetched by the app, so the model only reads them. Web search depends on the model (for example OpenRouter’s “:online” models or Perplexity Sonar).", "perplexity/sonar or openai/gpt-4o:online"),
];

pub fn driver(db: &Db) -> &'static str {
    let d = setting::get_str(db, "ai.driver");
    PROVIDERS.iter().map(|(k, _)| *k).find(|k| *k == d).unwrap_or("claude-code")
}

pub fn label(driver: &str) -> &'static str {
    PROVIDERS.iter().find(|(k, _)| *k == driver).map(|(_, l)| *l).unwrap_or("Claude Code")
}

pub fn detail(driver: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    DETAILS.iter().find(|d| d.0 == driver).map(|d| (d.1, d.2, d.3, d.4)).unwrap_or(("", "", "", ""))
}

pub fn model(db: &Db, driver: &str) -> String {
    setting::get_str(db, detail(driver).1)
}

pub fn configured(db: &Db) -> bool {
    match driver(db) {
        "api" => setting::get(db, "anthropic.key").is_some_and(|k| !k.is_empty()),
        "openai" => setting::get(db, "openai.key").is_some_and(|k| !k.is_empty()),
        "gemini" => setting::get(db, "gemini.key").is_some_and(|k| !k.is_empty()),
        "compatible" => !setting::get_str(db, "compatible.base_url").is_empty() && !setting::get_str(db, "compatible.model").is_empty(),
        _ => claude_binary(db).is_some(),
    }
}

/// `"OpenAI (gpt-5)"` or `"OpenAI: not set up yet"` (same text as the web app).
pub fn status(db: &Db) -> String {
    let d = driver(db);
    let name = label(d);
    if !configured(db) {
        return format!("{name}: not set up yet");
    }
    let m = model(db, d);
    if m.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({m})")
    }
}

/// Whether the provider can search the web itself.
pub fn supports_web(db: &Db) -> bool {
    match driver(db) {
        "compatible" => {
            let m = setting::get_str(db, "compatible.model").to_lowercase();
            m.contains(":online") || m.contains("sonar") || m.contains("search")
        }
        _ => true,
    }
}

// ---- Claude Code CLI detection ----------------------------------------------------------------------

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

fn is_executable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Numeric version parts of a directory name (`2.1.30` -> [2, 1, 30]) for sorting.
fn version_key(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

/// Copies of `claude` bundled with the Claude desktop app, newest first.
fn bundled_claude() -> Vec<PathBuf> {
    let h = home();
    let roots: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec![h.join("Library/Application Support/Claude/claude-code")]
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData/Roaming"));
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData/Local"));
        vec![appdata.join("Claude/claude-code"), local.join("Claude/claude-code"), local.join("AnthropicClaude/claude-code")]
    } else {
        vec![h.join(".config/Claude/claude-code")]
    };
    let names: &[&str] = if cfg!(target_os = "windows") { &["claude.exe"] } else { &["claude"] };
    let mut found: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(versions) = std::fs::read_dir(&root) else {
            continue;
        };
        for v in versions.flatten() {
            let vkey = version_key(&v.file_name().to_string_lossy());
            // <root>/<version>/<arch or build>/claude.app/Contents/MacOS/claude, or a flatter layout.
            let mut stack = vec![(v.path(), 0)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in entries.flatten() {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if p.is_dir() && depth < 5 {
                        stack.push((p, depth + 1));
                    } else if names.contains(&name.as_str()) && is_executable(&p) {
                        found.push((vkey.clone(), p));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Every place the CLI was found: `(path, where)`, in the order they are tried.
pub fn claude_candidates(db: &Db) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(p) = setting::get(db, "ai.claude_bin").filter(|p| !p.trim().is_empty()) {
        out.push((PathBuf::from(p.trim()), "Settings"));
    }
    let exe = if cfg!(target_os = "windows") { "claude.exe" } else { "claude" };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            out.push((dir.join(exe), "PATH"));
            if cfg!(target_os = "windows") {
                out.push((dir.join("claude.cmd"), "PATH"));
            }
        }
    }
    let h = home();
    let mut fixed = vec![
        h.join(".local/bin").join(exe),
        h.join(".claude/local").join(exe),
        h.join(".npm-global/bin").join(exe),
        h.join(".bun/bin").join(exe),
    ];
    if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData/Roaming"));
        fixed.push(appdata.join("npm/claude.cmd"));
    } else {
        fixed.extend([PathBuf::from("/opt/homebrew/bin/claude"), PathBuf::from("/usr/local/bin/claude"), PathBuf::from("/usr/bin/claude")]);
    }
    out.extend(fixed.into_iter().map(|p| (p, "Standard location")));
    out.extend(bundled_claude().into_iter().map(|p| (p, "Claude desktop app")));
    let mut seen = std::collections::HashSet::new();
    out.retain(|(p, _)| is_executable(p) && seen.insert(p.clone()));
    out
}

/// The `claude` CLI to use, if any.
pub fn claude_binary(db: &Db) -> Option<PathBuf> {
    claude_candidates(db).into_iter().next().map(|(p, _)| p)
}

/// `claude --version` (5 s limit).
pub fn claude_version(bin: &Path) -> Option<String> {
    let mut cmd = Command::new(bin);
    cmd.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    no_window(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return None,
        }
    }
    let mut s = String::new();
    child.stdout.take()?.read_to_string(&mut s).ok()?;
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[allow(unused_variables)]
fn no_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

// ---- Calls ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub input: i64,
    pub output: i64,
}

#[derive(Debug, Clone)]
pub struct Answer {
    pub data: Value,
    pub usage: Usage,
}

pub struct Ask {
    pub prompt: String,
    pub schema: Value,
    pub web: bool,
    pub timeout: u64,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Ask {
    pub fn new(prompt: String, schema: Value) -> Ask {
        Ask { prompt, schema, web: false, timeout: 240, cancel: None }
    }
}

fn short(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

/// The first JSON object in a model's text reply (code fences and prose around it are ignored).
pub fn parse_json_text(text: &str) -> Result<Value> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(t) {
        return Ok(v);
    }
    if let (Some(i), Some(j)) = (t.find('{'), t.rfind('}')) {
        if j > i {
            if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(&t[i..=j]) {
                return Ok(v);
            }
        }
    }
    Err(ApiError::msg("The answer could not be read. Try again."))
}

fn with_schema(prompt: &str, schema: &Value) -> String {
    format!("{prompt}\n\nAnswer with one JSON object only, no prose, matching this JSON Schema:\n{schema}")
}

fn err_message(body: &Value, raw: &str) -> String {
    body.pointer("/error/message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| raw.to_string())
}

/// Ask the configured provider for JSON matching `req.schema`.
pub async fn ask(db: &Db, req: Ask) -> Result<Answer> {
    if !configured(db) {
        return Err(ApiError::msg(format!("AI is not set up: {}.", status(db))));
    }
    let flag = req.cancel.clone().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let d = driver(db);
    let fut = async {
        match d {
            "api" => via_anthropic(db, &req).await,
            "openai" => via_openai(db, &req).await,
            "gemini" => via_gemini(db, &req).await,
            "compatible" => via_compatible(db, &req).await,
            _ => via_claude_code(db, &req, flag.clone()).await,
        }
    };
    super::runs::until_cancelled(&flag, fut).await.unwrap_or_else(|| Err(ApiError::msg(super::runs::CANCELLED)))
}

/// A tiny request that checks the provider answers. Returns the model's reply.
pub async fn ping(db: &Db) -> Result<String> {
    if !configured(db) {
        return Err(ApiError::msg(format!("Not set up: {}.", status(db))));
    }
    let schema =
        json!({ "type": "object", "properties": { "reply": { "type": "string" } }, "required": ["reply"], "additionalProperties": false });
    let mut req = Ask::new("Reply with the single word ready in the field \"reply\".".into(), schema);
    req.timeout = 90;
    let a = ask(db, req).await?;
    Ok(a.data.get("reply").and_then(Value::as_str).unwrap_or("ok").to_string())
}

fn http() -> &'static reqwest::Client {
    crate::api::http()
}

/// Models that take the dynamic-filtering web search tool and `output_config.effort`.
fn modern_claude(model: &str) -> bool {
    ["fable", "mythos", "opus-5", "opus-4-8", "opus-4-7", "opus-4-6", "sonnet-5", "sonnet-4-6", "haiku-5"].iter().any(|m| model.contains(m))
}

/// Models that accept the server-side refusal fallback.
fn takes_fallbacks(model: &str) -> bool {
    ["claude-fable-5-2", "claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"].contains(&model)
}

async fn via_anthropic(db: &Db, req: &Ask) -> Result<Answer> {
    let key = setting::get_str(db, "anthropic.key");
    let model = model(db, "api");
    let modern = modern_claude(&model);
    let mut output_config = json!({ "format": { "type": "json_schema", "schema": req.schema } });
    if modern {
        output_config["effort"] = json!("low");
    }
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "messages": [{ "role": "user", "content": format!("{} Answer only with the JSON object.", req.prompt) }],
        "output_config": output_config,
    });
    if req.web {
        let kind = if modern { "web_search_20260209" } else { "web_search_20250305" };
        body["tools"] = json!([{ "type": kind, "name": "web_search", "max_uses": 8 }]);
    }
    let mut fallbacks = takes_fallbacks(&model);
    let mut usage = Usage::default();
    let mut last: Value = Value::Null;
    for _turn in 0..4 {
        let send = |b: &Value, fb: bool| {
            let mut r = http()
                .post("https://api.anthropic.com/v1/messages")
                .timeout(Duration::from_secs(req.timeout))
                .header("x-api-key", &key)
                .header("anthropic-version", "2026-10-21")
                .header("content-type", "application/json");
            let mut b = b.clone();
            if fb {
                r = r.header("anthropic-beta", "server-side-fallback-2026-07-01");
                b["fallbacks"] = json!("default");
            }
            r.json(&b).send()
        };
        let mut res = send(&body, fallbacks).await.map_err(|e| ApiError::msg(format!("Anthropic: {}", net_err(&e))))?;
        let mut status = res.status();
        let mut raw = res.text().await.unwrap_or_default();
        if status.as_u16() == 400 && fallbacks && raw.contains("fallback") {
            fallbacks = false;
            res = send(&body, false).await.map_err(|e| ApiError::msg(format!("Anthropic: {}", net_err(&e))))?;
            status = res.status();
            raw = res.text().await.unwrap_or_default();
        }
        let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(ApiError::msg(format!("Anthropic: {}", short(&err_message(&v, &raw), 240))));
        }
        usage.input += v.pointer("/usage/input_tokens").and_then(Value::as_i64).unwrap_or(0)
            + v.pointer("/usage/cache_read_input_tokens").and_then(Value::as_i64).unwrap_or(0)
            + v.pointer("/usage/cache_creation_input_tokens").and_then(Value::as_i64).unwrap_or(0);
        usage.output += v.pointer("/usage/output_tokens").and_then(Value::as_i64).unwrap_or(0);
        let stop = v.get("stop_reason").and_then(Value::as_str).unwrap_or("").to_string();
        last = v;
        if stop != "pause_turn" {
            break;
        }
        let content = last.get("content").cloned().unwrap_or(json!([]));
        if let Some(msgs) = body["messages"].as_array_mut() {
            msgs.push(json!({ "role": "assistant", "content": content }));
        }
    }
    if last.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        return Err(ApiError::msg("The model declined this lookup."));
    }
    let texts: Vec<&str> = last
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect())
        .unwrap_or_default();
    let data = texts.last().and_then(|t| parse_json_text(t).ok()).map(Ok).unwrap_or_else(|| parse_json_text(&texts.concat()))?;
    Ok(Answer { data, usage })
}

fn net_err(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the request timed out".into()
    } else if e.is_connect() {
        "could not connect".into()
    } else {
        "network error".into()
    }
}

async fn via_openai(db: &Db, req: &Ask) -> Result<Answer> {
    let key = setting::get_str(db, "openai.key");
    let mut body = json!({
        "model": model(db, "openai"),
        "input": req.prompt,
        "text": { "format": { "type": "json_schema", "name": "result", "schema": req.schema, "strict": true } },
    });
    if req.web {
        body["tools"] = json!([{ "type": "web_search" }]);
    }
    let send = |b: &Value| {
        http().post("https://api.openai.com/v1/responses").timeout(Duration::from_secs(req.timeout)).bearer_auth(&key).json(b).send()
    };
    let res = send(&body).await.map_err(|e| ApiError::msg(format!("OpenAI: {}", net_err(&e))))?;
    let mut status = res.status();
    let mut raw = res.text().await.unwrap_or_default();
    let mut v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if status.as_u16() == 400 && err_message(&v, "").contains("schema") {
        // Some models reject strict schemas; fall back to asking for JSON in the prompt.
        if let Some(o) = body.as_object_mut() {
            o.remove("text");
        }
        body["input"] = json!(with_schema(&req.prompt, &req.schema));
        let res = send(&body).await.map_err(|e| ApiError::msg(format!("OpenAI: {}", net_err(&e))))?;
        status = res.status();
        raw = res.text().await.unwrap_or_default();
        v = serde_json::from_str(&raw).unwrap_or(Value::Null);
    }
    if !status.is_success() {
        return Err(ApiError::msg(format!("OpenAI: {}", short(&err_message(&v, &raw), 240))));
    }
    let text: String = v
        .get("output")
        .and_then(Value::as_array)
        .map(|out| {
            out.iter()
                .filter(|m| m["type"] == "message")
                .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
                .filter(|c| c["type"] == "output_text")
                .filter_map(|c| c["text"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let usage = Usage {
        input: v.pointer("/usage/input_tokens").and_then(Value::as_i64).unwrap_or(0),
        output: v.pointer("/usage/output_tokens").and_then(Value::as_i64).unwrap_or(0),
    };
    Ok(Answer { data: parse_json_text(&text)?, usage })
}

async fn via_gemini(db: &Db, req: &Ask) -> Result<Answer> {
    let key = setting::get_str(db, "gemini.key");
    let model = model(db, "gemini");
    let mut body = json!({ "contents": [{ "role": "user", "parts": [{ "text": with_schema(&req.prompt, &req.schema) }] }] });
    if req.web {
        body["tools"] = json!([{ "google_search": {} }]);
    } else {
        body["generationConfig"] = json!({ "responseMimeType": "application/json" });
    }
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", urlencode(&model));
    let res = http()
        .post(url)
        .timeout(Duration::from_secs(req.timeout))
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::msg(format!("Gemini: {}", net_err(&e))))?;
    let status = res.status();
    let raw = res.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(ApiError::msg(format!("Gemini: {}", short(&err_message(&v, &raw), 240))));
    }
    let text: String = v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect())
        .unwrap_or_default();
    let usage = Usage {
        input: v.pointer("/usageMetadata/promptTokenCount").and_then(Value::as_i64).unwrap_or(0),
        output: v.pointer("/usageMetadata/candidatesTokenCount").and_then(Value::as_i64).unwrap_or(0),
    };
    Ok(Answer { data: parse_json_text(&text)?, usage })
}

async fn via_compatible(db: &Db, req: &Ask) -> Result<Answer> {
    let base = setting::get_str(db, "compatible.base_url");
    let key = setting::get_str(db, "compatible.key");
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let send = |fmt: bool| {
        let mut body = json!({
            "model": setting::get_str(db, "compatible.model"),
            "messages": [{ "role": "user", "content": with_schema(&req.prompt, &req.schema) }],
        });
        if fmt {
            body["response_format"] = json!({ "type": "json_object" });
        }
        let mut r = http().post(&url).timeout(Duration::from_secs(req.timeout.max(180)));
        if !key.is_empty() {
            r = r.bearer_auth(&key);
        }
        r.json(&body).send()
    };
    let mut res = send(true).await.map_err(|e| ApiError::msg(format!("Provider: {}", net_err(&e))))?;
    if res.status().as_u16() == 400 {
        res = send(false).await.map_err(|e| ApiError::msg(format!("Provider: {}", net_err(&e))))?;
    }
    let status = res.status();
    let raw = res.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(ApiError::msg(format!("Provider: {}", short(&err_message(&v, &raw), 240))));
    }
    let text = v.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or("");
    let usage = Usage {
        input: v.pointer("/usage/prompt_tokens").and_then(Value::as_i64).unwrap_or(0),
        output: v.pointer("/usage/completion_tokens").and_then(Value::as_i64).unwrap_or(0),
    };
    Ok(Answer { data: parse_json_text(text)?, usage })
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Message shown when the CLI is not signed in.
pub const CLAUDE_SIGN_IN: &str = "Claude Code is not signed in for the app. Open Terminal and run “claude” once to sign in, or run “claude setup-token” and paste the token in Settings → AI. A token from “claude setup-token” is best for scheduled jobs, because it does not expire with your login.";

/// `CLAUDE_SIGN_IN` plus the CLI's own explanation, so the real reason (for example an expired login) is visible.
fn sign_in_error(cli_message: &str) -> ApiError {
    let reason = cli_message.trim();
    if reason.is_empty() {
        ApiError::msg(CLAUDE_SIGN_IN)
    } else {
        ApiError::msg(format!("{CLAUDE_SIGN_IN} Claude Code said: “{}”", short(reason, 240)))
    }
}

async fn via_claude_code(db: &Db, req: &Ask, cancel: Arc<AtomicBool>) -> Result<Answer> {
    let bin = claude_binary(db).ok_or_else(|| {
        ApiError::msg("Claude Code was not found on this computer. Install it from claude.com/code, or choose another provider.")
    })?;
    let token = setting::get(db, "ai.claude_token").filter(|t| !t.is_empty());
    let model = setting::get_str(db, "ai.claude_model");
    let prompt = req.prompt.clone();
    let schema = req.schema.to_string();
    let web = req.web;
    let timeout = req.timeout;
    tokio::task::spawn_blocking(move || run_claude(&bin, token.as_deref(), &model, &prompt, &schema, web, timeout, &cancel))
        .await
        .map_err(|_| ApiError::msg("Claude Code stopped unexpectedly."))?
}

#[allow(clippy::too_many_arguments)]
fn run_claude(
    bin: &Path,
    token: Option<&str>,
    model: &str,
    prompt: &str,
    schema: &str,
    web: bool,
    timeout: u64,
    cancel: &AtomicBool,
) -> Result<Answer> {
    let dir = std::env::temp_dir().join("manuscripttracker-ai");
    let _ = std::fs::create_dir_all(&dir);
    let mut cmd = Command::new(bin);
    cmd.current_dir(&dir)
        .arg("-p")
        .args(["--output-format", "json", "--json-schema", schema, "--no-session-persistence"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    if web {
        cmd.args(["--tools", "WebSearch,WebFetch", "--allowedTools", "WebSearch,WebFetch"]);
    } else {
        cmd.args(["--tools", ""]);
    }
    // GUI apps start with a minimal PATH; the CLI may need node from Homebrew.
    if !cfg!(target_os = "windows") {
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{path}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"));
    }
    if let Some(t) = token {
        cmd.env("CLAUDE_CODE_OAUTH_TOKEN", t);
    }
    // Never bill an API key from the environment when the subscription is meant.
    cmd.env_remove("ANTHROPIC_API_KEY");
    no_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| ApiError::msg(format!("Claude Code could not start: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let p = prompt.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(p.as_bytes());
        });
    }
    let out_handle = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut b = String::new();
            let _ = s.read_to_string(&mut b);
            b
        })
    });
    let err_handle = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut b = String::new();
            let _ = s.read_to_string(&mut b);
            b
        })
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if cancel.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ApiError::msg(super::runs::CANCELLED));
                }
                if start.elapsed() > Duration::from_secs(timeout) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ApiError::msg("Claude Code took too long to answer."));
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(ApiError::msg(format!("Claude Code: {e}"))),
        }
    }
    let stdout = out_handle.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();
    parse_claude_output(&stdout, &stderr)
}

/// Read `claude -p --output-format json` output.
pub fn parse_claude_output(stdout: &str, stderr: &str) -> Result<Answer> {
    let out: Value = match serde_json::from_str(stdout.trim()) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            let msg = if stderr.trim().is_empty() { stdout } else { stderr };
            let lower = msg.to_lowercase();
            if lower.contains("login")
                || lower.contains("logged in")
                || lower.contains("log in")
                || lower.contains("authenticat")
                || lower.contains("oauth")
            {
                return Err(sign_in_error(msg));
            }
            return Err(ApiError::msg(format!("Claude Code did not answer: {}", short(msg, 200))));
        }
    };
    let usage = Usage {
        input: out.pointer("/usage/input_tokens").and_then(Value::as_i64).unwrap_or(0)
            + out.pointer("/usage/cache_read_input_tokens").and_then(Value::as_i64).unwrap_or(0)
            + out.pointer("/usage/cache_creation_input_tokens").and_then(Value::as_i64).unwrap_or(0),
        output: out.pointer("/usage/output_tokens").and_then(Value::as_i64).unwrap_or(0),
    };
    if out.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        let msg = out.get("result").and_then(Value::as_str).unwrap_or("unknown error");
        let lower = msg.to_lowercase();
        if lower.contains("authenticate") || lower.contains("login") || lower.contains("invalid api key") || lower.contains("oauth") {
            return Err(sign_in_error(msg));
        }
        return Err(ApiError::msg(format!("Claude Code: {}", short(msg, 240))));
    }
    let data = match out.get("structured_output") {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => parse_json_text(out.get("result").and_then(Value::as_str).unwrap_or(""))?,
    };
    Ok(Answer { data, usage })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_from_text() {
        assert_eq!(parse_json_text("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert_eq!(parse_json_text("Here you go: {\"a\": {\"b\": 2}} hope it helps").unwrap()["a"]["b"], 2);
        assert!(parse_json_text("no json here").is_err());
        assert!(parse_json_text("[1,2]").is_err());
    }

    #[test]
    fn claude_output() {
        let ok = r#"{"type":"result","is_error":false,"result":"","structured_output":{"reply":"ready"},"usage":{"input_tokens":10,"cache_read_input_tokens":5,"output_tokens":3}}"#;
        let a = parse_claude_output(ok, "").unwrap();
        assert_eq!(a.data["reply"], "ready");
        assert_eq!(a.usage, Usage { input: 15, output: 3 });
        let fallback = r#"{"is_error":false,"result":"{\"reply\":\"ok\"}"}"#;
        assert_eq!(parse_claude_output(fallback, "").unwrap().data["reply"], "ok");
        let auth = r#"{"is_error":true,"result":"Invalid API key · Please run /login"}"#;
        let err = parse_claude_output(auth, "").unwrap_err().to_string();
        assert!(err.starts_with(CLAUDE_SIGN_IN) && err.contains("Invalid API key · Please run /login"));
        assert!(parse_claude_output("", "Error: not logged in").unwrap_err().to_string().contains("not signed in"));
    }

    #[test]
    fn status_and_detection() {
        let db = crate::db::test_db();
        setting::put(&db, "ai.driver", Some("openai")).unwrap();
        assert_eq!(status(&db), "OpenAI: not set up yet");
        setting::put(&db, "openai.key", Some("sk-test-123456789")).unwrap();
        assert_eq!(status(&db), "OpenAI (gpt-5)");
        assert!(configured(&db));
        setting::put(&db, "openai.key", None).unwrap();
        setting::put(&db, "ai.driver", Some("compatible")).unwrap();
        setting::put(&db, "compatible.base_url", Some("http://localhost:11434/v1")).unwrap();
        setting::put(&db, "compatible.model", Some("llama3.1")).unwrap();
        assert!(configured(&db));
        assert!(!supports_web(&db));
        setting::put(&db, "ai.driver", Some("nonsense")).unwrap();
        assert_eq!(driver(&db), "claude-code");
        assert_eq!(version_key("2.1.30"), vec![2, 1, 30]);
        assert!(modern_claude("claude-opus-5-5") && !modern_claude("claude-haiku-4-5"));
    }
}
