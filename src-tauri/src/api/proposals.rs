//! `proposals.*` handlers (review what research found) and the research entry points for venues and
//! publications: `venues.research`, `venues.refreshAll`, `publications.refreshCitations`,
//! `publications.refreshAll`. `api::dispatch` routes these names here before the area modules, so
//! `venues.rs` and `publications.rs` do not define them. See docs/ARCHITECTURE.md ("Research engine").

use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::research::{citations, proposals, runs::Run, venues};
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "proposals.list" => list(args, ctx),
        "proposals.count" => Ok(json!(proposals::pending_count(&ctx.db))),
        "proposals.apply" => apply(args, ctx),
        "proposals.dismiss" => dismiss(args, ctx),
        "venues.research" => venue_research(args, ctx),
        "venues.refreshAll" => venues_refresh_all(args, ctx),
        "publications.refreshCitations" => publication_refresh(args, ctx),
        "publications.refreshAll" => publications_refresh_all(args, ctx),
        _ => return None,
    })
}

fn ids(args: &Value, key: &str) -> Vec<i64> {
    args.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
}

/// `{ status? = "pending", kind?, target_id?, run_id? }` → `{ items, groups: [{ kind, target_id, target, items }], pending }`.
fn list(args: &Value, ctx: &Ctx) -> Result<Value> {
    proposals::prune_orphans(&ctx.db)?;
    let status = args.get("status").and_then(Value::as_str).unwrap_or("pending");
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
    let target = args.get("target_id").and_then(Value::as_i64).unwrap_or(0);
    let run = args.get("run_id").and_then(Value::as_i64).unwrap_or(0);
    let rows = ctx.db.rows(
        "SELECT * FROM proposals WHERE user_id = ?1 AND (?2 = 'all' OR status = ?2) AND (?3 = '' OR kind = ?3)
           AND (?4 = 0 OR target_id = ?4) AND (?5 = 0 OR run_id = ?5)
         ORDER BY kind, target_id, id LIMIT 1000",
        rusqlite::params![USER_ID, status, kind, target, run],
    )?;
    let items: Vec<Value> = rows.into_iter().map(|p| proposals::decorate(&ctx.db, p)).collect();
    let mut groups: Vec<Value> = Vec::new();
    for p in &items {
        let same = groups.last().is_some_and(|g| g["kind"] == p["kind"] && g["target_id"] == p["target_id"]);
        if !same {
            groups.push(json!({ "kind": p["kind"], "kind_label": p["kind_label"], "target_id": p["target_id"], "target": p["target"], "items": [] }));
        }
        if let Some(list) = groups.last_mut().and_then(|g| g["items"].as_array_mut()) {
            list.push(p.clone());
        }
    }
    Ok(json!({ "items": items, "groups": groups, "pending": proposals::pending_count(&ctx.db) }))
}

/// The pending ids selected by `{ ids }`, or by `{ kind, target_id }` / `{ all: true }` (optionally only
/// `verified_only: true`).
fn selected(args: &Value, ctx: &Ctx) -> Result<Vec<i64>> {
    let list = ids(args, "ids");
    if !list.is_empty() {
        return Ok(list);
    }
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
    let target = args.get("target_id").and_then(Value::as_i64).unwrap_or(0);
    let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
    if !all && (kind.is_empty() || target == 0) {
        return Err(ApiError::msg("Choose what to apply."));
    }
    let verified = args.get("verified_only").and_then(Value::as_bool).unwrap_or(false);
    let rows = ctx.db.rows(
        "SELECT id FROM proposals WHERE user_id = ?1 AND status = 'pending' AND (?2 = '' OR kind = ?2) AND (?3 = 0 OR target_id = ?3)
           AND (?4 = 0 OR verified = 1) ORDER BY id",
        rusqlite::params![USER_ID, kind, target, verified as i64],
    )?;
    Ok(rows.iter().filter_map(|r| r["id"].as_i64()).collect())
}

fn apply(args: &Value, ctx: &Ctx) -> Result<Value> {
    let list = selected(args, ctx)?;
    let n = proposals::apply(ctx, &list)?;
    Ok(json!({ "applied": n, "pending": proposals::pending_count(&ctx.db) }))
}

fn dismiss(args: &Value, ctx: &Ctx) -> Result<Value> {
    let list = selected(args, ctx)?;
    let n = proposals::dismiss(ctx, &list)?;
    Ok(json!({ "dismissed": n, "pending": proposals::pending_count(&ctx.db) }))
}

fn venue_id(args: &Value, ctx: &Ctx) -> Result<(i64, String)> {
    let id = args.get("id").and_then(Value::as_i64).ok_or_else(|| ApiError::field("id", "The id field is required."))?;
    let name: String = ctx
        .db
        .scalar("SELECT name FROM venues WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    Ok((id, name))
}

/// Research one venue → `{ run }`. Result on `ai-run` (`partial` = `{ venue_id, name, ok, applied, pending, sources }`).
fn venue_research(args: &Value, ctx: &Ctx) -> Result<Value> {
    let (id, name) = venue_id(args, ctx)?;
    let run = Run::begin(ctx, "venues", &format!("Updating {name}"), 1, Some("#/venues"))?;
    let rid = run.id;
    ctx.spawn(move |ctx| venues::refresh_one(ctx, run, id));
    Ok(json!({ "run": rid }))
}

/// `{ ids?: [..], tab?: conference|journal }` → `{ run, total }`. Without ids: every venue of the tab.
fn venues_refresh_all(args: &Value, ctx: &Ctx) -> Result<Value> {
    let tab = if args.get("tab").and_then(Value::as_str) == Some("journal") { "journal" } else { "conference" };
    let mut list = ids(args, "ids");
    if list.is_empty() {
        list = ctx
            .db
            .rows("SELECT id FROM venues WHERE user_id = ?1 AND kind = ?2 ORDER BY id", rusqlite::params![USER_ID, tab])?
            .iter()
            .filter_map(|r| r["id"].as_i64())
            .collect();
    }
    let owned: Vec<i64> = list
        .into_iter()
        .filter(|id| {
            ctx.db.count("SELECT COUNT(*) FROM venues WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID]).unwrap_or(0) > 0
        })
        .collect();
    if owned.is_empty() {
        return Err(ApiError::msg("No journals or conferences to update."));
    }
    let label = if tab == "journal" { "Updating journals" } else { "Updating conferences" };
    let run = Run::begin(ctx, "venues", label, owned.len() as i64, Some("#/venues"))?;
    let rid = run.id;
    let total = owned.len();
    ctx.spawn(move |ctx| venues::refresh_many(ctx, run, owned, false));
    Ok(json!({ "run": rid, "total": total }))
}

/// Refresh one publication → `{ run }`. Result on `ai-run` (`partial` = `{ publication_id, title, before, after, applied, pending }`).
fn publication_refresh(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = args.get("id").and_then(Value::as_i64).ok_or_else(|| ApiError::field("id", "The id field is required."))?;
    let title: String = ctx
        .db
        .scalar("SELECT title FROM publications WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    let run = Run::begin(ctx, "citations", &format!("Updating {}", crate::research::limit(&title, 80)), 1, Some("#/publications"))?;
    let rid = run.id;
    ctx.spawn(move |ctx| async move {
        crate::research::fetch::init_cache(&ctx);
        match citations::refresh_publication(&ctx, &run, id).await {
            Ok(rep) => {
                run.advance(&ctx, None);
                run.finish(&ctx, None, Some(rep.to_json()));
            }
            Err(e) => run.finish(&ctx, Some(&e.to_string()), Some(json!({ "publication_id": id, "ok": false }))),
        }
        ctx.changed("publications");
        ctx.changed("proposals");
    });
    Ok(json!({ "run": rid }))
}

/// `{ ids?: [..] }` → `{ run, total }` (all publications when omitted). Notifies "Citations updated".
fn publications_refresh_all(args: &Value, ctx: &Ctx) -> Result<Value> {
    let mut list = ids(args, "ids");
    if list.is_empty() {
        list = citations::stale(ctx, None)?;
    }
    if list.is_empty() {
        return Err(ApiError::msg("No publications to update."));
    }
    let run = Run::begin(ctx, "citations", "Citation update", list.len() as i64, Some("#/publications"))?;
    let rid = run.id;
    let total = list.len();
    ctx.spawn(move |ctx| async move {
        citations::refresh_many(ctx, run, list, true).await;
    });
    Ok(json!({ "run": rid, "total": total }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;
    use crate::research::proposals::{record, Draft};

    #[test]
    fn review_flow() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let v = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "CHI", "kind": "conference", "proceedings": "ACM" })).unwrap();
        let d = |f: &str, val: Value, verified: bool| Draft {
            field: f.into(),
            proposed: val,
            source_url: None,
            source_name: "Website".into(),
            quote: None,
            verified,
            metric: false,
        };
        record(
            &ctx,
            "venue",
            v,
            vec![
                d("proceedings", json!("ACM Digital Library"), true),
                d("acceptance_rate", json!("25%"), false),
                d("fee", json!("None"), false),
            ],
            None,
        )
        .unwrap();
        let l = dispatch("proposals.list", &json!({}), &ctx).unwrap();
        assert_eq!(l["pending"], 3);
        assert_eq!(l["groups"][0]["target"], "CHI");
        assert_eq!(l["groups"][0]["items"][0]["field_label"], "Proceedings");
        assert!(dispatch("proposals.apply", &json!({}), &ctx).is_err());
        let r = dispatch("proposals.apply", &json!({ "kind": "venue", "target_id": v, "verified_only": true }), &ctx).unwrap();
        assert_eq!(r["applied"], 1);
        let first = l["items"][1]["id"].as_i64().unwrap();
        assert_eq!(dispatch("proposals.dismiss", &json!({ "ids": [first] }), &ctx).unwrap()["dismissed"], 1);
        assert_eq!(dispatch("proposals.apply", &json!({ "all": true }), &ctx).unwrap()["applied"], 1);
        assert_eq!(dispatch("proposals.count", &json!({}), &ctx).unwrap(), 0);
        assert!(dispatch("venues.research", &json!({ "id": 999 }), &ctx).is_err());
    }
}
