//! `papers.*` handlers: ports of livewire/papers/index.blade.php and livewire/papers/detail.blade.php
//! (plus `papers.create` from livewire/new-paper.blade.php).
//!
//! | Action | Args → result |
//! | --- | --- |
//! | `papers.list` | `{}` → `{ papers: [...] }` (every paper with its current submission, review clock, trail and next venue) |
//! | `papers.get` | `{ id }` → the detail payload (see [`detail`]) |
//! | `papers.create` | `{ title, short_title?, stage?, method? }` → the paper row |
//! | `papers.update` | `{ id, title, short_title, stage, started_on, abstract, method, dataset, keywords, overleaf_url, drive_url, preprint_server, preprint_url, preprint_on, notes, tags }` → detail |
//! | `papers.setStage` | `{ id, stage }` → detail |
//! | `papers.duplicate` | `{ id }` → the new paper row (fields, tags and authors copied) |
//! | `papers.delete` | `{ id }` → `true` (rows cascade; stored files removed) |
//! | `papers.saveSubmission` | `{ paper_id, id?, venue_id?, venue_name, manuscript_id?, portal_url?, submitted_on?, status_on?, decision, decision_on?, revision_due_on?, note?, is_current }` → detail |
//! | `papers.makeCurrent` | `{ id }` (submission) → detail |
//! | `papers.setDecision` | `{ id, decision }` → detail |
//! | `papers.deleteSubmission` | `{ id }` → detail |
//! | `papers.submitNext` | `{ paper_id }` → `{ detail, submission }` (`submission` null when the queue is empty) |
//! | `papers.addToQueue` | `{ paper_id, venue_id }` → `{ detail, venue }` |
//! | `papers.removeQueue` | `{ id }` → detail |
//! | `papers.reorder` | `{ kind: queue \| authors \| items, parent_id, ids: [...] }` → detail (parent = paper, or submission for items) |
//! | `papers.importComments` | `{ submission_id, comments }` → `{ detail, added }` |
//! | `papers.toggleItem` / `papers.deleteItem` | `{ id }` → detail |
//! | `papers.saveResponse` | `{ id, response }` → `true` |
//! | `papers.saveAuthor` | `{ paper_id, id?, name, affiliation?, email?, orcid?, corresponding, roles: [] }` → detail |
//! | `papers.deleteAuthor` | `{ id }` → detail |

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{author, deadline, keys, label, paper, submission};
use crate::util::{parse_date, today, today_str};
use crate::validation::{as_integer, ensure, validate, Validator};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "papers.list" => list(ctx),
        "papers.get" => arg_id(args, "id").and_then(|id| detail(ctx, id)),
        "papers.create" => create(args, ctx),
        "papers.update" => update(args, ctx),
        "papers.setStage" => set_stage(args, ctx),
        "papers.duplicate" => duplicate(args, ctx),
        "papers.delete" => delete(args, ctx),
        "papers.saveSubmission" => save_submission(args, ctx),
        "papers.makeCurrent" => make_current(args, ctx),
        "papers.setDecision" => set_decision(args, ctx),
        "papers.deleteSubmission" => delete_submission(args, ctx),
        "papers.submitNext" => submit_next(args, ctx),
        "papers.addToQueue" => add_to_queue(args, ctx),
        "papers.removeQueue" => remove_queue(args, ctx),
        "papers.reorder" => reorder(args, ctx),
        "papers.importComments" => import_comments(args, ctx),
        "papers.toggleItem" => toggle_item(args, ctx),
        "papers.saveResponse" => save_response(args, ctx),
        "papers.deleteItem" => delete_item(args, ctx),
        "papers.saveAuthor" => save_author(args, ctx),
        "papers.deleteAuthor" => delete_author(args, ctx),
        _ => return None,
    })
}

/* ---------- small helpers ---------- */

fn arg_id(args: &Value, key: &str) -> Result<i64> {
    as_integer(args.get(key).unwrap_or(&Value::Null)).ok_or_else(ApiError::not_found)
}

fn s<'a>(row: &'a Value, k: &str) -> &'a str {
    row.get(k).and_then(Value::as_str).unwrap_or("")
}

fn i(row: &Value, k: &str) -> Option<i64> {
    row.get(k).and_then(Value::as_i64)
}

fn opt(v: Option<String>) -> Value {
    v.map(Value::String).unwrap_or(Value::Null)
}

/// The local user's paper, or "Not found.".
fn own_paper(conn: &Connection, id: i64) -> Result<Value> {
    conn.first("SELECT * FROM papers WHERE id = ?1 AND user_id = ?2", [id, USER_ID])?.ok_or_else(ApiError::not_found)
}

/// A submission of one of the local user's papers.
fn own_submission(conn: &Connection, id: i64) -> Result<Value> {
    conn.first("SELECT s.* FROM submissions s JOIN papers p ON p.id = s.paper_id WHERE s.id = ?1 AND p.user_id = ?2", [id, USER_ID])?
        .ok_or_else(ApiError::not_found)
}

/// Paper id of a child row (`queue_items`, `authors`) owned by the local user.
fn owner_of(conn: &Connection, table: &str, id: i64) -> Result<i64> {
    let sql = match table {
        "queue_items" => "SELECT q.paper_id FROM queue_items q JOIN papers p ON p.id = q.paper_id WHERE q.id = ?1 AND p.user_id = ?2",
        "authors" => "SELECT a.paper_id FROM authors a JOIN papers p ON p.id = a.paper_id WHERE a.id = ?1 AND p.user_id = ?2",
        "revision_items" => {
            "SELECT s.paper_id FROM revision_items r JOIN submissions s ON s.id = r.submission_id JOIN papers p ON p.id = s.paper_id
             WHERE r.id = ?1 AND p.user_id = ?2"
        }
        _ => return Err(ApiError::not_found()),
    };
    conn.scalar::<i64, _>(sql, [id, USER_ID])?.ok_or_else(ApiError::not_found)
}

/// Comparable key for the "current submission" rule: is_current, then submitted_on (NULL lowest), then id.
fn current_key(s: &Value) -> (bool, String, i64) {
    (
        s.get("is_current").and_then(Value::as_bool).unwrap_or(false),
        s.get("submitted_on").and_then(Value::as_str).unwrap_or("").to_string(),
        i(s, "id").unwrap_or(0),
    )
}

fn pick_current<'a>(subs: impl Iterator<Item = &'a Value>) -> Option<&'a Value> {
    subs.max_by_key(|s| current_key(s))
}

/// Add `days` (days in review) and `clock` to a submission row; `decision_days` is the venue's typical time.
fn with_clock(mut s: Value, decision_days: Option<i64>) -> Value {
    let today = today();
    let days = submission::days_in_review(&s, today);
    let clock = submission::clock(&s, decision_days, today);
    if let Some(o) = s.as_object_mut() {
        o.insert("days".into(), json!(days));
        o.insert("clock".into(), clock.unwrap_or(Value::Null));
    }
    s
}

/// Tags typed as "a, b, c": trimmed, unique (case-insensitive), at most 12; `None` when empty.
fn parse_tags(raw: &str) -> Option<Vec<String>> {
    let mut seen = Vec::<String>::new();
    let mut out = Vec::new();
    for t in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let low = t.to_lowercase();
        if !seen.contains(&low) {
            seen.push(low);
            out.push(t.to_string());
        }
    }
    out.truncate(12);
    (!out.is_empty()).then_some(out)
}

/// Same stage as the current submission's status (Livewire `syncStage`), if `sub_id` is the current one.
fn sync_stage(conn: &Connection, paper_id: i64, sub_id: i64) -> Result<()> {
    let Some(cur) = paper::current_submission(conn, paper_id)? else {
        return Ok(());
    };
    if i(&cur, "id") != Some(sub_id) {
        return Ok(());
    }
    let stage = submission::stage_for(s(&cur, "decision"));
    let current_stage: Option<String> = conn.scalar("SELECT stage FROM papers WHERE id = ?1", [paper_id])?;
    if current_stage.as_deref() != Some("published") || stage != "accepted" {
        conn.update("papers", paper_id, &json!({ "stage": stage }))?;
    }
    Ok(())
}

/* ---------- list ---------- */

fn list(ctx: &Ctx) -> Result<Value> {
    ctx.db.with(|c| {
        let papers = c.rows(
            "SELECT id, title, short_title, stage, started_on, tags, keywords, method, created_at, updated_at
             FROM papers WHERE user_id = ?1 ORDER BY started_on DESC, id DESC",
            [USER_ID],
        )?;
        let subs = c.rows(
            "SELECT s.id, s.paper_id, s.venue_id, s.venue_name, s.manuscript_id, s.submitted_on, s.status_on, s.decision,
                    s.decision_on, s.is_current, v.decision_days AS venue_decision_days
             FROM submissions s JOIN papers p ON p.id = s.paper_id LEFT JOIN venues v ON v.id = s.venue_id
             WHERE p.user_id = ?1 ORDER BY s.submitted_on, s.id",
            [USER_ID],
        )?;
        let queue = c.rows(
            "SELECT q.paper_id, v.name FROM queue_items q JOIN papers p ON p.id = q.paper_id JOIN venues v ON v.id = q.venue_id
             WHERE p.user_id = ?1 ORDER BY q.position, q.id",
            [USER_ID],
        )?;
        let mut by_paper: HashMap<i64, Vec<Value>> = HashMap::new();
        for sub in subs {
            by_paper.entry(i(&sub, "paper_id").unwrap_or(0)).or_default().push(sub);
        }
        let mut next: HashMap<i64, String> = HashMap::new();
        for q in queue {
            next.entry(i(&q, "paper_id").unwrap_or(0)).or_insert_with(|| s(&q, "name").to_string());
        }
        let out: Vec<Value> = papers
            .into_iter()
            .map(|mut p| {
                let id = i(&p, "id").unwrap_or(0);
                // Trail: submissions in date order (undated first), as in the web list.
                let trail = by_paper.remove(&id).unwrap_or_default();
                let current = pick_current(trail.iter()).map(|c| {
                    let typical = i(c, "venue_decision_days");
                    with_clock(c.clone(), typical)
                });
                if let Some(o) = p.as_object_mut() {
                    o.insert("display_title".into(), json!(paper::display_title(&Value::Object(o.clone()))));
                    o.insert("current".into(), current.unwrap_or(Value::Null));
                    o.insert("trail".into(), Value::Array(trail));
                    o.insert("next_venue".into(), opt(next.remove(&id)));
                }
                p
            })
            .collect();
        Ok(json!({ "papers": out }))
    })
}

/* ---------- detail ---------- */

/// Everything the detail view shows, in a handful of queries under one lock.
pub fn detail(ctx: &Ctx, id: i64) -> Result<Value> {
    let files = crate::files::list_for(&ctx.db, id)?;
    ctx.db.with(|c| {
        let mut paper = own_paper(c, id)?;
        let mut subs = c.rows(
            "SELECT s.*, v.decision_days AS venue_decision_days, v.review_type AS venue_review_type, v.name AS venue_library_name
             FROM submissions s LEFT JOIN venues v ON v.id = s.venue_id
             WHERE s.paper_id = ?1 ORDER BY s.submitted_on DESC, s.id DESC",
            [id],
        )?;
        let items = c.rows(
            "SELECT r.* FROM revision_items r JOIN submissions s ON s.id = r.submission_id
             WHERE s.paper_id = ?1 ORDER BY r.position, r.id",
            [id],
        )?;
        let queue = c.rows(
            "SELECT q.id, q.paper_id, q.venue_id, q.position, v.name AS venue_name, v.portal_url AS venue_portal_url
             FROM queue_items q JOIN venues v ON v.id = q.venue_id WHERE q.paper_id = ?1 ORDER BY q.position, q.id",
            [id],
        )?;
        let authors = c.rows("SELECT * FROM authors WHERE paper_id = ?1 ORDER BY position, id", [id])?;
        let today = today();
        let deadlines: Vec<Value> = c
            .rows("SELECT * FROM deadlines WHERE paper_id = ?1 AND user_id = ?2 ORDER BY due_on, id", [id, USER_ID])?
            .into_iter()
            .map(|mut d| {
                let left = deadline::days_left(&d, today);
                if let Some(o) = d.as_object_mut() {
                    o.insert("days_left".into(), json!(left));
                }
                d
            })
            .collect();
        let venues = c.rows("SELECT id, name, kind, portal_url FROM venues WHERE user_id = ?1 ORDER BY kind, name", [USER_ID])?;

        let mut items_by: HashMap<i64, Vec<Value>> = HashMap::new();
        for it in items {
            items_by.entry(i(&it, "submission_id").unwrap_or(0)).or_default().push(it);
        }
        let current = pick_current(subs.iter()).cloned();
        let current_id = current.as_ref().and_then(|c| i(c, "id"));
        let risk = current.as_ref().and_then(|cur| {
            let venue = cur.get("venue_review_type").filter(|v| !v.is_null()).map(|rt| json!({ "review_type": rt }));
            paper::anonymity_risk(&paper, Some(cur), venue.as_ref()).map(|r| json!({ "id": r["id"], "venue_name": r["venue_name"] }))
        });
        let needs_next = paper::needs_next_venue(&paper, current.as_ref());
        for sub in subs.iter_mut() {
            let sid = i(sub, "id").unwrap_or(0);
            let typical = i(sub, "venue_decision_days");
            let decorated = with_clock(std::mem::take(sub), typical);
            *sub = decorated;
            if let Some(o) = sub.as_object_mut() {
                o.insert("revision_items".into(), Value::Array(items_by.remove(&sid).unwrap_or_default()));
                o.insert("is_current_journal".into(), json!(Some(sid) == current_id));
            }
        }

        // CRediT statement: "Role: A, B." for every role someone has, in CRediT order.
        let credit = author::CREDIT
            .iter()
            .filter_map(|role| {
                let who: Vec<&str> = authors
                    .iter()
                    .filter(|a| a.get("roles").and_then(Value::as_array).is_some_and(|r| r.iter().any(|x| x.as_str() == Some(role))))
                    .map(|a| s(a, "name"))
                    .collect();
                (!who.is_empty()).then(|| format!("{role}: {}.", who.join(", ")))
            })
            .collect::<Vec<_>>()
            .join(" ");

        if let Some(o) = paper.as_object_mut() {
            let dt = paper::display_title(&Value::Object(o.clone()));
            o.insert("display_title".into(), json!(dt));
        }
        Ok(json!({
            "paper": paper,
            "current_id": current_id,
            "submissions": subs,
            "queue": queue,
            "authors": authors,
            "files": files,
            "deadlines": deadlines,
            "venues": venues,
            "risk": risk,
            "needs_next_venue": needs_next,
            "credit": credit,
        }))
    })
}

/* ---------- paper ---------- */

fn create(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = validate(
        args,
        &[
            ("title", "required|string|max:255"),
            ("short_title", "nullable|string|max:80"),
            ("stage", &format!("required|in:{}", keys(paper::STAGES))),
            ("method", "nullable|string|max:255"),
        ],
    )?;
    let id = ctx.db.with(|c| {
        paper::insert(
            c,
            &json!({ "title": v.string("title"), "short_title": opt(v.opt("short_title")), "stage": v.string("stage"), "method": opt(v.opt("method")) }),
        )
    })?;
    ctx.db.find("papers", id)?.ok_or_else(ApiError::not_found)
}

const PAPER_RULES: &[(&str, &str)] = &[
    ("title", "required|string|max:255"),
    ("short_title", "nullable|string|max:80"),
    ("stage", "required|in:idea,drafting,ready,submitted,under_review,revision,accepted,published,shelved"),
    ("abstract", "nullable|string|max:10000"),
    ("method", "nullable|string|max:255"),
    ("dataset", "nullable|string|max:255"),
    ("keywords", "nullable|string|max:255"),
    ("overleaf_url", "nullable|url|max:255"),
    ("drive_url", "nullable|url|max:255"),
    ("preprint_server", "nullable|string|max:255"),
    ("preprint_url", "nullable|url|max:255"),
    ("preprint_on", "nullable|date"),
    ("started_on", "nullable|date"),
    ("notes", "nullable|string|max:10000"),
    ("tags", "nullable|string|max:500"),
];

fn update(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let v = validate(args, PAPER_RULES)?;
    ctx.db.with(|c| own_paper(c, id).map(|_| ()))?;
    let mut data = Map::new();
    for (field, _) in PAPER_RULES {
        if *field == "tags" {
            data.insert("tags".into(), json!(parse_tags(&v.string("tags"))));
        } else if matches!(*field, "started_on" | "preprint_on") {
            // Dates are stored as YYYY-MM-DD.
            data.insert((*field).into(), opt(v.opt(field).and_then(|d| parse_date(&d).map(|x| x.to_string()).or(Some(d)))));
        } else {
            data.insert((*field).into(), opt(v.opt(field)));
        }
    }
    ctx.db.update("papers", id, &Value::Object(data))?;
    detail(ctx, id)
}

fn set_stage(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let v = validate(args, &[("stage", &format!("required|in:{}", keys(paper::STAGES)))])?;
    ctx.db.with(|c| own_paper(c, id).map(|_| ()))?;
    ctx.db.update("papers", id, &json!({ "stage": v.string("stage") }))?;
    detail(ctx, id)
}

fn duplicate(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let new_id = ctx.db.tx(|tx| {
        let p = own_paper(tx, id)?;
        let mut data = p.as_object().cloned().unwrap_or_default();
        for k in ["id", "user_id", "created_at", "updated_at", "started_on"] {
            data.remove(k);
        }
        let title: String = format!("{} (copy)", s(&p, "title")).chars().take(255).collect();
        data.insert("title".into(), json!(title));
        let new_id = paper::insert(tx, &Value::Object(data))?;
        for mut a in tx.rows("SELECT * FROM authors WHERE paper_id = ?1 ORDER BY position, id", [id])? {
            if let Some(o) = a.as_object_mut() {
                for k in ["id", "created_at", "updated_at"] {
                    o.remove(k);
                }
                o.insert("paper_id".into(), json!(new_id));
            }
            tx.insert("authors", &a)?;
        }
        Ok(new_id)
    })?;
    ctx.db.find("papers", new_id)?.ok_or_else(ApiError::not_found)
}

fn delete(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    ctx.db.with(|c| own_paper(c, id).map(|_| ()))?;
    ctx.db.delete("papers", id)?;
    // Stored files go with the paper (rows cascade). No app in unit tests: nothing stored to remove.
    if ctx.app().is_ok() {
        crate::files::delete_paper_dir(ctx, id)?;
    }
    Ok(json!(true))
}

/* ---------- submissions ---------- */

fn save_submission(args: &Value, ctx: &Ctx) -> Result<Value> {
    let paper_id = arg_id(args, "paper_id")?;
    let sub_id = as_integer(args.get("id").unwrap_or(&Value::Null));
    let v = validate(
        args,
        &[
            ("venue_id", "nullable"),
            ("venue_name", "required|string|max:255"),
            ("manuscript_id", "nullable|string|max:255"),
            ("portal_url", "nullable|url|max:255"),
            ("submitted_on", "nullable|date"),
            ("status_on", "nullable|date"),
            ("decision", &format!("required|in:{}", keys(submission::DECISIONS))),
            ("decision_on", "nullable|date"),
            ("revision_due_on", "nullable|date"),
            ("note", "nullable|string|max:5000"),
            ("is_current", "boolean"),
        ],
    )?;
    let decision = v.string("decision");
    let mut decision_on = v.opt("decision_on");
    if !submission::is_waiting_decision(&decision) && decision_on.is_none() {
        decision_on = Some(today_str());
    }
    let is_current = v.bool("is_current");
    let venue_name = v.string("venue_name");
    ctx.db.tx(|tx| {
        own_paper(tx, paper_id)?;
        if let Some(sid) = sub_id {
            let sub = own_submission(tx, sid)?;
            if i(&sub, "paper_id") != Some(paper_id) {
                return Err(ApiError::not_found());
            }
        }
        // A venue from the library, or one created from the typed name (firstOrCreate).
        let venue_id: Option<i64> = match v.i64("venue_id") {
            Some(vid) => tx.scalar("SELECT id FROM venues WHERE id = ?1 AND user_id = ?2", [vid, USER_ID])?,
            None => match tx.scalar::<i64, _>(
                "SELECT id FROM venues WHERE user_id = ?1 AND name = ?2 COLLATE NOCASE ORDER BY id LIMIT 1",
                rusqlite::params![USER_ID, venue_name],
            )? {
                Some(x) => Some(x),
                None => Some(tx.insert("venues", &json!({ "user_id": USER_ID, "name": venue_name }))?),
            },
        };
        if is_current {
            tx.update_where("submissions", &json!({ "is_current": false }), "paper_id = ?", &[json!(paper_id)])?;
        }
        let data = json!({
            "venue_id": venue_id,
            "venue_name": venue_name,
            "manuscript_id": opt(v.opt("manuscript_id")),
            "portal_url": opt(v.opt("portal_url")),
            "submitted_on": opt(v.opt("submitted_on")),
            "status_on": opt(v.opt("status_on")),
            "decision": decision,
            "decision_on": opt(decision_on.clone()),
            "revision_due_on": opt(v.opt("revision_due_on")),
            "note": opt(v.opt("note")),
            "is_current": is_current,
        });
        let sid = match sub_id {
            Some(sid) => {
                tx.update("submissions", sid, &data)?;
                sid
            }
            None => {
                let mut d = data;
                d["paper_id"] = json!(paper_id);
                tx.insert("submissions", &d)?
            }
        };
        if let Some(vid) = venue_id {
            tx.exec("DELETE FROM queue_items WHERE paper_id = ?1 AND venue_id = ?2", [paper_id, vid])?;
        }
        sync_stage(tx, paper_id, sid)
    })?;
    detail(ctx, paper_id)
}

fn make_current(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.tx(|tx| {
        let sub = own_submission(tx, id)?;
        let paper_id = i(&sub, "paper_id").unwrap_or(0);
        tx.update_where("submissions", &json!({ "is_current": false }), "paper_id = ?", &[json!(paper_id)])?;
        tx.update("submissions", id, &json!({ "is_current": true }))?;
        sync_stage(tx, paper_id, id)?;
        Ok(paper_id)
    })?;
    detail(ctx, paper_id)
}

fn set_decision(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let v = validate(args, &[("decision", &format!("required|in:{}", keys(submission::DECISIONS)))])?;
    let decision = v.string("decision");
    let paper_id = ctx.db.tx(|tx| {
        let sub = own_submission(tx, id)?;
        let paper_id = i(&sub, "paper_id").unwrap_or(0);
        let decision_on = if submission::is_waiting_decision(&decision) {
            Value::Null
        } else {
            sub.get("decision_on").filter(|d| !d.is_null()).cloned().unwrap_or_else(|| json!(today_str()))
        };
        tx.update("submissions", id, &json!({ "decision": decision, "status_on": today_str(), "decision_on": decision_on }))?;
        sync_stage(tx, paper_id, id)?;
        Ok(paper_id)
    })?;
    detail(ctx, paper_id)
}

fn delete_submission(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.with(|c| own_submission(c, id).map(|s| i(&s, "paper_id").unwrap_or(0)))?;
    ctx.db.delete("submissions", id)?;
    detail(ctx, paper_id)
}

fn submit_next(args: &Value, ctx: &Ctx) -> Result<Value> {
    let paper_id = arg_id(args, "paper_id")?;
    ctx.db.with(|c| own_paper(c, paper_id).map(|_| ()))?;
    let sub = paper::submit_to_next(&ctx.db, paper_id)?;
    Ok(json!({ "detail": detail(ctx, paper_id)?, "submission": sub }))
}

/* ---------- queue ---------- */

fn add_to_queue(args: &Value, ctx: &Ctx) -> Result<Value> {
    let paper_id = arg_id(args, "paper_id")?;
    let venue_id = as_integer(args.get("venue_id").unwrap_or(&Value::Null));
    let venue = ctx.db.tx(|tx| {
        own_paper(tx, paper_id)?;
        let Some(vid) = venue_id else {
            return Ok(Value::Null);
        };
        let Some(venue) = tx.first("SELECT id, name FROM venues WHERE id = ?1 AND user_id = ?2", [vid, USER_ID])? else {
            return Ok(Value::Null);
        };
        let exists = tx.count("SELECT COUNT(*) FROM queue_items WHERE paper_id = ?1 AND venue_id = ?2", [paper_id, vid])? > 0;
        if !exists {
            let max: Option<i64> = tx.scalar("SELECT MAX(position) FROM queue_items WHERE paper_id = ?1", [paper_id])?;
            tx.insert("queue_items", &json!({ "paper_id": paper_id, "venue_id": vid, "position": max.unwrap_or(0) + 1 }))?;
        }
        Ok(venue)
    })?;
    Ok(json!({ "detail": detail(ctx, paper_id)?, "venue": venue }))
}

fn remove_queue(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.with(|c| owner_of(c, "queue_items", id))?;
    ctx.db.delete("queue_items", id)?;
    detail(ctx, paper_id)
}

/// Save a new order for a paper's queue or authors, or a submission's reviewer comments.
/// Ids that do not belong to the parent are ignored; rows not listed keep their place after the listed ones.
fn reorder(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = validate(
        args,
        &[("kind", "required|in:queue,authors,items"), ("parent_id", "required|integer"), ("ids", "required|array"), ("ids.*", "integer")],
    )?;
    let parent = v.i64("parent_id").unwrap_or(0);
    let ids: Vec<i64> = v.array("ids").iter().filter_map(as_integer).collect();
    let (table, parent_col) = match v.string("kind").as_str() {
        "queue" => ("queue_items", "paper_id"),
        "authors" => ("authors", "paper_id"),
        _ => ("revision_items", "submission_id"),
    };
    let paper_id = ctx.db.tx(|tx| {
        let paper_id = if table == "revision_items" {
            i(&own_submission(tx, parent)?, "paper_id").unwrap_or(0)
        } else {
            i(&own_paper(tx, parent)?, "id").unwrap_or(0)
        };
        let existing: Vec<i64> = tx
            .rows(&format!("SELECT id FROM {table} WHERE {parent_col} = ?1 ORDER BY position, id"), [parent])?
            .iter()
            .filter_map(|r| i(r, "id"))
            .collect();
        let mut order: Vec<i64> = ids.iter().copied().filter(|x| existing.contains(x)).collect();
        order.dedup();
        for x in &existing {
            if !order.contains(x) {
                order.push(*x);
            }
        }
        for (pos, rid) in order.iter().enumerate() {
            tx.exec(&format!("UPDATE {table} SET position = ?1 WHERE id = ?2"), [pos as i64, *rid])?;
        }
        Ok(paper_id)
    })?;
    detail(ctx, paper_id)
}

/* ---------- revision checklist ---------- */

/// Does the line start a new numbered/bulleted point (`1.`, `2)`, `-`, `•`, `*`, `R1`, `Reviewer 2`, `Comment 3`)?
fn starts_point(line: &str) -> bool {
    let t = line.trim_start();
    let low = t.to_lowercase();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && matches!(t[digits..].chars().next(), Some('.' | ')')) {
        return true;
    }
    if t.starts_with(['-', '•', '*']) {
        return true;
    }
    let after = |prefix: &str| -> bool {
        low.strip_prefix(prefix).is_some_and(|rest| rest.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()))
    };
    (low.starts_with('r') && low[1..].chars().next().is_some_and(|c| c.is_ascii_digit())) || after("reviewer") || after("comment")
}

/// Strip a leading `1.` / `2)` / `-` / `•` / `*` marker.
fn strip_marker(part: &str) -> &str {
    let t = part.trim_start();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && matches!(t[digits..].chars().next(), Some('.' | ')')) {
        return t[digits + 1..].trim_start();
    }
    if let Some(c) = t.chars().next().filter(|c| matches!(c, '-' | '•' | '*')) {
        return t[c.len_utf8()..].trim_start();
    }
    t
}

/// Split pasted reviewer comments: each paragraph or numbered/bulleted point becomes one item.
pub fn split_comments(text: &str) -> Vec<String> {
    let text = text.replace('\r', "");
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (n, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if n > 0 && starts_point(line) && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts.iter().map(|p| strip_marker(p.trim()).trim().to_string()).filter(|p| p.chars().count() > 3).collect()
}

fn import_comments(args: &Value, ctx: &Ctx) -> Result<Value> {
    let sub_id = arg_id(args, "submission_id")?;
    let text = args.get("comments").and_then(Value::as_str).unwrap_or("").to_string();
    let (paper_id, added) = ctx.db.tx(|tx| {
        let sub = own_submission(tx, sub_id)?;
        let start: i64 = tx.scalar::<i64, _>("SELECT MAX(position) FROM revision_items WHERE submission_id = ?1", [sub_id])?.unwrap_or(0);
        let parts = split_comments(&text);
        for (n, part) in parts.iter().enumerate() {
            tx.insert("revision_items", &json!({ "submission_id": sub_id, "comment": part, "position": start + n as i64 + 1 }))?;
        }
        Ok((i(&sub, "paper_id").unwrap_or(0), parts.len()))
    })?;
    Ok(json!({ "detail": detail(ctx, paper_id)?, "added": added }))
}

fn toggle_item(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.with(|c| owner_of(c, "revision_items", id))?;
    ctx.db
        .exec("UPDATE revision_items SET done = NOT done, updated_at = ?1 WHERE id = ?2", rusqlite::params![crate::util::now_ts(), id])?;
    detail(ctx, paper_id)
}

fn save_response(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    ctx.db.with(|c| owner_of(c, "revision_items", id))?;
    let response: String = args.get("response").and_then(Value::as_str).unwrap_or("").chars().take(5000).collect();
    ctx.db.update("revision_items", id, &json!({ "response": response }))?;
    Ok(json!(true))
}

fn delete_item(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.with(|c| owner_of(c, "revision_items", id))?;
    ctx.db.delete("revision_items", id)?;
    detail(ctx, paper_id)
}

/* ---------- authors ---------- */

fn save_author(args: &Value, ctx: &Ctx) -> Result<Value> {
    let paper_id = arg_id(args, "paper_id")?;
    let author_id = as_integer(args.get("id").unwrap_or(&Value::Null));
    let roles_rule = format!("in:{}", author::CREDIT.join(","));
    let v = Validator::new(args)
        .rule("name", "required|string|max:255")
        .rule("affiliation", "nullable|string|max:255")
        .rule("email", "nullable|email|max:255")
        .rule("orcid", "nullable|string")
        .rule("corresponding", "boolean")
        .rule("roles", "array")
        .rule("roles.*", &roles_rule)
        .validate()?;
    if let Some(orcid) = v.opt("orcid") {
        let ok = regex::Regex::new(r"^\d{4}-\d{4}-\d{4}-\d{3}[\dX]$").map(|re| re.is_match(&orcid)).unwrap_or(false);
        ensure(ok, "orcid", "The orcid field format is invalid.")?;
    }
    let roles: Vec<Value> = v.array("roles");
    let corresponding = v.bool("corresponding");
    ctx.db.tx(|tx| {
        own_paper(tx, paper_id)?;
        if let Some(aid) = author_id {
            if owner_of(tx, "authors", aid)? != paper_id {
                return Err(ApiError::not_found());
            }
        }
        if corresponding {
            tx.update_where("authors", &json!({ "corresponding": false }), "paper_id = ?", &[json!(paper_id)])?;
        }
        let data = json!({
            "name": v.string("name"),
            "affiliation": opt(v.opt("affiliation")),
            "email": opt(v.opt("email")),
            "orcid": opt(v.opt("orcid")),
            "corresponding": corresponding,
            "roles": roles,
        });
        match author_id {
            Some(aid) => {
                tx.update("authors", aid, &data)?;
            }
            None => {
                let max: Option<i64> = tx.scalar("SELECT MAX(position) FROM authors WHERE paper_id = ?1", [paper_id])?;
                let mut d = data;
                d["paper_id"] = json!(paper_id);
                d["position"] = json!(max.unwrap_or(0) + 1);
                tx.insert("authors", &d)?;
            }
        }
        Ok(())
    })?;
    detail(ctx, paper_id)
}

fn delete_author(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = arg_id(args, "id")?;
    let paper_id = ctx.db.with(|c| owner_of(c, "authors", id))?;
    ctx.db.delete("authors", id)?;
    detail(ctx, paper_id)
}

/// Label of a stage key (for messages built on the backend).
pub fn stage_label(stage: &str) -> &str {
    label(paper::STAGES, stage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    fn ctx() -> Ctx {
        Ctx::for_tests(test_db())
    }

    fn call(ctx: &Ctx, name: &str, args: Value) -> Result<Value> {
        crate::api::dispatch(name, &args, ctx)
    }

    #[test]
    fn create_validates_and_defaults() {
        let ctx = ctx();
        let err = serde_json::to_value(call(&ctx, "papers.create", json!({ "title": "", "stage": "drafting" })).unwrap_err()).unwrap();
        assert_eq!(err["kind"], "validation");
        assert_eq!(err["errors"]["title"][0], "The title field is required.");
        let err = serde_json::to_value(call(&ctx, "papers.create", json!({ "title": "T", "stage": "nope" })).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["stage"][0], "The selected stage is invalid.");

        let p =
            call(&ctx, "papers.create", json!({ "title": " Trust in AI ", "short_title": "", "stage": "drafting", "method": "Survey" }))
                .unwrap();
        assert_eq!(p["title"], "Trust in AI");
        assert_eq!(p["short_title"], Value::Null);
        assert_eq!(p["started_on"], today_str());
        assert_eq!(p["user_id"], 1);

        let list = call(&ctx, "papers.list", json!({})).unwrap();
        assert_eq!(list["papers"].as_array().unwrap().len(), 1);
        assert_eq!(list["papers"][0]["display_title"], "Trust in AI");
        assert_eq!(list["papers"][0]["current"], Value::Null);

        let id = p["id"].as_i64().unwrap();
        let d = call(
            &ctx,
            "papers.update",
            json!({ "id": id, "title": "Trust", "stage": "ready", "tags": "Thesis, thesis , Preprint,", "overleaf_url": "not a url" }),
        );
        let err = serde_json::to_value(d.unwrap_err()).unwrap();
        assert_eq!(err["errors"]["overleaf_url"][0], "The overleaf url field must be a valid URL.");
        let d = call(
            &ctx,
            "papers.update",
            json!({ "id": id, "title": "Trust", "stage": "ready", "tags": "Thesis, thesis , Preprint,", "started_on": "2026-01-05" }),
        )
        .unwrap();
        assert_eq!(d["paper"]["tags"], json!(["Thesis", "Preprint"]));
        assert_eq!(d["paper"]["stage"], "ready");
        assert_eq!(d["paper"]["started_on"], "2026-01-05");

        let copy = call(&ctx, "papers.duplicate", json!({ "id": id })).unwrap();
        assert_eq!(copy["title"], "Trust (copy)");
        call(&ctx, "papers.delete", json!({ "id": copy["id"] })).unwrap();
        assert!(call(&ctx, "papers.get", json!({ "id": copy["id"] })).is_err());
    }

    #[test]
    fn submission_status_syncs_stage() {
        let ctx = ctx();
        let id = call(&ctx, "papers.create", json!({ "title": "P", "stage": "ready" })).unwrap()["id"].as_i64().unwrap();
        let v = ctx
            .db
            .insert("venues", &json!({ "user_id": 1, "name": "MIS Quarterly", "decision_days": 90, "portal_url": "https://misq.example" }))
            .unwrap();
        ctx.db.insert("queue_items", &json!({ "paper_id": id, "venue_id": v, "position": 1 })).unwrap();

        // Recording a submission to a queued venue removes it from the queue and sets the stage.
        let d = call(&ctx, "papers.saveSubmission", json!({
            "paper_id": id, "venue_id": v, "venue_name": "MIS Quarterly", "submitted_on": today_str(), "decision": "submitted", "is_current": true,
        }))
        .unwrap();
        assert_eq!(d["paper"]["stage"], "submitted");
        assert_eq!(d["queue"].as_array().unwrap().len(), 0);
        let sid = d["current_id"].as_i64().unwrap();
        assert_eq!(d["submissions"][0]["clock"]["typical"], 90);
        assert_eq!(d["submissions"][0]["days"], 0);

        let d = call(&ctx, "papers.setDecision", json!({ "id": sid, "decision": "pending" })).unwrap();
        assert_eq!(d["paper"]["stage"], "under_review");
        assert_eq!(d["submissions"][0]["status_on"], today_str());
        assert_eq!(d["submissions"][0]["decision_on"], Value::Null);

        let d = call(&ctx, "papers.setDecision", json!({ "id": sid, "decision": "major" })).unwrap();
        assert_eq!(d["paper"]["stage"], "revision");
        assert_eq!(d["submissions"][0]["decision_on"], today_str());

        // A published paper stays published when the decision is "accept".
        call(&ctx, "papers.setStage", json!({ "id": id, "stage": "published" })).unwrap();
        let d = call(&ctx, "papers.setDecision", json!({ "id": sid, "decision": "accept" })).unwrap();
        assert_eq!(d["paper"]["stage"], "published");

        // A typed venue name creates a library venue; a non-current older submission does not change the stage.
        let d = call(
            &ctx,
            "papers.saveSubmission",
            json!({
                "paper_id": id, "venue_name": "Old Journal", "submitted_on": "2020-01-01", "decision": "reject", "is_current": false,
            }),
        )
        .unwrap();
        assert_eq!(d["paper"]["stage"], "published");
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM venues WHERE name = 'Old Journal'", []).unwrap(), 1);
        let old = d["submissions"].as_array().unwrap().iter().find(|s| s["venue_name"] == "Old Journal").unwrap().clone();
        assert_eq!(old["decision_on"], today_str());

        // Making the rejected one current moves the stage back to ready.
        let d = call(&ctx, "papers.makeCurrent", json!({ "id": old["id"] })).unwrap();
        assert_eq!(d["current_id"], old["id"]);
        assert_eq!(d["paper"]["stage"], "ready");
        assert_eq!(d["needs_next_venue"], true);

        let err = serde_json::to_value(
            call(&ctx, "papers.saveSubmission", json!({ "paper_id": id, "venue_name": "", "decision": "x" })).unwrap_err(),
        )
        .unwrap();
        assert_eq!(err["errors"]["venue_name"][0], "The venue name field is required.");
        assert_eq!(err["errors"]["decision"][0], "The selected decision is invalid.");

        let d = call(&ctx, "papers.deleteSubmission", json!({ "id": old["id"] })).unwrap();
        assert_eq!(d["submissions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn submit_to_next_and_queue_reorder() {
        let ctx = ctx();
        let id = call(&ctx, "papers.create", json!({ "title": "P", "stage": "drafting" })).unwrap()["id"].as_i64().unwrap();
        let a = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "A", "portal_url": "https://a.example" })).unwrap();
        let b = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "B" })).unwrap();
        let c = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "C" })).unwrap();
        for v in [a, b, c, a] {
            let r = call(&ctx, "papers.addToQueue", json!({ "paper_id": id, "venue_id": v })).unwrap();
            assert!(!r["venue"].is_null());
        }
        let d = call(&ctx, "papers.get", json!({ "id": id })).unwrap();
        let names: Vec<&str> = d["queue"].as_array().unwrap().iter().map(|q| q["venue_name"].as_str().unwrap()).collect();
        assert_eq!(names, ["A", "B", "C"]);
        let ids: Vec<i64> = d["queue"].as_array().unwrap().iter().map(|q| q["id"].as_i64().unwrap()).collect();

        // New order C, A, B (B left out: it keeps its place after the listed ones).
        let d = call(&ctx, "papers.reorder", json!({ "kind": "queue", "parent_id": id, "ids": [ids[2], ids[0], 9999] })).unwrap();
        let names: Vec<&str> = d["queue"].as_array().unwrap().iter().map(|q| q["venue_name"].as_str().unwrap()).collect();
        assert_eq!(names, ["C", "A", "B"]);

        let r = call(&ctx, "papers.submitNext", json!({ "paper_id": id })).unwrap();
        assert_eq!(r["submission"]["venue_name"], "C");
        assert_eq!(r["detail"]["paper"]["stage"], "submitted");
        assert_eq!(r["detail"]["queue"].as_array().unwrap().len(), 2);
        assert_eq!(r["detail"]["current_id"], r["submission"]["id"]);

        let d = call(&ctx, "papers.removeQueue", json!({ "id": ids[0] })).unwrap();
        assert_eq!(d["queue"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn comments_authors_and_credit() {
        let ctx = ctx();
        let id = call(&ctx, "papers.create", json!({ "title": "P", "stage": "drafting" })).unwrap()["id"].as_i64().unwrap();
        let d = call(&ctx, "papers.saveSubmission", json!({ "paper_id": id, "venue_name": "J", "decision": "major", "is_current": true }))
            .unwrap();
        let sid = d["current_id"].as_i64().unwrap();
        let text = "1. The sampling strategy needs more detail.\n2) Please discuss limits.\n   continued here\n\nReviewer 2\nok\n- Fix typo in table 3\nno";
        let r = call(&ctx, "papers.importComments", json!({ "submission_id": sid, "comments": text })).unwrap();
        assert_eq!(r["added"], 4);
        let items = r["detail"]["submissions"][0]["revision_items"].as_array().unwrap().clone();
        assert_eq!(items[0]["comment"], "The sampling strategy needs more detail.");
        assert_eq!(items[1]["comment"], "Please discuss limits.\n   continued here");
        assert_eq!(items[2]["comment"], "Reviewer 2\nok");
        assert_eq!(items[3]["comment"], "Fix typo in table 3\nno");

        let d = call(&ctx, "papers.toggleItem", json!({ "id": items[0]["id"] })).unwrap();
        assert_eq!(d["submissions"][0]["revision_items"][0]["done"], true);
        call(&ctx, "papers.saveResponse", json!({ "id": items[0]["id"], "response": "Added a section." })).unwrap();
        let d =
            call(&ctx, "papers.reorder", json!({ "kind": "items", "parent_id": sid, "ids": [items[3]["id"], items[0]["id"]] })).unwrap();
        let first = &d["submissions"][0]["revision_items"][0];
        assert_eq!(first["id"], items[3]["id"]);
        let d = call(&ctx, "papers.deleteItem", json!({ "id": items[1]["id"] })).unwrap();
        assert_eq!(d["submissions"][0]["revision_items"].as_array().unwrap().len(), 3);

        let err =
            serde_json::to_value(call(&ctx, "papers.saveAuthor", json!({ "paper_id": id, "name": "A", "orcid": "123" })).unwrap_err())
                .unwrap();
        assert_eq!(err["errors"]["orcid"][0], "The orcid field format is invalid.");
        let err = serde_json::to_value(
            call(&ctx, "papers.saveAuthor", json!({ "paper_id": id, "name": "A", "roles": ["Dancing"] })).unwrap_err(),
        )
        .unwrap();
        assert_eq!(err["errors"]["roles.0"][0], "The selected roles.0 is invalid.");
        call(&ctx, "papers.saveAuthor", json!({ "paper_id": id, "name": "Ann Lee", "orcid": "0000-0002-1825-009X", "corresponding": true, "roles": ["Conceptualization", "Software"] })).unwrap();
        let d = call(&ctx, "papers.saveAuthor", json!({ "paper_id": id, "name": "Bo Chen", "corresponding": true, "roles": ["Software"] }))
            .unwrap();
        assert_eq!(d["credit"], "Conceptualization: Ann Lee. Software: Ann Lee, Bo Chen.");
        assert_eq!(d["authors"][0]["corresponding"], false);
        assert_eq!(d["authors"][1]["corresponding"], true);
        let ids: Vec<Value> = d["authors"].as_array().unwrap().iter().map(|a| a["id"].clone()).rev().collect();
        let d = call(&ctx, "papers.reorder", json!({ "kind": "authors", "parent_id": id, "ids": ids })).unwrap();
        assert_eq!(d["authors"][0]["name"], "Bo Chen");
        let d = call(&ctx, "papers.deleteAuthor", json!({ "id": d["authors"][0]["id"] })).unwrap();
        assert_eq!(d["authors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn anonymity_risk_in_detail() {
        let ctx = ctx();
        let id = call(&ctx, "papers.create", json!({ "title": "P", "stage": "drafting" })).unwrap()["id"].as_i64().unwrap();
        let v = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "Blind J", "review_type": "double" })).unwrap();
        call(&ctx, "papers.update", json!({ "id": id, "title": "P", "stage": "drafting", "preprint_url": "https://arxiv.org/abs/1" }))
            .unwrap();
        let d = call(
            &ctx,
            "papers.saveSubmission",
            json!({ "paper_id": id, "venue_id": v, "venue_name": "Blind J", "decision": "pending", "is_current": true }),
        )
        .unwrap();
        assert_eq!(d["risk"]["venue_name"], "Blind J");
        assert_eq!(stage_label("revision"), "Revising");
    }
}
