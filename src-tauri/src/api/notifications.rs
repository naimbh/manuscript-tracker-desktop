//! `notifications.*` handlers: port of livewire/notification-bell.blade.php (list, unread count, AI runs in
//! progress, open, mark read, mark all read, clear).

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::ai_run;
use crate::util::now_ts;
use rusqlite::params;
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "notifications.list" => list(ctx),
        "notifications.open" => open(args, ctx),
        "notifications.markRead" => mark_read(args, ctx),
        "notifications.markAllRead" => mark_all_read(ctx),
        "notifications.clear" => clear(ctx),
        _ => return None,
    })
}

const MINE: &str = "notifiable_type = 'App\\Models\\User' AND notifiable_id = ?1";

/// Bell state: the 12 latest notifications, the unread count and the AI runs still going.
fn list(ctx: &Ctx) -> Result<Value> {
    // Jobs that died without finishing (e.g. the machine slept) are closed after 20 minutes.
    ai_run::close_stale(ctx)?;
    let (items, unread, running) = ctx.db.with(|c| {
        let items =
            c.rows(&format!("SELECT * FROM notifications WHERE {MINE} ORDER BY created_at DESC, rowid DESC LIMIT 12"), [USER_ID])?;
        let unread = c.count(&format!("SELECT COUNT(*) FROM notifications WHERE {MINE} AND read_at IS NULL"), [USER_ID])?;
        let running = c.rows(
            "SELECT id, kind, label, status, total, done, step, url, created_at, updated_at FROM ai_runs
             WHERE user_id = ?1 AND status = 'running' ORDER BY created_at DESC, id DESC",
            [USER_ID],
        )?;
        Ok((items, unread, running))
    })?;
    let running: Vec<Value> = running
        .into_iter()
        .map(|mut r| {
            let pct = ai_run::percent(&r);
            r["percent"] = json!(pct);
            r
        })
        .collect();
    Ok(json!({ "items": items, "unread": unread, "running": running }))
}

fn id_arg(args: &Value) -> Result<String> {
    let id = args.get("id").and_then(Value::as_str).unwrap_or("").trim();
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::not_found());
    }
    Ok(id.to_string())
}

fn mark(ctx: &Ctx, id: &str) -> Result<Value> {
    let n = ctx
        .db
        .first(&format!("SELECT * FROM notifications WHERE id = ?2 AND {MINE}"), params![USER_ID, id])?
        .ok_or_else(ApiError::not_found)?;
    if n["read_at"].is_null() {
        let now = now_ts();
        ctx.db.exec("UPDATE notifications SET read_at = ?1, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    Ok(n)
}

/// Mark one read and return its link (`{ url }`), like `openItem` on the web.
fn open(args: &Value, ctx: &Ctx) -> Result<Value> {
    let n = mark(ctx, &id_arg(args)?)?;
    let url = n["data"].get("url").cloned().unwrap_or(Value::Null);
    let mut out = list(ctx)?;
    out["url"] = url;
    Ok(out)
}

fn mark_read(args: &Value, ctx: &Ctx) -> Result<Value> {
    mark(ctx, &id_arg(args)?)?;
    list(ctx)
}

fn mark_all_read(ctx: &Ctx) -> Result<Value> {
    let now = now_ts();
    ctx.db
        .exec(&format!("UPDATE notifications SET read_at = ?2, updated_at = ?2 WHERE {MINE} AND read_at IS NULL"), params![USER_ID, now])?;
    list(ctx)
}

fn clear(ctx: &Ctx) -> Result<Value> {
    ctx.db.exec(&format!("DELETE FROM notifications WHERE {MINE}"), [USER_ID])?;
    list(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;
    use crate::models::ai_run::AiRun;

    #[test]
    fn bell_flow() {
        let ctx = Ctx::for_tests(test_db());
        let l = list(&ctx).unwrap();
        assert_eq!(l["unread"], 0);
        let a = ctx.notify("ai_job_finished", json!({ "title": "A", "body": "a", "status": "success", "url": "#/venues" })).unwrap();
        ctx.notify("ai_job_finished", json!({ "title": "B", "body": "b", "status": "warning", "url": null })).unwrap();
        let run = AiRun::begin(&ctx, "venues", "Refreshing venues", 4, Some("#/venues"), None).unwrap();
        run.advance(&ctx, Some("One")).unwrap();
        let l = list(&ctx).unwrap();
        assert_eq!(l["unread"], 2);
        assert_eq!(l["items"].as_array().unwrap().len(), 2);
        assert_eq!(l["items"][0]["data"]["title"], "B");
        assert_eq!(l["running"][0]["percent"], 25);

        let o = open(&json!({ "id": a["id"] }), &ctx).unwrap();
        assert_eq!(o["url"], "#/venues");
        assert_eq!(o["unread"], 1);
        assert!(open(&json!({ "id": "nope" }), &ctx).is_err());
        assert_eq!(mark_all_read(&ctx).unwrap()["unread"], 0);
        let c = clear(&ctx).unwrap();
        assert_eq!(c["items"].as_array().unwrap().len(), 0);
        let _ = mark_read;
    }
}
