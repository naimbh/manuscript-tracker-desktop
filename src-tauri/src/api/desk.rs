//! `desk.*` handlers: the Desk (port of livewire/desk.blade.php), the New paper modal
//! (livewire/new-paper.blade.php), the dock badge count and the command palette search.

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{deadline, paper, submission, venue};
use crate::util::{parse_date, today, DATE_FORMAT};
use crate::validation::{as_integer, Validator};
use chrono::{Datelike, Local, Months, NaiveDate, NaiveDateTime};
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "desk.data" => data(ctx),
        "desk.attentionCount" => attention_count(ctx),
        "desk.submitNext" => submit_next(args, ctx),
        "desk.createPaper" => create_paper(args, ctx),
        "desk.search" => search(args, ctx),
        "desk.runScans" => run_scans(ctx),
        _ => return None,
    })
}

/// Everything the Desk needs, loaded with a handful of queries under one lock.
struct Loaded {
    papers: Vec<Value>,
    /// Submissions per paper, newest first (submitted_on DESC, id DESC), with `v_*` venue columns.
    subs: HashMap<i64, Vec<Value>>,
    /// Name of the first venue in each paper's queue.
    next_venue: HashMap<i64, String>,
}

fn load(ctx: &Ctx) -> Result<Loaded> {
    ctx.db.with(|c| {
        let papers = c.rows("SELECT id, title, short_title, stage, preprint_url FROM papers WHERE user_id = ?1 ORDER BY id", [USER_ID])?;
        let rows = c.rows(
            "SELECT s.*, v.id AS v_id, v.review_type AS v_review_type, v.decision_days AS v_decision_days
             FROM submissions s JOIN papers p ON p.id = s.paper_id LEFT JOIN venues v ON v.id = s.venue_id
             WHERE p.user_id = ?1 ORDER BY s.paper_id, s.submitted_on DESC, s.id DESC",
            [USER_ID],
        )?;
        let mut subs: HashMap<i64, Vec<Value>> = HashMap::new();
        for r in rows {
            let pid = r["paper_id"].as_i64().unwrap_or(0);
            subs.entry(pid).or_default().push(r);
        }
        let queue = c.rows(
            "SELECT q.paper_id, v.name FROM queue_items q JOIN venues v ON v.id = q.venue_id JOIN papers p ON p.id = q.paper_id
             WHERE p.user_id = ?1 ORDER BY q.position, q.id",
            [USER_ID],
        )?;
        let mut next_venue = HashMap::new();
        for q in queue {
            let pid = q["paper_id"].as_i64().unwrap_or(0);
            next_venue.entry(pid).or_insert_with(|| q["name"].as_str().unwrap_or("").to_string());
        }
        Ok(Loaded { papers, subs, next_venue })
    })
}

/// The current submission among a paper's submissions (is_current, then latest submitted_on, then id).
fn current(subs: &[Value]) -> Option<&Value> {
    subs.iter().max_by(|a, b| {
        let key = |s: &Value| {
            (
                s["is_current"].as_bool().unwrap_or(false),
                // NULL dates sort last (lowest).
                s["submitted_on"].as_str().map(str::to_string),
                s["id"].as_i64().unwrap_or(0),
            )
        };
        key(a).cmp(&key(b))
    })
}

fn venue_of(s: &Value) -> Option<Value> {
    s["v_id"].as_i64().map(|_| json!({ "review_type": s["v_review_type"], "decision_days": s["v_decision_days"] }))
}

fn paper_ref(p: &Value) -> Value {
    json!({ "id": p["id"], "title": paper::display_title(p) })
}

fn attention(l: &Loaded, today: NaiveDate) -> Vec<Value> {
    let empty = Vec::new();
    let mut out: VecDeque<Value> = VecDeque::new();
    for p in &l.papers {
        let pid = p["id"].as_i64().unwrap_or(0);
        let subs = l.subs.get(&pid).unwrap_or(&empty);
        let cur = current(subs);
        let cur_venue = cur.and_then(venue_of);
        if let Some(s) = paper::anonymity_risk(p, cur, cur_venue.as_ref()) {
            out.push_back(json!({
                "tone": "red", "icon": "eye", "paper": paper_ref(p),
                "text": format!("Public preprint while under double-anonymous review at {}.", s["venue_name"].as_str().unwrap_or("")),
            }));
        }
        if let Some(s) = cur {
            let typical = s["v_decision_days"].as_i64();
            if let Some(c) = submission::clock(s, typical, today) {
                if c["overdue"].as_bool() == Some(true) {
                    let over = c["days"].as_i64().unwrap_or(0) - c["typical"].as_i64().unwrap_or(0);
                    out.push_back(json!({
                        "tone": "amber", "icon": "clock", "paper": paper_ref(p),
                        "text": format!("{} is {} days past its usual decision time. Consider a status query.", s["venue_name"].as_str().unwrap_or(""), over),
                    }));
                }
            }
            let decision = s["decision"].as_str().unwrap_or("");
            if let Some(due) = s["revision_due_on"].as_str().and_then(parse_date) {
                // Carbon: $due->diffInDays(now()) <= 14 (signed, fractional): due in the future or under 14 days ago.
                if matches!(decision, "major" | "minor") && (today - due).num_days() < 14 {
                    out.push_back(json!({
                        "tone": "amber", "icon": "pencil-square", "paper": paper_ref(p),
                        "text": null, "revision_due_on": due.format(DATE_FORMAT).to_string(),
                    }));
                }
            }
        }
        let next = l.next_venue.get(&pid);
        if p["stage"].as_str() == Some("ready") {
            out.push_front(json!({
                "tone": "violet", "icon": "paper-airplane", "paper": paper_ref(p),
                "text": match next {
                    Some(n) => format!("Ready to submit. Next: {n}."),
                    None => "Ready to submit. Add a journal or conference to its “Up next” list.".to_string(),
                },
                "next": next.is_some(),
            }));
        } else if paper::needs_next_venue(p, cur) {
            out.push_back(json!({
                "tone": "indigo", "icon": "arrow-right", "paper": paper_ref(p),
                "text": match next {
                    Some(n) => format!("Ready for its next journal or conference: {n}."),
                    None => "Rejected last time, and its “Up next” list is empty.".to_string(),
                },
                "next": next.is_some(),
            }));
        }
    }
    out.into()
}

fn days_between(a: NaiveDateTime, b: NaiveDateTime) -> f64 {
    (b - a).num_milliseconds() as f64 / 86_400_000.0
}

fn midnight(d: NaiveDate) -> NaiveDateTime {
    d.and_hms_opt(0, 0, 0).unwrap_or_default()
}

/// The 12-month journey graphic (lanes of submission bars), as in the web Desk.
fn journey(l: &Loaded, now: NaiveDateTime) -> Value {
    let today = now.date();
    let first_of_month = today.with_day(1).unwrap_or(today);
    let start_d = first_of_month.checked_sub_months(Months::new(11)).unwrap_or(first_of_month);
    let start = midnight(start_d);
    let next_month = first_of_month.checked_add_months(Months::new(1)).unwrap_or(first_of_month);
    let end = midnight(next_month) - chrono::Duration::microseconds(1);
    let span = days_between(start, end).max(1.0);
    let empty = Vec::new();
    let mut lanes = Vec::new();
    for p in &l.papers {
        let pid = p["id"].as_i64().unwrap_or(0);
        let subs = l.subs.get(&pid).unwrap_or(&empty);
        let sent: Vec<(&Value, NaiveDate)> =
            subs.iter().filter_map(|s| s["submitted_on"].as_str().and_then(parse_date).map(|d| (s, d))).collect();
        let in_window = sent.iter().any(|(s, _)| {
            let end = s["decision_on"].as_str().and_then(parse_date).map(midnight).unwrap_or(now);
            end >= start
        });
        if !in_window {
            continue;
        }
        let bars: Vec<Value> = sent
            .iter()
            .map(|(s, submitted)| {
                let from = days_between(start, midnight(*submitted)).max(0.0);
                let until = if submission::is_waiting(s) {
                    now
                } else {
                    s["decision_on"].as_str().and_then(parse_date).map(midnight).unwrap_or(now)
                };
                let to = (from + 3.0).max(days_between(start, until));
                let x = from / span * 100.0;
                let w = ((to - from) / span * 100.0).max(1.2);
                let decision = s["decision"].as_str().unwrap_or("");
                json!({
                    "id": s["id"],
                    "x": x,
                    "w": w.min(100.0 - x),
                    "venue_name": s["venue_name"],
                    "decision": decision,
                    "decision_label": submission::decision_label(decision),
                    "tone": submission::tone(decision),
                    "days": submission::days_in_review(s, today),
                })
            })
            .filter(|b| b["x"].as_f64().unwrap_or(0.0) + b["w"].as_f64().unwrap_or(0.0) > 0.0)
            .collect();
        lanes.push(json!({ "paper": paper_ref(p), "bars": bars }));
    }
    let months: Vec<String> =
        (0..12).map(|i| start_d.checked_add_months(Months::new(i)).unwrap_or(start_d).format(DATE_FORMAT).to_string()).collect();
    json!({ "lanes": lanes, "months": months, "today_x": days_between(start, now) / span * 100.0 })
}

fn data(ctx: &Ctx) -> Result<Value> {
    let now = Local::now().naive_local();
    let today = now.date();
    let l = load(ctx)?;
    let all: Vec<&Value> = l.papers.iter().flat_map(|p| l.subs.get(&p["id"].as_i64().unwrap_or(0)).into_iter().flatten()).collect();
    let year = today.year().to_string();
    let waiting = all.iter().filter(|s| submission::is_waiting(s)).count();
    let revising = l.papers.iter().filter(|p| p["stage"] == "revision").count();
    let accepted = l.papers.iter().filter(|p| matches!(p["stage"].as_str(), Some("accepted" | "published"))).count();
    let sent_this_year = all.iter().filter(|s| s["submitted_on"].as_str().is_some_and(|d| d.starts_with(&year))).count();

    let attention = attention(&l, today);

    let titles: HashMap<i64, String> = l.papers.iter().map(|p| (p["id"].as_i64().unwrap_or(0), paper::display_title(p))).collect();
    let mut clocks: Vec<&Value> = all.iter().copied().filter(|s| submission::is_waiting(s)).collect();
    // Stable sort by submitted_on, NULLs first (Laravel's sortBy).
    clocks.sort_by(|a, b| a["submitted_on"].as_str().cmp(&b["submitted_on"].as_str()));
    let clocks: Vec<Value> = clocks
        .into_iter()
        .map(|s| {
            let pid = s["paper_id"].as_i64().unwrap_or(0);
            json!({
                "id": s["id"],
                "paper_id": pid,
                "paper_title": titles.get(&pid).cloned().unwrap_or_default(),
                "venue_name": s["venue_name"],
                "days": submission::days_in_review(s, today),
                "clock": submission::clock(s, s["v_decision_days"].as_i64(), today),
            })
        })
        .collect();

    let since = (today - chrono::Duration::days(7)).format(DATE_FORMAT).to_string();
    let deadlines: Vec<Value> = ctx
        .db
        .rows(
            "SELECT d.id, d.title, d.due_on, d.paper_id, p.title AS paper_title, p.short_title AS paper_short_title
             FROM deadlines d LEFT JOIN papers p ON p.id = d.paper_id
             WHERE d.user_id = ?1 AND d.done = 0 AND d.due_on >= ?2 ORDER BY d.due_on, d.id LIMIT 6",
            params![USER_ID, since],
        )?
        .into_iter()
        .map(|d| decorate_deadline(d, today))
        .collect();

    Ok(json!({
        "empty": l.papers.is_empty(),
        "stats": [
            { "label": "With journals", "value": waiting, "icon": "clock", "grad": "from-sky-500 to-indigo-500" },
            { "label": "Revising", "value": revising, "icon": "pencil-square", "grad": "from-amber-400 to-orange-500" },
            { "label": "Accepted", "value": accepted, "icon": "check-badge", "grad": "from-emerald-400 to-teal-500" },
            { "label": "Sent this year", "value": sent_this_year, "icon": "paper-airplane", "grad": "from-fuchsia-500 to-violet-500" },
        ],
        "attention": attention,
        "clocks": clocks,
        "deadlines": deadlines,
        "journey": journey(&l, now),
    }))
}

/// Adds `paper` (display title or null) and `days_left` to a deadline row joined with its paper's titles.
pub fn decorate_deadline(mut d: Value, today: NaiveDate) -> Value {
    let days_left = deadline::days_left(&d, today);
    if let Some(o) = d.as_object_mut() {
        let title = o.remove("paper_title");
        let short = o.remove("paper_short_title");
        let paper = match title {
            Some(Value::String(t)) => Some(paper::display_title(&json!({ "title": t, "short_title": short }))),
            _ => None,
        };
        o.insert("paper".into(), json!(paper));
        o.insert("days_left".into(), json!(days_left));
    }
    d
}

/// Number of Desk "Needs attention" items (dock badge).
fn attention_count(ctx: &Ctx) -> Result<Value> {
    if ctx.user()?.is_none() {
        return Ok(json!(0));
    }
    let l = load(ctx)?;
    Ok(json!(attention(&l, today()).len()))
}

fn submit_next(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = as_integer(args.get("paper_id").unwrap_or(&Value::Null)).ok_or_else(ApiError::not_found)?;
    let p = ctx
        .db
        .first("SELECT id, title, short_title FROM papers WHERE id = ?1 AND user_id = ?2", params![id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    let sub = paper::submit_to_next(&ctx.db, id)?;
    if sub.is_some() {
        ctx.changed("papers");
    }
    Ok(json!({ "paper": paper_ref(&p), "submission": sub }))
}

fn create_paper(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("title", "required|string|max:255")
        .rule("short_title", "nullable|string|max:80")
        .rule("stage", &format!("required|in:{}", paper::stage_keys()))
        .rule("method", "nullable|string|max:255")
        .validate()?;
    ctx.require_user()?;
    let id = ctx.db.with(|c| {
        paper::insert(
            c,
            &json!({
                "title": v.string("title"),
                "short_title": v.opt("short_title"),
                "stage": v.string("stage"),
                "method": v.opt("method"),
            }),
        )
    })?;
    ctx.changed("papers");
    ctx.db.find("papers", id)?.ok_or_else(ApiError::not_found)
}

/// Command palette search: papers and venues matching `q`.
fn search(args: &Value, ctx: &Ctx) -> Result<Value> {
    let q = args.get("q").and_then(Value::as_str).unwrap_or("").trim();
    if q.is_empty() {
        return Ok(json!({ "papers": [], "venues": [] }));
    }
    let q: String = q.chars().take(100).collect();
    let like = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let papers = ctx.db.rows(
        "SELECT id, title, short_title, stage FROM papers WHERE user_id = ?1
         AND (title LIKE ?2 ESCAPE '\\' OR short_title LIKE ?2 ESCAPE '\\' OR keywords LIKE ?2 ESCAPE '\\')
         ORDER BY updated_at DESC, id DESC LIMIT 8",
        params![USER_ID, like],
    )?;
    let venues = ctx.db.rows(
        "SELECT id, name, kind, publisher FROM venues WHERE user_id = ?1
         AND (name LIKE ?2 ESCAPE '\\' OR publisher LIKE ?2 ESCAPE '\\' OR field LIKE ?2 ESCAPE '\\')
         ORDER BY name LIMIT 8",
        params![USER_ID, like],
    )?;
    let papers: Vec<Value> = papers
        .into_iter()
        .map(|p| {
            json!({
                "id": p["id"], "title": paper::display_title(&p),
                "stage": paper::stage_label(p["stage"].as_str().unwrap_or("")),
            })
        })
        .collect();
    let venues: Vec<Value> = venues
        .into_iter()
        .map(|v| {
            json!({
                "id": v["id"], "name": v["name"],
                "kind": crate::models::label(venue::KINDS, v["kind"].as_str().unwrap_or("")),
                "publisher": v["publisher"],
            })
        })
        .collect();
    Ok(json!({ "papers": papers, "venues": venues }))
}

/// "Run scans now" from the command palette (the menu bar item runs them natively).
fn run_scans(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    Ok(json!({ "started": crate::scheduler::run_now(ctx, None) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    fn ins(ctx: &Ctx, table: &str, v: Value) -> i64 {
        ctx.db.insert(table, &v).unwrap()
    }

    #[test]
    fn desk_data_attention_clocks_and_journey() {
        let ctx = Ctx::for_tests(test_db());
        let empty = data(&ctx).unwrap();
        assert_eq!(empty["empty"], true);
        assert_eq!(empty["attention"].as_array().unwrap().len(), 0);

        let t = today();
        let d = |n: i64| (t - chrono::Duration::days(n)).format(DATE_FORMAT).to_string();
        let v = ins(&ctx, "venues", json!({ "user_id": 1, "name": "J Double", "review_type": "double", "decision_days": 30 }));
        let v2 = ins(&ctx, "venues", json!({ "user_id": 1, "name": "Next J", "review_type": "single" }));
        // Overdue review with a public preprint (red + amber).
        let p1 = ins(
            &ctx,
            "papers",
            json!({ "user_id": 1, "title": "Paper one", "stage": "under_review", "preprint_url": "https://arxiv.org/abs/1" }),
        );
        ins(
            &ctx,
            "submissions",
            json!({ "paper_id": p1, "venue_id": v, "venue_name": "J Double", "submitted_on": d(60), "decision": "submitted" }),
        );
        // Ready with a queue (violet, prepended, Submit button).
        let p2 = ins(&ctx, "papers", json!({ "user_id": 1, "title": "Paper two long", "short_title": "P2", "stage": "ready" }));
        ins(&ctx, "queue_items", json!({ "paper_id": p2, "venue_id": v2, "position": 0 }));
        // Rejected, empty queue (indigo) and a revision due soon on another.
        let p3 = ins(&ctx, "papers", json!({ "user_id": 1, "title": "Paper three", "stage": "ready" }));
        let _ = p3;
        let p4 = ins(&ctx, "papers", json!({ "user_id": 1, "title": "Paper four", "stage": "drafting" }));
        ins(
            &ctx,
            "submissions",
            json!({ "paper_id": p4, "venue_name": "Conf", "submitted_on": d(100), "decision": "reject", "decision_on": d(40) }),
        );
        let p5 = ins(&ctx, "papers", json!({ "user_id": 1, "title": "Paper five", "stage": "revision" }));
        ins(
            &ctx,
            "submissions",
            json!({ "paper_id": p5, "venue_name": "Rev J", "submitted_on": d(90), "decision": "major", "decision_on": d(10), "revision_due_on": d(-5) }),
        );
        ins(&ctx, "deadlines", json!({ "user_id": 1, "title": "Camera ready", "due_on": d(-3), "paper_id": p2 }));
        ins(&ctx, "deadlines", json!({ "user_id": 1, "title": "Old", "due_on": d(30) }));

        let out = data(&ctx).unwrap();
        assert_eq!(out["empty"], false);
        let att = out["attention"].as_array().unwrap();
        let tones: Vec<&str> = att.iter().map(|a| a["tone"].as_str().unwrap()).collect();
        // Ready papers are prepended (last one first).
        assert_eq!(tones, vec!["violet", "violet", "red", "amber", "indigo", "amber"]);
        assert_eq!(att[0]["text"], "Ready to submit. Add a journal or conference to its “Up next” list.");
        assert_eq!(att[0]["next"], false);
        assert_eq!(att[1]["paper"]["title"], "P2");
        assert_eq!(att[1]["text"], "Ready to submit. Next: Next J.");
        assert_eq!(att[1]["next"], true);
        assert_eq!(att[2]["text"], "Public preprint while under double-anonymous review at J Double.");
        assert_eq!(att[3]["text"], "J Double is 30 days past its usual decision time. Consider a status query.");
        assert_eq!(att[4]["text"], "Rejected last time, and its “Up next” list is empty.");
        assert_eq!(att[5]["revision_due_on"], d(-5));
        assert_eq!(attention_count(&ctx).unwrap(), 6);

        assert_eq!(out["stats"][0]["value"], 1); // with journals
        assert_eq!(out["stats"][1]["value"], 1); // revising
        let clocks = out["clocks"].as_array().unwrap();
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0]["clock"]["overdue"], true);
        assert_eq!(clocks[0]["paper_title"], "Paper one");
        let dl = out["deadlines"].as_array().unwrap();
        assert_eq!(dl.len(), 1);
        assert_eq!(dl[0]["paper"], "P2");
        assert_eq!(dl[0]["days_left"], 3);

        let j = &out["journey"];
        assert_eq!(j["months"].as_array().unwrap().len(), 12);
        let lanes = j["lanes"].as_array().unwrap();
        assert_eq!(lanes.len(), 3);
        for lane in lanes {
            for b in lane["bars"].as_array().unwrap() {
                let x = b["x"].as_f64().unwrap();
                assert!((0.0..=100.0).contains(&x));
                assert!(b["w"].as_f64().unwrap() > 0.0);
            }
        }
        let tx = j["today_x"].as_f64().unwrap();
        assert!(tx > 90.0 && tx <= 100.0);

        // Submit to next venue.
        let r = submit_next(&json!({ "paper_id": p2 }), &ctx).unwrap();
        assert_eq!(r["submission"]["venue_name"], "Next J");
        assert_eq!(r["paper"]["title"], "P2");
        assert!(submit_next(&json!({ "paper_id": 999 }), &ctx).is_err());
    }

    #[test]
    fn create_paper_validates() {
        let ctx = Ctx::for_tests(test_db());
        let err = serde_json::to_value(create_paper(&json!({ "title": "", "stage": "nope" }), &ctx).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["title"][0], "The title field is required.");
        assert_eq!(err["errors"]["stage"][0], "The selected stage is invalid.");
        let long = "x".repeat(81);
        let err =
            serde_json::to_value(create_paper(&json!({ "title": "T", "short_title": long, "stage": "idea" }), &ctx).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["short_title"][0], "The short title field must not be greater than 80 characters.");
        let p = create_paper(&json!({ "title": " My paper ", "short_title": "", "stage": "idea", "method": "Survey" }), &ctx).unwrap();
        assert_eq!(p["title"], "My paper");
        assert!(p["short_title"].is_null());
        assert_eq!(p["started_on"], crate::util::today_str());

        let s = search(&json!({ "q": "my" }), &ctx).unwrap();
        assert_eq!(s["papers"][0]["title"], "My paper");
        assert_eq!(s["papers"][0]["stage"], "Idea");
        let s = search(&json!({ "q": "%" }), &ctx).unwrap();
        assert_eq!(s["papers"].as_array().unwrap().len(), 0);
    }
}
