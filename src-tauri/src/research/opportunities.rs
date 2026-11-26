//! Opportunity scans (port of OpportunityScanner and the opportunities:scan command).
//!
//! 1. WikiCFP RSS for each interest (first six), then each call's WikiCFP page (dates, place, official link)
//!    and the official call page. Calls whose deadline has passed are dropped before any AI work.
//! 2. The AI ranks the candidates against the interests and papers in progress (fit 1–5 with a one-line
//!    why) from the fetched text only. Without AI, a keyword match gives the fit.
//! 3. Optionally the provider's web search looks for journal special issues the feeds miss; each one is
//!    kept only if its page can be fetched and shows the deadline.
//!
//! Duplicates are removed by normalised URL (tracking parameters, scheme, `www.` and trailing slash ignored)
//! and by fuzzy title against everything already stored.

use super::extract::{self, Doc};
use super::fetch::{fetcher, Opts};
use super::providers::{self, Ask};
use super::runs::Run;
use super::{clean_str, clean_url, object_schema, quote_in, similarity};
use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::util::{now_ts, today};
use chrono::NaiveDate;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TRACKING: &[&str] =
    &["fbclid", "gclid", "mc_cid", "mc_eid", "ref", "ref_src", "source", "share", "igshid", "_hsenc", "_hsmi", "copyownerid", "spm"];

/// Normalised URL for dedupe: lower-case host without `www.`, no scheme, no fragment, no tracking
/// parameters (`utm_*`, `fbclid`, ...), sorted query, no trailing slash.
pub fn normalize_url(u: &str) -> String {
    let Ok(mut url) = url::Url::parse(u.trim()) else {
        return u.trim().trim_end_matches('/').to_lowercase();
    };
    url.set_fragment(None);
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_lowercase();
            !k.starts_with("utm_") && !TRACKING.contains(&k.as_str())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    let host = url.host_str().unwrap_or("").to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let path = url.path().trim_end_matches('/').to_string();
    let query = if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"))
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    format!("{host}{port}{path}{query}")
}

/// `opportunities.url_hash`: SHA-256 of the normalised URL.
pub fn url_hash(u: &str) -> String {
    Sha256::digest(normalize_url(u).as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether two calls are the same: same normalised URL, or very similar titles (same deadline when both
/// have one).
pub fn same_call(a_url: &str, a_title: &str, a_deadline: Option<&str>, b_url: &str, b_title: &str, b_deadline: Option<&str>) -> bool {
    if normalize_url(a_url) == normalize_url(b_url) {
        return true;
    }
    let sim = similarity(a_title, b_title);
    match (a_deadline, b_deadline) {
        (Some(x), Some(y)) if x != y => false,
        _ => sim >= 0.88,
    }
}

/// The interests text (port of OpportunityScanner::interestsFor): the saved interests, else the keywords of the
/// user's papers. Empty when there are neither; nothing field-specific is assumed.
pub fn interests_for(ctx: &Ctx, saved: Option<&str>) -> String {
    if let Some(i) = saved.map(str::trim).filter(|i| !i.is_empty()) {
        return i.to_string();
    }
    let rows = ctx.db.rows("SELECT keywords FROM papers WHERE user_id = ?1 AND keywords IS NOT NULL", [USER_ID]).unwrap_or_default();
    let mut kws: Vec<String> = Vec::new();
    for r in rows {
        for k in r["keywords"].as_str().unwrap_or("").split([';', ',']) {
            let k = k.trim();
            if !k.is_empty() && !kws.iter().any(|x| x.eq_ignore_ascii_case(k)) && kws.len() < 15 {
                kws.push(k.to_string());
            }
        }
    }
    kws.join(", ")
}

/// Interests used by a scan (the first eight phrases), lower-case.
pub fn terms(interests: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in interests.split([';', ',', '\n']) {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out.truncate(8);
    out
}

/// Words that carry no topic on their own.
const STOP: &[&str] = &[
    "a",
    "an",
    "and",
    "or",
    "of",
    "the",
    "for",
    "in",
    "on",
    "with",
    "to",
    "by",
    "at",
    "from",
    "via",
    "using",
    "based",
    "study",
    "studies",
    "research",
    "applied",
    "advanced",
    "new",
    "modern",
    "approach",
    "approaches",
    "method",
    "methods",
    "theory",
    "issues",
    "topics",
];

/// The topic words of an interest: lower-case, without stop words, at least three letters.
pub fn topic_words(term: &str) -> Vec<String> {
    term.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}

/// WikiCFP lists calls under short topic categories ("chemistry", "polymer", "machine learning"), so a
/// specific interest such as "polymer chemistry" has no category of its own. Each interest is looked up as
/// written first; when that category is empty or small, its topic words are looked up too (the last word,
/// usually the field, first). Returns `(category, interest)` pairs, at most `max`.
pub fn feed_queries(terms: &[String], counts: &std::collections::HashMap<String, usize>, max: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for t in terms {
        if counts.get(t).copied().unwrap_or(0) >= 8 {
            continue;
        }
        let mut words = topic_words(t);
        if words.len() < 2 && words.first().is_some_and(|w| w == t) {
            continue;
        }
        words.reverse();
        for w in words {
            if out.len() >= max {
                return out;
            }
            if !terms.contains(&w) && !out.iter().any(|(q, _)| q == &w) {
                out.push((w, t.clone()));
            }
        }
    }
    out
}

/// Word stem for loose matching ("polymers" ~ "polymeric" ~ "polymer"): the first six letters.
fn stem(w: &str) -> String {
    w.chars().take(6).collect()
}

/// How much of one interest a text covers: 1.0 for the whole phrase or every topic word, else the share of
/// topic words found (by stem).
pub fn coverage(term: &str, text_lower: &str, text_stems: &std::collections::HashSet<String>) -> f64 {
    if term.len() > 2 && text_lower.contains(term) {
        return 1.0;
    }
    let words = topic_words(term);
    if words.is_empty() {
        return 0.0;
    }
    let hit = words.iter().filter(|w| text_stems.contains(&stem(w))).count();
    hit as f64 / words.len() as f64
}

/// Keyword fit when no AI is set up: 1–5 from how well the call covers the interests. An interest counts
/// fully when the call names it or all its topic words, and half when it names at least half of them.
pub fn keyword_fit(interests: &[String], text: &str) -> (i64, Vec<String>) {
    let t = text.to_lowercase();
    let stems: std::collections::HashSet<String> =
        t.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(stem).collect();
    let mut full = Vec::new();
    let mut part = Vec::new();
    for i in interests {
        let c = coverage(i, &t, &stems);
        if c >= 0.999 {
            full.push(i.clone());
        } else if c >= 0.5 {
            part.push(format!("{i} (in part)"));
        }
    }
    let fit = match (full.len(), part.len()) {
        (0, 0) => 1,
        (0, _) => 3,
        (1, 0) => 4,
        _ => 5,
    };
    full.extend(part);
    (fit, full)
}

#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub title: String,
    pub link: String,
    pub description: String,
    pub term: String,
    pub event: Option<super::venues::WikiEvent>,
    pub official: Option<Doc>,
}

/// WikiCFP RSS: `[place] [Sep 20, 2027 - Sep 23, 2027]` in the description.
pub fn feed_candidates(xml: &str, term: &str) -> Vec<Candidate> {
    extract::feed(xml)
        .into_iter()
        .map(|i| Candidate {
            title: super::limit(&i.title, 200),
            link: i.link,
            description: i.description,
            term: term.to_string(),
            ..Default::default()
        })
        .collect()
}

fn item_schema() -> Value {
    let s = |d: &str| json!({ "type": ["string", "null"], "description": d });
    object_schema(vec![(
        "items",
        json!({ "type": "array", "items": object_schema(vec![
            ("id", json!({ "type": "string", "description": "The candidate id, e.g. C3, or W1 for a call you found on the web." })),
            ("keep", json!({ "type": "boolean", "description": "True only if it fits the interests and leads to archival publication." })),
            ("title", json!({ "type": "string", "description": "Name of the call, e.g. \"Special issue: Platform labor in the Global South\"." })),
            ("kind", json!({ "type": "string", "description": "conference, special_issue, journal or workshop." })),
            ("venue", s("Conference or journal name.")),
            ("publisher", s("Publisher.")),
            ("outlet", s("Where accepted work appears, e.g. \"ACM Digital Library\" or the journal's name.")),
            ("deadline_on", s("Submission deadline, YYYY-MM-DD, as written on the page.")),
            ("deadline_quote", s("Exact short quote from the page that shows the deadline.")),
            ("event", s("Conference dates and place, or issue date.")),
            ("url", s("Official call-for-papers URL.")),
            ("summary", json!({ "type": "string", "description": "One or two sentences on what the call wants." })),
            ("why", json!({ "type": "string", "description": "One sentence on why it fits these interests or papers." })),
            ("fit", json!({ "type": "integer", "description": "Fit from 1 (weak) to 5 (excellent)." })),
        ]) }),
    )])
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub title: String,
    pub kind: String,
    pub venue: Option<String>,
    pub publisher: Option<String>,
    pub outlet: Option<String>,
    pub deadline_on: Option<String>,
    pub event: Option<String>,
    pub url: String,
    pub summary: Option<String>,
    pub why: Option<String>,
    pub fit: i64,
}

/// Validate one model item (port of the scan loop checks). `None` when it must be dropped.
pub fn clean_item(item: &Value, today: NaiveDate) -> Option<Found> {
    let url = item.get("url").and_then(|u| clean_url(u, 500))?;
    let title = clean_str(item.get("title")?, 250)?;
    let deadline = item
        .get("deadline_on")
        .and_then(Value::as_str)
        .and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok().or_else(|| extract::parse_date_loose(d)));
    if deadline.is_some_and(|d| d < today) {
        return None;
    }
    let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
    let kind = if crate::models::opportunity::KINDS.iter().any(|(k, _)| *k == kind) { kind } else { "conference" };
    let s = |k: &str, max| item.get(k).and_then(|v| clean_str(v, max));
    Some(Found {
        title,
        kind: kind.to_string(),
        venue: s("venue", 255),
        publisher: s("publisher", 255),
        outlet: s("outlet", 255),
        deadline_on: deadline.map(|d| d.format("%Y-%m-%d").to_string()),
        event: s("event", 255),
        url,
        summary: s("summary", 800),
        why: s("why", 500),
        fit: item.get("fit").and_then(Value::as_i64).unwrap_or(3).clamp(1, 5),
    })
}

/// Store a call unless it duplicates one already stored. Returns the new row id.
pub fn save_found(ctx: &Ctx, scan_id: i64, f: &Found) -> Result<Option<i64>> {
    let hash = url_hash(&f.url);
    if ctx.db.count("SELECT COUNT(*) FROM opportunities WHERE user_id = ?1 AND url_hash = ?2", rusqlite::params![USER_ID, hash])? > 0 {
        return Ok(None);
    }
    let existing =
        ctx.db.rows("SELECT url, title, deadline_on FROM opportunities WHERE user_id = ?1 ORDER BY id DESC LIMIT 500", [USER_ID])?;
    if existing.iter().any(|e| {
        same_call(
            e["url"].as_str().unwrap_or(""),
            e["title"].as_str().unwrap_or(""),
            e["deadline_on"].as_str(),
            &f.url,
            &f.title,
            f.deadline_on.as_deref(),
        )
    }) {
        return Ok(None);
    }
    let id = ctx.db.insert(
        "opportunities",
        &json!({
            "user_id": USER_ID, "scan_id": scan_id, "title": f.title, "kind": f.kind, "venue": f.venue, "publisher": f.publisher,
            "outlet": f.outlet, "deadline_on": f.deadline_on, "event": f.event, "url": f.url, "url_hash": hash,
            "summary": f.summary, "why": f.why, "fit": f.fit, "status": "new",
        }),
    )?;
    Ok(Some(id))
}

async fn gather(ctx: &Ctx, run: &Run, interests: &str) -> Result<Vec<Candidate>> {
    let terms = terms(interests);
    run.step(ctx, &format!("Reading WikiCFP feeds for {} {}", terms.len(), if terms.len() == 1 { "interest" } else { "interests" }));
    let feed = |cat: String, term: String| async move {
        let url =
            format!("http://www.wikicfp.com/cfp/rss?cat={}", url::form_urlencoded::byte_serialize(cat.as_bytes()).collect::<String>());
        (cat, term, fetcher().get(&url, &Opts::FEED).await)
    };
    let mut out: Vec<Candidate> = Vec::new();
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // First the interests as written, then (for interests with few calls) their topic words.
    for pass in 0..2 {
        let queries: Vec<(String, String)> = if pass == 0 {
            terms.iter().map(|t| (t.clone(), t.clone())).collect()
        } else {
            let q = feed_queries(&terms, &counts, 16);
            if !q.is_empty() {
                run.note(
                    ctx,
                    &format!("Also reading broader topics: {}", q.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ")),
                );
            }
            q
        };
        let mut set = tokio::task::JoinSet::new();
        for (cat, term) in queries {
            set.spawn(feed(cat, term));
        }
        while let Some(r) = run.guard(set.join_next()).await? {
            match r {
                Ok((cat, term, Ok(p))) => {
                    let found = feed_candidates(&p.body, &term);
                    if pass == 0 {
                        counts.insert(cat.clone(), found.len());
                    }
                    for c in found {
                        if !out.iter().any(|o| normalize_url(&o.link) == normalize_url(&c.link)) {
                            out.push(c);
                        }
                    }
                }
                Ok((cat, _, Err(e))) => run.note(ctx, &format!("WikiCFP feed for “{cat}” failed: {e}")),
                Err(_) => {}
            }
        }
    }
    // Skip calls already stored before reading their pages.
    let known = ctx.db.rows("SELECT url, title FROM opportunities WHERE user_id = ?1 ORDER BY id DESC LIMIT 500", [USER_ID])?;
    out.retain(|c| {
        !known.iter().any(|k| same_call(k["url"].as_str().unwrap_or(""), k["title"].as_str().unwrap_or(""), None, &c.link, &c.title, None))
    });
    // Events already over (from the feed's date range) are not worth a request.
    out.retain(|c| extract::dates_in(&c.description).last().is_none_or(|d| *d >= today()));
    // The calls that match the interests best are read first (the list is capped).
    out.sort_by_key(|c| -keyword_fit(&terms, &format!("{} {}", c.title, c.description)).0);
    out.truncate(40);
    Ok(out)
}

async fn read_pages(ctx: &Ctx, run: &Run, cands: &mut [Candidate]) -> Result<()> {
    run.step(ctx, &format!("Fetching {} call pages", cands.len()));
    let mut set = tokio::task::JoinSet::new();
    for (i, c) in cands.iter().enumerate() {
        let link = c.link.clone();
        set.spawn(async move {
            let ev = match fetcher().page(&link).await {
                Ok(p) => Some(super::venues::parse_wikicfp_event(&extract::html(&p.body, &p.final_url))),
                Err(_) => None,
            };
            let official = match ev.as_ref().and_then(|e| e.link.clone()) {
                Some(u) if ev.as_ref().and_then(|e| e.deadline).is_none_or(|d| d >= today()) => {
                    fetcher().page(&u).await.ok().filter(|p| p.is_html()).map(|p| extract::html(&p.body, &p.final_url))
                }
                _ => None,
            };
            (i, ev, official)
        });
    }
    let mut done = 0;
    while let Some(r) = run.guard(set.join_next()).await? {
        if let Ok((i, ev, official)) = r {
            done += 1;
            if let Some(c) = cands.get_mut(i) {
                if let Some(o) = &official {
                    run.note(ctx, &format!("Reading {}", if o.title.is_empty() { &c.title } else { &o.title }));
                }
                c.event = ev;
                c.official = official;
            }
            run.set_progress(ctx, 10 + (done * 40 / cands.len().max(1)) as i64);
        }
    }
    Ok(())
}

fn candidate_block(i: usize, c: &Candidate) -> String {
    let mut s = format!("[C{}] {}\nWikiCFP: {}\n", i + 1, c.title, c.link);
    if let Some(e) = &c.event {
        if let Some(l) = &e.when_line {
            s.push_str(&format!("{l}\n"));
        }
        if let Some(p) = &e.place {
            s.push_str(&format!("Where | {p}\n"));
        }
        if let Some(l) = &e.deadline_line {
            s.push_str(&format!("{l}\n"));
        }
        if let Some(l) = &e.link {
            s.push_str(&format!("Official page: {l}\n"));
        }
        if !e.categories.is_empty() {
            s.push_str(&format!("Categories: {}\n", e.categories.join(", ")));
        }
    } else if !c.description.is_empty() {
        s.push_str(&format!("{}\n", c.description));
    }
    if let Some(d) = &c.official {
        s.push_str(&format!(
            "Official page text:\n{}\n",
            d.excerpt(&["deadline", "due", "proceedings", "topics", "submission", "published", "journal", "special issue"], 1500)
        ));
    }
    s
}

/// Deadline from WikiCFP or the official page for a candidate (verified facts only).
fn known_deadline(c: &Candidate) -> Option<NaiveDate> {
    c.event.as_ref().and_then(|e| e.deadline)
}

/// Run a scan (the whole job, including scans row, notifications and events).
pub async fn scan(ctx: Ctx, run: Run, scan_id: i64) {
    super::fetch::init_cache(&ctx);
    let provider = providers::status(&ctx.db);
    let _ = ctx.db.update("scans", scan_id, &json!({ "provider": provider }));
    let result = scan_inner(&ctx, &run, scan_id).await;
    let log: Option<String> = ctx.db.scalar("SELECT log FROM ai_runs WHERE id = ?1", [run.id]).ok().flatten();
    match result {
        Ok((found, returned)) => {
            let _ = ctx.db.update("scans", scan_id, &json!({ "status": "done", "found": found, "finished_at": now_ts() }));
            run.note(&ctx, &format!("Saved {found} new of {returned} returned"));
            run.finish(&ctx, None, Some(json!({ "found": found, "scan_id": scan_id })));
            let log: Option<String> = ctx.db.scalar("SELECT log FROM ai_runs WHERE id = ?1", [run.id]).ok().flatten();
            let _ = ctx.db.update("scans", scan_id, &json!({ "log": log }));
            let body = if found > 0 {
                format!("{found} new {} that fit your interests.", if found == 1 { "call" } else { "calls" })
            } else {
                "No new calls this time.".into()
            };
            let _ = ctx.notify(
                "ai_job_finished",
                json!({ "title": "Calls for papers scan finished", "body": body, "status": "success", "url": "#/opportunities" }),
            );
        }
        Err(e) => {
            let msg = e.to_string();
            let _ =
                ctx.db.update("scans", scan_id, &json!({ "status": "failed", "error": super::limit(&msg, 600), "finished_at": now_ts() }));
            run.finish(&ctx, Some(&msg), Some(json!({ "scan_id": scan_id })));
            let log: Option<String> = ctx.db.scalar("SELECT log FROM ai_runs WHERE id = ?1", [run.id]).ok().flatten().or(log);
            let _ = ctx.db.update("scans", scan_id, &json!({ "log": log }));
            if msg != super::runs::CANCELLED {
                let _ = ctx.notify("ai_job_finished", json!({ "title": "Calls for papers scan failed", "body": super::limit(&msg, 200), "status": "danger", "url": "#/opportunities" }));
            }
        }
    }
    ctx.changed("opportunities");
}

async fn scan_inner(ctx: &Ctx, run: &Run, scan_id: i64) -> Result<(usize, usize)> {
    let saved: Option<String> = ctx.db.scalar("SELECT interests FROM users WHERE id = ?1", [USER_ID])?;
    let interests = interests_for(ctx, saved.as_deref());
    if terms(&interests).is_empty() {
        return Err(ApiError::msg("Add your research interests first (Calls for Papers → Interests), so the scan knows what to look for."));
    }
    let mut cands = gather(ctx, run, &interests).await?;
    ctx.db.update("scans", scan_id, &json!({ "candidates": cands.len() }))?;
    run.advance(ctx, None);
    read_pages(ctx, run, &mut cands).await?;
    // Deadlines that passed (known from WikiCFP) are dropped before ranking.
    let before = cands.len();
    cands.retain(|c| known_deadline(c).is_none_or(|d| d >= today()));
    if before > cands.len() {
        run.note(ctx, &format!("Dropped {} calls whose deadline has passed", before - cands.len()));
    }
    run.check()?;
    let term_list = terms(&interests);
    let ai = providers::configured(&ctx.db);
    let mut returned = 0usize;
    let mut found = 0usize;
    let mut save = |f: Found, ctx: &Ctx| -> Result<()> {
        returned += 1;
        if let Some(id) = save_found(ctx, scan_id, &f)? {
            found += 1;
            let row = ctx.db.find("opportunities", id)?.unwrap_or(Value::Null);
            run.partial(ctx, json!({ "opportunity": row }));
            run.note(ctx, &format!("{} new {}", found, if found == 1 { "call" } else { "calls" }));
        }
        Ok(())
    };

    if !ai {
        run.step(ctx, "Ranking calls by your interests (AI is not set up)");
        for c in &cands {
            let text = format!("{} {} {}", c.title, c.description, c.event.as_ref().map(|e| e.categories.join(" ")).unwrap_or_default());
            let (fit, hits) = keyword_fit(&term_list, &text);
            if fit < 3 {
                continue;
            }
            let ev = c.event.as_ref();
            let (venue, title) = match c.title.split_once(" : ") {
                Some((a, b)) => (Some(a.trim().to_string()), b.trim().to_string()),
                None => (None, c.title.clone()),
            };
            save(
                Found {
                    title: super::limit(&title, 250),
                    kind: if c.title.to_lowercase().contains("workshop") { "workshop".into() } else { "conference".into() },
                    venue,
                    publisher: None,
                    outlet: None,
                    deadline_on: known_deadline(c).map(|d| d.format("%Y-%m-%d").to_string()),
                    event: ev.and_then(super::venues::edition_text),
                    url: ev.and_then(|e| e.link.clone()).unwrap_or_else(|| c.link.clone()),
                    summary: None,
                    why: Some(format!("Matches your interests: {}.", hits.join(", "))),
                    fit,
                },
                ctx,
            )?;
        }
        run.advance(ctx, None);
        return Ok((found, returned));
    }

    // AI ranking from fetched text.
    let papers =
        ctx.db.rows("SELECT title, method FROM papers WHERE user_id = ?1 AND stage NOT IN ('published', 'shelved') LIMIT 20", [USER_ID])?;
    let papers: Vec<String> = papers
        .iter()
        .map(|p| match p["method"].as_str().filter(|m| !m.is_empty()) {
            Some(m) => format!("- {} ({m})", p["title"].as_str().unwrap_or("")),
            None => format!("- {}", p["title"].as_str().unwrap_or("")),
        })
        .collect();
    let blocks: String = cands.iter().enumerate().map(|(i, c)| candidate_block(i, c)).collect::<Vec<_>>().join("\n");
    let base = format!(
        "Today is {}. Rank calls for papers for a researcher.\nResearch interests: {interests}\n{}\n\
         Keep only calls that (a) have a submission deadline after today and (b) lead to archival publication: conference proceedings \
         (ACM, IEEE, AIS eLibrary, Springer and similar), journal special issues, or journals with open calls. Leave out events without \
         proceedings and predatory or pay-to-publish outlets. Use only the information given for each candidate; take the deadline from \
         the \"Submission Deadline\" line or the official page text, and copy the exact line into deadline_quote. Use the official page URL \
         as url when there is one. Rate fit from 1 (weak) to 5 (excellent) and give a one-sentence why.",
        today(),
        if papers.is_empty() { String::new() } else { format!("Papers in progress:\n{}\n", papers.join("\n")) }
    );
    if !cands.is_empty() {
        run.step(ctx, &format!("AI is checking {} calls against your interests", cands.len()));
        let mut req = Ask::new(format!("{base}\n\nCandidates:\n{blocks}"), item_schema());
        req.cancel = Some(run.flag());
        req.timeout = 300;
        let ans = providers::ask(&ctx.db, req).await?;
        run.usage(ctx, ans.usage.input, ans.usage.output);
        run.set_progress(ctx, 75);
        let mut items: Vec<Value> = ans.data.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        items.sort_by_key(|i| -i.get("fit").and_then(Value::as_i64).unwrap_or(0));
        for item in items.iter().filter(|i| i.get("keep").and_then(Value::as_bool).unwrap_or(true)).take(15) {
            let idx = item
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| s.trim_start_matches(['C', 'c']).parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));
            let Some(c) = idx.and_then(|i| cands.get(i)) else {
                continue;
            };
            let Some(mut f) = clean_item(item, today()) else {
                continue;
            };
            // Facts from WikiCFP win over the model's reading.
            if let Some(d) = known_deadline(c) {
                f.deadline_on = Some(d.format("%Y-%m-%d").to_string());
            } else if let (Some(d), Some(q)) = (&f.deadline_on, item.get("deadline_quote").and_then(Value::as_str)) {
                let on_page = c.official.as_ref().is_some_and(|o| quote_in(q, &o.text()));
                let parsed = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                if !on_page || !parsed.is_some_and(|p| extract::dates_in(q).contains(&p)) {
                    f.deadline_on = None;
                }
            } else {
                f.deadline_on = None;
            }
            let official = c.event.as_ref().and_then(|e| e.link.clone());
            let url_known = [Some(c.link.clone()), official.clone()].iter().flatten().any(|u| normalize_url(u) == normalize_url(&f.url));
            if !url_known {
                f.url = official.unwrap_or_else(|| c.link.clone());
            }
            save(f, ctx)?;
        }
    }
    run.check()?;

    // Web search for special issues the feeds miss (verified by fetching each page).
    if providers::supports_web(&ctx.db) && super::flag(&ctx.db, "research.web_fallback") {
        run.step(ctx, "Searching publisher call pages for special issues");
        let known: Vec<String> = ctx
            .db
            .rows("SELECT url FROM opportunities WHERE user_id = ?1 ORDER BY id DESC LIMIT 60", [USER_ID])?
            .iter()
            .filter_map(|r| r["url"].as_str().map(str::to_string))
            .collect();
        let prompt = format!(
            "{base}\n\nSearch the web for journal special issues and conference tracks in these areas, for example on Taylor & Francis, SAGE, \
             Wiley, Elsevier, Springer, ACM and AIS call pages. Use ids W1, W2, ... Give the official call page as url and copy the deadline \
             line from that page into deadline_quote.{}\nReturn up to 8 items, best fit first.",
            if known.is_empty() { String::new() } else { format!("\nAlready shown before (do not repeat):\n{}", known.join("\n")) }
        );
        let mut req = Ask::new(prompt, item_schema());
        req.web = true;
        req.cancel = Some(run.flag());
        req.timeout = 420;
        match providers::ask(&ctx.db, req).await {
            Ok(ans) => {
                run.usage(ctx, ans.usage.input, ans.usage.output);
                let items: Vec<Value> = ans.data.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
                let items: Vec<Value> =
                    items.into_iter().filter(|i| i.get("keep").and_then(Value::as_bool).unwrap_or(true)).take(8).collect();
                run.step(ctx, &format!("Checking {} calls found on the web", items.len()));
                for item in items {
                    run.check()?;
                    let Some(mut f) = clean_item(&item, today()) else {
                        continue;
                    };
                    let Some(q) = item.get("deadline_quote").and_then(Value::as_str) else {
                        continue;
                    };
                    let Ok(Ok(p)) = run.guard(fetcher().page(&f.url)).await else {
                        run.note(ctx, &format!("Skipped {} (page could not be read)", f.title));
                        continue;
                    };
                    let text = extract::html(&p.body, &p.final_url).text();
                    let dates = extract::dates_in(q);
                    let ok = quote_in(q, &text)
                        && f.deadline_on
                            .as_deref()
                            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                            .is_some_and(|d| dates.contains(&d));
                    if !ok {
                        run.note(ctx, &format!("Skipped {} (deadline not shown on its page)", f.title));
                        continue;
                    }
                    f.url = p.final_url.clone();
                    save(f, ctx)?;
                }
            }
            Err(e) if e.to_string() == super::runs::CANCELLED => return Err(e),
            Err(e) => run.note(ctx, &format!("Web search failed: {e}")),
        }
    }
    run.advance(ctx, Some("Saving the calls that fit"));
    Ok((found, returned))
}

/// Start a scan unless one is running (started in the last 15 minutes). Returns `(scan_id, run_id)`.
pub fn start(ctx: &Ctx, trigger: &str) -> Result<Option<(i64, Run)>> {
    let running = ctx.db.count(
        "SELECT COUNT(*) FROM scans WHERE user_id = ?1 AND status = 'running' AND created_at > ?2",
        rusqlite::params![USER_ID, crate::util::ts_minutes_ago(15)],
    )?;
    if running > 0 {
        return Ok(None);
    }
    let scan_id = ctx.db.insert("scans", &json!({ "user_id": USER_ID, "trigger": trigger, "status": "running" }))?;
    let label = if trigger == "daily" { "Daily calls for papers scan" } else { "Calls for papers scan" };
    let run = Run::begin(ctx, "scan", label, 3, Some("#/opportunities")).inspect_err(|_| {
        let _ = ctx.db.update("scans", scan_id, &json!({ "status": "failed", "error": "Could not start.", "finished_at": now_ts() }));
    })?;
    Ok(Some((scan_id, run)))
}

/// The `opportunities:scan` job (blocking).
pub fn scheduled_scan(ctx: &Ctx) -> Result<()> {
    if ctx.user()?.is_none() {
        return Ok(());
    }
    super::fetch::fetcher().prune_cache();
    match start(ctx, "daily")? {
        Some((scan_id, run)) => {
            ctx.block_on(scan(ctx.clone(), run, scan_id));
            Ok(())
        }
        None => Err(ApiError::msg("A scan is already running.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalisation() {
        assert_eq!(normalize_url("HTTPS://www.Example.org/cfp/?utm_source=x&b=2&a=1#top"), "example.org/cfp?a=1&b=2");
        assert_eq!(normalize_url("http://example.org/cfp"), normalize_url("https://example.org/cfp/"));
        assert_eq!(
            normalize_url("http://www.wikicfp.com/cfp/servlet/event.showcfp?eventid=203910&copyownerid=49757"),
            "wikicfp.com/cfp/servlet/event.showcfp?eventid=203910"
        );
        assert_ne!(normalize_url("https://a.org/x?id=1"), normalize_url("https://a.org/x?id=2"));
        assert_eq!(url_hash("https://a.org/x/"), url_hash("http://www.a.org/x?fbclid=1"));
    }

    #[test]
    fn dedupe() {
        assert!(same_call("https://a.org/si", "x", None, "http://www.a.org/si/", "y", None));
        assert!(same_call(
            "https://a.org/1",
            "Special issue: Platform labor in the Global South",
            Some("2027-03-01"),
            "https://b.org/2",
            "Special Issue - Platform Labour in the Global South",
            Some("2027-03-01")
        ));
        assert!(!same_call("https://a.org/1", "CHI 2027", Some("2027-01-01"), "https://b.org/2", "CHI 2027", Some("2027-02-01")));
        assert!(!same_call("https://a.org/1", "Platform labor", None, "https://b.org/2", "Generative AI at work", None));

        let ctx = Ctx::for_tests(crate::db::test_db());
        let scan = ctx.db.insert("scans", &json!({ "user_id": 1 })).unwrap();
        let f = Found {
            title: "Special issue: AI and work".into(),
            kind: "special_issue".into(),
            venue: None,
            publisher: None,
            outlet: None,
            deadline_on: Some("2099-01-01".into()),
            event: None,
            url: "https://j.org/si?utm_medium=email".into(),
            summary: None,
            why: None,
            fit: 4,
        };
        assert!(save_found(&ctx, scan, &f).unwrap().is_some());
        assert!(save_found(&ctx, scan, &Found { url: "https://www.j.org/si/".into(), ..f.clone() }).unwrap().is_none());
        assert!(save_found(
            &ctx,
            scan,
            &Found { url: "https://other.org/x".into(), title: "Special Issue — AI and Work".into(), ..f.clone() }
        )
        .unwrap()
        .is_none());
        assert!(save_found(&ctx, scan, &Found { url: "https://other.org/y".into(), title: "Workshop on fairness".into(), ..f })
            .unwrap()
            .is_some());
    }

    #[test]
    fn items_are_validated() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let ok = json!({ "id": "C1", "keep": true, "title": " CHI 2027 ", "kind": "nonsense", "venue": "CHI", "publisher": null, "outlet": "",
            "deadline_on": "2027-01-21", "deadline_quote": null, "event": null, "url": "https://chi2027.acm.org/", "summary": "S", "why": "W", "fit": 9 });
        let f = clean_item(&ok, today).unwrap();
        assert_eq!((f.title.as_str(), f.kind.as_str(), f.fit), ("CHI 2027", "conference", 5));
        assert!(f.outlet.is_none());
        assert!(clean_item(&json!({ "title": "Old", "url": "https://x.org", "deadline_on": "2026-01-01" }), today).is_none());
        assert!(clean_item(&json!({ "title": "No url", "url": "mailto:x@y.z" }), today).is_none());
        let s = item_schema();
        assert_eq!(s["properties"]["items"]["items"]["additionalProperties"], false);
    }

    #[test]
    fn feeds_and_interests() {
        let xml = r#"<rss><channel><item><title>MobileHCI 2027 : Mobile HCI</title><link>http://www.wikicfp.com/cfp/servlet/event.showcfp?eventid=1</link><description>[Limassol] [Sep 20, 2027 - Sep 23, 2027]</description></item></channel></rss>"#;
        let c = feed_candidates(xml, "hci");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].term, "hci");
        assert_eq!(terms("a, B; c,,d, e, f, g, h, i, a"), vec!["a", "b", "c", "d", "e", "f", "g", "h"]);
        let (fit, hits) = keyword_fit(&terms("platform labor, CSCW, ICT4D"), "CSCW workshop on platform labor");
        assert_eq!((fit, hits.len()), (5, 2));
        // Specific interests match broader calls by their topic words, and plural or adjective forms count.
        let chem = terms("polymer chemistry, organic synthesis, catalysis");
        let (fit, hits) = keyword_fit(&chem, "International Conference on Chemistry and Polymers");
        assert_eq!((fit, hits), (4, vec!["polymer chemistry".to_string()]));
        assert_eq!(keyword_fit(&chem, "Conference on Green Chemistry"), (3, vec!["polymer chemistry (in part)".to_string()]));
        assert_eq!(keyword_fit(&chem, "Workshop on Machine Learning").0, 1);
        assert_eq!(topic_words("Studies of polymer chemistry"), vec!["polymer", "chemistry"]);
        // Interests with an empty or small WikiCFP category are also looked up by their topic words, field first.
        let counts: std::collections::HashMap<String, usize> =
            [("polymer chemistry".to_string(), 0), ("organic synthesis".to_string(), 12), ("catalysis".to_string(), 3)]
                .into_iter()
                .collect();
        let q = feed_queries(&chem, &counts, 16);
        assert_eq!(
            q,
            vec![("chemistry".to_string(), "polymer chemistry".to_string()), ("polymer".to_string(), "polymer chemistry".to_string())]
        );
        let ctx = Ctx::for_tests(crate::db::test_db());
        assert_eq!(interests_for(&ctx, None), "");
        ctx.db.insert("papers", &json!({ "user_id": 1, "title": "P", "keywords": "gig work; CSCW, gig work" })).unwrap();
        assert_eq!(interests_for(&ctx, Some("  ")), "gig work, CSCW");
        assert_eq!(interests_for(&ctx, Some("mine")), "mine");
    }
}
