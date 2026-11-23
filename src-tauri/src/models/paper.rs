//! Port of app/Models/Paper.php.

use crate::db::{Db, Query};
use crate::error::Result;
use crate::models::{submission, venue};
use crate::util::today_str;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const STAGES: &[(&str, &str)] = &[
    ("idea", "Idea"),
    ("drafting", "Drafting"),
    ("ready", "Ready to submit"),
    ("submitted", "Submitted"),
    ("under_review", "Under review"),
    ("revision", "Revising"),
    ("accepted", "Accepted"),
    ("published", "Published"),
    ("shelved", "Shelved"),
];

pub fn stage_label(stage: &str) -> &str {
    STAGES.iter().find(|(k, _)| *k == stage).map(|(_, l)| *l).unwrap_or(stage)
}

/// `"idea,drafting,..."` for `in:` validation rules.
pub fn stage_keys() -> String {
    STAGES.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(",")
}

/// Insert a paper for the local user; `started_on` defaults to today (Paper::creating).
pub fn insert(conn: &Connection, data: &Value) -> Result<i64> {
    let mut data = data.clone();
    if let Some(o) = data.as_object_mut() {
        o.insert("user_id".into(), json!(crate::db::USER_ID));
        if o.get("started_on").is_none_or(Value::is_null) {
            o.insert("started_on".into(), json!(today_str()));
        }
    }
    conn.insert("papers", &data)
}

/// `short_title` if set, otherwise `title`.
pub fn display_title(paper: &Value) -> String {
    let s = paper.get("short_title").and_then(Value::as_str).unwrap_or("");
    if !s.is_empty() {
        s.to_string()
    } else {
        paper.get("title").and_then(Value::as_str).unwrap_or("").to_string()
    }
}

/// The ordering behind "current submission": one marked current wins, then the latest `submitted_on`
/// (NULLs last), then the highest id. Same as `ofMany(['is_current' => 'max', 'submitted_on' => 'max', 'id' => 'max'])`.
pub const CURRENT_ORDER: &str = "is_current DESC, submitted_on DESC, id DESC";

/// A CTE exposing every paper's current submission as `current_subs` (one row per paper, plus `rn`):
/// `format!("WITH {CURRENT_SUBMISSIONS_CTE} SELECT p.*, cs.decision FROM papers p LEFT JOIN current_subs cs ON cs.paper_id = p.id")`.
pub const CURRENT_SUBMISSIONS_CTE: &str = "current_subs AS (SELECT * FROM (SELECT s.*, ROW_NUMBER() OVER (PARTITION BY s.paper_id ORDER BY s.is_current DESC, s.submitted_on DESC, s.id DESC) AS rn FROM submissions s) WHERE rn = 1)";

/// The paper's current submission row, if any.
pub fn current_submission(conn: &Connection, paper_id: i64) -> Result<Option<Value>> {
    conn.first(&format!("SELECT * FROM submissions WHERE paper_id = ?1 ORDER BY {CURRENT_ORDER} LIMIT 1"), [paper_id])
}

/// Current submissions for many papers at once (paper_id -> submission row). One query.
pub fn current_submissions(conn: &Connection) -> Result<HashMap<i64, Value>> {
    let rows = conn.rows(&format!("WITH {CURRENT_SUBMISSIONS_CTE} SELECT * FROM current_subs"), [])?;
    Ok(rows
        .into_iter()
        .filter_map(|mut r| {
            r.as_object_mut().map(|o| o.remove("rn"));
            Some((r.get("paper_id")?.as_i64()?, r))
        })
        .collect())
}

/// A public preprint while an anonymous review is pending: returns the submission at risk.
/// `venue` is the current submission's venue row (or `None`).
pub fn anonymity_risk<'a>(paper: &Value, current: Option<&'a Value>, venue: Option<&Value>) -> Option<&'a Value> {
    let preprint = paper.get("preprint_url").and_then(Value::as_str).unwrap_or("");
    if preprint.is_empty() {
        return None;
    }
    let s = current?;
    (submission::is_active(s) && venue.is_some_and(venue::is_anonymous)).then_some(s)
}

/// [`anonymity_risk`] loading what it needs from the database.
pub fn anonymity_risk_for(conn: &Connection, paper: &Value) -> Result<Option<Value>> {
    let Some(id) = paper.get("id").and_then(Value::as_i64) else {
        return Ok(None);
    };
    let current = current_submission(conn, id)?;
    let venue = match current.as_ref().and_then(|s| s.get("venue_id")).and_then(Value::as_i64) {
        Some(vid) => conn.find("venues", vid)?,
        None => None,
    };
    Ok(anonymity_risk(paper, current.as_ref(), venue.as_ref()).cloned())
}

/// Rejected or withdrawn last time, with nothing under review now.
pub fn needs_next_venue(paper: &Value, current: Option<&Value>) -> bool {
    let stage = paper.get("stage").and_then(Value::as_str).unwrap_or("");
    current.is_some_and(submission::is_rejection) && !matches!(stage, "accepted" | "published" | "shelved")
}

/// Submit to the first venue in the queue, today. Returns the new submission, or `None` if the queue is empty.
pub fn submit_to_next(db: &Db, paper_id: i64) -> Result<Option<Value>> {
    db.tx(|tx| {
        let Some(next) = tx.first(
            "SELECT q.id, q.venue_id, v.name, v.portal_url FROM queue_items q JOIN venues v ON v.id = q.venue_id
             WHERE q.paper_id = ?1 ORDER BY q.position, q.id LIMIT 1",
            [paper_id],
        )?
        else {
            return Ok(None);
        };
        tx.update_where("submissions", &json!({ "is_current": false }), "paper_id = ?", &[json!(paper_id)])?;
        let id = tx.insert(
            "submissions",
            &json!({
                "paper_id": paper_id,
                "venue_id": next["venue_id"],
                "venue_name": next["name"],
                "portal_url": next["portal_url"],
                "submitted_on": today_str(),
                "decision": "submitted",
            }),
        )?;
        tx.delete("queue_items", next["id"].as_i64().unwrap_or(0))?;
        tx.update("papers", paper_id, &json!({ "stage": "submitted" }))?;
        tx.find("submissions", id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn current_submission_rule() {
        let db = test_db();
        let p = db.insert("papers", &json!({ "user_id": 1, "title": "Long title", "short_title": "" })).unwrap();
        let a = db
            .insert("submissions", &json!({ "paper_id": p, "venue_name": "A", "submitted_on": "2026-01-01", "decision": "reject" }))
            .unwrap();
        let b = db
            .insert("submissions", &json!({ "paper_id": p, "venue_name": "B", "submitted_on": "2026-05-01", "decision": "pending" }))
            .unwrap();
        let c = db.insert("submissions", &json!({ "paper_id": p, "venue_name": "C", "submitted_on": null })).unwrap();
        let conn = db.conn();
        // Latest submitted_on wins; NULL dates sort last.
        assert_eq!(current_submission(&conn, p).unwrap().unwrap()["id"], b);
        // Same date: highest id wins.
        let d = conn.insert("submissions", &json!({ "paper_id": p, "venue_name": "D", "submitted_on": "2026-05-01" })).unwrap();
        assert_eq!(current_submission(&conn, p).unwrap().unwrap()["id"], d);
        // is_current beats everything.
        conn.update("submissions", a, &json!({ "is_current": true })).unwrap();
        assert_eq!(current_submission(&conn, p).unwrap().unwrap()["id"], a);
        let all = current_submissions(&conn).unwrap();
        assert_eq!(all[&p]["id"], a);
        assert!(all[&p].get("rn").is_none());
        let _ = c;

        let paper = conn.find("papers", p).unwrap().unwrap();
        assert_eq!(display_title(&paper), "Long title");
        let cur = current_submission(&conn, p).unwrap();
        assert!(needs_next_venue(&paper, cur.as_ref()));
    }

    #[test]
    fn anonymity_and_submit_to_next() {
        let db = test_db();
        let v = db
            .insert("venues", &json!({ "user_id": 1, "name": "J1", "review_type": "double", "portal_url": "https://j1.example" }))
            .unwrap();
        let p = db.insert("papers", &json!({ "user_id": 1, "title": "T", "preprint_url": "https://arxiv.org/abs/1" })).unwrap();
        db.insert("queue_items", &json!({ "paper_id": p, "venue_id": v, "position": 0 })).unwrap();
        let s = submit_to_next(&db, p).unwrap().unwrap();
        assert_eq!(s["venue_name"], "J1");
        assert_eq!(s["decision"], "submitted");
        assert_eq!(db.count("SELECT COUNT(*) FROM queue_items", []).unwrap(), 0);
        let paper = db.find("papers", p).unwrap().unwrap();
        assert_eq!(paper["stage"], "submitted");
        let risk = anonymity_risk_for(&db.conn(), &paper).unwrap();
        assert_eq!(risk.unwrap()["id"], s["id"]);
        assert!(submit_to_next(&db, p).unwrap().is_none());
    }
}
