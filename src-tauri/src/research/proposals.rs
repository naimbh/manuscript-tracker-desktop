//! Proposals: what a research run found, field by field, waiting for review.
//!
//! Rules (nothing the user entered is silently overwritten):
//! - A value equal to the current one is skipped; a value the user dismissed before is not proposed again.
//! - Verified values for EMPTY fields are applied at once when `research.autofill_empty` is on (default).
//! - Tracked metrics (`metric: true`, e.g. citation counts from an index) are applied when verified.
//! - Everything else is stored `pending` (one pending row per target and field; newer runs replace it).
//! - Unverified values are never applied automatically.

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::util::blank;
use serde_json::{json, Value};

pub const KINDS: &[(&str, &str)] = &[("venue", "Journal or conference"), ("publication", "Publication"), ("profile", "Research profile")];
pub const STATUSES: &[&str] = &["pending", "applied", "dismissed"];

/// Fields a venue proposal may write, with labels for the review screen.
pub const VENUE_FIELDS: &[(&str, &str)] = &[
    ("next_edition", "Next edition"),
    ("deadline_on", "Deadline"),
    ("deadline_label", "Deadline label"),
    ("deadline_expected", "Deadline is an estimate"),
    ("website_url", "Website"),
    ("acceptance_rate", "Acceptance rate"),
    ("first_decision", "First decision"),
    ("fee", "Fee"),
    ("word_limit", "Word limit"),
    ("review_type", "Review type"),
    ("proceedings", "Proceedings"),
    ("publisher", "Publisher"),
    ("portal_url", "Submission portal"),
    ("notes", "Notes"),
];

pub const PUBLICATION_FIELDS: &[(&str, &str)] = &[
    ("citations", "Citations"),
    ("doi", "DOI"),
    ("url", "Publisher page"),
    ("venue", "Journal or conference"),
    ("volume", "Volume"),
    ("issue", "Issue"),
    ("pages", "Pages"),
    ("year", "Year"),
    ("publisher", "Publisher"),
    ("scholar_url", "Scholar record"),
];

pub const PROFILE_FIELDS: &[(&str, &str)] = &[("scholar_stats", "Citation statistics")];

/// Fields applied together with `deadline_on`.
const DEADLINE_COMPANIONS: &[&str] = &["deadline_label", "deadline_expected"];

pub fn fields(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "venue" => VENUE_FIELDS,
        "publication" => PUBLICATION_FIELDS,
        "profile" => PROFILE_FIELDS,
        _ => &[],
    }
}

pub fn field_label(kind: &str, field: &str) -> &'static str {
    fields(kind).iter().find(|(k, _)| *k == field).map(|(_, l)| *l).unwrap_or("")
}

fn table(kind: &str) -> Result<&'static str> {
    match kind {
        "venue" => Ok("venues"),
        "publication" => Ok("publications"),
        "profile" => Ok("users"),
        _ => Err(ApiError::msg("Unknown proposal kind.")),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub field: String,
    pub proposed: Value,
    pub source_url: Option<String>,
    pub source_name: String,
    pub quote: Option<String>,
    pub verified: bool,
    /// A tracked number (citations): applied when verified, even over an existing value.
    pub metric: bool,
}

impl Draft {
    pub fn new(field: &str, proposed: Value, ev: &super::Evidence) -> Draft {
        Draft {
            field: field.to_string(),
            proposed,
            source_url: ev.source_url.clone(),
            source_name: ev.source_name.clone(),
            quote: ev.quote.clone(),
            verified: ev.verified,
            metric: false,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    pub applied: Vec<String>,
    pub pending: Vec<String>,
    pub skipped: usize,
}

/// Whether two field values mean the same thing (case, spacing, `true`/`1`, dates with times).
pub fn equivalent(a: &Value, b: &Value) -> bool {
    if blank(a) && blank(b) {
        return true;
    }
    let s = |v: &Value| match v {
        Value::Bool(x) => (if *x { "1" } else { "0" }).to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(t) => t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let (x, y) = (s(a), s(b));
    if x == y {
        return true;
    }
    // Dates stored with a time part.
    x.len() >= 10
        && y.len() >= 10
        && crate::util::parse_date(&x).is_some()
        && x.get(..10) == y.get(..10)
        && (x.len() == 10 || y.len() == 10)
}

/// JSON text for the `current`/`proposed` columns (declared JSON, so plain strings must be quoted).
fn as_json_text(v: &Value) -> Value {
    if v.is_null() {
        Value::Null
    } else {
        Value::String(v.to_string())
    }
}

/// Record what a run found for one target. Applies what may be applied, stores the rest as pending.
pub fn record(ctx: &Ctx, kind: &str, target_id: i64, drafts: Vec<Draft>, run_id: Option<i64>) -> Result<Outcome> {
    let tbl = table(kind)?;
    let autofill = super::flag(&ctx.db, "research.autofill_empty");
    let allowed = fields(kind);
    let mut out = Outcome::default();
    let row = ctx.db.find(tbl, target_id)?.ok_or_else(ApiError::not_found)?;
    let mut applied_deadline = false;
    // deadline_on first so its companions follow it.
    let mut drafts = drafts;
    drafts.sort_by_key(|d| if d.field == "deadline_on" { 0 } else { 1 });
    for d in drafts {
        if !allowed.iter().any(|(f, _)| *f == d.field) || blank(&d.proposed) {
            out.skipped += 1;
            continue;
        }
        let current = row.get(&d.field).cloned().unwrap_or(Value::Null);
        if equivalent(&current, &d.proposed) {
            out.skipped += 1;
            continue;
        }
        let dismissed = ctx.db.rows(
            "SELECT proposed FROM proposals WHERE kind = ?1 AND target_id = ?2 AND field = ?3 AND status = 'dismissed'",
            rusqlite::params![kind, target_id, d.field],
        )?;
        if dismissed.iter().any(|r| equivalent(&r["proposed"], &d.proposed)) {
            out.skipped += 1;
            continue;
        }
        let companion = DEADLINE_COMPANIONS.contains(&d.field.as_str()) && applied_deadline;
        let auto = d.verified && (d.metric || (autofill && (blank(&current) || companion)));
        let data = json!({
            "user_id": USER_ID, "kind": kind, "target_id": target_id, "field": d.field,
            "current": as_json_text(&current), "proposed": as_json_text(&d.proposed),
            "source_url": d.source_url, "source_name": d.source_name, "quote": d.quote, "verified": d.verified,
            "status": if auto { "applied" } else { "pending" }, "run_id": run_id,
        });
        ctx.db.tx(|tx| {
            // One pending row per field: a newer finding replaces the older one.
            tx.exec(
                "DELETE FROM proposals WHERE kind = ?1 AND target_id = ?2 AND field = ?3 AND status = 'pending'",
                rusqlite::params![kind, target_id, d.field],
            )?;
            tx.insert("proposals", &data)?;
            if auto {
                tx.update(tbl, target_id, &json!({ d.field.clone(): d.proposed }))?;
            }
            Ok(())
        })?;
        if auto {
            if d.field == "deadline_on" {
                applied_deadline = true;
            }
            out.applied.push(d.field);
        } else {
            out.pending.push(d.field);
        }
    }
    Ok(out)
}

/// Apply pending proposals by id (with `deadline_on` its label and estimate flag follow). Returns how many.
pub fn apply(ctx: &Ctx, ids: &[i64]) -> Result<usize> {
    let mut n = 0;
    let mut areas = std::collections::BTreeSet::new();
    for &id in ids {
        let Some(p) = ctx.db.first("SELECT * FROM proposals WHERE id = ?1 AND status = 'pending'", [id])? else {
            continue;
        };
        let kind = p["kind"].as_str().unwrap_or("").to_string();
        let field = p["field"].as_str().unwrap_or("").to_string();
        let target = p["target_id"].as_i64().unwrap_or(0);
        let tbl = table(&kind)?;
        if !fields(&kind).iter().any(|(f, _)| *f == field) {
            continue;
        }
        if ctx.db.find(tbl, target)?.is_none() {
            ctx.db.delete("proposals", id)?;
            continue;
        }
        let mut group = vec![p.clone()];
        if field == "deadline_on" {
            for c in DEADLINE_COMPANIONS {
                if let Some(cp) = ctx.db.first(
                    "SELECT * FROM proposals WHERE kind = ?1 AND target_id = ?2 AND field = ?3 AND status = 'pending'",
                    rusqlite::params![kind, target, c],
                )? {
                    group.push(cp);
                }
            }
        }
        ctx.db.tx(|tx| {
            for g in &group {
                let f = g["field"].as_str().unwrap_or("");
                tx.update(tbl, target, &json!({ f: g["proposed"].clone() }))?;
                tx.exec(
                    "UPDATE proposals SET status = 'applied', updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![crate::util::now_ts(), g["id"].as_i64()],
                )?;
            }
            Ok(())
        })?;
        n += group.len();
        areas.insert(match kind.as_str() {
            "venue" => "venues",
            "publication" => "publications",
            _ => "profile",
        });
    }
    for a in areas {
        ctx.changed(a);
    }
    if n > 0 {
        ctx.changed("proposals");
    }
    Ok(n)
}

/// Dismiss pending proposals (they will not be proposed again with the same value).
pub fn dismiss(ctx: &Ctx, ids: &[i64]) -> Result<usize> {
    let mut n = 0;
    for &id in ids {
        n += ctx.db.exec(
            "UPDATE proposals SET status = 'dismissed', updated_at = ?1 WHERE id = ?2 AND status = 'pending'",
            rusqlite::params![crate::util::now_ts(), id],
        )?;
    }
    if n > 0 {
        ctx.changed("proposals");
    }
    Ok(n)
}

/// Pending proposals whose target still exists.
pub fn pending_count(db: &crate::db::Db) -> i64 {
    db.count(
        "SELECT COUNT(*) FROM proposals p WHERE p.status = 'pending' AND p.field NOT IN ('rank', 'quartile', 'sjr', 'abs', 'abdc') AND (
            (p.kind = 'venue' AND EXISTS (SELECT 1 FROM venues v WHERE v.id = p.target_id)) OR
            (p.kind = 'publication' AND EXISTS (SELECT 1 FROM publications x WHERE x.id = p.target_id)) OR
            (p.kind = 'profile'))",
        [],
    )
    .unwrap_or(0)
}

/// Remove pending proposals whose venue or publication was deleted, and old pending ranking proposals
/// (quartile, rank, SJR, ABS, ABDC), which the app no longer computes or shows.
pub fn prune_orphans(db: &crate::db::Db) -> Result<usize> {
    db.exec(
        "DELETE FROM proposals WHERE (kind = 'venue' AND target_id NOT IN (SELECT id FROM venues))
            OR (kind = 'publication' AND target_id NOT IN (SELECT id FROM publications))
            OR (status = 'pending' AND field IN ('rank', 'quartile', 'sjr', 'abs', 'abdc'))",
        [],
    )
}

/// A proposal row decorated for the review screen.
pub fn decorate(db: &crate::db::Db, mut p: Value) -> Value {
    let kind = p["kind"].as_str().unwrap_or("").to_string();
    let field = p["field"].as_str().unwrap_or("").to_string();
    let target = p["target_id"].as_i64().unwrap_or(0);
    let name: Option<String> = match kind.as_str() {
        "venue" => db.scalar("SELECT name FROM venues WHERE id = ?1", [target]).ok().flatten(),
        "publication" => db.scalar("SELECT title FROM publications WHERE id = ?1", [target]).ok().flatten(),
        _ => db.scalar("SELECT name FROM users WHERE id = ?1", [target]).ok().flatten(),
    };
    if let Some(o) = p.as_object_mut() {
        o.insert("target".into(), json!(name));
        o.insert("field_label".into(), json!(field_label(&kind, &field)));
        o.insert("kind_label".into(), json!(crate::models::label(KINDS, &kind)));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(field: &str, v: Value, verified: bool) -> Draft {
        Draft {
            field: field.into(),
            proposed: v,
            source_url: Some("https://x.org".into()),
            source_name: "Website".into(),
            quote: Some("q".into()),
            verified,
            metric: false,
        }
    }

    #[test]
    fn autofill_rules() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let id = ctx
            .db
            .insert("venues", &json!({ "user_id": 1, "name": "CHI", "kind": "conference", "rank": "CORE A*", "fee": "None" }))
            .unwrap();
        let out = record(
            &ctx,
            "venue",
            id,
            vec![
                draft("deadline_on", json!("2027-01-21"), true),     // empty + verified -> applied
                draft("deadline_label", json!("Full papers"), true), // companion, empty -> applied
                draft("deadline_expected", json!(false), true),      // equal to current (false) -> skipped
                draft("rank", json!("CORE A"), true),                // rankings are never written -> skipped
                draft("acceptance_rate", json!("~25%"), false),      // unverified -> pending
                draft("fee", json!("APC US$ 3,000"), true),          // user value -> pending
                draft("name", json!("Hacked"), true),                // not allowed -> skipped
            ],
            None,
        )
        .unwrap();
        assert_eq!(out.applied, vec!["deadline_on", "deadline_label"]);
        assert_eq!(out.pending, vec!["acceptance_rate", "fee"]);
        assert_eq!(out.skipped, 3);
        let v = ctx.db.find("venues", id).unwrap().unwrap();
        assert_eq!(v["deadline_on"], "2027-01-21");
        assert_eq!(v["fee"], "None");
        assert_eq!(v["name"], "CHI");
        assert_eq!(pending_count(&ctx.db), 2);

        // A newer finding replaces the pending one; dismissing blocks the same value.
        record(&ctx, "venue", id, vec![draft("fee", json!("APC US$ 3,500"), true)], None).unwrap();
        let pend = ctx.db.rows("SELECT * FROM proposals WHERE field = 'fee' AND status = 'pending'", []).unwrap();
        assert_eq!(pend.len(), 1);
        assert_eq!(pend[0]["proposed"], "APC US$ 3,500");
        assert_eq!(pend[0]["current"], "None");
        dismiss(&ctx, &[pend[0]["id"].as_i64().unwrap()]).unwrap();
        let again = record(&ctx, "venue", id, vec![draft("fee", json!("APC US$ 3,500"), true)], None).unwrap();
        assert_eq!(again.skipped, 1);

        // Apply a pending proposal.
        let rate = ctx.db.first("SELECT id FROM proposals WHERE field = 'acceptance_rate'", []).unwrap().unwrap();
        assert_eq!(apply(&ctx, &[rate["id"].as_i64().unwrap()]).unwrap(), 1);
        assert_eq!(ctx.db.find("venues", id).unwrap().unwrap()["acceptance_rate"], "~25%");
        assert_eq!(pending_count(&ctx.db), 0);
    }

    #[test]
    fn autofill_off_and_metrics() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        super::super::set_flag(&ctx.db, "research.autofill_empty", false).unwrap();
        let id = ctx.db.insert("publications", &json!({ "user_id": 1, "title": "Paper", "citations": 4 })).unwrap();
        let mut cites = draft("citations", json!(9), true);
        cites.metric = true;
        let out = record(&ctx, "publication", id, vec![draft("doi", json!("10.1/x"), true), cites], None).unwrap();
        assert_eq!(out.applied, vec!["citations"]);
        assert_eq!(out.pending, vec!["doi"]);
        assert_eq!(ctx.db.find("publications", id).unwrap().unwrap()["citations"], 9);
        // Deadline companions follow deadline_on on apply.
        let vid = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "X", "kind": "conference" })).unwrap();
        record(
            &ctx,
            "venue",
            vid,
            vec![
                draft("deadline_on", json!("2027-03-01"), true),
                draft("deadline_label", json!("Papers"), true),
                draft("deadline_expected", json!(true), true),
            ],
            None,
        )
        .unwrap();
        let d = ctx.db.first("SELECT id FROM proposals WHERE field = 'deadline_on'", []).unwrap().unwrap();
        assert_eq!(apply(&ctx, &[d["id"].as_i64().unwrap()]).unwrap(), 3);
        let v = ctx.db.find("venues", vid).unwrap().unwrap();
        assert_eq!(v["deadline_expected"], true);
        assert_eq!(v["deadline_label"], "Papers");
        ctx.db.delete("venues", vid).unwrap();
        assert!(prune_orphans(&ctx.db).unwrap() > 0);
    }

    #[test]
    fn equivalence() {
        assert!(equivalent(&json!(null), &json!("  ")));
        assert!(equivalent(&json!(true), &json!(1)));
        assert!(equivalent(&json!("2027-01-21"), &json!("2027-01-21 00:00:00")));
        assert!(!equivalent(&json!("Q1"), &json!("Q2")));
    }
}
