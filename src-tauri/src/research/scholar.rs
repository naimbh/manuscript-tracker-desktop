//! Google Scholar profile refresh (port of ScholarProfileUpdater), reading the profile page directly.
//!
//! The page is fetched once (`/citations?user=…&pagesize=100`, which Scholar's robots.txt allows) and parsed:
//! citations, h-index and i10-index (all and since), citations per year, and every article with its
//! "cited by" count. Statistics and per-article counts are applied (they are what the page shows); articles
//! not in the library are kept for `research.importPublications`.
//!
//! When Scholar blocks automated reading (captcha, 429), the AI provider's web search is tried (its numbers
//! cannot be checked, so they become proposals for review); without AI, OpenAlex author statistics found
//! through the user's own DOIs are proposed instead.

use super::fetch::{fetcher, FetchError, Opts};
use super::proposals::{self, Draft};
use super::providers::{self, Ask};
use super::runs::Run;
use super::{norm_title, object_schema, similarity, Evidence};
use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use scraper::{ElementRef, Html, Selector};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const CACHE_KEY: &str = "scholar:articles";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Article {
    pub title: String,
    pub authors: Option<String>,
    pub venue: Option<String>,
    pub year: Option<i64>,
    pub cited_by: Option<i64>,
    pub scholar_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profile {
    pub name: Option<String>,
    pub since: Option<i64>,
    pub all: [Option<i64>; 3],
    pub recent: [Option<i64>; 3],
    pub by_year: BTreeMap<i64, i64>,
    pub articles: Vec<Article>,
}

impl Profile {
    /// `users.scholar_stats` in the web app's shape.
    pub fn stats(&self) -> Value {
        let by_year: Map<String, Value> = self.by_year.iter().map(|(y, c)| (y.to_string(), json!(c))).collect();
        json!({
            "since": self.since,
            "all": { "citations": self.all[0], "h": self.all[1], "i10": self.all[2] },
            "recent": { "citations": self.recent[0], "h": self.recent[1], "i10": self.recent[2] },
            "by_year": by_year,
        })
    }
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|_| Selector::parse("*").expect("selector"))
}

fn text(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn num(s: &str) -> Option<i64> {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

/// The profile URL to fetch: `https://<scholar host>/citations?user=<id>&hl=en&view_op=list_works&sortby=pubdate&pagesize=100`.
pub fn profile_url(scholar_url: &str) -> Option<String> {
    let u = url::Url::parse(scholar_url.trim()).ok()?;
    let host = u.host_str()?.to_string();
    if !host.starts_with("scholar.google.") {
        return None;
    }
    let user = u.query_pairs().find(|(k, _)| k == "user").map(|(_, v)| v.into_owned())?;
    if user.is_empty() || !user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    Some(format!("https://{host}/citations?user={user}&hl=en&view_op=list_works&sortby=pubdate&pagesize=100"))
}

/// Parse a Scholar profile page.
pub fn parse_profile(html: &str, base: &str) -> Option<Profile> {
    let doc = Html::parse_document(html);
    let mut p = Profile { name: doc.select(&sel("#gsc_prf_in")).next().map(text), ..Default::default() };
    let table = doc.select(&sel("#gsc_rsb_st")).next();
    if let Some(t) = table {
        p.since = t.select(&sel("th")).map(text).find_map(|h| h.strip_prefix("Since ").and_then(num));
        for row in t.select(&sel("tbody tr")) {
            let label = row.select(&sel("td")).next().map(text).unwrap_or_default().to_lowercase();
            let vals: Vec<Option<i64>> = row.select(&sel("td.gsc_rsb_std")).map(|c| num(&text(c))).collect();
            let i = if label.starts_with("citations") {
                0
            } else if label.starts_with("h-index") {
                1
            } else if label.starts_with("i10") {
                2
            } else {
                continue;
            };
            p.all[i] = vals.first().copied().flatten();
            p.recent[i] = vals.get(1).copied().flatten();
        }
    }
    // Citations per year: years in .gsc_g_t; bars in .gsc_g_a with "z-index:N" counting from the last year.
    let years: Vec<i64> = doc.select(&sel(".gsc_g_t")).filter_map(|y| num(&text(y))).collect();
    let bars: Vec<ElementRef> = doc.select(&sel("a.gsc_g_a")).collect();
    if !years.is_empty() {
        let mut counts = vec![0i64; years.len()];
        if bars.len() == years.len() {
            for (i, b) in bars.iter().enumerate() {
                counts[i] = num(&text(*b)).unwrap_or(0);
            }
        } else {
            for b in &bars {
                let z =
                    b.value().attr("style").and_then(|s| s.split("z-index:").nth(1)).and_then(|z| num(z.split(';').next().unwrap_or("")));
                if let Some(z) = z.map(|z| z as usize).filter(|z| *z >= 1 && *z <= years.len()) {
                    counts[years.len() - z] = num(&text(*b)).unwrap_or(0);
                }
            }
        }
        p.by_year = years.into_iter().zip(counts).collect();
    }
    let base = url::Url::parse(base).ok();
    for row in doc.select(&sel("tr.gsc_a_tr")) {
        let Some(a) = row.select(&sel("a.gsc_a_at")).next() else {
            continue;
        };
        let title = text(a);
        if title.is_empty() {
            continue;
        }
        let grays: Vec<String> = row.select(&sel("div.gs_gray")).map(text).collect();
        let href = a.value().attr("href").or(a.value().attr("data-href"));
        let scholar_url = href.and_then(|h| base.as_ref().and_then(|b| b.join(h).ok())).map(|u| u.to_string());
        let cited_by = row.select(&sel("a.gsc_a_ac")).next().and_then(|c| num(&text(c)));
        let year = row.select(&sel(".gsc_a_y span, .gsc_a_h")).next().and_then(|y| num(&text(y)));
        // The venue line ends with ", 2024"; drop the year.
        let venue = grays
            .get(1)
            .map(|v| match v.rsplit_once(", ") {
                Some((head, y)) if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) => head.to_string(),
                _ if v.len() == 4 && v.chars().all(|c| c.is_ascii_digit()) => String::new(),
                _ => v.clone(),
            })
            .filter(|v| !v.is_empty());
        p.articles.push(Article { title, authors: grays.first().cloned().filter(|a| !a.is_empty()), venue, year, cited_by, scholar_url });
    }
    (table.is_some() || !p.articles.is_empty()).then_some(p)
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub updated: usize,
    pub new_titles: Vec<String>,
    pub total: Option<i64>,
    /// "scholar", "ai" or "openalex".
    pub source: &'static str,
    pub pending: usize,
}

fn find_pub<'a>(pubs: &'a [Value], title: &str) -> Option<&'a Value> {
    let n = norm_title(title);
    pubs.iter()
        .find(|p| norm_title(p["title"].as_str().unwrap_or("")) == n)
        .or_else(|| pubs.iter().find(|p| similarity(p["title"].as_str().unwrap_or(""), title) >= 0.95))
}

/// Apply a profile read from Scholar itself (verified).
fn apply_profile(ctx: &Ctx, run: &Run, url: &str, p: &Profile) -> Result<Outcome> {
    ctx.db.update("users", USER_ID, &json!({ "scholar_stats": p.stats(), "scholar_checked_at": crate::util::now_ts() }))?;
    let pubs = ctx.db.rows("SELECT id, title, citations, scholar_url FROM publications WHERE user_id = ?1", [USER_ID])?;
    let mut out = Outcome { total: p.all[0], source: "scholar", ..Default::default() };
    for a in &p.articles {
        match find_pub(&pubs, &a.title) {
            Some(row) => {
                let id = row["id"].as_i64().unwrap_or(0);
                let base = Evidence {
                    value: Value::Null,
                    source_url: Some(url.to_string()),
                    quote: Some(format!("{} — Cited by {}", a.title, a.cited_by.unwrap_or(0))),
                    source_name: "Google Scholar".into(),
                    verified: true,
                };
                let mut drafts = Vec::new();
                let mut c = Draft::new("citations", json!(a.cited_by.unwrap_or(0)), &base);
                c.metric = true;
                drafts.push(c);
                if let Some(u) = &a.scholar_url {
                    drafts.push(Draft::new("scholar_url", json!(u), &base));
                }
                let r = proposals::record(ctx, "publication", id, drafts, Some(run.id))?;
                out.pending += r.pending.len();
                ctx.db.update("publications", id, &json!({ "checked_at": crate::util::now_ts() }))?;
                out.updated += 1;
            }
            None => out.new_titles.push(a.title.clone()),
        }
    }
    let list: Vec<Value> = p.articles.iter().map(article_json).collect();
    ctx.db.cache_put(CACHE_KEY, &Value::Array(list).to_string())?;
    Ok(out)
}

pub fn article_json(a: &Article) -> Value {
    json!({ "title": a.title, "authors": a.authors, "venue": a.venue, "year": a.year, "cited_by": a.cited_by, "scholar_url": a.scholar_url })
}

fn ai_schema() -> Value {
    let int = || json!({ "type": ["integer", "null"] });
    object_schema(vec![
        ("read_ok", json!({ "type": "boolean", "description": "True only if the Google Scholar profile page itself was read." })),
        ("since_year", json!({ "type": ["integer", "null"], "description": "The \"Since\" column year, e.g. 2021." })),
        ("citations_all", int()),
        ("citations_since", int()),
        ("h_all", int()),
        ("h_since", int()),
        ("i10_all", int()),
        ("i10_since", int()),
        (
            "articles",
            json!({ "type": "array", "items": object_schema(vec![
            ("title", json!({ "type": "string" })), ("cited_by", int()), ("year", int()),
        ]) }),
        ),
    ])
}

/// Fallback 1: the provider's web search reads the profile. Numbers become proposals (unverified).
async fn via_ai(ctx: &Ctx, run: &Run, scholar_url: &str) -> Result<Outcome> {
    run.step(ctx, "Google Scholar blocked the app; asking the AI to read the profile");
    let prompt = format!(
        "Fetch this Google Scholar profile and copy what it shows: {scholar_url}\nRead the \"Cited by\" table (All and Since columns for \
         Citations, h-index and i10-index) and every article in the list with its exact title, \"cited by\" number and year. Copy the \
         numbers exactly as the page shows them. If the page cannot be read, set read_ok to false and leave the numbers null."
    );
    let mut req = Ask::new(prompt, ai_schema());
    req.web = true;
    req.cancel = Some(run.flag());
    let ans = providers::ask(&ctx.db, req).await?;
    run.usage(ctx, ans.usage.input, ans.usage.output);
    let d = ans.data;
    if !d["read_ok"].as_bool().unwrap_or(false) {
        return Err(ApiError::msg("Google Scholar could not be read this time. Try again later."));
    }
    let n = |k: &str| d[k].as_i64().filter(|v| *v >= 0);
    let p = Profile {
        since: n("since_year"),
        all: [n("citations_all"), n("h_all"), n("i10_all")],
        recent: [n("citations_since"), n("h_since"), n("i10_since")],
        ..Default::default()
    };
    let ev = Evidence {
        value: Value::Null,
        source_url: Some(scholar_url.to_string()),
        quote: None,
        source_name: "AI web search".into(),
        verified: false,
    };
    let mut out = Outcome { total: p.all[0], source: "ai", ..Default::default() };
    let r = proposals::record(ctx, "profile", USER_ID, vec![Draft::new("scholar_stats", p.stats(), &ev)], Some(run.id))?;
    out.pending += r.pending.len();
    let pubs = ctx.db.rows("SELECT id, title, citations FROM publications WHERE user_id = ?1", [USER_ID])?;
    for a in d["articles"].as_array().cloned().unwrap_or_default() {
        let title = a["title"].as_str().unwrap_or("");
        match find_pub(&pubs, title) {
            Some(row) => {
                if let Some(c) = a["cited_by"].as_i64().filter(|c| *c >= 0) {
                    let r = proposals::record(
                        ctx,
                        "publication",
                        row["id"].as_i64().unwrap_or(0),
                        vec![Draft::new("citations", json!(c), &ev)],
                        Some(run.id),
                    )?;
                    out.pending += r.pending.len();
                    out.updated += 1;
                }
            }
            None if !title.is_empty() => out.new_titles.push(title.to_string()),
            None => {}
        }
    }
    Ok(out)
}

/// Fallback 2: OpenAlex author statistics, found through the user's own DOIs (no name is sent).
async fn via_openalex(ctx: &Ctx, run: &Run) -> Result<Outcome> {
    run.step(ctx, "Google Scholar blocked the app; looking you up on OpenAlex");
    let name = ctx.require_user()?["name"].as_str().unwrap_or("").to_string();
    let dois: Vec<String> = ctx
        .db
        .rows("SELECT doi FROM publications WHERE user_id = ?1 AND doi IS NOT NULL AND doi != '' LIMIT 5", [USER_ID])?
        .iter()
        .filter_map(|r| r["doi"].as_str().and_then(super::citations::clean_doi))
        .collect();
    let mut votes: BTreeMap<String, usize> = BTreeMap::new();
    for d in dois.iter().take(3) {
        let Ok(w) = run.guard(fetcher().json(&format!("https://api.openalex.org/works/doi:{d}"))).await? else {
            continue;
        };
        for a in w["authorships"].as_array().cloned().unwrap_or_default() {
            let (id, dn) = (a.pointer("/author/id").and_then(Value::as_str), a.pointer("/author/display_name").and_then(Value::as_str));
            if let (Some(id), Some(dn)) = (id, dn) {
                if similarity(dn, &name) >= 0.75 {
                    *votes.entry(id.to_string()).or_default() += 1;
                }
            }
        }
    }
    let Some((author, _)) = votes.into_iter().max_by_key(|(_, n)| *n) else {
        return Err(ApiError::msg("Google Scholar could not be read this time. Try again later."));
    };
    let key = author.rsplit('/').next().unwrap_or("");
    let a = run.guard(fetcher().json(&format!("https://api.openalex.org/authors/{key}"))).await??;
    let mut by_year = BTreeMap::new();
    for c in a["counts_by_year"].as_array().cloned().unwrap_or_default() {
        if let (Some(y), Some(n)) = (c["year"].as_i64(), c["cited_by_count"].as_i64()) {
            by_year.insert(y, n);
        }
    }
    let p = Profile {
        all: [
            a["cited_by_count"].as_i64(),
            a.pointer("/summary_stats/h_index").and_then(Value::as_i64),
            a.pointer("/summary_stats/i10_index").and_then(Value::as_i64),
        ],
        by_year,
        ..Default::default()
    };
    let mut stats = p.stats();
    stats["source"] = json!("openalex");
    let ev = Evidence {
        value: Value::Null,
        source_url: Some(author.clone()),
        quote: Some(format!("cited_by_count: {}", p.all[0].unwrap_or(0))),
        source_name: "OpenAlex".into(),
        verified: true,
    };
    // Different index from Scholar: always reviewed, never filled in automatically.
    let mut d = Draft::new("scholar_stats", stats, &ev);
    d.verified = false;
    let r = proposals::record(ctx, "profile", USER_ID, vec![d], Some(run.id))?;
    Ok(Outcome { total: p.all[0], source: "openalex", pending: r.pending.len(), ..Default::default() })
}

/// Shown when Scholar answers 404: the profile ID is wrong or the profile is private.
pub const PROFILE_NOT_FOUND: &str =
    "Google Scholar couldn’t find this profile. Check the link in Settings → Research profile, and make sure the \
     profile is public (in Scholar: edit your profile and tick “Make my profile public”).";

/// The error for a failed profile fetch that is not a block (blocks fall back to other sources).
fn fetch_error(e: &FetchError) -> ApiError {
    match e {
        FetchError::Status(404) => ApiError::msg(PROFILE_NOT_FOUND),
        e => ApiError::msg(format!("Google Scholar could not be read: {e}.")),
    }
}

/// Read the profile (Scholar, then fallbacks) and apply or propose the results.
pub async fn refresh(ctx: &Ctx, run: &Run) -> Result<Outcome> {
    super::fetch::init_cache(ctx);
    let user = ctx.require_user()?;
    let scholar_url = user["scholar_url"]
        .as_str()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| ApiError::msg("Add your Google Scholar profile link first."))?
        .to_string();
    let url = profile_url(&scholar_url)
        .ok_or_else(|| ApiError::msg("Use your profile link, which starts with https://scholar.google.com/citations?user=…"))?;
    run.step(ctx, "Reading your Google Scholar profile");
    // Fresh read (no cache), robots.txt respected, 429 = blocked (never hammer Scholar).
    let opts = Opts { cache: false, robots: true, accept: Opts::PAGE.accept, retries: 0, no_retry_429: true };
    let blocked = match run.guard(fetcher().get(&url, &opts)).await? {
        Ok(page) => match parse_profile(&page.body, &page.final_url) {
            Some(p) => {
                run.set_progress(ctx, 60);
                run.note(ctx, &format!("Read {} articles from Scholar", p.articles.len()));
                return apply_profile(ctx, run, &scholar_url, &p);
            }
            None => "the page layout was not recognised".to_string(),
        },
        Err(FetchError::Blocked(why)) => why,
        Err(FetchError::Disallowed) => "robots.txt".into(),
        Err(e) => return Err(fetch_error(&e)),
    };
    run.note(ctx, &format!("Google Scholar is not available ({blocked})"));
    if providers::configured(&ctx.db) && providers::supports_web(&ctx.db) {
        match via_ai(ctx, run, &scholar_url).await {
            Ok(o) => return Ok(o),
            Err(e) if e.to_string() == super::runs::CANCELLED => return Err(e),
            Err(e) => run.note(ctx, &format!("AI fallback failed: {e}")),
        }
    }
    via_openalex(ctx, run).await
}

/// Run a Scholar refresh as a job with the web app's notification texts.
pub async fn refresh_job(ctx: Ctx, run: Run, scheduled: bool) {
    match refresh(&ctx, &run).await {
        Ok(o) => {
            let mut body = match o.source {
                "scholar" if scheduled => format!(
                    "{}{} articles refreshed from Google Scholar.{}",
                    o.total.map(|t| format!("{t} citations in total. ")).unwrap_or_default(),
                    o.updated,
                    if o.new_titles.is_empty() {
                        String::new()
                    } else {
                        format!(" New on Scholar: {}.", o.new_titles.iter().take(2).cloned().collect::<Vec<_>>().join("; "))
                    }
                ),
                "scholar" => format!(
                    "{} articles updated from Google Scholar.{}",
                    o.updated,
                    if o.new_titles.is_empty() {
                        String::new()
                    } else {
                        format!(" Not in your list yet: {}.", o.new_titles.iter().take(3).cloned().collect::<Vec<_>>().join("; "))
                    }
                ),
                "ai" => format!("Google Scholar blocked the app, so the AI read it instead. {} figures to review.", o.pending),
                _ => "Google Scholar blocked the app, so statistics from OpenAlex are ready to review.".into(),
            };
            if o.source == "scholar" && o.pending > 0 {
                body.push_str(&format!(" {} {} to review.", o.pending, if o.pending == 1 { "change" } else { "changes" }));
            }
            let result = json!({ "updated": o.updated, "new": o.new_titles, "total": o.total, "source": o.source, "pending": o.pending });
            run.finish(&ctx, None, Some(result));
            let _ = ctx.notify("ai_job_finished", json!({ "title": "Citations updated", "body": body, "status": if o.source == "scholar" { "success" } else { "warning" }, "url": "#/publications" }));
        }
        Err(e) => {
            let msg = e.to_string();
            run.finish(&ctx, Some(&msg), None);
            if msg != super::runs::CANCELLED {
                let title = if scheduled { "Citation update could not read Scholar" } else { "Update failed" };
                let _ = ctx.notify(
                    "ai_job_finished",
                    json!({ "title": title, "body": msg, "status": if scheduled { "warning" } else { "danger" }, "url": "#/publications" }),
                );
            }
        }
    }
    ctx.changed("publications");
    ctx.changed("profile");
    ctx.changed("proposals");
}

/// Last article list read from Scholar, each with `in_library`.
pub fn last_articles(ctx: &Ctx) -> Result<Vec<Value>> {
    let raw = ctx.db.cache_get(CACHE_KEY)?.unwrap_or_else(|| "[]".into());
    let list: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
    let pubs = ctx.db.rows("SELECT id, title FROM publications WHERE user_id = ?1", [USER_ID])?;
    Ok(list
        .into_iter()
        .map(|mut a| {
            let hit = find_pub(&pubs, a["title"].as_str().unwrap_or("")).and_then(|p| p["id"].as_i64());
            a["in_library"] = json!(hit.is_some());
            a["publication_id"] = json!(hit);
            a
        })
        .collect())
}

/// Add Scholar articles that are not in the library (all, or the given titles). Returns how many.
pub fn import(ctx: &Ctx, titles: Option<&[String]>) -> Result<usize> {
    let mut n = 0;
    for a in last_articles(ctx)? {
        if a["in_library"].as_bool().unwrap_or(false) {
            continue;
        }
        let title = a["title"].as_str().unwrap_or("").to_string();
        if titles.is_some_and(|t| !t.iter().any(|x| norm_title(x) == norm_title(&title))) {
            continue;
        }
        let venue = a["venue"].as_str().unwrap_or("").to_lowercase();
        let kind = if venue.contains("conference") || venue.contains("proceedings") || venue.contains("symposium") {
            "conference"
        } else if venue.contains("arxiv") || venue.contains("ssrn") || venue.contains("preprint") {
            "preprint"
        } else {
            "article"
        };
        ctx.db.insert(
            "publications",
            &json!({ "user_id": USER_ID, "title": super::limit(&title, 255), "authors": a["authors"], "venue": a["venue"], "year": a["year"],
                     "citations": a["cited_by"], "scholar_url": a["scholar_url"], "kind": kind, "checked_at": crate::util::now_ts() }),
        )?;
        n += 1;
    }
    if n > 0 {
        ctx.changed("publications");
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_profile_message() {
        use super::{fetch_error, FetchError, PROFILE_NOT_FOUND};
        assert_eq!(fetch_error(&FetchError::Status(404)).to_string(), PROFILE_NOT_FOUND);
        assert!(PROFILE_NOT_FOUND.contains("Make my profile public"));
        assert_eq!(fetch_error(&FetchError::Status(500)).to_string(), "Google Scholar could not be read: HTTP 500.");
    }

    use super::*;

    pub const FIXTURE: &str = r##"<html><body>
      <div id="gsc_prf_in">Ada Researcher</div>
      <table id="gsc_rsb_st"><thead><tr><th></th><th class="gsc_rsb_sth">All</th><th class="gsc_rsb_sth">Since 2021</th></tr></thead>
      <tbody><tr><td class="gsc_rsb_sc1"><a>Citations</a></td><td class="gsc_rsb_std">1,234</td><td class="gsc_rsb_std">987</td></tr>
      <tr><td class="gsc_rsb_sc1"><a>h-index</a></td><td class="gsc_rsb_std">15</td><td class="gsc_rsb_std">12</td></tr>
      <tr><td class="gsc_rsb_sc1"><a>i10-index</a></td><td class="gsc_rsb_std">20</td><td class="gsc_rsb_std">16</td></tr></tbody></table>
      <div class="gsc_md_hist_b"><span class="gsc_g_t">2022</span><span class="gsc_g_t">2023</span><span class="gsc_g_t">2024</span>
        <a href="#" class="gsc_g_a" style="left:10px;height:5px;z-index:3"><span class="gsc_g_al">40</span></a>
        <a href="#" class="gsc_g_a" style="left:50px;height:9px;z-index:1"><span class="gsc_g_al">120</span></a></div>
      <table id="gsc_a_t"><tbody id="gsc_a_b">
        <tr class="gsc_a_tr"><td class="gsc_a_t"><a href="/citations?view_op=view_citation&amp;citation_for_view=abc:1" class="gsc_a_at">Platform labor in the Global South</a>
          <div class="gs_gray">A Researcher, B Coauthor</div><div class="gs_gray">Information Systems Journal 34 (2), 100-120, 2024</div></td>
          <td class="gsc_a_c"><a href="https://scholar.google.com/scholar?cites=1" class="gsc_a_ac gs_ibl">57</a></td><td class="gsc_a_y"><span class="gsc_a_h gsc_a_hc gs_ibl">2024</span></td></tr>
        <tr class="gsc_a_tr"><td class="gsc_a_t"><a href="/citations?view_op=view_citation&amp;citation_for_view=abc:2" class="gsc_a_at">A new working paper</a>
          <div class="gs_gray">A Researcher</div><div class="gs_gray">SSRN, 2025</div></td>
          <td class="gsc_a_c"><a class="gsc_a_ac gs_ibl"></a></td><td class="gsc_a_y"><span class="gsc_a_h gsc_a_hc gs_ibl">2025</span></td></tr>
      </tbody></table></body></html>"##;

    #[test]
    fn parses_profile() {
        let p = parse_profile(FIXTURE, "https://scholar.google.com/citations?user=abc&hl=en").unwrap();
        assert_eq!(p.name.as_deref(), Some("Ada Researcher"));
        assert_eq!(p.since, Some(2021));
        assert_eq!(p.all, [Some(1234), Some(15), Some(20)]);
        assert_eq!(p.recent, [Some(987), Some(12), Some(16)]);
        assert_eq!(p.by_year, BTreeMap::from([(2022, 40), (2023, 0), (2024, 120)]));
        assert_eq!(p.articles.len(), 2);
        let a = &p.articles[0];
        assert_eq!(a.cited_by, Some(57));
        assert_eq!(a.year, Some(2024));
        assert_eq!(a.venue.as_deref(), Some("Information Systems Journal 34 (2), 100-120"));
        assert_eq!(a.scholar_url.as_deref(), Some("https://scholar.google.com/citations?view_op=view_citation&citation_for_view=abc:1"));
        assert_eq!(p.articles[1].cited_by, None);
        assert_eq!(p.stats()["by_year"]["2024"], 120);
        assert!(parse_profile("<html><body>Please show you're not a robot</body></html>", "https://scholar.google.com/").is_none());
    }

    #[test]
    fn profile_urls() {
        assert_eq!(
            profile_url("https://scholar.google.com/citations?hl=en&user=AbC_12-x").as_deref(),
            Some("https://scholar.google.com/citations?user=AbC_12-x&hl=en&view_op=list_works&sortby=pubdate&pagesize=100")
        );
        assert!(profile_url("https://example.com/citations?user=x").is_none());
        assert!(profile_url("https://scholar.google.com/citations?view_op=x").is_none());
    }

    #[test]
    fn applies_and_imports() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let id = ctx
            .db
            .insert("publications", &json!({ "user_id": 1, "title": "Platform Labor in the Global South", "citations": 60 }))
            .unwrap();
        let run = Run::begin(&ctx, crate::research::runs::SCHOLAR_KIND, crate::research::runs::SCHOLAR_LABEL, 1, None).unwrap();
        let p = parse_profile(FIXTURE, "https://scholar.google.com/citations?user=abc").unwrap();
        let o = apply_profile(&ctx, &run, "https://scholar.google.com/citations?user=abc", &p).unwrap();
        assert_eq!(o.updated, 1);
        assert_eq!(o.new_titles, vec!["A new working paper"]);
        let row = ctx.db.find("publications", id).unwrap().unwrap();
        assert_eq!(row["citations"], 57, "Scholar's count is applied as shown");
        assert!(row["scholar_url"].as_str().unwrap().contains("citation_for_view=abc:1"), "empty field filled");
        let u = ctx.user().unwrap().unwrap();
        assert_eq!(u["scholar_stats"]["all"]["citations"], 1234);
        let list = last_articles(&ctx).unwrap();
        assert_eq!(list.iter().filter(|a| a["in_library"] == true).count(), 1);
        assert_eq!(import(&ctx, None).unwrap(), 1);
        let added = ctx.db.first("SELECT * FROM publications WHERE title = 'A new working paper'", []).unwrap().unwrap();
        assert_eq!(added["kind"], "preprint");
        assert_eq!(import(&ctx, None).unwrap(), 0);
    }
}
