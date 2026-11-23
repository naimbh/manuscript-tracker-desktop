//! `venues.*` handlers: port of resources/views/livewire/venues.blade.php (venue library).
//!
//! The research entry points (`venues.research`, `venues.refreshAll`, `venues.restoreCatalog`) live in
//! `api/proposals.rs`, which `api::dispatch` consults first.

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{keys, venue};
use crate::research::providers;
use crate::util::today;
use crate::validation::Validator;
use serde_json::{json, Map, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "venues.list" => list(ctx),
        "venues.get" => get(args, ctx),
        "venues.save" => save(args, ctx),
        "venues.delete" => delete(args, ctx),
        "venues.toggleWatch" => toggle_watch(args, ctx),
        _ => return None,
    })
}

/// Called once by `app.onboard` after the local user is created (VenueCatalog::seedFor in the web app).
pub fn on_user_created(ctx: &Ctx) -> Result<()> {
    crate::research::venues::seed_catalog(ctx)?;
    Ok(())
}

/// Scheduled jobs (`venues:refresh`) are registered by `research::register_jobs`.
pub fn register_jobs() {}

/// The editable columns (Venues::FIELDS in the web app, minus `rank` and `quartile`: those columns stay in the
/// database and in backups, but the app never shows, edits or computes them).
pub const FIELDS: &[&str] = &[
    "name",
    "kind",
    "field",
    "publisher",
    "acceptance_rate",
    "first_decision",
    "proceedings",
    "next_edition",
    "deadline_on",
    "deadline_label",
    "deadline_expected",
    "review_type",
    "word_limit",
    "decision_days",
    "fee",
    "preprint_policy",
    "portal_url",
    "website_url",
    "watch",
    "notes",
];

fn id_of(args: &Value) -> Result<i64> {
    args.get("id")
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .ok_or_else(|| ApiError::field("id", "The id field is required."))
}

fn find(ctx: &Ctx, id: i64) -> Result<Value> {
    ctx.db.first("SELECT * FROM venues WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID])?.ok_or_else(ApiError::not_found)
}

/// Every venue of the user with `submissions_count`, `deadline_days` and `pending` (proposals to review),
/// plus the AI status. Filtering, tabs and sorting happen in the page (instant while typing).
fn list(ctx: &Ctx) -> Result<Value> {
    let rows = ctx.db.rows(
        "SELECT v.*, COALESCE(s.n, 0) AS submissions_count, COALESCE(p.n, 0) AS pending
         FROM venues v
         LEFT JOIN (SELECT venue_id, COUNT(*) AS n FROM submissions WHERE venue_id IS NOT NULL GROUP BY venue_id) s ON s.venue_id = v.id
         LEFT JOIN (SELECT target_id, COUNT(*) AS n FROM proposals WHERE kind = 'venue' AND status = 'pending' GROUP BY target_id) p ON p.target_id = v.id
         WHERE v.user_id = ?1
         ORDER BY v.name COLLATE NOCASE",
        [USER_ID],
    )?;
    let today = today();
    let list: Vec<Value> = rows
        .into_iter()
        .map(|mut v| {
            let days = venue::deadline_days(&v, today);
            if let Some(o) = v.as_object_mut() {
                o.insert("deadline_days".into(), json!(days));
                // Rankings stay in the database (backups) but are never shown.
                o.remove("rank");
                o.remove("quartile");
            }
            v
        })
        .collect();
    let pending: i64 = list.iter().filter_map(|v| v["pending"].as_i64()).sum();
    Ok(json!({
        "venues": list,
        "pending": pending,
        "aiReady": providers::configured(&ctx.db),
        "aiStatus": providers::status(&ctx.db),
    }))
}

/// The form values of one venue (`deadline_on` as `YYYY-MM-DD`, blanks as "").
fn get(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = find(ctx, id_of(args)?)?;
    let mut form = Map::new();
    form.insert("id".into(), v["id"].clone());
    for f in FIELDS {
        let val = match (*f, &v[*f]) {
            ("deadline_expected" | "watch", x) => json!(x.as_bool().unwrap_or(false)),
            ("deadline_on", Value::String(s)) => json!(s.chars().take(10).collect::<String>()),
            (_, Value::Null) => json!(""),
            (_, x) => x.clone(),
        };
        form.insert((*f).into(), val);
    }
    Ok(Value::Object(form))
}

fn validated(args: &Value) -> Result<Value> {
    let review = keys(venue::REVIEW_TYPES);
    let preprint = keys(venue::PREPRINT_POLICIES);
    let v = Validator::new(args)
        .rule("name", "required|string|max:255")
        .rule("kind", "required|in:journal,conference,preprint")
        .rule("field", "nullable|string|max:255")
        .rule("publisher", "nullable|string|max:255")
        .rule("acceptance_rate", "nullable|string|max:255")
        .rule("first_decision", "nullable|string|max:255")
        .rule("proceedings", "nullable|string|max:255")
        .rule("next_edition", "nullable|string|max:255")
        .rule("deadline_on", "nullable|date")
        .rule("deadline_label", "nullable|string|max:255")
        .rule("deadline_expected", "boolean")
        .rule("review_type", &format!("required|in:{review}"))
        .rule("word_limit", "nullable|string|max:255")
        .rule("decision_days", "nullable|integer|min:1|max:2000")
        .rule("fee", "nullable|string|max:255")
        .rule("preprint_policy", &format!("required|in:{preprint}"))
        .rule("portal_url", "nullable|url|max:255")
        .rule("website_url", "nullable|url|max:255")
        .rule("watch", "boolean")
        .rule("notes", "nullable|string|max:5000")
        .validate()?;
    let mut data = Map::new();
    for f in FIELDS {
        let val = match *f {
            "deadline_expected" | "watch" => json!(v.bool(f)),
            "decision_days" => json!(v.i64(f)),
            "deadline_on" => json!(v.opt(f).map(|s| s.chars().take(10).collect::<String>())),
            // Laravel keeps the value as typed (no trim); blanks become null.
            "notes" => match v.get(f) {
                Value::String(s) if !s.trim().is_empty() => json!(s),
                _ => Value::Null,
            },
            _ => json!(v.opt(f)),
        };
        data.insert((*f).into(), val);
    }
    Ok(Value::Object(data))
}

/// Add (`id` absent) or update a venue → `{ venue, created, tab }`.
fn save(args: &Value, ctx: &Ctx) -> Result<Value> {
    let mut data = validated(args)?;
    let editing = args.get("id").filter(|v| !v.is_null()).map(|_| id_of(args)).transpose()?;
    let id = match editing {
        Some(id) => {
            find(ctx, id)?;
            ctx.db.update("venues", id, &data)?;
            id
        }
        None => {
            data["user_id"] = json!(USER_ID);
            ctx.db.insert("venues", &data)?
        }
    };
    let tab = if data["kind"] == "journal" { "journal" } else { "conference" };
    Ok(json!({ "venue": find(ctx, id)?, "created": editing.is_none(), "tab": tab }))
}

/// Remove a venue. Submissions keep their row (`venue_id` becomes null); queue entries go.
fn delete(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = id_of(args)?;
    find(ctx, id)?;
    ctx.db.tx(|tx| {
        tx.exec("DELETE FROM proposals WHERE kind = 'venue' AND target_id = ?1", [id])?;
        tx.delete("venues", id)?;
        Ok(())
    })?;
    Ok(json!({ "deleted": true }))
}

fn toggle_watch(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = find(ctx, id_of(args)?)?;
    let id = v["id"].as_i64().unwrap_or(0);
    let watch = !v["watch"].as_bool().unwrap_or(false);
    ctx.db.update("venues", id, &json!({ "watch": watch }))?;
    Ok(json!({ "id": id, "watch": watch }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;

    fn base() -> Value {
        json!({ "name": "CHI 2027", "kind": "conference", "review_type": "double", "preprint_policy": "unknown",
                "deadline_expected": false, "watch": true, "deadline_on": "2099-01-21", "decision_days": "60", "notes": "" })
    }

    fn errors(r: Result<Value>) -> Value {
        serde_json::to_value(r.unwrap_err()).unwrap()["errors"].clone()
    }

    #[test]
    fn crud() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let r = dispatch("venues.save", &base(), &ctx).unwrap();
        assert_eq!(r["created"], true);
        assert_eq!(r["tab"], "conference");
        let id = r["venue"]["id"].as_i64().unwrap();
        assert_eq!(r["venue"]["decision_days"], 60);
        assert_eq!(r["venue"]["watch"], true);
        assert!(r["venue"]["notes"].is_null());

        let form = dispatch("venues.get", &json!({ "id": id }), &ctx).unwrap();
        assert_eq!(form["deadline_on"], "2099-01-21");
        assert_eq!(form["publisher"], "");

        let mut upd = base();
        upd["id"] = json!(id);
        upd["kind"] = json!("journal");
        upd["quartile"] = json!("Q1");
        let r = dispatch("venues.save", &upd, &ctx).unwrap();
        assert_eq!(r["created"], false);
        assert_eq!(r["tab"], "journal");
        // Rankings are not editable any more (the column stays untouched).
        assert!(ctx.db.find("venues", id).unwrap().unwrap()["quartile"].is_null());

        let paper = ctx.db.insert("papers", &json!({ "user_id": 1, "title": "P" })).unwrap();
        ctx.db.insert("submissions", &json!({ "paper_id": paper, "venue_id": id, "venue_name": "CHI", "decision": "submitted" })).unwrap();
        ctx.db.insert("proposals", &json!({ "user_id": 1, "kind": "venue", "target_id": id, "field": "fee", "proposed": "None" })).unwrap();
        let l = dispatch("venues.list", &json!({}), &ctx).unwrap();
        assert_eq!(l["venues"][0]["submissions_count"], 1);
        assert_eq!(l["venues"][0]["pending"], 1);
        assert_eq!(l["pending"], 1);
        assert!(l["venues"][0]["deadline_days"].as_i64().unwrap() > 0);
        assert!(l["aiReady"].is_boolean());

        let w = dispatch("venues.toggleWatch", &json!({ "id": id }), &ctx).unwrap();
        assert_eq!(w["watch"], false);

        dispatch("venues.delete", &json!({ "id": id }), &ctx).unwrap();
        assert!(ctx.db.find("venues", id).unwrap().is_none());
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM submissions WHERE venue_id IS NULL", []).unwrap(), 1);
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM proposals", []).unwrap(), 0);
        assert!(dispatch("venues.get", &json!({ "id": id }), &ctx).is_err());
    }

    #[test]
    fn validation() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let e = errors(dispatch(
            "venues.save",
            &json!({ "name": "", "kind": "x", "review_type": "double", "preprint_policy": "unknown" }),
            &ctx,
        ));
        assert_eq!(e["name"][0], "The name field is required.");
        assert_eq!(e["kind"][0], "The selected kind is invalid.");
        let mut bad = base();
        bad["website_url"] = json!("not a url");
        bad["decision_days"] = json!("0");
        bad["deadline_on"] = json!("soon");
        let e = errors(dispatch("venues.save", &bad, &ctx));
        assert_eq!(e["website_url"][0], "The website url field must be a valid URL.");
        assert_eq!(e["decision_days"][0], "The decision days field must be at least 1.");
        assert_eq!(e["deadline_on"][0], "The deadline on field must be a valid date.");
        assert!(dispatch("venues.delete", &json!({ "id": 999 }), &ctx).is_err());
    }

    #[test]
    fn catalog_seeding() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        ctx.db.insert("venues", &json!({ "user_id": 1, "name": "iconference", "kind": "conference" })).unwrap();
        on_user_created(&ctx).unwrap();
        let total = ctx.db.count("SELECT COUNT(*) FROM venues", []).unwrap();
        assert!(total > 40);
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM venues WHERE lower(name) = 'iconference'", []).unwrap(), 1);
        assert!(ctx.db.count("SELECT COUNT(*) FROM venues WHERE kind = 'journal' AND watch = 0", []).unwrap() > 20);
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM venues WHERE kind = 'conference' AND watch = 0", []).unwrap(), 1);
        let chi = ctx.db.first("SELECT * FROM venues WHERE name LIKE 'CHI %'", []).unwrap().unwrap();
        assert_eq!(chi["deadline_on"], "2027-01-21");
        assert!(chi["checked_at"].is_null());
        // Seeding again adds nothing.
        on_user_created(&ctx).unwrap();
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM venues", []).unwrap(), total);
        let r = dispatch("venues.restoreCatalog", &json!({}), &ctx).unwrap();
        assert_eq!(r["added"], 0);
    }
}
