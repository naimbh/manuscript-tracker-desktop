//! Port of app/Models/Submission.php. Functions take submission rows (JSON objects from `db.rows`).

use crate::util::{parse_date, DATE_FORMAT};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};

pub const DECISIONS: &[(&str, &str)] = &[
    ("submitted", "Submitted"),
    ("with_editor", "With editor"),
    ("pending", "Under review"),
    ("awaiting_decision", "Awaiting decision"),
    ("returned", "Returned to author"),
    ("desk_reject", "Desk rejected"),
    ("reject", "Rejected"),
    ("major", "Major revision"),
    ("minor", "Minor revision"),
    ("accept", "Accepted"),
    ("withdrawn", "Withdrawn"),
];

/// Statuses where the paper is with the journal and no decision has been made yet.
pub const WAITING: &[&str] = &["submitted", "with_editor", "pending", "awaiting_decision", "returned"];

pub fn decision_label(decision: &str) -> &str {
    DECISIONS.iter().find(|(k, _)| *k == decision).map(|(_, l)| *l).unwrap_or(decision)
}

/// `"submitted,with_editor,..."` for `in:` validation rules.
pub fn decision_keys() -> String {
    DECISIONS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(",")
}

/// The paper stage that matches a submission status.
pub fn stage_for(decision: &str) -> &'static str {
    match decision {
        "submitted" | "with_editor" | "returned" => "submitted",
        "pending" | "awaiting_decision" => "under_review",
        "major" | "minor" => "revision",
        "accept" => "accepted",
        _ => "ready",
    }
}

fn decision(s: &Value) -> &str {
    s.get("decision").and_then(Value::as_str).unwrap_or("")
}

fn date(s: &Value, k: &str) -> Option<NaiveDate> {
    s.get(k).and_then(Value::as_str).and_then(parse_date)
}

pub fn is_waiting_decision(d: &str) -> bool {
    WAITING.contains(&d)
}

pub fn is_waiting(s: &Value) -> bool {
    is_waiting_decision(decision(s))
}

pub fn is_active(s: &Value) -> bool {
    is_waiting(s) || matches!(decision(s), "major" | "minor")
}

pub fn is_rejection(s: &Value) -> bool {
    matches!(decision(s), "desk_reject" | "reject" | "withdrawn")
}

/// The day the review clock starts: `status_on` while under review (if known), otherwise `submitted_on`.
fn clock_start(s: &Value) -> Option<NaiveDate> {
    let submitted = date(s, "submitted_on")?;
    if decision(s) == "pending" {
        if let Some(st) = date(s, "status_on") {
            return Some(st);
        }
    }
    Some(submitted)
}

/// Days with the journal so far (or until the decision). `today` is usually `util::today()`.
pub fn days_in_review(s: &Value, today: NaiveDate) -> Option<i64> {
    date(s, "submitted_on")?;
    let end = if is_waiting(s) { today } else { date(s, "decision_on").unwrap_or(today) };
    let start = clock_start(s)?;
    Some((end - start).num_days())
}

/// Fraction of the venue's typical decision time used so far, or `None`.
/// `typical_days` is the venue's `decision_days`.
pub fn clock(s: &Value, typical_days: Option<i64>, today: NaiveDate) -> Option<Value> {
    let days = days_in_review(s, today)?;
    let typical = typical_days.filter(|t| *t > 0)?;
    if !is_waiting(s) {
        return None;
    }
    let start = clock_start(s)?;
    let pct = ((days as f64 / typical as f64) * 100.0).round() as i64;
    Some(json!({
        "days": days,
        "typical": typical,
        "pct": pct.min(100),
        "overdue": days > typical,
        "expected_on": (start + Duration::days(typical)).format(DATE_FORMAT).to_string(),
    }))
}

/// Badge colour for a decision.
pub fn tone(decision: &str) -> &'static str {
    match decision {
        "accept" => "green",
        "minor" | "major" => "amber",
        "desk_reject" | "reject" => "red",
        "withdrawn" => "zinc",
        "submitted" => "indigo",
        "with_editor" => "fuchsia",
        "awaiting_decision" => "teal",
        "returned" => "orange",
        _ => "blue",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn stage_for_maps_every_decision() {
        assert_eq!(stage_for("submitted"), "submitted");
        assert_eq!(stage_for("with_editor"), "submitted");
        assert_eq!(stage_for("returned"), "submitted");
        assert_eq!(stage_for("pending"), "under_review");
        assert_eq!(stage_for("awaiting_decision"), "under_review");
        assert_eq!(stage_for("major"), "revision");
        assert_eq!(stage_for("minor"), "revision");
        assert_eq!(stage_for("accept"), "accepted");
        assert_eq!(stage_for("reject"), "ready");
        assert_eq!(stage_for("withdrawn"), "ready");
        assert_eq!(tone("with_editor"), "fuchsia");
        assert_eq!(tone("pending"), "blue");
    }

    #[test]
    fn clock_and_days() {
        let today = d("2026-11-22");
        let s = json!({ "decision": "pending", "submitted_on": "2026-09-01", "status_on": "2026-09-23" });
        assert_eq!(days_in_review(&s, today), Some(60));
        let c = clock(&s, Some(90), today).unwrap();
        assert_eq!(c["days"], 60);
        assert_eq!(c["pct"], 67);
        assert_eq!(c["overdue"], false);
        assert_eq!(c["expected_on"], "2026-12-22");

        let s = json!({ "decision": "submitted", "submitted_on": "2026-08-01", "status_on": "2026-09-23" });
        let c = clock(&s, Some(30), today).unwrap();
        assert_eq!(c["days"], 113);
        assert_eq!(c["pct"], 100);
        assert_eq!(c["overdue"], true);

        let s = json!({ "decision": "reject", "submitted_on": "2026-08-01", "decision_on": "2026-08-11" });
        assert_eq!(days_in_review(&s, today), Some(10));
        assert!(clock(&s, Some(30), today).is_none());
        assert!(clock(&json!({ "decision": "pending" }), Some(30), today).is_none());
        assert!(clock(&json!({ "decision": "pending", "submitted_on": "2026-11-01" }), None, today).is_none());
        assert!(is_active(&json!({ "decision": "major" })));
        assert!(is_rejection(&json!({ "decision": "desk_reject" })));
    }
}
