//! The research engine: venue research, opportunity scans, citation and Google Scholar refreshes.
//!
//! Fetch first, AI second. Rust fetches the sources itself ([`fetch`]), turns them into readable text
//! ([`extract`]), takes what free scholarly APIs already know (OpenAlex, Crossref, Semantic Scholar, WikiCFP),
//! and only then asks the configured AI ([`providers`]) to extract the remaining fields from the fetched text.
//! Every field carries its source URL and a short quote; fields whose quote cannot be found on the source page
//! are marked unverified and are never applied automatically. Results become [`proposals`] that the user
//! reviews, except verified values for empty fields (setting `research.autofill_empty`, on by default) and
//! tracked metrics (citation counts, Scholar statistics).
//!
//! Long jobs run as [`runs::Run`]s: an `ai_runs` row plus live `ai-run` events, cancellable with `ai.cancel`.

pub mod citations;
pub mod extract;
pub mod fetch;
pub mod opportunities;
pub mod proposals;
pub mod providers;
pub mod runs;
pub mod scholar;
pub mod venues;

use crate::db::Db;
use crate::models::setting;
use serde_json::{json, Value};

/// Schema change for the research engine (appended to `db::MIGRATIONS`).
pub const MIGRATION: &str = r#"
CREATE TABLE proposals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL DEFAULT 1 REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    field TEXT NOT NULL,
    current JSON NULL,
    proposed JSON NULL,
    source_url TEXT NULL,
    source_name TEXT NULL,
    quote TEXT NULL,
    verified BOOLEAN NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    run_id INTEGER NULL REFERENCES ai_runs (id) ON DELETE SET NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX proposals_kind_target_id_status_index ON proposals (kind, target_id, status);
CREATE INDEX proposals_status_index ON proposals (status);
CREATE INDEX proposals_run_id_index ON proposals (run_id);
CREATE INDEX proposals_user_id_index ON proposals (user_id);
ALTER TABLE ai_runs ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_runs ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_runs ADD COLUMN duration_ms INTEGER NULL;
"#;

/// Register the scheduled jobs (`opportunities:scan`, `venues:refresh`, `publications:refresh`). Called at
/// startup after the area modules' `register_jobs`, so these definitions win.
pub fn register_jobs() {
    crate::scheduler::register_job("opportunities:scan", opportunities::scheduled_scan);
    crate::scheduler::register_job("venues:refresh", venues::scheduled_refresh);
    crate::scheduler::register_job("publications:refresh", citations::scheduled_refresh);
}

/// Research settings that are not in `setting::DEFAULTS` (stored like any other setting).
pub const SETTINGS: &[(&str, bool)] = &[
    // Verified values for empty fields are filled in without review.
    ("research.autofill_empty", true),
    // Ask the provider's own web search when the fetched pages lack an answer.
    ("research.web_fallback", true),
];

/// A boolean research setting with its default.
pub fn flag(db: &Db, key: &str) -> bool {
    let default = SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, d)| *d).unwrap_or(false);
    if db.count("SELECT COUNT(*) FROM settings WHERE key = ?1", [key]).unwrap_or(0) == 0 {
        return default;
    }
    let saved: Option<String> = db.scalar("SELECT value FROM settings WHERE key = ?1", [key]).ok().flatten();
    matches!(saved.as_deref(), Some("1" | "true" | "on" | "yes"))
}

/// Save a boolean research setting.
pub fn set_flag(db: &Db, key: &str, on: bool) -> crate::error::Result<()> {
    setting::put(db, key, Some(if on { "1" } else { "0" }))
}

/// Lower-case, ASCII letters and digits only, single spaces: for matching titles.
pub fn norm_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        } else {
            space = true;
        }
    }
    out
}

/// Sørensen–Dice similarity of character bigrams of the normalised strings (0.0–1.0).
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (norm_title(a), norm_title(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let grams = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.chars().filter(|c| *c != ' ').collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (ga, mut gb) = (grams(&a), grams(&b));
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let total = (ga.len() + gb.len()) as f64;
    let mut hits = 0usize;
    for g in &ga {
        if let Some(i) = gb.iter().position(|x| x == g) {
            gb.swap_remove(i);
            hits += 1;
        }
    }
    2.0 * hits as f64 / total
}

/// First `max` characters.
pub fn limit(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A trimmed, non-empty string from a JSON value, cut to `max` characters.
pub fn clean_str(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?.trim();
    (!s.is_empty()).then(|| limit(s, max))
}

/// An http(s) URL of at most `max` characters.
pub fn clean_url(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?.trim();
    let ok = url::Url::parse(s).ok().is_some_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some());
    (ok && s.chars().count() <= max).then(|| s.to_string())
}

/// Normalise text for quote matching: lower case, unified quotes and dashes, single spaces.
pub fn norm_quote(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '`' | '\u{00B4}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            '\u{00A0}' | '\u{2009}' | '\u{202F}' => ' ',
            c => c,
        })
        .flat_map(char::to_lowercase)
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `quote` occurs in `text` (normalised). Long quotes match on their first and last 60 characters, and
/// quotes that span a table row (`|` separators) or a line break match with the separators ignored.
pub fn quote_in(quote: &str, text: &str) -> bool {
    let q = norm_quote(quote.trim_matches(|c: char| c == '"' || c == '\u{201C}' || c == '\u{201D}' || c.is_whitespace()));
    if q.chars().count() < 4 {
        return false;
    }
    let t = norm_quote(text);
    if t.contains(&q) {
        return true;
    }
    let strip = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    let (qs, ts) = (strip(&q), strip(&t));
    if qs.len() >= 4 && ts.contains(&qs) {
        return true;
    }
    let n = qs.chars().count();
    if n > 140 {
        let head: String = qs.chars().take(60).collect();
        let tail: String = qs.chars().skip(n - 60).collect();
        return ts.contains(&head) && ts.contains(&tail);
    }
    false
}

/// JSON Schema for one evidence-backed field: `{ value, source_url, quote }`.
pub fn evidence_schema(value_type: &str, description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "value": { "type": [value_type, "null"], "description": description },
            "source_url": { "type": ["string", "null"], "description": "URL of the page the value was read on." },
            "quote": { "type": ["string", "null"], "description": "Short exact quote (under 200 characters) from that page that shows the value." }
        },
        "required": ["value", "source_url", "quote"],
        "additionalProperties": false
    })
}

/// Object schema with every property required and no extras (strict mode on every provider).
pub fn object_schema(props: Vec<(&str, Value)>) -> Value {
    let required: Vec<&str> = props.iter().map(|(k, _)| *k).collect();
    let mut map = serde_json::Map::new();
    for (k, v) in props {
        map.insert(k.to_string(), v);
    }
    json!({ "type": "object", "properties": map, "required": required, "additionalProperties": false })
}

/// A field read from a source, before it becomes a proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub value: Value,
    pub source_url: Option<String>,
    pub quote: Option<String>,
    /// "OpenAlex", "WikiCFP", "Website", "AI web search", ...
    pub source_name: String,
    pub verified: bool,
}

/// Read `{ value, source_url, quote }` from a model answer.
pub fn evidence_from(v: &Value, source_name: &str) -> Option<Evidence> {
    let value = v.get("value").cloned().unwrap_or(Value::Null);
    if crate::util::blank(&value) {
        return None;
    }
    Some(Evidence {
        value,
        source_url: v.get("source_url").and_then(|u| clean_url(u, 500)),
        quote: v.get("quote").and_then(|q| clean_str(q, 300)),
        source_name: source_name.to_string(),
        verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_similarity() {
        assert_eq!(norm_title("  Platform Labor: the Gig—Economy! "), "platform labor the gig economy");
        assert!(similarity("Platform labor in the Global South", "Platform Labour in the Global South") > 0.9);
        assert!(similarity("CHI 2027", "Information Systems Research") < 0.3);
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn quotes_match_loosely() {
        let page = "Important dates\nSubmission Deadline | Jan 21, 2027\nPapers are due “21 January 2027”.";
        assert!(quote_in("Submission Deadline Jan 21, 2027", page));
        assert!(quote_in("Papers are due \"21 January 2027\"", page));
        assert!(!quote_in("Deadline: 3 March 2027", page));
        assert!(!quote_in("a", page));
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean_url(&json!("https://chi2027.acm.org/"), 255).as_deref(), Some("https://chi2027.acm.org/"));
        assert!(clean_url(&json!("javascript:alert(1)"), 255).is_none());
        assert!(clean_url(&json!("ftp://x.org"), 255).is_none());
        assert_eq!(clean_str(&json!("  ab  "), 1).as_deref(), Some("a"));
        assert!(evidence_from(&json!({ "value": null, "source_url": null, "quote": null }), "x").is_none());
        let e = evidence_from(&json!({ "value": "Q1", "source_url": "https://x.org/a", "quote": "Q1" }), "Website").unwrap();
        assert_eq!(e.source_url.as_deref(), Some("https://x.org/a"));
    }

    /// Live check against OpenAlex, Crossref and WikiCFP (no keys): `cargo test live_sources -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_sources() {
        use super::fetch::{fetcher, Opts};
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let v = fetcher().json("https://api.openalex.org/sources?search=Information%20Systems%20Journal&per-page=5").await.unwrap();
            let s = venues::pick_source(&v, "Information Systems Journal").expect("OpenAlex source");
            println!("OpenAlex source: {} / {:?} / {:?} / APC {:?}", s.name, s.publisher, s.homepage, s.apc_usd);
            assert!(s.publisher.is_some());

            let w = fetcher().json("https://api.openalex.org/works/doi:10.1145/3290605.3300233").await.unwrap();
            let work = citations::openalex_work(&w).expect("OpenAlex work");
            println!("OpenAlex work: {:?} cited by {:?}", work.title, work.citations);
            let c = fetcher().json("https://api.crossref.org/works/10.1145%2F3290605.3300233").await.unwrap();
            let cr = c.get("message").and_then(citations::crossref_work).expect("Crossref work");
            println!("Crossref: {:?} {:?} pages {:?} cited by {:?}", cr.venue, cr.publisher, cr.pages, cr.citations);

            let feed = fetcher().get("http://www.wikicfp.com/cfp/rss?cat=hci", &Opts::FEED).await.unwrap();
            let items = opportunities::feed_candidates(&feed.body, "hci");
            println!("WikiCFP RSS: {} items, first: {}", items.len(), items.first().map(|i| i.title.as_str()).unwrap_or(""));
            assert!(!items.is_empty());
            let page = fetcher().page(&items[0].link).await.unwrap();
            let ev = venues::parse_wikicfp_event(&extract::html(&page.body, &page.final_url));
            println!(
                "WikiCFP event: {:?} when {:?}–{:?} where {:?} deadline {:?} link {:?}",
                ev.title, ev.start, ev.end, ev.place, ev.deadline, ev.link
            );
            assert!(ev.start.is_some());

            let search = fetcher().page("http://www.wikicfp.com/cfp/servlet/tool.search?q=CHI&year=f").await.unwrap();
            let hits = venues::wikicfp_search_hits(&extract::html(&search.body, &search.final_url), "CHI");
            println!("WikiCFP search CHI: {hits:?}");
            assert!(!hits.is_empty());
        });
    }

    /// Live venue research without AI (OpenAlex + WikiCFP + homepage only): `cargo test live_venue -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_venue() {
        let ctx = crate::api::Ctx::for_tests(crate::db::test_db());
        setting::put(&ctx.db, "ai.driver", Some("compatible")).unwrap(); // not configured: no AI calls
        let conf = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "MobileHCI (ACM International Conference on Mobile Human-Computer Interaction)", "kind": "conference" })).unwrap();
        let journal = ctx.db.insert("venues", &json!({ "user_id": 1, "name": "Information Systems Journal", "kind": "journal" })).unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            for id in [conf, journal] {
                let run = runs::Run::begin(&ctx, "venues", "Live", 1, None).unwrap();
                let rep = venues::research(&ctx, &run, id).await.unwrap();
                println!("{}: applied {:?}, pending {:?}, sources {:?}", rep.name, rep.applied, rep.pending, rep.sources);
            }
        });
        println!("{}", ctx.db.find("venues", conf).unwrap().unwrap());
        println!("{}", ctx.db.find("venues", journal).unwrap().unwrap());
    }

    #[test]
    fn flags_default() {
        let db = crate::db::test_db();
        assert!(flag(&db, "research.autofill_empty"));
        set_flag(&db, "research.autofill_empty", false).unwrap();
        assert!(!flag(&db, "research.autofill_empty"));
    }
}
