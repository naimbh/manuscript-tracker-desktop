//! `calendar.*` handlers: port of CalendarController (an iCalendar feed of deadlines, revision due dates and
//! expected decisions, each with a reminder one day before).
//!
//! The desktop app has no public web server, so the feed is offered two ways:
//! - `calendar.export` writes a `.ics` file (native save dialog);
//! - an optional subscription feed served on `http://127.0.0.1:<port>/calendar/<token>.ics` (off by default,
//!   `calendar.setServe { serve }`), for calendar apps on this computer (Apple Calendar, Outlook).
//!
//! | Action | Args → result |
//! | --- | --- |
//! | `calendar.info` | → `{ serve, running, url, port, error }` |
//! | `calendar.export` | → `{ saved, events }` or `null` (cancelled) |
//! | `calendar.setServe` | `{ serve }` → like `calendar.info` |
//! | `calendar.resetToken` | → like `calendar.info` (the old link stops working) |

use crate::api::Ctx;
use crate::db::{Db, Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{paper, setting, submission};
use crate::util::{parse_date, random_token, today};
use chrono::{Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Default port of the local subscription feed (setting `calendar.port`).
pub const DEFAULT_PORT: u16 = 47311;

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "calendar.info" => info(ctx),
        "calendar.export" => export(ctx),
        "calendar.setServe" => set_serve(args, ctx),
        "calendar.resetToken" => reset_token(ctx),
        _ => return None,
    })
}

struct Event {
    uid: String,
    date: NaiveDate,
    summary: String,
}

/// The user's calendar token, created on first use (User::calendarToken()).
pub fn token(db: &Db) -> Result<String> {
    let t: Option<String> = db.scalar("SELECT calendar_token FROM users WHERE id = ?1", [USER_ID])?;
    if let Some(t) = t.filter(|t| !t.is_empty()) {
        return Ok(t);
    }
    if db.find("users", USER_ID)?.is_none() {
        return Err(ApiError::msg("Finish setting up the app first."));
    }
    let t = random_token(40);
    db.update("users", USER_ID, &json!({ "calendar_token": t }))?;
    Ok(t)
}

fn events(db: &Db, today: NaiveDate) -> Result<Vec<Event>> {
    db.with(|c| {
        let mut out = Vec::new();
        for d in c.rows(
            "SELECT d.id, d.title, d.due_on, p.title AS paper_title, p.short_title AS paper_short
             FROM deadlines d LEFT JOIN papers p ON p.id = d.paper_id
             WHERE d.user_id = ?1 AND d.done = 0 ORDER BY d.due_on",
            [USER_ID],
        )? {
            let Some(date) = d["due_on"].as_str().and_then(parse_date) else { continue };
            let mut summary = d["title"].as_str().unwrap_or("").to_string();
            if d["paper_title"].is_string() {
                summary.push_str(" · ");
                summary.push_str(&paper::display_title(&json!({ "title": d["paper_title"], "short_title": d["paper_short"] })));
            }
            out.push(Event { uid: format!("deadline-{}", d["id"]), date, summary });
        }
        for p in c.rows("SELECT id, title, short_title FROM papers WHERE user_id = ?1 ORDER BY id", [USER_ID])? {
            let title = paper::display_title(&p);
            for s in c.rows(
                "SELECT s.*, v.decision_days AS venue_decision_days FROM submissions s LEFT JOIN venues v ON v.id = s.venue_id
                 WHERE s.paper_id = ?1 ORDER BY s.submitted_on DESC, s.id DESC",
                [p["id"].as_i64().unwrap_or(0)],
            )? {
                let venue = s["venue_name"].as_str().unwrap_or("");
                let decision = s["decision"].as_str().unwrap_or("");
                if let Some(due) = s["revision_due_on"].as_str().and_then(parse_date) {
                    if matches!(decision, "major" | "minor") {
                        out.push(Event {
                            uid: format!("revision-{}", s["id"]),
                            date: due,
                            summary: format!("Revision due · {title} ({venue})"),
                        });
                    }
                }
                if let Some(clock) = submission::clock(&s, s["venue_decision_days"].as_i64(), today) {
                    if let Some(date) = clock["expected_on"].as_str().and_then(parse_date) {
                        out.push(Event {
                            uid: format!("decision-{}", s["id"]),
                            date,
                            summary: format!("Decision expected · {title} ({venue})"),
                        });
                    }
                }
            }
        }
        Ok(out)
    })
}

/// `addcslashes($t, ",;\\")` (plus newlines, which would break the line).
fn esc(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for ch in t.chars() {
        match ch {
            ',' | ';' | '\\' => {
                s.push('\\');
                s.push(ch);
            }
            '\r' => {}
            '\n' => s.push_str("\\n"),
            _ => s.push(ch),
        }
    }
    s
}

/// The feed body, exactly as CalendarController builds it.
pub fn ics(db: &Db) -> Result<String> {
    ics_at(db, today(), &Utc::now().format("%Y%m%dT%H%M%SZ").to_string())
}

fn ics_at(db: &Db, today: NaiveDate, stamp: &str) -> Result<String> {
    // The app name is fixed (no longer editable).
    let name = "Manuscript Tracker";
    let mut lines: Vec<String> = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Manuscript Tracker//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        format!("X-WR-CALNAME:{name}"),
    ];
    let fmt = |d: NaiveDate| d.format("%Y%m%d").to_string();
    for e in events(db, today)? {
        let summary = esc(&e.summary);
        lines.extend([
            "BEGIN:VEVENT".into(),
            format!("UID:{}@manuscripttracker", e.uid),
            format!("DTSTAMP:{stamp}"),
            format!("DTSTART;VALUE=DATE:{}", fmt(e.date)),
            format!("DTEND;VALUE=DATE:{}", fmt(e.date + Duration::days(1))),
            format!("SUMMARY:{summary}"),
            "BEGIN:VALARM".into(),
            "TRIGGER:-P1D".into(),
            "ACTION:DISPLAY".into(),
            format!("DESCRIPTION:{summary}"),
            "END:VALARM".into(),
            "END:VEVENT".into(),
        ]);
    }
    lines.push("END:VCALENDAR".into());
    Ok(lines.join("\r\n") + "\r\n")
}

fn export(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let body = ics(&ctx.db)?;
    use tauri_plugin_dialog::DialogExt;
    let app = ctx.app()?;
    let mut dialog = app.dialog().file().set_file_name("manuscript-tracker.ics").add_filter("Calendar", &["ics"]);
    if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
        dialog = dialog.set_parent(&w);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(Value::Null);
    };
    let dest = dest.into_path().map_err(|e| ApiError::msg(format!("Could not save there: {e}")))?;
    std::fs::write(&dest, &body)?;
    Ok(json!({ "saved": true, "events": body.matches("BEGIN:VEVENT").count() }))
}

// ---- Local subscription feed ------------------------------------------------------------------------------

struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
}

fn server() -> &'static Mutex<Option<Server>> {
    static S: OnceLock<Mutex<Option<Server>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn last_error() -> &'static Mutex<Option<String>> {
    static E: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(None))
}

pub fn port(db: &Db) -> u16 {
    setting::get_i64(db, "calendar.port").and_then(|p| u16::try_from(p).ok()).filter(|p| *p >= 1024).unwrap_or(DEFAULT_PORT)
}

fn running_port() -> Option<u16> {
    server().lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.port)
}

/// Start the feed on 127.0.0.1 (no-op when it is already running on that port).
pub fn start_server(db: Db) -> Result<u16> {
    let port = port(&db);
    let mut guard = server().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = guard.as_ref() {
        if s.port == port {
            return Ok(port);
        }
    }
    if let Some(s) = guard.take() {
        stop(s);
    }
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).map_err(|e| {
        let m = format!("Could not start the calendar feed on port {port}: {e}");
        *last_error().lock().unwrap_or_else(|x| x.into_inner()) = Some(m.clone());
        ApiError::msg(m)
    })?;
    *last_error().lock().unwrap_or_else(|x| x.into_inner()) = None;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();
    std::thread::Builder::new()
        .name("calendar-feed".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = stream {
                    let db = db.clone();
                    std::thread::spawn(move || {
                        let _ = serve_one(stream, &db, port);
                    });
                }
            }
        })
        .map_err(|e| ApiError::msg(format!("Could not start the calendar feed: {e}")))?;
    *guard = Some(Server { port, stop: stop_flag });
    Ok(port)
}

fn stop(s: Server) {
    s.stop.store(true, Ordering::SeqCst);
    // Wake the accept loop so it sees the flag.
    let _ = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], s.port)), std::time::Duration::from_millis(300));
}

pub fn stop_server() {
    if let Some(s) = server().lock().unwrap_or_else(|e| e.into_inner()).take() {
        stop(s);
    }
}

/// Start the feed at launch when it is switched on.
pub fn start_if_enabled(db: &Db) {
    if setting::get_bool(db, "calendar.serve") && db.find("users", USER_ID).ok().flatten().is_some() {
        let _ = start_server(db.clone());
    }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Answer one HTTP request: `GET /calendar/<token>.ics` with the right token and a local Host header.
fn serve_one(mut stream: TcpStream, db: &Db, port: u16) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
    let mut reader = BufReader::new(std::io::Read::take(stream.try_clone()?, 16 * 1024));
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut host = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("host") {
                host = v.trim().to_ascii_lowercase();
            }
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let local_host = host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}");
    let reply = |stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8], head: bool| -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        if !head {
            stream.write_all(body)?;
        }
        stream.flush()
    };
    if method != "GET" && method != "HEAD" {
        return reply(&mut stream, "405 Method Not Allowed", "text/plain; charset=utf-8", b"Method not allowed.", false);
    }
    let path = target.split('?').next().unwrap_or("");
    let given = path.strip_prefix("/calendar/").and_then(|t| t.strip_suffix(".ics")).unwrap_or("");
    let expected = token(db).unwrap_or_default();
    if !local_host || given.is_empty() || expected.is_empty() || !ct_eq(given.as_bytes(), expected.as_bytes()) {
        return reply(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"Not found.", method == "HEAD");
    }
    match ics(db) {
        Ok(body) => reply(&mut stream, "200 OK", "text/calendar; charset=utf-8", body.as_bytes(), method == "HEAD"),
        Err(_) => reply(&mut stream, "500 Internal Server Error", "text/plain; charset=utf-8", b"Something went wrong.", false),
    }
}

fn info(ctx: &Ctx) -> Result<Value> {
    let serve = setting::get_bool(&ctx.db, "calendar.serve");
    let running = running_port();
    let url = match (running, ctx.db.find("users", USER_ID)?.is_some()) {
        (Some(p), true) => Some(format!("http://127.0.0.1:{p}/calendar/{}.ics", token(&ctx.db)?)),
        _ => None,
    };
    let error = if serve && running.is_none() { last_error().lock().unwrap_or_else(|e| e.into_inner()).clone() } else { None };
    Ok(json!({ "serve": serve, "running": running.is_some(), "url": url, "port": port(&ctx.db), "error": error }))
}

fn set_serve(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = crate::validation::Validator::new(args).rule("serve", "required|boolean").validate()?;
    ctx.require_user()?;
    let on = v.bool("serve");
    setting::put(&ctx.db, "calendar.serve", Some(if on { "1" } else { "0" }))?;
    if on {
        if let Err(e) = start_server(ctx.db.clone()) {
            setting::put(&ctx.db, "calendar.serve", Some("0"))?;
            return Err(e);
        }
    } else {
        stop_server();
    }
    info(ctx)
}

fn reset_token(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    ctx.db.update("users", USER_ID, &json!({ "calendar_token": random_token(40) }))?;
    info(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn ics_output() {
        let db = test_db();
        let vid = db.insert("venues", &json!({ "user_id": 1, "name": "J; One", "decision_days": 30 })).unwrap();
        let pid = db.insert("papers", &json!({ "user_id": 1, "title": "Long title", "short_title": "Short, title" })).unwrap();
        db.insert("deadlines", &json!({ "user_id": 1, "paper_id": pid, "title": "Camera ready", "due_on": "2026-12-01" })).unwrap();
        db.insert("deadlines", &json!({ "user_id": 1, "title": "Done one", "due_on": "2026-12-02", "done": true })).unwrap();
        db.insert(
            "submissions",
            &json!({ "paper_id": pid, "venue_id": vid, "venue_name": "J; One", "submitted_on": "2026-11-01", "decision": "submitted", "is_current": true }),
        )
        .unwrap();
        db.insert(
            "submissions",
            &json!({ "paper_id": pid, "venue_name": "Other", "submitted_on": "2026-09-01", "decision": "major", "decision_on": "2026-10-01", "revision_due_on": "2026-12-20" }),
        )
        .unwrap();
        let out = ics_at(&db, NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(), "20261122T120000Z").unwrap();
        assert!(out.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Manuscript Tracker//EN\r\nCALSCALE:GREGORIAN\r\nX-WR-CALNAME:Manuscript Tracker\r\n"));
        assert!(out.ends_with("END:VCALENDAR\r\n"));
        assert_eq!(out.matches("BEGIN:VEVENT").count(), 3);
        assert!(out.contains("SUMMARY:Camera ready · Short\\, title\r\n"));
        assert!(out.contains("DTSTART;VALUE=DATE:20261201\r\nDTEND;VALUE=DATE:20261202\r\n"));
        assert!(out.contains("SUMMARY:Revision due · Short\\, title (Other)\r\n"));
        assert!(out.contains("SUMMARY:Decision expected · Short\\, title (J\\; One)\r\nBEGIN:VALARM\r\nTRIGGER:-P1D\r\n"));
        assert!(out.contains("DTSTART;VALUE=DATE:20261201\r\n"));
        assert!(out.contains("DTSTAMP:20261122T120000Z\r\n"));
        assert!(!out.contains("Done one"));
    }

    #[test]
    fn local_feed() {
        let db = test_db();
        setting::put(&db, "calendar.port", Some("47399")).unwrap();
        let p = start_server(db.clone()).unwrap();
        let tok = token(&db).unwrap();
        let get = |path: &str, host: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", p)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
            let mut out = String::new();
            std::io::Read::read_to_string(&mut s, &mut out).unwrap();
            out
        };
        let ok = get(&format!("/calendar/{tok}.ics"), &format!("127.0.0.1:{p}"));
        assert!(ok.starts_with("HTTP/1.1 200 OK"), "{ok}");
        assert!(ok.contains("BEGIN:VCALENDAR"));
        assert!(get("/calendar/wrong.ics", &format!("127.0.0.1:{p}")).starts_with("HTTP/1.1 404"));
        assert!(get(&format!("/calendar/{tok}.ics"), "evil.example:80").starts_with("HTTP/1.1 404"));
        stop_server();
    }
}
