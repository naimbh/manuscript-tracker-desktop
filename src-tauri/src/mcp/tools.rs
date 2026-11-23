//! The MCP tools (ports of app/Mcp/Tools/*.php). Each returns text or JSON on success and a message on
//! failure (sent back with `isError: true`, like `Response::error`).

use super::Server;
use crate::db::{Query, USER_ID};
use crate::models::{label, paper, paper_file, publication, submission, venue};
use crate::util::{parse_date, random_token, today, today_str};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

pub enum Out {
    Text(String),
    Json(Value),
}

type R = std::result::Result<Out, String>;

/// Paper fields an agent may set (PaperFields::paperData), in the web app's order.
const PAPER_FIELDS: &[&str] = &[
    "title",
    "short_title",
    "stage",
    "started_on",
    "abstract",
    "keywords",
    "method",
    "dataset",
    "tags",
    "notes",
    "overleaf_url",
    "drive_url",
    "preprint_server",
    "preprint_url",
    "preprint_on",
];

pub fn call(server: &Server, name: &str, args: &Value) -> R {
    let conn = server.db.conn();
    let conn: &Connection = &conn;
    owner(conn)?;
    match name {
        "list-papers" => list_papers(conn, args),
        "get-paper" => get_paper(conn, args),
        "add-paper" => add_paper(conn, args),
        "update-paper" => update_paper(conn, args),
        "record-submission" => record_submission(conn, args),
        "update-submission" => update_submission(conn, args),
        "attach-file" => attach_file(conn, args, &server.files_root),
        "record-decision" => record_decision(conn, args),
        "list-deadlines" => list_deadlines(conn),
        "add-deadline" => add_deadline(conn, args),
        "search-venues" => search_venues(conn, args),
        "list-opportunities" => list_opportunities(conn, args),
        "list-publications" => list_publications(conn),
        _ => Err(format!("Tool [{name}] not found.")),
    }
}

fn db<T>(r: crate::error::Result<T>) -> std::result::Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// ResolvesUser::owner: the local user.
fn owner(conn: &Connection) -> std::result::Result<Value, String> {
    db(conn.find("users", USER_ID))?.ok_or_else(|| "No account found. Open Manuscript Tracker and finish setting it up first.".to_string())
}

/// `$request->get($k)`, with null treated as missing.
fn get<'a>(args: &'a Value, k: &str) -> Option<&'a Value> {
    args.get(k).filter(|v| !v.is_null())
}

/// A string argument (numbers are accepted and turned into text).
fn s(args: &Value, k: &str) -> Option<String> {
    match get(args, k)? {
        Value::String(x) => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "1".into() } else { String::new() }),
        _ => None,
    }
}

/// A non-empty string argument (PHP truthiness for `if ($x = $request->get(...))`).
fn truthy(args: &Value, k: &str) -> Option<String> {
    s(args, k).filter(|x| !x.is_empty() && x != "0")
}

fn id(args: &Value, k: &str) -> Option<i64> {
    crate::validation::as_integer(get(args, k)?)
}

fn date_arg(args: &Value, k: &str) -> std::result::Result<Option<String>, String> {
    match s(args, k) {
        None => Ok(None),
        Some(x) => match parse_date(&x) {
            Some(d) => Ok(Some(d.format("%Y-%m-%d").to_string())),
            None => Err(format!("The {k} must be a date in YYYY-MM-DD form.")),
        },
    }
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn find_paper(conn: &Connection, args: &Value) -> std::result::Result<Value, String> {
    let pid = id(args, "paper_id");
    pid.and_then(|pid| db(conn.first("SELECT * FROM papers WHERE id = ?1 AND user_id = ?2", [pid, USER_ID])).ok().flatten())
        .ok_or_else(|| "No paper with that id.".to_string())
}

fn find_submission(conn: &Connection, args: &Value) -> std::result::Result<Value, String> {
    let sid = id(args, "submission_id");
    sid.and_then(|sid| {
        db(conn
            .first("SELECT s.* FROM submissions s JOIN papers p ON p.id = s.paper_id WHERE s.id = ?1 AND p.user_id = ?2", [sid, USER_ID]))
        .ok()
        .flatten()
    })
    .ok_or_else(|| "No submission with that id.".to_string())
}

/// PaperFields::paperData: the fields present, trimmed to the column limits.
fn paper_data(args: &Value) -> std::result::Result<Map<String, Value>, String> {
    let mut d = Map::new();
    for &k in PAPER_FIELDS {
        let Some(v) = get(args, k) else { continue };
        let v = match k {
            "tags" => match v {
                Value::Array(a) => Value::Array(
                    a.iter()
                        .filter_map(|t| match t {
                            Value::String(x) => Some(json!(x)),
                            Value::Number(n) => Some(json!(n.to_string())),
                            _ => None,
                        })
                        .collect(),
                ),
                Value::String(x) => json!([x]),
                _ => continue,
            },
            "started_on" | "preprint_on" => match date_arg(args, k)? {
                Some(d) => json!(d),
                None => continue,
            },
            "method" | "dataset" | "short_title" => match s(args, k) {
                Some(x) => json!(x.chars().take(255).collect::<String>()),
                None => continue,
            },
            _ => match s(args, k) {
                Some(x) => json!(x),
                None => continue,
            },
        };
        d.insert(k.to_string(), v);
    }
    if let Some(stage) = d.get("stage").and_then(Value::as_str) {
        if !paper::STAGES.iter().any(|(key, _)| *key == stage) {
            d.remove("stage");
        }
    }
    Ok(d)
}

fn list_papers(conn: &Connection, args: &Value) -> R {
    let mut sql = String::from("SELECT * FROM papers WHERE user_id = ?1");
    let mut params: Vec<Value> = vec![json!(USER_ID)];
    if let Some(stage) = truthy(args, "stage") {
        params.push(json!(stage));
        sql.push_str(&format!(" AND stage = ?{}", params.len()));
    }
    if let Some(q) = truthy(args, "search") {
        params.push(json!(format!("%{q}%")));
        let n = params.len();
        sql.push_str(&format!(" AND (title LIKE ?{n} OR short_title LIKE ?{n} OR keywords LIKE ?{n})"));
    }
    sql.push_str(" ORDER BY updated_at DESC, id DESC");
    let papers = db(conn.rows(&sql, crate::db::params_json(&params)))?;
    let current = db(paper::current_submissions(conn))?;
    let t = today();
    let out: Vec<Value> = papers
        .iter()
        .map(|p| {
            let cur = p["id"].as_i64().and_then(|id| current.get(&id));
            json!({
                "id": p["id"], "title": p["title"], "short_title": p["short_title"], "stage": p["stage"], "tags": p["tags"],
                "current_venue": cur.map(|s| s["venue_name"].clone()),
                "manuscript_id": cur.map(|s| s["manuscript_id"].clone()),
                "decision": cur.map(|s| s["decision"].clone()),
                "days_in_review": cur.and_then(|s| submission::days_in_review(s, t)),
            })
        })
        .collect();
    Ok(Out::Json(Value::Array(out)))
}

fn get_paper(conn: &Connection, args: &Value) -> R {
    let p = find_paper(conn, args)?;
    let pid = p["id"].as_i64().unwrap_or(0);
    let authors: Vec<Value> = db(conn.rows("SELECT * FROM authors WHERE paper_id = ?1 ORDER BY position", [pid]))?
        .into_iter()
        .map(|a| json!({ "name": a["name"], "affiliation": a["affiliation"], "orcid": a["orcid"], "corresponding": a["corresponding"], "roles": a["roles"] }))
        .collect();
    let mut subs = Vec::new();
    for s in db(conn.rows("SELECT * FROM submissions WHERE paper_id = ?1 ORDER BY submitted_on DESC, id DESC", [pid]))? {
        let items: Vec<Value> =
            db(conn.rows("SELECT * FROM revision_items WHERE submission_id = ?1 ORDER BY position", [&s["id"].as_i64().unwrap_or(0)]))?
                .into_iter()
                .map(|i| json!({ "comment": i["comment"], "done": i["done"], "response": i["response"] }))
                .collect();
        subs.push(json!({
            "id": s["id"], "venue": s["venue_name"], "manuscript_id": s["manuscript_id"], "submitted_on": s["submitted_on"],
            "status_since": s["status_on"], "decision": s["decision"], "decision_on": s["decision_on"],
            "revision_due_on": s["revision_due_on"], "current": s["is_current"], "portal": s["portal_url"], "note": s["note"],
            "reviewer_comments": items,
        }));
    }
    let queue: Vec<Value> =
        db(conn
            .rows("SELECT v.name FROM queue_items q JOIN venues v ON v.id = q.venue_id WHERE q.paper_id = ?1 ORDER BY q.position", [pid]))?
        .into_iter()
        .map(|r| r["name"].clone())
        .collect();
    Ok(Out::Json(json!({
        "id": p["id"], "title": p["title"], "short_title": p["short_title"], "stage": p["stage"], "tags": p["tags"],
        "abstract": p["abstract"], "keywords": p["keywords"], "method": p["method"], "data": p["dataset"],
        "preprint": p["preprint_url"], "notes": p["notes"],
        "authors": authors,
        "submissions": subs,
        "next_venues": queue,
    })))
}

fn add_paper(conn: &Connection, args: &Value) -> R {
    let mut d = paper_data(args)?;
    if d.get("title").and_then(Value::as_str).unwrap_or("").trim().is_empty() {
        return Err("Give the paper title.".into());
    }
    d.entry("stage").or_insert(json!("drafting"));
    let id = db(paper::insert(conn, &Value::Object(d)))?;
    let p = db(conn.find("papers", id))?.unwrap_or_default();
    Ok(Out::Text(format!("Added paper #{id}: “{}” ({}).", str_of(&p, "title"), str_of(&p, "stage"))))
}

fn update_paper(conn: &Connection, args: &Value) -> R {
    let p = find_paper(conn, args)?;
    let d = paper_data(args)?;
    if d.is_empty() {
        return Err("Nothing to change.".into());
    }
    let pid = p["id"].as_i64().unwrap_or(0);
    let keys = d.keys().cloned().collect::<Vec<_>>().join(", ");
    db(conn.update("papers", pid, &Value::Object(d)))?;
    let p = db(conn.find("papers", pid))?.unwrap_or(p);
    Ok(Out::Text(format!("Updated “{}”: {keys}.", str_of(&p, "title"))))
}

fn record_submission(conn: &Connection, args: &Value) -> R {
    let p = find_paper(conn, args)?;
    let pid = p["id"].as_i64().unwrap_or(0);
    let name = s(args, "venue").unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return Err("Give the venue name.".into());
    }
    let existing =
        db(conn.first("SELECT * FROM venues WHERE user_id = ?1 AND name = ?2 ORDER BY id LIMIT 1", rusqlite::params![USER_ID, name]))?;
    let v = match existing {
        Some(v) => v,
        None => {
            let kind = s(args, "kind").filter(|k| venue::KINDS.iter().any(|(x, _)| x == k)).unwrap_or_else(|| "journal".into());
            let vid = db(conn.insert("venues", &json!({ "user_id": USER_ID, "name": name, "kind": kind })))?;
            db(conn.find("venues", vid))?.unwrap_or_default()
        }
    };
    let vid = v["id"].as_i64().unwrap_or(0);
    let submitted_on = date_arg(args, "submitted_on")?.unwrap_or_else(today_str);
    let portal = s(args, "portal_url").map(Value::String).unwrap_or_else(|| v["portal_url"].clone());
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    db(tx.exec(
        "UPDATE submissions SET is_current = 0, updated_at = ?2 WHERE paper_id = ?1",
        rusqlite::params![pid, crate::util::now_ts()],
    ))?;
    let sid = db(tx.insert(
        "submissions",
        &json!({
            "paper_id": pid, "venue_id": vid, "venue_name": v["name"], "manuscript_id": s(args, "manuscript_id"),
            "submitted_on": submitted_on, "portal_url": portal, "decision": "submitted", "is_current": true,
            "note": s(args, "note"),
        }),
    ))?;
    db(tx.exec("DELETE FROM queue_items WHERE paper_id = ?1 AND venue_id = ?2", [pid, vid]))?;
    db(tx.update("papers", pid, &json!({ "stage": "submitted" })))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Out::Text(format!("Recorded submission #{sid} of “{}” to {}.", str_of(&p, "title"), str_of(&v, "name"))))
}

fn append_note(existing: &Value, note: &str) -> String {
    let old = existing.as_str().unwrap_or("");
    let joined = if old.is_empty() { note.to_string() } else { format!("{old}\n{note}") };
    joined.trim().to_string()
}

fn update_submission(conn: &Connection, args: &Value) -> R {
    let s0 = find_submission(conn, args)?;
    let sid = s0["id"].as_i64().unwrap_or(0);
    let pid = s0["paper_id"].as_i64().unwrap_or(0);
    let mut d = Map::new();
    for k in ["manuscript_id", "portal_url"] {
        if let Some(v) = s(args, k) {
            d.insert(k.into(), json!(v));
        }
    }
    for k in ["submitted_on", "revision_due_on"] {
        if let Some(v) = date_arg(args, k)? {
            d.insert(k.into(), json!(v));
        }
    }
    if let Some(status) = truthy(args, "status") {
        if !submission::DECISIONS.iter().any(|(k, _)| *k == status) {
            return Err("Unknown status.".into());
        }
        let status_on = date_arg(args, "status_on")?.unwrap_or_else(today_str);
        let decision_on = if submission::WAITING.contains(&status.as_str()) {
            Value::Null
        } else {
            json!(date_arg(args, "decision_on")?.unwrap_or_else(|| status_on.clone()))
        };
        d.insert("decision".into(), json!(status));
        d.insert("status_on".into(), json!(status_on));
        d.insert("decision_on".into(), decision_on);
    } else if let Some(v) = date_arg(args, "status_on")? {
        d.insert("status_on".into(), json!(v));
    }
    if let Some(note) = truthy(args, "note") {
        d.insert("note".into(), json!(append_note(&s0["note"], &note)));
    }
    let make_current = get(args, "current") == Some(&Value::Bool(true));
    if make_current {
        d.insert("is_current".into(), json!(true));
    }
    if d.is_empty() {
        return Err("Nothing to change.".into());
    }
    let keys = d.keys().cloned().collect::<Vec<_>>().join(", ");
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if make_current {
        db(tx.exec(
            "UPDATE submissions SET is_current = 0, updated_at = ?2 WHERE paper_id = ?1",
            rusqlite::params![pid, crate::util::now_ts()],
        ))?;
    }
    db(tx.update("submissions", sid, &Value::Object(d)))?;
    let s1 = db(tx.find("submissions", sid))?.unwrap_or_default();
    let p = db(tx.find("papers", pid))?.unwrap_or_default();
    let current = db(paper::current_submission(&tx, pid))?;
    if current.as_ref().and_then(|c| c["id"].as_i64()) == Some(sid) && p["stage"] != "published" {
        db(tx.update("papers", pid, &json!({ "stage": submission::stage_for(s1["decision"].as_str().unwrap_or("")) })))?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Out::Text(format!("Updated {} for “{}”: {keys}.", str_of(&s1, "venue_name"), str_of(&p, "title"))))
}

fn attach_file(conn: &Connection, args: &Value, files_root: &std::path::Path) -> R {
    let p = find_paper(conn, args)?;
    let pid = p["id"].as_i64().unwrap_or(0);
    let src_str = s(args, "path").unwrap_or_default();
    let src = std::path::PathBuf::from(&src_str);
    let meta = std::fs::metadata(&src).ok().filter(|m| m.is_file());
    let Some(meta) = meta.filter(|_| std::fs::File::open(&src).is_ok()) else {
        return Err(format!("File not found: {src_str}"));
    };
    if meta.len() > 50 * 1_048_576 {
        return Err("The file is larger than 50 MB.".into());
    }
    let kind = s(args, "kind").unwrap_or_else(|| "manuscript".into());
    if !paper_file::KINDS.iter().any(|(k, _)| *k == kind) {
        return Err("Unknown kind.".into());
    }
    let sub_id = id(args, "submission_id");
    if let Some(sub) = sub_id {
        if db(conn.count("SELECT COUNT(*) FROM submissions WHERE id = ?1 AND paper_id = ?2", [sub, pid]))? == 0 {
            return Err("That submission does not belong to this paper.".into());
        }
    }
    let base = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    let rel = format!("papers/{pid}/{}{}", random_token(40), ext.map(|e| format!(".{e}")).unwrap_or_default());
    let dest = files_root.join(&rel);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not store the file: {e}"))?;
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("Could not store the file: {e}"))?;
    let name = truthy(args, "name").unwrap_or(base.clone());
    let size = meta.len() as i64;
    let row = json!({
        "paper_id": pid, "submission_id": sub_id, "kind": kind, "label": s(args, "label"), "path": rel,
        "original_name": name, "mime": crate::files::mime_for(&base), "size": size,
    });
    if let Err(e) = conn.insert("paper_files", &row) {
        let _ = std::fs::remove_file(&dest);
        return Err(e.to_string());
    }
    Ok(Out::Text(format!(
        "Attached {name} ({}) to “{}” as {}.",
        paper_file::human_size(size),
        str_of(&p, "title"),
        label(paper_file::KINDS, &kind)
    )))
}

fn record_decision(conn: &Connection, args: &Value) -> R {
    let s0 = find_submission(conn, args)?;
    let sid = s0["id"].as_i64().unwrap_or(0);
    let pid = s0["paper_id"].as_i64().unwrap_or(0);
    let decision = s(args, "decision").unwrap_or_default();
    if !submission::DECISIONS.iter().any(|(k, _)| *k == decision) {
        return Err("Unknown decision.".into());
    }
    let date = date_arg(args, "date")?.unwrap_or_else(today_str);
    let revision_due = match date_arg(args, "revision_due_on")? {
        Some(d) => json!(d),
        None => s0["revision_due_on"].clone(),
    };
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let was_current =
        s0["is_current"] == json!(true) || db(paper::current_submission(&tx, pid))?.and_then(|c| c["id"].as_i64()) == Some(sid);
    db(tx.update(
        "submissions",
        sid,
        &json!({
            "decision": decision, "status_on": date,
            "decision_on": if submission::WAITING.contains(&decision.as_str()) { Value::Null } else { json!(date) },
            "revision_due_on": revision_due,
        }),
    ))?;
    if was_current {
        db(tx.update("papers", pid, &json!({ "stage": submission::stage_for(&decision) })))?;
    }
    if let Some(note) = truthy(args, "note") {
        db(tx.update("submissions", sid, &json!({ "note": append_note(&s0["note"], &note) })))?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Out::Text(format!("{}: {} on {date}.", str_of(&s0, "venue_name"), submission::decision_label(&decision))))
}

fn list_deadlines(conn: &Connection) -> R {
    let t = today();
    let days = |d: &str| parse_date(d).map(|x| (x - t).num_days());
    let mut rows: Vec<Value> = db(conn.rows(
        "SELECT d.title, d.due_on, p.title AS paper_title, p.short_title AS paper_short FROM deadlines d
         LEFT JOIN papers p ON p.id = d.paper_id WHERE d.user_id = ?1 AND d.done = 0 ORDER BY d.due_on",
        [USER_ID],
    ))?
    .into_iter()
    .map(|d| {
        let paper = d["paper_title"]
            .is_string()
            .then(|| paper::display_title(&json!({ "title": d["paper_title"], "short_title": d["paper_short"] })));
        json!({ "title": d["title"], "due_on": d["due_on"], "days_left": days(d["due_on"].as_str().unwrap_or("")), "paper": paper })
    })
    .collect();
    let rev = db(conn.rows(
        "SELECT s.venue_name, s.revision_due_on, p.title, p.short_title FROM submissions s JOIN papers p ON p.id = s.paper_id
         WHERE p.user_id = ?1 AND s.decision IN ('major', 'minor') AND s.revision_due_on IS NOT NULL ORDER BY s.id",
        [USER_ID],
    ))?;
    rows.extend(rev.into_iter().map(|s| {
        json!({
            "title": format!("Revision due: {}", str_of(&s, "venue_name")),
            "due_on": s["revision_due_on"],
            "days_left": days(s["revision_due_on"].as_str().unwrap_or("")),
            "paper": paper::display_title(&s),
        })
    }));
    rows.sort_by(|a, b| a["due_on"].as_str().unwrap_or("").cmp(b["due_on"].as_str().unwrap_or("")));
    Ok(Out::Json(Value::Array(rows)))
}

fn add_deadline(conn: &Connection, args: &Value) -> R {
    let paper_id = match id(args, "paper_id") {
        Some(pid) => db(conn.first("SELECT id FROM papers WHERE id = ?1 AND user_id = ?2", [pid, USER_ID]))?.and_then(|p| p["id"].as_i64()),
        None => None,
    };
    let title = s(args, "title").unwrap_or_default();
    let Some(due) = date_arg(args, "due_on")? else {
        return Err("Give the due date (YYYY-MM-DD).".into());
    };
    db(conn.insert("deadlines", &json!({ "user_id": USER_ID, "title": title, "due_on": due, "paper_id": paper_id })))?;
    Ok(Out::Text(format!("Added “{title}” on {due}.")))
}

fn search_venues(conn: &Connection, args: &Value) -> R {
    let mut sql = String::from("SELECT * FROM venues WHERE user_id = ?1");
    let mut params: Vec<Value> = vec![json!(USER_ID)];
    if let Some(kind) = truthy(args, "kind") {
        params.push(json!(kind));
        sql.push_str(&format!(" AND kind = ?{}", params.len()));
    }
    if let Some(q) = truthy(args, "search") {
        params.push(json!(format!("%{q}%")));
        let n = params.len();
        sql.push_str(&format!(" AND (name LIKE ?{n} OR field LIKE ?{n})"));
    }
    sql.push_str(" ORDER BY name LIMIT 50");
    const KEEP: &[&str] = &[
        "id",
        "name",
        "kind",
        "field",
        "publisher",
        "acceptance_rate",
        "first_decision",
        "proceedings",
        "next_edition",
        "deadline_label",
        "deadline_expected",
        "review_type",
        "word_limit",
        "fee",
        "preprint_policy",
        "website_url",
    ];
    let out: Vec<Value> = db(conn.rows(&sql, crate::db::params_json(&params)))?
        .into_iter()
        .map(|v| {
            let mut o = Map::new();
            for &k in KEEP.iter().chain(["deadline_on"].iter()) {
                let x = &v[k];
                if x.is_null() || x.as_str() == Some("") {
                    continue;
                }
                o.insert(k.to_string(), x.clone());
            }
            Value::Object(o)
        })
        .collect();
    Ok(Out::Json(Value::Array(out)))
}

fn list_opportunities(conn: &Connection, args: &Value) -> R {
    let status = s(args, "status").unwrap_or_else(|| "new".into());
    let rows = db(conn.rows(
        "SELECT * FROM opportunities WHERE user_id = ?1 AND status = ?2 AND (deadline_on IS NULL OR deadline_on >= ?3)
         ORDER BY fit DESC, id LIMIT 30",
        rusqlite::params![USER_ID, status, today_str()],
    ))?;
    Ok(Out::Json(Value::Array(
        rows.into_iter()
            .map(|o| {
                json!({ "title": o["title"], "kind": o["kind"], "venue": o["venue"], "outlet": o["outlet"],
                    "deadline_on": o["deadline_on"], "fit": o["fit"], "why": o["why"], "url": o["url"] })
            })
            .collect(),
    )))
}

fn list_publications(conn: &Connection) -> R {
    let user = owner(conn)?;
    let pubs: Vec<Value> = db(conn.rows("SELECT * FROM publications WHERE user_id = ?1 ORDER BY year DESC, id", [USER_ID]))?
        .into_iter()
        .map(|p| {
            let cite = publication::citation(&p);
            let venue = if cite.is_empty() { p["publisher"].clone() } else { json!(cite) };
            json!({ "title": p["title"], "venue": venue, "year": p["year"], "kind": p["kind"],
                "citations": p["citations"], "doi": p["doi"] })
        })
        .collect();
    Ok(Out::Json(json!({ "summary": user["scholar_stats"], "publications": pubs })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    fn txt(r: R) -> String {
        match r {
            Ok(Out::Text(t)) => t,
            Ok(Out::Json(v)) => v.to_string(),
            Err(e) => format!("ERR {e}"),
        }
    }

    #[test]
    fn write_tools() {
        let tmp = std::env::temp_dir().join(format!("mt-tools-{}", random_token(8)));
        std::fs::create_dir_all(&tmp).unwrap();
        let server = Server::new(test_db(), &tmp.join("files"));
        let c = |name: &str, a: Value| txt(call(&server, name, &a));
        assert_eq!(c("add-paper", json!({ "title": " " })), "ERR Give the paper title.");
        assert_eq!(
            c("add-paper", json!({ "title": "Paper A", "short_title": "A", "stage": "ready" })),
            "Added paper #1: “Paper A” (ready)."
        );
        assert_eq!(c("update-paper", json!({ "paper_id": 1, "keywords": "x; y", "stage": "bogus" })), "Updated “Paper A”: keywords.");
        assert_eq!(c("update-paper", json!({ "paper_id": 1 })), "ERR Nothing to change.");
        assert_eq!(
            c("record-submission", json!({ "paper_id": 1, "venue": "Journal X", "manuscript_id": "JX-1", "submitted_on": "2026-11-01" })),
            "Recorded submission #1 of “Paper A” to Journal X."
        );
        let p = server.db.find("papers", 1).unwrap().unwrap();
        assert_eq!(p["stage"], "submitted");
        assert_eq!(
            c(
                "update-submission",
                json!({ "submission_id": 1, "status": "pending", "status_on": "2026-11-05", "note": "Sent to reviewers." })
            ),
            "Updated Journal X for “Paper A”: decision, status_on, decision_on, note."
        );
        assert_eq!(server.db.find("papers", 1).unwrap().unwrap()["stage"], "under_review");
        assert_eq!(c("update-submission", json!({ "submission_id": 1, "status": "nah" })), "ERR Unknown status.");
        assert_eq!(
            c("record-decision", json!({ "submission_id": 1, "decision": "major", "date": "2026-11-20", "revision_due_on": "2027-01-10" })),
            "Journal X: Major revision on 2026-11-20."
        );
        assert_eq!(server.db.find("papers", 1).unwrap().unwrap()["stage"], "revision");
        assert_eq!(c("record-decision", json!({ "submission_id": 99, "decision": "major" })), "ERR No submission with that id.");
        assert_eq!(
            c("add-deadline", json!({ "title": "Resubmit", "due_on": "2027-01-05", "paper_id": 1 })),
            "Added “Resubmit” on 2027-01-05."
        );
        let dl: Value = serde_json::from_str(&c("list-deadlines", json!({}))).unwrap();
        assert_eq!(dl[0]["title"], "Resubmit");
        assert_eq!(dl[0]["paper"], "A");
        assert_eq!(dl[1]["title"], "Revision due: Journal X");
        let paper: Value = serde_json::from_str(&c("get-paper", json!({ "paper_id": "1" }))).unwrap();
        assert_eq!(paper["submissions"][0]["note"], "Sent to reviewers.");
        assert_eq!(paper["submissions"][0]["current"], json!(true));
        let venues: Value = serde_json::from_str(&c("search-venues", json!({ "search": "journal" }))).unwrap();
        assert_eq!(venues[0]["name"], "Journal X");
        assert!(venues[0].get("field").is_none());

        let src = tmp.join("Draft v2.PDF");
        std::fs::write(&src, b"%PDF").unwrap();
        assert_eq!(
            c("attach-file", json!({ "paper_id": 1, "path": src.to_string_lossy(), "kind": "manuscript", "submission_id": 1 })),
            "Attached Draft v2.PDF (1 KB) to “Paper A” as Manuscript."
        );
        let f = server.db.first("SELECT * FROM paper_files", []).unwrap().unwrap();
        assert!(f["path"].as_str().unwrap().ends_with(".pdf"));
        assert!(tmp.join("files").join(f["path"].as_str().unwrap()).is_file());
        assert_eq!(c("attach-file", json!({ "paper_id": 1, "path": "/nope/x.pdf" })), "ERR File not found: /nope/x.pdf");
        assert_eq!(c("attach-file", json!({ "paper_id": 1, "path": src.to_string_lossy(), "kind": "x" })), "ERR Unknown kind.");
        let pubs: Value = serde_json::from_str(&c("list-publications", json!({}))).unwrap();
        assert!(pubs["summary"].is_null());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn no_user() {
        let server = Server::new(crate::db::Db::open_in_memory().unwrap(), std::path::Path::new("/tmp"));
        assert!(txt(call(&server, "list-papers", &json!({}))).starts_with("ERR No account found."));
    }
}
