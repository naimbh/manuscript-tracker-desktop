//! Ports of the Eloquent model logic in app/Models. Rows are `serde_json::Value` objects from `db.rows`;
//! helpers are plain functions over them (plus a few that touch the database).

pub mod ai_run;
pub mod paper;
pub mod setting;
pub mod submission;
pub mod venue;

use crate::util::parse_date;
use chrono::NaiveDate;
use serde_json::Value;

/// Label for a key in a `(key, label)` table such as `paper::STAGES`.
pub fn label<'a>(table: &'a [(&'a str, &'a str)], key: &'a str) -> &'a str {
    table.iter().find(|(k, _)| *k == key).map(|(_, l)| *l).unwrap_or(key)
}

/// `"a,b,c"` from a `(key, label)` table, for `in:` validation rules.
pub fn keys(table: &[(&str, &str)]) -> String {
    table.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(",")
}

/// A `(key, label)` table as a JSON object (ordered) for the renderer.
pub fn to_json(table: &[(&str, &str)]) -> Value {
    Value::Object(table.iter().map(|(k, l)| ((*k).to_string(), Value::String((*l).to_string()))).collect())
}

pub mod author {
    /// CRediT contributor roles.
    pub const CREDIT: &[&str] = &[
        "Conceptualization",
        "Data curation",
        "Formal analysis",
        "Funding acquisition",
        "Investigation",
        "Methodology",
        "Project administration",
        "Resources",
        "Software",
        "Supervision",
        "Validation",
        "Visualization",
        "Writing – original draft",
        "Writing – review & editing",
    ];
}

pub mod opportunity {
    pub const KINDS: &[(&str, &str)] =
        &[("conference", "Conference"), ("special_issue", "Special issue"), ("journal", "Journal call"), ("workshop", "Workshop")];
    pub const STATUSES: &[&str] = &["new", "saved", "dismissed"];
}

pub mod scan {
    pub const STATUSES: &[&str] = &["running", "done", "failed"];
    pub const TRIGGERS: &[&str] = &["manual", "daily"];
}

pub mod deadline {
    use super::*;
    /// Days until `due_on` (negative when overdue).
    pub fn days_left(deadline: &Value, today: NaiveDate) -> Option<i64> {
        let d = deadline.get("due_on").and_then(Value::as_str).and_then(parse_date)?;
        Some((d - today).num_days())
    }
}

pub mod publication {
    use serde_json::Value;

    pub const KINDS: &[(&str, &str)] = &[
        ("article", "Journal article"),
        ("conference", "Conference paper"),
        ("letter", "Letter / short paper"),
        ("chapter", "Book chapter"),
        ("thesis", "Thesis"),
        ("preprint", "Preprint"),
    ];

    fn s<'a>(p: &'a Value, k: &str) -> &'a str {
        match p.get(k) {
            Some(Value::String(v)) => v.as_str(),
            _ => "",
        }
    }

    /// `Venue, 12(3), pp. 1-20`.
    pub fn citation(p: &Value) -> String {
        let num = |k: &str| match p.get(k) {
            Some(Value::Number(n)) => n.to_string(),
            _ => s(p, k).to_string(),
        };
        let volume = num("volume");
        let issue = num("issue");
        let vol = (!volume.is_empty()).then(|| if issue.is_empty() { volume.clone() } else { format!("{volume}({issue})") });
        let pages = num("pages");
        [Some(s(p, "venue").to_string()), vol, (!pages.is_empty()).then(|| format!("pp. {pages}"))]
            .into_iter()
            .flatten()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub mod paper_file {
    use serde_json::Value;

    pub const KINDS: &[(&str, &str)] = &[
        ("manuscript", "Manuscript"),
        ("anonymized", "Anonymized manuscript"),
        ("cover", "Cover letter"),
        ("response", "Response to reviewers"),
        ("proof", "Proof PDF"),
        ("supplementary", "Supplementary"),
    ];

    pub fn is_pdf(f: &Value) -> bool {
        f.get("mime").and_then(Value::as_str) == Some("application/pdf")
            || f.get("original_name").and_then(Value::as_str).is_some_and(|n| n.to_lowercase().ends_with(".pdf"))
    }

    /// `1.5 MB` / `12 KB`, like PaperFile::humanSize().
    pub fn human_size(bytes: i64) -> String {
        let fmt = |x: f64| {
            if x.fract() == 0.0 {
                format!("{}", x as i64)
            } else {
                format!("{x}")
            }
        };
        if bytes >= 1_048_576 {
            format!("{} MB", fmt((bytes as f64 / 1_048_576.0 * 10.0).round() / 10.0))
        } else {
            format!("{} KB", fmt((bytes as f64 / 1024.0).round().max(1.0)))
        }
    }
}

pub mod user {
    use serde_json::Value;

    /// Initials from the name: "Naim Bin Hasan" -> "NBH".
    pub fn initials(user: &Value) -> String {
        user.get("name").and_then(Value::as_str).unwrap_or("").split(' ').filter_map(|w| w.chars().next()).collect()
    }

    /// Columns never sent to the renderer.
    pub const HIDDEN: &[&str] = &["password", "remember_token"];
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn small_models() {
        assert_eq!(
            publication::citation(&json!({ "venue": "MISQ", "volume": "12", "issue": "3", "pages": "1-20" })),
            "MISQ, 12(3), pp. 1-20"
        );
        assert_eq!(publication::citation(&json!({ "venue": "MISQ", "volume": null, "pages": "" })), "MISQ");
        assert_eq!(paper_file::human_size(1_048_576), "1 MB");
        assert_eq!(paper_file::human_size(1_572_864), "1.5 MB");
        assert_eq!(paper_file::human_size(100), "1 KB");
        assert_eq!(paper_file::human_size(12_800), "13 KB");
        assert!(paper_file::is_pdf(&json!({ "original_name": "A.PDF" })));
        assert_eq!(user::initials(&json!({ "name": "Naim Bin Hasan" })), "NBH");
        assert_eq!(keys(paper::STAGES).split(',').count(), 9);
        assert_eq!(label(submission::DECISIONS, "pending"), "Under review");
    }
}
