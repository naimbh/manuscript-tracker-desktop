//! `opportunities.*` handlers (port of resources/views/livewire/opportunities.blade.php).
//! See docs/ARCHITECTURE.md ("Research engine").

use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::models::opportunity;
use crate::research::{self, providers};
use crate::util::{parse_ts, start_of_week_ts, today};
use crate::validation::{validate, Validator};
use chrono::{Datelike, Duration, Local, TimeZone, Utc};
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "opportunities.list" => list(args, ctx),
        "opportunities.saveInterests" => save_interests(args, ctx),
        "opportunities.scan" => scan(ctx),
        "opportunities.setStatus" => set_status(args, ctx),
        "opportunities.addDeadline" => add_deadline(args, ctx),
        "opportunities.addVenue" => add_venue(args, ctx),
        "opportunities.newThisWeek" => new_this_week(ctx).map(|n| json!(n)),
        _ => return None,
    })
}

/// Scheduled jobs are registered in `research::register_jobs` (called at startup after this).
pub fn register_jobs() {}

pub fn new_this_week(ctx: &Ctx) -> Result<i64> {
    ctx.db.count(
        "SELECT COUNT(*) FROM opportunities WHERE user_id = ?1 AND status = 'new' AND created_at >= ?2",
        rusqlite::params![USER_ID, start_of_week_ts()],
    )
}

fn local_date(ts: &str) -> Option<chrono::NaiveDate> {
    parse_ts(ts).map(|t| Utc.from_utc_datetime(&t).with_timezone(&Local).date_naive())
}

fn decorate(mut o: Value) -> Value {
    let days = o["deadline_on"].as_str().and_then(crate::util::parse_date).map(|d| (d - today()).num_days());
    let is_new = o["status"] == "new" && o["created_at"].as_str().is_some_and(|c| c > crate::util::ts_minutes_ago(2 * 24 * 60).as_str());
    let kind = o["kind"].as_str().unwrap_or("").to_string();
    if let Some(m) = o.as_object_mut() {
        m.insert("days".into(), json!(days));
        m.insert("is_new".into(), json!(is_new));
        m.insert("kind_label".into(), json!(crate::models::label(opportunity::KINDS, &kind)));
    }
    o
}

/// `{ show: open|saved|dismissed|history }` → everything the page shows.
fn list(args: &Value, ctx: &Ctx) -> Result<Value> {
    let show = args.get("show").and_then(Value::as_str).unwrap_or("open");
    let user = ctx.require_user()?;
    let (cond, extra) = match show {
        "saved" => ("status = 'saved'", ""),
        "dismissed" => ("status = 'dismissed'", ""),
        _ => ("status != 'dismissed'", "AND (deadline_on IS NULL OR deadline_on >= ?2)"),
    };
    let sql = format!("SELECT * FROM opportunities WHERE user_id = ?1 AND {cond} {extra} ORDER BY created_at DESC, fit DESC LIMIT 200");
    let rows = if extra.is_empty() {
        ctx.db.rows(&sql, [USER_ID])?
    } else {
        ctx.db.rows(&sql, rusqlite::params![USER_ID, crate::util::today_str()])?
    };
    let this_monday = today() - Duration::days(today().weekday().num_days_from_monday() as i64);
    let mut weeks: Vec<Value> = Vec::new();
    if show != "history" {
        for o in rows {
            let d = o["created_at"].as_str().and_then(local_date).unwrap_or_else(today);
            let monday = d - Duration::days(d.weekday().num_days_from_monday() as i64);
            let key = monday.format("%Y-%m-%d").to_string();
            if weeks.last().is_none_or(|w| w["week"] != key) {
                let label = if monday == this_monday { "This week".to_string() } else { format!("Week of {}", monday.format("%-d %b")) };
                weeks.push(json!({ "week": key, "label": label, "items": [] }));
            }
            if let Some(items) = weeks.last_mut().and_then(|w| w["items"].as_array_mut()) {
                items.push(decorate(o));
            }
        }
        for w in &mut weeks {
            let n = w["items"].as_array().map_or(0, Vec::len);
            w["count"] = json!(n);
        }
    }
    let last = ctx.db.first("SELECT * FROM scans WHERE user_id = ?1 ORDER BY id DESC LIMIT 1", [USER_ID])?;
    let running = last.as_ref().is_some_and(|l| {
        l["status"] == "running" && l["created_at"].as_str().is_some_and(|c| c > crate::util::ts_minutes_ago(15).as_str())
    });
    let (scans, runs) = if show == "history" {
        (
            ctx.db.rows("SELECT * FROM scans WHERE user_id = ?1 ORDER BY id DESC LIMIT 60", [USER_ID])?,
            ctx.db.rows("SELECT * FROM ai_runs WHERE user_id = ?1 ORDER BY id DESC LIMIT 60", [USER_ID])?,
        )
    } else {
        (vec![], vec![])
    };
    let scan_run: Option<i64> = if running {
        ctx.db.scalar(
            "SELECT id FROM ai_runs WHERE user_id = ?1 AND kind = 'scan' AND status = 'running' ORDER BY id DESC LIMIT 1",
            [USER_ID],
        )?
    } else {
        None
    };
    Ok(json!({
        "show": show,
        "weeks": weeks,
        "last": last,
        "running": running,
        "run_id": scan_run,
        "aiReady": providers::configured(&ctx.db),
        "aiStatus": providers::status(&ctx.db),
        "scans": scans,
        "runs": runs,
        "newThisWeek": new_this_week(ctx)?,
        "interests": research::opportunities::interests_for(ctx, user["interests"].as_str()),
        "scan_time": ctx.setting("schedule.scan_time"),
    }))
}

fn save_interests(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let v = validate(args, &[("interests", "required|string|max:1000")])?;
    ctx.db.update("users", USER_ID, &json!({ "interests": v.string("interests") }))?;
    Ok(json!({ "interests": v.string("interests") }))
}

/// "Scan now" → `{ started, run, scan }` (`started: false` when a scan is already running). Works without
/// AI too (keyword ranking).
fn scan(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    match research::opportunities::start(ctx, "manual")? {
        None => Ok(json!({ "started": false })),
        Some((scan_id, run)) => {
            let id = run.id;
            ctx.spawn(move |ctx| research::opportunities::scan(ctx, run, scan_id));
            Ok(json!({ "started": true, "run": id, "scan": scan_id }))
        }
    }
}

fn find(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = args.get("id").and_then(Value::as_i64).ok_or_else(|| ApiError::field("id", "The id field is required."))?;
    ctx.db
        .first("SELECT * FROM opportunities WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID])?
        .ok_or_else(ApiError::not_found)
}

/// `{ id, status: new|saved|dismissed }`.
fn set_status(args: &Value, ctx: &Ctx) -> Result<Value> {
    let o = find(args, ctx)?;
    let v = Validator::new(args).rule("status", &format!("required|in:{}", opportunity::STATUSES.join(","))).validate()?;
    let id = o["id"].as_i64().unwrap_or(0);
    ctx.db.update("opportunities", id, &json!({ "status": v.string("status") }))?;
    Ok(decorate(ctx.db.find("opportunities", id)?.unwrap_or(Value::Null)))
}

/// "Add deadline" → `{ added }` (firstOrCreate on title + date; marks the call saved).
fn add_deadline(args: &Value, ctx: &Ctx) -> Result<Value> {
    let o = find(args, ctx)?;
    let Some(due) = o["deadline_on"].as_str() else {
        return Ok(json!({ "added": false }));
    };
    let title = research::limit(o["title"].as_str().unwrap_or(""), 200);
    let exists = ctx.db.count(
        "SELECT COUNT(*) FROM deadlines WHERE user_id = ?1 AND title = ?2 AND due_on = ?3",
        rusqlite::params![USER_ID, title, due],
    )? > 0;
    if !exists {
        ctx.db.insert("deadlines", &json!({ "user_id": USER_ID, "title": title, "due_on": due }))?;
    }
    ctx.db.update("opportunities", o["id"].as_i64().unwrap_or(0), &json!({ "status": "saved" }))?;
    ctx.changed("deadlines");
    Ok(json!({ "added": true }))
}

/// "Add venue to library" → `{ venue_id, created }`. Uses the call's venue name; an existing venue with the
/// same name gets nothing overwritten.
fn add_venue(args: &Value, ctx: &Ctx) -> Result<Value> {
    let o = find(args, ctx)?;
    let name = o["venue"].as_str().filter(|v| !v.trim().is_empty()).or(o["title"].as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err(ApiError::msg("This call has no journal or conference name."));
    }
    let existing: Option<i64> =
        ctx.db.scalar("SELECT id FROM venues WHERE user_id = ?1 AND lower(name) = lower(?2)", rusqlite::params![USER_ID, name])?;
    if let Some(id) = existing {
        return Ok(json!({ "venue_id": id, "created": false }));
    }
    let kind = match o["kind"].as_str() {
        Some("journal") | Some("special_issue") => "journal",
        _ => "conference",
    };
    let mut data = json!({
        "user_id": USER_ID, "name": research::limit(&name, 255), "kind": kind, "publisher": o["publisher"],
        "website_url": research::clean_url(&o["url"], 255), "watch": kind == "conference",
        "notes": o["summary"],
    });
    if kind == "conference" {
        data["next_edition"] = o["event"].clone();
        if o["deadline_on"].is_string() {
            data["deadline_on"] = o["deadline_on"].clone();
            data["deadline_label"] = json!("From the call");
        }
    }
    let id = ctx.db.insert("venues", &data)?;
    ctx.changed("venues");
    Ok(json!({ "venue_id": id, "created": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;

    #[test]
    fn list_actions() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let mk = |title: &str, deadline: Option<&str>, status: &str| {
            ctx.db
                .insert(
                    "opportunities",
                    &json!({ "user_id": 1, "title": title, "url": format!("https://x.org/{title}"), "url_hash": title,
                "deadline_on": deadline, "status": status, "venue": "Journal X", "kind": "special_issue", "fit": 4 }),
                )
                .unwrap()
        };
        let a = mk("open", Some("2099-01-01"), "new");
        mk("past", Some("2001-01-01"), "new");
        mk("gone", None, "dismissed");
        let l = dispatch("opportunities.list", &json!({}), &ctx).unwrap();
        assert_eq!(l["weeks"][0]["label"], "This week");
        assert_eq!(l["weeks"][0]["count"], 1);
        assert_eq!(l["weeks"][0]["items"][0]["is_new"], true);
        assert_eq!(l["weeks"][0]["items"][0]["kind_label"], "Special issue");
        assert_eq!(l["newThisWeek"], 2);
        assert_eq!(dispatch("opportunities.list", &json!({ "show": "dismissed" }), &ctx).unwrap()["weeks"][0]["count"], 1);
        assert!(dispatch("opportunities.setStatus", &json!({ "id": a, "status": "bogus" }), &ctx).is_err());
        assert_eq!(dispatch("opportunities.addDeadline", &json!({ "id": a }), &ctx).unwrap()["added"], true);
        dispatch("opportunities.addDeadline", &json!({ "id": a }), &ctx).unwrap();
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM deadlines", []).unwrap(), 1);
        assert_eq!(ctx.db.find("opportunities", a).unwrap().unwrap()["status"], "saved");
        let v = dispatch("opportunities.addVenue", &json!({ "id": a }), &ctx).unwrap();
        assert_eq!(v["created"], true);
        assert_eq!(dispatch("opportunities.addVenue", &json!({ "id": a }), &ctx).unwrap()["created"], false);
        assert!(dispatch("opportunities.saveInterests", &json!({ "interests": "" }), &ctx).is_err());
    }
}
