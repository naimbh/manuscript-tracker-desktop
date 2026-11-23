//! `publications.*` handlers: port of resources/views/livewire/publications.blade.php (Published).
//!
//! Citation refreshes (`publications.refreshCitations`, `publications.refreshAll`) live in `api/proposals.rs`;
//! the Google Scholar profile (`research.saveScholar`, `research.refreshScholar`) in `api/research.rs`.

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{keys, label, publication, setting};
use crate::research::providers;
use crate::validation::Validator;
use serde_json::{json, Map, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "publications.list" => list(args, ctx),
        "publications.save" => save(args, ctx),
        "publications.delete" => delete(args, ctx),
        _ => return None,
    })
}

/// Scheduled jobs (`publications:refresh`) are registered by `research::register_jobs`.
pub fn register_jobs() {}

/// The editable columns (Publications::FIELDS in the web app, minus the journal rankings: the `quartile`, `sjr`,
/// `abs` and `abdc` columns stay in the database and in backups, but the app never shows, edits or computes them).
pub const FIELDS: &[&str] = &[
    "title",
    "authors",
    "kind",
    "venue",
    "volume",
    "issue",
    "pages",
    "year",
    "publisher",
    "citations",
    "scholar_url",
    "cites_url",
    "doi",
    "url",
    "notes",
];

/// Journal-ranking columns kept for backup compatibility only (never sent to the screen).
pub const RANKING_COLUMNS: &[&str] = &["quartile", "sjr", "abs", "abdc"];

const DAYS: &[&str] = &["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn id_of(args: &Value) -> Result<i64> {
    args.get("id")
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .ok_or_else(|| ApiError::field("id", "The id field is required."))
}

fn find(ctx: &Ctx, id: i64) -> Result<Value> {
    ctx.db
        .first("SELECT * FROM publications WHERE id = ?1 AND user_id = ?2", rusqlite::params![id, USER_ID])?
        .ok_or_else(ApiError::not_found)
}

/// h-index and i10-index from citation counts.
pub fn indices(cites: &[i64]) -> (i64, i64) {
    let mut c: Vec<i64> = cites.iter().copied().filter(|n| *n > 0).collect();
    c.sort_unstable_by(|a, b| b.cmp(a));
    let mut h = 0;
    for (i, n) in c.iter().enumerate() {
        if *n > i as i64 {
            h = i as i64 + 1;
        }
    }
    (h, c.iter().filter(|n| **n >= 10).count() as i64)
}

/// "Updated every Wednesday." (follows Settings → AI → automatic jobs).
fn schedule_text(ctx: &Ctx) -> String {
    if !setting::get_bool(&ctx.db, "schedule.citations_enabled") {
        return "Automatic updates are off.".into();
    }
    if setting::get_str(&ctx.db, "schedule.citations_frequency") == "daily" {
        return "Updated every day.".into();
    }
    let day = setting::get_i64(&ctx.db, "schedule.citations_day").and_then(|d| DAYS.get(d as usize)).copied().unwrap_or("week");
    format!("Updated every {day}.")
}

fn int(v: &Value) -> Option<i64> {
    v.as_i64()
}

/// `{ sort: year|cites }` → everything the page shows.
fn list(args: &Value, ctx: &Ctx) -> Result<Value> {
    let sort = if args.get("sort").and_then(Value::as_str) == Some("cites") { "cites" } else { "year" };
    let user = ctx.require_user()?;
    let mut pubs = ctx.db.rows(
        "SELECT p.*, COALESCE(x.n, 0) AS pending
         FROM publications p
         LEFT JOIN (SELECT target_id, COUNT(*) AS n FROM proposals WHERE kind = 'publication' AND status = 'pending' GROUP BY target_id) x ON x.target_id = p.id
         WHERE p.user_id = ?1 ORDER BY p.id",
        [USER_ID],
    )?;
    let cites: Vec<i64> = pubs.iter().filter_map(|p| int(&p["citations"])).collect();
    let (h, i10) = indices(&cites);
    let total: i64 = cites.iter().sum();
    let max = cites.iter().copied().max().unwrap_or(1).max(1);
    if sort == "cites" {
        pubs.sort_by(|a, b| {
            let ka = (int(&a["citations"]).unwrap_or(-1), int(&a["year"]).unwrap_or(0));
            let kb = (int(&b["citations"]).unwrap_or(-1), int(&b["year"]).unwrap_or(0));
            kb.cmp(&ka)
        });
    } else {
        pubs.sort_by(|a, b| {
            let ka = (int(&a["year"]).unwrap_or(0), int(&a["citations"]).unwrap_or(0));
            let kb = (int(&b["year"]).unwrap_or(0), int(&b["citations"]).unwrap_or(0));
            kb.cmp(&ka)
        });
    }
    let last_check = pubs.iter().filter_map(|p| p["checked_at"].as_str()).max().map(str::to_string);
    let pending_total: i64 = pubs.iter().filter_map(|p| p["pending"].as_i64()).sum();
    let profile_pending =
        ctx.db.count("SELECT COUNT(*) FROM proposals WHERE user_id = ?1 AND kind = 'profile' AND status = 'pending'", [USER_ID])?;
    let decorated: Vec<Value> = pubs
        .into_iter()
        .map(|mut p| {
            let citation = publication::citation(&p);
            let kind = p["kind"].as_str().unwrap_or("").to_string();
            if let Some(o) = p.as_object_mut() {
                o.insert("citation".into(), json!(citation));
                o.insert("kind_label".into(), json!(label(publication::KINDS, &kind)));
                for k in RANKING_COLUMNS {
                    o.remove(*k);
                }
            }
            p
        })
        .collect();
    let running: Option<i64> = ctx.db.scalar(
        "SELECT id FROM ai_runs WHERE user_id = ?1 AND kind = 'citations' AND status = 'running' ORDER BY id DESC LIMIT 1",
        [USER_ID],
    )?;
    Ok(json!({
        "sort": sort,
        "pubs": decorated,
        "total": total,
        "h": h,
        "i10": i10,
        "max": max,
        "lastCheck": last_check,
        "aiReady": providers::configured(&ctx.db),
        "aiStatus": providers::status(&ctx.db),
        "scholar": user["scholar_url"],
        "stats": user["scholar_stats"],
        "statsChecked": user["scholar_checked_at"],
        "userName": user["name"],
        "schedule": schedule_text(ctx),
        "pending": pending_total + profile_pending,
        "running": running,
    }))
}

fn validated(args: &Value) -> Result<Value> {
    let kinds = keys(publication::KINDS);
    let v = Validator::new(args)
        .rule("title", "required|string|max:500")
        .rule("authors", "nullable|string|max:255")
        .rule("kind", &format!("required|in:{kinds}"))
        .rule("venue", "nullable|string|max:255")
        .rule("volume", "nullable|string|max:40")
        .rule("issue", "nullable|string|max:40")
        .rule("pages", "nullable|string|max:40")
        .rule("year", "nullable|integer|min:1900|max:2100")
        .rule("publisher", "nullable|string|max:255")
        .rule("citations", "nullable|integer|min:0")
        .rule("scholar_url", "nullable|url|max:500")
        .rule("cites_url", "nullable|url|max:500")
        .rule("doi", "nullable|string|max:255")
        .rule("url", "nullable|url|max:500")
        .rule("notes", "nullable|string|max:5000")
        .validate()?;
    let mut data = Map::new();
    for f in FIELDS {
        let val = match *f {
            "year" | "citations" => json!(v.i64(f)),
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

/// Add (`id` absent) or update a publication → the row.
fn save(args: &Value, ctx: &Ctx) -> Result<Value> {
    let mut data = validated(args)?;
    let editing = args.get("id").filter(|v| !v.is_null()).map(|_| id_of(args)).transpose()?;
    let id = match editing {
        Some(id) => {
            find(ctx, id)?;
            ctx.db.update("publications", id, &data)?;
            id
        }
        None => {
            data["user_id"] = json!(USER_ID);
            ctx.db.insert("publications", &data)?
        }
    };
    find(ctx, id)
}

fn delete(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = id_of(args)?;
    find(ctx, id)?;
    ctx.db.tx(|tx| {
        tx.exec("DELETE FROM proposals WHERE kind = 'publication' AND target_id = ?1", [id])?;
        tx.delete("publications", id)?;
        Ok(())
    })?;
    Ok(json!({ "deleted": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;

    fn errors(r: Result<Value>) -> Value {
        serde_json::to_value(r.unwrap_err()).unwrap()["errors"].clone()
    }

    #[test]
    fn indices_match_scholar() {
        assert_eq!(indices(&[10, 8, 5, 4, 3]), (4, 1));
        assert_eq!(indices(&[]), (0, 0));
        assert_eq!(indices(&[0, 0, 25]), (1, 1));
    }

    #[test]
    fn crud_and_list() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let mk = |title: &str, year: &str, cites: Value, q: &str| {
            dispatch(
                "publications.save",
                &json!({ "title": title, "kind": "article", "year": year, "citations": cites, "quartile": q,
                "venue": "MISQ", "volume": "12", "issue": "3", "pages": "1-20", "authors": "A", "doi": "" }),
                &ctx,
            )
            .unwrap()
        };
        let a = mk("Old", "2019", json!("12"), "Q1");
        mk("New", "2024", json!(3), "");
        mk("Mid", "2021", json!(null), "Q2");
        assert_eq!(a["year"], 2019);
        assert_eq!(a["citations"], 12);
        assert!(a["doi"].is_null());

        let l = dispatch("publications.list", &json!({}), &ctx).unwrap();
        let titles: Vec<&str> = l["pubs"].as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["New", "Mid", "Old"]);
        assert_eq!(l["pubs"][0]["citation"], "MISQ, 12(3), pp. 1-20");
        assert_eq!(l["pubs"][0]["kind_label"], "Journal article");
        assert_eq!(l["total"], 15);
        assert_eq!(l["h"], 2);
        assert_eq!(l["i10"], 1);
        assert_eq!(l["max"], 12);
        assert!(l.get("ranks").is_none());
        assert!(l["pubs"][0].get("quartile").is_none());
        // Rankings sent by an old client are ignored; the column is left as it was.
        assert!(ctx.db.find("publications", a["id"].as_i64().unwrap()).unwrap().unwrap()["quartile"].is_null());
        assert_eq!(l["schedule"], "Updated every Wednesday.");

        let l = dispatch("publications.list", &json!({ "sort": "cites" }), &ctx).unwrap();
        let titles: Vec<&str> = l["pubs"].as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Old", "New", "Mid"]);

        let id = a["id"].as_i64().unwrap();
        let upd = dispatch("publications.save", &json!({ "id": id, "title": "Old (revised)", "kind": "chapter" }), &ctx).unwrap();
        assert_eq!(upd["title"], "Old (revised)");
        assert!(upd["year"].is_null());

        ctx.db
            .insert("proposals", &json!({ "user_id": 1, "kind": "publication", "target_id": id, "field": "doi", "proposed": "10.1/x" }))
            .unwrap();
        assert_eq!(dispatch("publications.list", &json!({}), &ctx).unwrap()["pending"], 1);
        dispatch("publications.delete", &json!({ "id": id }), &ctx).unwrap();
        assert!(ctx.db.find("publications", id).unwrap().is_none());
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM proposals", []).unwrap(), 0);
        assert!(dispatch("publications.delete", &json!({ "id": id }), &ctx).is_err());
    }

    #[test]
    fn validation() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let e = errors(dispatch("publications.save", &json!({ "title": "", "kind": "book" }), &ctx));
        assert_eq!(e["title"][0], "The title field is required.");
        assert_eq!(e["kind"][0], "The selected kind is invalid.");
        let e = errors(dispatch(
            "publications.save",
            &json!({ "title": "T", "kind": "article", "year": "1800", "citations": "-1", "url": "x", "volume": "x".repeat(41) }),
            &ctx,
        ));
        assert_eq!(e["year"][0], "The year field must be at least 1900.");
        assert_eq!(e["citations"][0], "The citations field must be at least 0.");
        assert_eq!(e["url"][0], "The url field must be a valid URL.");
        assert_eq!(e["volume"][0], "The volume field must not be greater than 40 characters.");
    }
}
