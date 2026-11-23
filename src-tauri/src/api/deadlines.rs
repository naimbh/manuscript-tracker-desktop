//! `deadlines.*` handlers: port of livewire/deadlines.blade.php.

use crate::api::desk::decorate_deadline;
use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::paper;
use crate::util::today;
use crate::validation::{as_integer, Validator};
use rusqlite::params;
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "deadlines.list" => list(ctx),
        "deadlines.create" => create(args, ctx),
        "deadlines.toggle" => toggle(args, ctx),
        "deadlines.delete" => remove(args, ctx),
        _ => return None,
    })
}

const SELECT: &str = "SELECT d.id, d.title, d.due_on, d.done, d.paper_id, p.title AS paper_title, p.short_title AS paper_short_title
     FROM deadlines d LEFT JOIN papers p ON p.id = d.paper_id WHERE d.user_id = ?1";

fn list(ctx: &Ctx) -> Result<Value> {
    let t = today();
    let (upcoming, done, papers) = ctx.db.with(|c| {
        let upcoming = c.rows(&format!("{SELECT} AND d.done = 0 ORDER BY d.due_on, d.id"), [USER_ID])?;
        let done = c.rows(&format!("{SELECT} AND d.done = 1 ORDER BY d.due_on DESC, d.id LIMIT 20"), [USER_ID])?;
        let papers = c.rows("SELECT id, title, short_title FROM papers WHERE user_id = ?1 ORDER BY title, id", [USER_ID])?;
        Ok((upcoming, done, papers))
    })?;
    let deco = |rows: Vec<Value>| rows.into_iter().map(|d| decorate_deadline(d, t)).collect::<Vec<_>>();
    let papers: Vec<Value> = papers.iter().map(|p| json!({ "id": p["id"], "title": paper::display_title(p) })).collect();
    Ok(json!({ "upcoming": deco(upcoming), "done": deco(done), "papers": papers }))
}

fn create(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("title", "required|string|max:255")
        .rule("due_on", "required|date")
        .rule("paper_id", "nullable")
        .validate()?;
    ctx.require_user()?;
    // Only the user's own papers; anything else becomes a general deadline.
    let paper_id = match v.i64("paper_id") {
        Some(id) => ctx.db.scalar::<i64, _>("SELECT id FROM papers WHERE id = ?1 AND user_id = ?2", params![id, USER_ID])?,
        None => None,
    };
    let due = crate::util::parse_date(&v.string("due_on")).map(|d| d.format(crate::util::DATE_FORMAT).to_string());
    let id = ctx.db.insert(
        "deadlines",
        &json!({ "user_id": USER_ID, "title": v.string("title"), "due_on": due.unwrap_or_else(|| v.string("due_on")), "paper_id": paper_id }),
    )?;
    ctx.changed("deadlines");
    ctx.db.find("deadlines", id)?.ok_or_else(ApiError::not_found)
}

fn own(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = as_integer(args.get("id").unwrap_or(&Value::Null)).ok_or_else(ApiError::not_found)?;
    ctx.db.first("SELECT * FROM deadlines WHERE id = ?1 AND user_id = ?2", params![id, USER_ID])?.ok_or_else(ApiError::not_found)
}

fn toggle(args: &Value, ctx: &Ctx) -> Result<Value> {
    let d = own(args, ctx)?;
    let id = d["id"].as_i64().unwrap_or(0);
    ctx.db.update("deadlines", id, &json!({ "done": !d["done"].as_bool().unwrap_or(false) }))?;
    ctx.changed("deadlines");
    ctx.db.find("deadlines", id)?.ok_or_else(ApiError::not_found)
}

fn remove(args: &Value, ctx: &Ctx) -> Result<Value> {
    let d = own(args, ctx)?;
    ctx.db.delete("deadlines", d["id"].as_i64().unwrap_or(0))?;
    ctx.changed("deadlines");
    Ok(json!(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn deadlines_crud() {
        let ctx = Ctx::for_tests(test_db());
        let err = serde_json::to_value(create(&json!({ "title": "", "due_on": "nope" }), &ctx).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["title"][0], "The title field is required.");
        assert_eq!(err["errors"]["due_on"][0], "The due on field must be a valid date.");

        let p = ctx.db.insert("papers", &json!({ "user_id": 1, "title": "Zeta", "short_title": "Z" })).unwrap();
        let a = create(&json!({ "title": "Camera ready", "due_on": "2099-01-02", "paper_id": p.to_string() }), &ctx).unwrap();
        assert_eq!(a["paper_id"], p);
        let b = create(&json!({ "title": "General", "due_on": "2099-01-01", "paper_id": "9999" }), &ctx).unwrap();
        assert!(b["paper_id"].is_null());
        let c = create(&json!({ "title": "Old", "due_on": "2000-01-01", "paper_id": "" }), &ctx).unwrap();

        let l = list(&ctx).unwrap();
        let up = l["upcoming"].as_array().unwrap();
        assert_eq!(up.len(), 3);
        assert_eq!(up[0]["title"], "Old");
        assert!(up[0]["days_left"].as_i64().unwrap() < 0);
        assert_eq!(up[2]["paper"], "Z");
        assert!(up[1]["paper"].is_null());
        assert_eq!(l["papers"][0]["title"], "Z");

        let t = toggle(&json!({ "id": c["id"] }), &ctx).unwrap();
        assert_eq!(t["done"], true);
        let l = list(&ctx).unwrap();
        assert_eq!(l["upcoming"].as_array().unwrap().len(), 2);
        assert_eq!(l["done"][0]["title"], "Old");
        remove(&json!({ "id": a["id"] }), &ctx).unwrap();
        assert!(remove(&json!({ "id": a["id"] }), &ctx).is_err());
        assert_eq!(list(&ctx).unwrap()["upcoming"].as_array().unwrap().len(), 1);
    }
}
