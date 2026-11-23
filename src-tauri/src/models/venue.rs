//! Port of app/Models/Venue.php.

use crate::util::parse_date;
use chrono::NaiveDate;
use serde_json::Value;

pub const KINDS: &[(&str, &str)] = &[("journal", "Journal"), ("conference", "Conference"), ("preprint", "Preprint server")];

pub const REVIEW_TYPES: &[(&str, &str)] =
    &[("double", "Double-anonymous"), ("single", "Single-anonymous"), ("open", "Open review"), ("none", "No peer review")];

pub const PREPRINT_POLICIES: &[(&str, &str)] =
    &[("allowed", "Preprints allowed"), ("after", "Only after decision"), ("discouraged", "Discouraged"), ("unknown", "Not checked")];

/// Days until the venue's deadline (negative when past), or `None` without one.
pub fn deadline_days(venue: &Value, today: NaiveDate) -> Option<i64> {
    let d = venue.get("deadline_on").and_then(Value::as_str).and_then(parse_date)?;
    Some((d - today).num_days())
}

pub fn is_anonymous(venue: &Value) -> bool {
    venue.get("review_type").and_then(Value::as_str) == Some("double")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn venue_helpers() {
        let today = parse_date("2026-11-22").unwrap();
        assert_eq!(deadline_days(&json!({ "deadline_on": "2026-12-01" }), today), Some(9));
        assert_eq!(deadline_days(&json!({ "deadline_on": "2026-11-20" }), today), Some(-2));
        assert_eq!(deadline_days(&json!({ "deadline_on": null }), today), None);
        assert!(is_anonymous(&json!({ "review_type": "double" })));
    }
}
