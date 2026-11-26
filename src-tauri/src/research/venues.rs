//! Venue research (port of VenueResearcher::research/clean/apply and the venues:refresh command), plus
//! suggestions of journals and conferences from the user's research interests.
//!
//! Order of sources: OpenAlex (journals: publisher, homepage, APC) and WikiCFP (conferences: next edition,
//! deadline, official link) give structured facts; the venue's own pages (homepage, call for papers, author
//! guidelines, found through links) are fetched and the AI extracts the remaining fields from that text with
//! a quote per field; the provider's web search is used only for fields still missing. Everything becomes
//! proposals (see `proposals.rs`). Suggestions for new venues come from the user's research interests
//! (OpenAlex, see "Suggestions" below); no venue list is built in.

use super::extract::{self, Doc};
use super::fetch::fetcher;
use super::proposals::{self, Draft};
use super::providers::{self, Ask};
use super::runs::Run;
use super::{clean_str, clean_url, evidence_from, evidence_schema, object_schema, quote_in, similarity, Evidence};
use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::util::{blank, today};
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_PAGES: usize = 5;
const PAGE_CHARS: usize = 7000;
const TOTAL_CHARS: usize = 26000;

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// `CSCW (ACM Conference on ...)` -> `CSCW`.
pub fn acronym(name: &str) -> String {
    name.split(" (").next().unwrap_or(name).trim().to_string()
}

// ---- OpenAlex sources (journals) --------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OaSource {
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub issn_l: Option<String>,
    pub issns: Vec<String>,
    pub apc_usd: Option<i64>,
    pub has_apc_data: bool,
    pub is_oa: bool,
    pub is_in_doaj: bool,
    pub h_index: Option<i64>,
    pub works_count: Option<i64>,
}

/// Pick the OpenAlex source matching a venue name from a `/sources?search=` response.
pub fn pick_source(resp: &Value, name: &str) -> Option<OaSource> {
    let wanted = [name.to_string(), acronym(name)];
    let best = resp
        .get("results")?
        .as_array()?
        .iter()
        .filter_map(|r| {
            let dn = r.get("display_name")?.as_str()?;
            let score = wanted.iter().map(|w| similarity(w, dn)).fold(0.0, f64::max);
            Some((score, r))
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))?;
    if best.0 < 0.85 {
        return None;
    }
    let r = best.1;
    let s = |k: &str| r.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    let prices = r.get("apc_prices").and_then(Value::as_array);
    Some(OaSource {
        id: s("id").unwrap_or_default(),
        name: s("display_name").unwrap_or_default(),
        publisher: s("host_organization_name"),
        homepage: s("homepage_url"),
        issn_l: s("issn_l"),
        issns: r
            .get("issn")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        apc_usd: r.get("apc_usd").and_then(Value::as_i64),
        has_apc_data: prices.is_some_and(|p| !p.is_empty()) || r.get("apc_usd").is_some_and(|v| !v.is_null()),
        is_oa: r.get("is_oa").and_then(Value::as_bool).unwrap_or(false),
        is_in_doaj: r.get("is_in_doaj").and_then(Value::as_bool).unwrap_or(false),
        h_index: r.pointer("/summary_stats/h_index").and_then(Value::as_i64),
        works_count: r.get("works_count").and_then(Value::as_i64),
    })
}

fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Facts from an OpenAlex source record.
pub fn source_evidence(src: &OaSource) -> BTreeMap<&'static str, Evidence> {
    let mut out = BTreeMap::new();
    let ev = |value: Value, quote: String| Evidence {
        value,
        source_url: Some(src.id.clone()),
        quote: Some(quote),
        source_name: "OpenAlex".into(),
        verified: true,
    };
    if let Some(p) = &src.publisher {
        out.insert("publisher", ev(json!(p), format!("host_organization_name: {p}")));
    }
    if let Some(h) = &src.homepage {
        if clean_url(&json!(h), 255).is_some() {
            out.insert("website_url", ev(json!(h), format!("homepage_url: {h}")));
        }
    }
    if src.has_apc_data {
        match src.apc_usd {
            Some(n) if n > 0 => {
                let text =
                    if src.is_oa { format!("APC US${}", thousands(n)) } else { format!("APC US${} (optional open access)", thousands(n)) };
                out.insert("fee", ev(json!(text), format!("apc_usd: {n}")));
            }
            Some(0) if src.is_oa => {
                out.insert("fee", ev(json!("No fee (diamond open access)"), "apc_usd: 0".into()));
            }
            _ => {}
        }
    }
    out
}

async fn openalex_source(name: &str) -> Option<OaSource> {
    let url = format!("https://api.openalex.org/sources?search={}&per-page=5", enc(&acronym(name)));
    let resp = fetcher().json(&url).await.ok()?;
    pick_source(&resp, name)
}

// ---- WikiCFP (conferences) --------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WikiEvent {
    pub url: String,
    pub title: String,
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub when_line: Option<String>,
    pub place: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub deadline_line: Option<String>,
    pub abstract_due: Option<NaiveDate>,
    pub link: Option<String>,
    pub categories: Vec<String>,
}

/// Parse a WikiCFP event page (`event.showcfp`).
pub fn parse_wikicfp_event(doc: &Doc) -> WikiEvent {
    let mut e = WikiEvent { url: doc.url.clone(), title: doc.title.clone(), ..Default::default() };
    for line in &doc.lines {
        let Some((head, rest)) = line.split_once(" | ") else {
            if let Some(i) = line.find("Link: ") {
                let u = line[i + 6..].split_whitespace().next().unwrap_or("");
                e.link = clean_url(&json!(u), 500);
            }
            continue;
        };
        match head.trim() {
            "When" => {
                let d = extract::dates_in(rest);
                e.start = d.first().copied();
                e.end = d.get(1).copied().or(e.start);
                e.when_line = Some(line.clone());
            }
            "Where" => e.place = Some(rest.trim().to_string()).filter(|p| !p.is_empty() && p != "N/A"),
            "Submission Deadline" => {
                e.deadline = extract::parse_date_loose(rest);
                e.deadline_line = Some(line.clone());
            }
            "Abstract Registration Due" => e.abstract_due = extract::parse_date_loose(rest),
            _ => {}
        }
    }
    for l in &doc.links {
        if l.url.contains("/cfp/call?conference=") && !l.text.is_empty() {
            e.categories.push(l.text.clone());
        }
    }
    e
}

/// `next_edition` text from a WikiCFP event: `20–23 Sep 2027, Limassol, Cyprus`.
pub fn edition_text(e: &WikiEvent) -> Option<String> {
    let (a, b) = (e.start?, e.end?);
    let mut s = extract::format_range(a, b);
    if let Some(p) = &e.place {
        s = format!("{s}, {p}");
    }
    Some(super::limit(&s, 255))
}

/// Event links from a WikiCFP search page whose name matches the acronym, newest year first.
pub fn wikicfp_search_hits(doc: &Doc, acr: &str) -> Vec<(i32, String)> {
    let acr_n = super::norm_title(acr);
    let mut prefixes = vec![acr_n.clone()];
    for org in ["acm ", "ieee ", "ifip ", "aaai ", "acm ieee "] {
        if let Some(rest) = acr_n.strip_prefix(org) {
            prefixes.push(rest.to_string());
        }
    }
    let mut hits: Vec<(i32, String)> = doc
        .links
        .iter()
        .filter(|l| l.url.contains("event.showcfp"))
        .filter_map(|l| {
            let t = super::norm_title(&l.text);
            let (name, year) = t.rsplit_once(' ')?;
            let year: i32 = year.parse().ok()?;
            prefixes.iter().any(|p| p == name).then(|| (year, l.url.clone()))
        })
        .collect();
    hits.sort_by_key(|h| h.0);
    hits.dedup_by(|a, b| a.1 == b.1);
    hits
}

async fn wikicfp_event(acr: &str) -> Option<WikiEvent> {
    if acr.split_whitespace().count() > 4 || acr.len() < 2 {
        return None;
    }
    let url = format!("http://www.wikicfp.com/cfp/servlet/tool.search?q={}&year=f", enc(acr));
    let page = fetcher().page(&url).await.ok()?;
    let doc = extract::html(&page.body, &page.final_url);
    let this_year = chrono::Datelike::year(&today());
    let mut fallback: Option<WikiEvent> = None;
    for (year, link) in wikicfp_search_hits(&doc, acr).into_iter().filter(|(y, _)| *y >= this_year).take(3) {
        let _ = year;
        let Ok(p) = fetcher().page(&link).await else {
            continue;
        };
        let ev = parse_wikicfp_event(&extract::html(&p.body, &p.final_url));
        if ev.deadline.is_some_and(|d| d >= today()) {
            return Some(ev);
        }
        if ev.start.is_some_and(|d| d >= today()) && fallback.is_none() {
            fallback = Some(ev);
        }
    }
    fallback
}

// ---- Validation (port of clean()) -------------------------------------------------------------------

/// Validate one field value. `Err` says why it was dropped.
pub fn clean_field(field: &str, v: &Value) -> std::result::Result<Value, String> {
    let s = |max| clean_str(v, max).map(Value::String).ok_or_else(|| "empty".to_string());
    match field {
        "next_edition" => s(255),
        "deadline_on" => {
            let raw = v.as_str().unwrap_or("").trim();
            let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok().or_else(|| extract::parse_date_loose(raw)).ok_or("not a date")?;
            if date < today() {
                return Err(format!("deadline {date} has passed"));
            }
            if (date - today()).num_days() > 3 * 366 {
                return Err(format!("deadline {date} is too far ahead"));
            }
            Ok(json!(date.format("%Y-%m-%d").to_string()))
        }
        "deadline_label" => s(80),
        "deadline_expected" => match v {
            Value::Bool(b) => Ok(json!(*b)),
            Value::String(t) if t == "true" || t == "false" => Ok(json!(t == "true")),
            _ => Err("not true or false".into()),
        },
        "website_url" | "portal_url" => clean_url(v, 255).map(Value::String).ok_or_else(|| "not a web address".into()),
        "acceptance_rate" => {
            let t = clean_str(v, 90).ok_or("empty")?;
            if !t.chars().any(|c| c.is_ascii_digit()) {
                return Err("no number".into());
            }
            Ok(json!(t))
        }
        "first_decision" => s(80),
        "fee" | "word_limit" => s(120),
        "review_type" => {
            let t = v.as_str().unwrap_or("").trim().to_lowercase();
            let t = match t.as_str() {
                "double-blind" | "double blind" | "double anonymous" | "double-anonymous" => "double",
                "single-blind" | "single blind" | "single anonymous" => "single",
                x => x,
            };
            if ["double", "single", "open", "none"].contains(&t) {
                Ok(json!(t))
            } else {
                Err("unknown review type".into())
            }
        }
        "proceedings" => s(160),
        "publisher" => s(255),
        "notes" => s(600),
        _ => Err("unknown field".into()),
    }
}

// ---- AI extraction ----------------------------------------------------------------------------------

const CONF_FIELDS: &[(&str, &str, &str)] = &[
    ("next_edition", "string", "Dates and city of the next edition that has not happened yet, e.g. \"10–14 May 2027, Pittsburgh\"."),
    ("deadline_on", "string", "Next full-paper submission deadline as YYYY-MM-DD."),
    ("deadline_label", "string", "Short label for that deadline, e.g. \"Full papers\" or \"Abstracts\"."),
    ("deadline_expected", "boolean", "True only if the page says the date is tentative or estimated."),
    ("acceptance_rate", "string", "Recent acceptance rate, e.g. \"~25%\"."),
    ("proceedings", "string", "Where the proceedings are published."),
    ("word_limit", "string", "Page or word limit for full papers."),
    ("review_type", "string", "Peer review type: double, single, open or none."),
    ("portal_url", "string", "Submission system URL (EasyChair, PCS, OpenReview...)."),
    ("notes", "string", "One or two sentences on anything else worth knowing (other tracks, format changes)."),
];

const JOURNAL_FIELDS: &[(&str, &str, &str)] = &[
    ("acceptance_rate", "string", "Acceptance rate, e.g. \"12%\"."),
    ("first_decision", "string", "Time to first decision, e.g. \"28 days (median)\"."),
    ("fee", "string", "Article processing charge, or \"No fee (subscription)\"."),
    ("word_limit", "string", "Word limit for research articles, e.g. \"8,000 incl. references\"."),
    ("review_type", "string", "Peer review type: double, single, open or none."),
    ("portal_url", "string", "Submission system URL (ScholarOne, Editorial Manager...)."),
    ("notes", "string", "One or two sentences on anything else worth knowing (special issues, format)."),
];

/// Fields that matter most; missing ones trigger the web-search fallback.
fn key_fields(kind: &str) -> &'static [&'static str] {
    if kind == "journal" {
        &["acceptance_rate", "first_decision", "word_limit"]
    } else {
        &["next_edition", "deadline_on"]
    }
}

fn field_defs(kind: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    if kind == "journal" {
        JOURNAL_FIELDS
    } else {
        CONF_FIELDS
    }
}

pub fn extraction_schema(kind: &str, only: Option<&[&str]>) -> Value {
    let props = field_defs(kind)
        .iter()
        .filter(|(f, _, _)| only.is_none_or(|o| o.contains(f)))
        .map(|(f, t, d)| (*f, evidence_schema(t, d)))
        .collect();
    object_schema(props)
}

struct Source {
    url: String,
    doc: Doc,
}

fn same_site(a: &str, b: &str) -> bool {
    let host = |u: &str| url::Url::parse(u).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
    let base = |h: String| h.rsplitn(3, '.').take(2).collect::<Vec<_>>().join(".");
    let (ha, hb) = (host(a), host(b));
    !ha.is_empty() && base(ha) == base(hb)
}

/// Links worth reading from a venue homepage, best first.
pub fn follow_links(doc: &Doc, kind: &str, limit: usize) -> Vec<String> {
    let needles: &[&str] = if kind == "journal" {
        &[
            "author guideline",
            "guide for author",
            "instructions for author",
            "author instruction",
            "submission guideline",
            "for authors",
            "submit",
            "aims and scope",
            "about this journal",
            "about the journal",
            "journal metrics",
            "special issue",
            "call for papers",
            "peer review",
            "open access",
            "article publishing charge",
        ]
    } else {
        &[
            "call for papers",
            "call-for-papers",
            "cfp",
            "important dates",
            "dates",
            "submission",
            "papers",
            "tracks",
            "authors",
            "registration",
        ]
    };
    let mut scored: Vec<(usize, String)> = doc
        .links
        .iter()
        .filter(|l| l.url != doc.url && !l.url.ends_with(".pdf") && !l.url.contains("login"))
        .filter_map(|l| {
            let t = format!("{} {}", l.text, l.url).to_lowercase();
            let hits = needles.iter().filter(|n| t.contains(*n)).count();
            (hits > 0).then(|| (hits * 2 + usize::from(same_site(&l.url, &doc.url)) * 3, l.url.clone()))
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut out: Vec<String> = Vec::new();
    for (_, u) in scored {
        if !out.contains(&u) {
            out.push(u);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn prompt(venue: &Value, kind: &str, sources: &[Source]) -> String {
    let name = venue["name"].as_str().unwrap_or("");
    let today = today();
    let task = if kind == "journal" {
        "This is a journal. Find its acceptance rate, time to first decision, article processing charge (or that publishing is free), peer review type, word limit for research articles and submission system."
    } else {
        "This is a conference. Find the next edition that has not happened yet (dates and place), its next submission deadline for full papers, where the proceedings are published, a recent acceptance rate, page limits and review type. Ignore deadlines that have already passed."
    };
    let kw: Vec<&str> = if kind == "journal" {
        vec!["acceptance", "decision", "review", "word", "apc", "charge", "fee", "submit", "days"]
    } else {
        vec!["deadline", "due", "submission", "paper", "proceedings", "acceptance", "review", "page", "anonym"]
    };
    let mut pages = String::new();
    let per = (TOTAL_CHARS / sources.len().max(1)).min(PAGE_CHARS);
    for (i, s) in sources.iter().enumerate() {
        pages.push_str(&format!("\n[P{}] {}\nTitle: {}\n{}\n", i + 1, s.url, s.doc.title, s.doc.excerpt(&kw, per)));
    }
    format!(
        "Today is {today}. Extract facts about the academic venue \"{name}\" from the pages below. {task}\n\
         Use only what the pages say. For each field give the value, the URL of the page it came from (one of the [P#] URLs), \
         and a short exact quote copied from that page (under 200 characters) that shows the value. \
         Use null for the value, URL and quote when the pages do not say it. Do not guess and do not use outside knowledge. \
         Dates must be YYYY-MM-DD.\n\nPages:{pages}"
    )
}

fn web_prompt(venue: &Value, kind: &str, missing: &[&str]) -> String {
    let name = venue["name"].as_str().unwrap_or("");
    let known: Vec<String> = [("Field", "field"), ("Publisher", "publisher"), ("Next edition", "next_edition"), ("Website", "website_url")]
        .iter()
        .filter_map(|(l, k)| venue[*k].as_str().filter(|v| !v.is_empty()).map(|v| format!("{l}: {v}")))
        .collect();
    format!(
        "Today is {}. Look up the latest official information about the academic {} \"{name}\".\n{}\
         Find only these fields: {}.\nPrefer the venue’s own website, call for papers or publisher page. \
         For each field give the value, the URL of the page where you read it, and a short exact quote from that page. \
         Do not invent anything: use null for what you cannot confirm on a page.",
        today(),
        if kind == "journal" { "journal" } else { "conference" },
        if known.is_empty() { String::new() } else { format!("What is known so far:\n{}\n", known.join("\n")) },
        missing.join(", ")
    )
}

fn norm_url(u: &str) -> String {
    super::opportunities::normalize_url(u)
}

/// Check an AI answer against the fetched pages: a field is verified when its quote is on its source page
/// (or, without a usable URL, on any fetched page, which then becomes its source).
fn verify_against(ev: &mut Evidence, sources: &[Source]) {
    let Some(q) = ev.quote.clone() else { return };
    let named = ev.source_url.as_deref().map(norm_url);
    let page = sources
        .iter()
        .find(|s| named.as_deref().is_some_and(|n| n == norm_url(&s.url) || n == norm_url(&s.doc.url)) && quote_in(&q, &s.doc.text()))
        .or_else(|| sources.iter().find(|s| quote_in(&q, &s.doc.text())));
    if let Some(s) = page {
        ev.verified = true;
        ev.source_url = Some(s.url.clone());
    }
}

/// The value must also be in the quote for dates and numbers.
fn value_supported(field: &str, value: &Value, quote: Option<&str>) -> bool {
    let Some(q) = quote else { return false };
    match field {
        "deadline_on" => {
            value.as_str().and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok()).is_some_and(|d| extract::dates_in(q).contains(&d))
        }
        "acceptance_rate" => {
            let digits: String = value.as_str().unwrap_or("").chars().filter(char::is_ascii_digit).collect();
            digits.is_empty() || q.chars().filter(char::is_ascii_digit).collect::<String>().contains(&digits)
        }
        _ => true,
    }
}

#[derive(Debug, Clone, Default)]
pub struct VenueReport {
    pub id: i64,
    pub name: String,
    pub applied: Vec<String>,
    pub pending: Vec<String>,
    pub sources: Vec<String>,
    pub unverified: usize,
}

impl VenueReport {
    pub fn to_json(&self) -> Value {
        json!({ "venue_id": self.id, "name": self.name, "ok": true, "applied": self.applied, "pending": self.pending,
                "sources": self.sources, "unverified": self.unverified })
    }
}

/// Research one venue and record proposals. Network and AI work; call from async code.
pub async fn research(ctx: &Ctx, run: &Run, venue_id: i64) -> Result<VenueReport> {
    let venue = ctx.db.find("venues", venue_id)?.ok_or_else(ApiError::not_found)?;
    let name = venue["name"].as_str().unwrap_or("").to_string();
    let kind = if venue["kind"] == "journal" { "journal" } else { "conference" };
    let mut found: BTreeMap<String, Evidence> = BTreeMap::new();
    let put = |found: &mut BTreeMap<String, Evidence>, f: &str, e: Evidence| {
        let better = match found.get(f) {
            None => true,
            Some(old) => e.verified && (!old.verified || e.source_name == "Website"),
        };
        if better {
            found.insert(f.to_string(), e);
        }
    };

    // 1. Structured sources.
    if kind == "journal" {
        run.step(ctx, &format!("Looking up {name} on OpenAlex"));
        if let Some(src) = run.guard(openalex_source(&name)).await? {
            for (f, e) in source_evidence(&src) {
                put(&mut found, f, e);
            }
        }
    } else {
        run.step(ctx, &format!("Looking for {} on WikiCFP", acronym(&name)));
        if let Some(ev) = run.guard(wikicfp_event(&acronym(&name))).await? {
            let base = |value: Value, quote: Option<String>| Evidence {
                value,
                source_url: Some(ev.url.clone()),
                quote,
                source_name: "WikiCFP".into(),
                verified: true,
            };
            if let Some(d) = ev.deadline.filter(|d| *d >= today()) {
                put(&mut found, "deadline_on", base(json!(d.format("%Y-%m-%d").to_string()), ev.deadline_line.clone()));
                put(&mut found, "deadline_expected", base(json!(false), ev.deadline_line.clone()));
                put(&mut found, "deadline_label", base(json!("Submission deadline"), ev.deadline_line.clone()));
            }
            if let Some(t) = edition_text(&ev) {
                put(&mut found, "next_edition", base(json!(t), ev.when_line.clone()));
            }
            if let Some(l) = &ev.link {
                put(&mut found, "website_url", base(json!(l), Some(format!("Link: {l}"))));
            }
        }
    }
    run.check()?;

    // 2. The venue's own pages.
    let home = venue["website_url"]
        .as_str()
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .or_else(|| found.get("website_url").and_then(|e| e.value.as_str().map(str::to_string)));
    let mut sources: Vec<Source> = Vec::new();
    if let Some(home) = home {
        run.step(ctx, &format!("Reading {name} website"));
        match run.guard(fetcher().page(&home)).await? {
            Ok(p) if p.is_html() => {
                let doc = extract::html(&p.body, &p.final_url);
                let links = follow_links(&doc, kind, MAX_PAGES - 1);
                sources.push(Source { url: home.clone(), doc });
                if !links.is_empty() {
                    run.step(ctx, &format!("Fetching {} pages", links.len()));
                    let mut set = tokio::task::JoinSet::new();
                    for (i, l) in links.into_iter().enumerate() {
                        set.spawn(async move { (i, l.clone(), fetcher().page(&l).await) });
                    }
                    let mut got: Vec<(usize, Source)> = Vec::new();
                    while let Some(res) = run.guard(set.join_next()).await? {
                        if let Ok((i, l, Ok(p))) = res {
                            if p.is_html() {
                                got.push((i, Source { url: l, doc: extract::html(&p.body, &p.final_url) }));
                            }
                        }
                    }
                    got.sort_by_key(|(i, _)| *i);
                    sources.extend(got.into_iter().map(|(_, s)| s));
                }
            }
            Ok(_) => run.note(ctx, &format!("{home} is not a web page")),
            Err(e) => run.note(ctx, &format!("Could not read {home}: {e}")),
        }
    }
    run.check()?;

    // 3. AI extraction from the fetched text.
    let ai = providers::configured(&ctx.db);
    let mut unverified = 0usize;
    if ai && !sources.is_empty() {
        run.step(ctx, &format!("AI is reading {} {}", sources.len(), if sources.len() == 1 { "page" } else { "pages" }));
        let mut req = Ask::new(prompt(&venue, kind, &sources), extraction_schema(kind, None));
        req.cancel = Some(run.flag());
        req.timeout = 240;
        let ans = match providers::ask(&ctx.db, req).await {
            Ok(a) => a,
            // Keep the structured facts when the AI fails; fail only when nothing was found at all.
            Err(e) if e.to_string() != super::runs::CANCELLED && !found.is_empty() => {
                run.note(ctx, &format!("AI could not read the pages: {e}"));
                providers::Answer { data: json!({}), usage: Default::default() }
            }
            Err(e) => return Err(e),
        };
        run.usage(ctx, ans.usage.input, ans.usage.output);
        for (f, _, _) in field_defs(kind) {
            if let Some(mut e) = ans.data.get(*f).and_then(|v| evidence_from(v, "Website")) {
                verify_against(&mut e, &sources);
                if e.verified && !value_supported(f, &e.value, e.quote.as_deref()) {
                    e.verified = false;
                }
                put(&mut found, f, e);
            }
        }
    } else if !ai {
        run.note(ctx, "AI is not set up; only OpenAlex and WikiCFP facts were used.");
    }
    run.check()?;

    // 4. Web search fallback for key fields still missing.
    let missing: Vec<&str> =
        key_fields(kind).iter().copied().filter(|f| !found.get(*f).is_some_and(|e| e.verified) && blank(&venue[*f])).collect();
    if ai && !missing.is_empty() && providers::supports_web(&ctx.db) && super::flag(&ctx.db, "research.web_fallback") {
        run.step(ctx, &format!("Searching the web for {}", missing.join(", ").replace('_', " ")));
        let mut req = Ask::new(web_prompt(&venue, kind, &missing), extraction_schema(kind, Some(&missing)));
        req.web = true;
        req.cancel = Some(run.flag());
        req.timeout = 300;
        match providers::ask(&ctx.db, req).await {
            Ok(ans) => {
                run.usage(ctx, ans.usage.input, ans.usage.output);
                for f in &missing {
                    let Some(mut e) = ans.data.get(*f).and_then(|v| evidence_from(v, "AI web search")) else {
                        continue;
                    };
                    // Check the quote on the page the model cites.
                    if let (Some(u), Some(q)) = (e.source_url.clone(), e.quote.clone()) {
                        if let Ok(Ok(p)) = run.guard(fetcher().page(&u)).await {
                            let doc = extract::html(&p.body, &p.final_url);
                            e.verified = quote_in(&q, &doc.text()) && value_supported(f, &e.value, Some(&q));
                        }
                    }
                    put(&mut found, f, e);
                }
            }
            Err(e) if e.to_string() == super::runs::CANCELLED => return Err(e),
            Err(e) => run.note(ctx, &format!("Web search failed: {e}")),
        }
    }
    run.check()?;

    // 5. Validate and record.
    run.step(ctx, "Checking dates and values");
    let mut drafts = Vec::new();
    for (f, e) in &found {
        match clean_field(f, &e.value) {
            Ok(v) => {
                if !e.verified {
                    unverified += 1;
                }
                drafts.push(Draft::new(f, v, e));
            }
            Err(why) => run.note(ctx, &format!("{name}: dropped {f} ({why})")),
        }
    }
    // A deadline label/estimate only makes sense next to a deadline.
    if !drafts.iter().any(|d| d.field == "deadline_on") {
        drafts.retain(|d| d.field != "deadline_label" && d.field != "deadline_expected");
    }
    let out = proposals::record(ctx, "venue", venue_id, drafts, Some(run.id))?;
    let mut urls: Vec<String> = found.values().filter_map(|e| e.source_url.clone()).filter(|u| u.starts_with("http")).collect();
    urls.extend(sources.iter().map(|s| s.url.clone()));
    let mut seen = std::collections::HashSet::new();
    urls.retain(|u| seen.insert(norm_url(u)));
    urls.truncate(8);
    ctx.db.update("venues", venue_id, &json!({ "sources": urls, "checked_at": crate::util::now_ts() }))?;
    Ok(VenueReport { id: venue_id, name, applied: out.applied, pending: out.pending, sources: urls, unverified })
}

/// Venues for the scheduled refresh: watched and not checked within `stale_days` (like `venues:refresh`).
pub fn stale_watched(ctx: &Ctx, stale_days: i64) -> Result<Vec<i64>> {
    let cutoff = crate::util::ts_minutes_ago(stale_days * 24 * 60);
    let rows = ctx.db.rows(
        "SELECT id FROM venues WHERE user_id = ?1 AND watch = 1 AND (checked_at IS NULL OR checked_at < ?2) ORDER BY id",
        rusqlite::params![USER_ID, cutoff],
    )?;
    Ok(rows.iter().filter_map(|r| r["id"].as_i64()).collect())
}

/// Research several venues as one run (3 at a time). `scheduled` uses the weekly texts.
pub async fn refresh_many(ctx: Ctx, run: Run, ids: Vec<i64>, scheduled: bool) {
    super::fetch::init_cache(&ctx);
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut set = tokio::task::JoinSet::new();
    for id in ids.clone() {
        let (ctx, run, sem) = (ctx.clone(), run.clone(), sem.clone());
        set.spawn(async move {
            let _permit = sem.acquire().await;
            if run.cancelled() {
                return (id, Err(ApiError::msg(super::runs::CANCELLED)));
            }
            let name: String = ctx.db.scalar("SELECT name FROM venues WHERE id = ?1", [id]).ok().flatten().unwrap_or_default();
            run.step(&ctx, &format!("Checking {name}"));
            let r = research(&ctx, &run, id).await;
            (id, r)
        });
    }
    let (mut ok, mut failed, mut pending, mut applied) = (0, 0, 0, 0);
    while let Some(res) = set.join_next().await {
        let Ok((id, r)) = res else { continue };
        match r {
            Ok(rep) => {
                ok += 1;
                pending += rep.pending.len();
                applied += rep.applied.len();
                run.advance(&ctx, None);
                run.partial(&ctx, json!({ "venue": rep.to_json() }));
            }
            Err(e) => {
                if e.to_string() != super::runs::CANCELLED {
                    failed += 1;
                    let name: String = ctx.db.scalar("SELECT name FROM venues WHERE id = ?1", [id]).ok().flatten().unwrap_or_default();
                    run.note(&ctx, &format!("Error on {name}: {e}"));
                    run.advance(&ctx, None);
                    run.partial(&ctx, json!({ "venue": { "venue_id": id, "name": name, "ok": false, "error": e.to_string() } }));
                }
            }
        }
    }
    let cancelled = run.cancelled();
    let result = json!({ "checked": ok + failed, "ok": ok, "failed": failed, "applied": applied, "pending": pending });
    run.finish(&ctx, cancelled.then_some(super::runs::CANCELLED), Some(result));
    ctx.changed("venues");
    ctx.changed("proposals");
    if cancelled {
        return;
    }
    let review =
        if pending > 0 { format!(" {pending} {} to review.", if pending == 1 { "change" } else { "changes" }) } else { String::new() };
    let body = if scheduled {
        format!(
            "{ok} {} updated{}{review}",
            if ok == 1 { "journal or conference" } else { "journals and conferences" },
            if failed > 0 { format!(", {failed} failed.") } else { ".".into() }
        )
    } else {
        format!(
            "{} {} checked{}{review}",
            ok + failed,
            if ok + failed == 1 { "journal or conference" } else { "journals and conferences" },
            if failed > 0 { format!(", {failed} failed.") } else { ".".into() }
        )
    };
    let _ = ctx.notify(
        "ai_job_finished",
        json!({ "title": "Journal & conference update finished", "body": body, "status": if failed > 0 { "warning" } else { "success" }, "url": "#/venues" }),
    );
}

/// Research one venue as its own run (the venue row's "Update" button).
pub async fn refresh_one(ctx: Ctx, run: Run, id: i64) {
    super::fetch::init_cache(&ctx);
    let r = research(&ctx, &run, id).await;
    match r {
        Ok(rep) => {
            run.advance(&ctx, None);
            run.finish(&ctx, None, Some(rep.to_json()));
        }
        Err(e) => run.finish(&ctx, Some(&e.to_string()), Some(json!({ "venue_id": id, "ok": false }))),
    }
    ctx.changed("venues");
    ctx.changed("proposals");
}

/// The `venues:refresh` job (blocking; runs on the scheduler's thread).
pub fn scheduled_refresh(ctx: &Ctx) -> Result<()> {
    if ctx.user()?.is_none() {
        return Ok(());
    }
    let ids = stale_watched(ctx, 7)?;
    if ids.is_empty() {
        return Ok(());
    }
    let run = Run::begin(ctx, "venues", "Weekly journal & conference update", ids.len() as i64, Some("#/venues"))?;
    ctx.block_on(refresh_many(ctx.clone(), run, ids, true));
    Ok(())
}

// ---- Suggestions from the user's research interests -----------------------------------------------------
//
// Nothing field-specific is built in: every user works in a different field. For each research interest,
// OpenAlex (free, no key, no personal data in the request) is asked which journals and conference proceedings
// published the most English works with that phrase in the title or abstract over the last three years. A
// venue's score for an interest is its number of such works divided by the square root of all its works, so a
// focused journal ranks above a mega-journal that prints a few such papers among thousands. Scores are scaled
// so the best venue for each interest gets 1, then added up over the interests.

/// Fewest matching works for a venue to be suggested for an interest.
const MIN_HITS: i64 = 3;
/// Interests used (the rest are ignored, to keep the number of requests small).
pub const MAX_TERMS: usize = 8;

/// One suggested venue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Suggestion {
    pub openalex_id: String,
    pub name: String,
    pub kind: String,
    pub publisher: Option<String>,
    pub website_url: Option<String>,
    pub score: f64,
    /// `(interest, matching works)`, best first.
    pub hits: Vec<(String, i64)>,
}

/// Facts about one OpenAlex source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceFacts {
    pub name: String,
    pub kind: String,
    pub works: i64,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
}

/// Interests as separate phrases: split on commas, semicolons and new lines; at most [`MAX_TERMS`].
pub fn interest_terms(interests: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in interests.split([',', ';', '\n']) {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.chars().filter(|c| c.is_alphanumeric()).count() >= 2 && !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    out.truncate(MAX_TERMS);
    out
}

/// OpenAlex search phrase: letters, digits, spaces and hyphens only (the filter syntax uses `,` `|` `:`).
fn phrase(term: &str) -> String {
    let t: String = term.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { ' ' }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn group_url(term: &str, from: &str) -> String {
    let q = format!("\"{}\"", phrase(term));
    format!(
        "https://api.openalex.org/works?filter=title_and_abstract.search:{},from_publication_date:{from},language:en&group_by=primary_location.source.id&per-page=60",
        enc(&q)
    )
}

/// `(source id, works)` from an OpenAlex `group_by` response.
pub fn parse_groups(resp: &Value) -> Vec<(String, i64)> {
    resp.get("group_by")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|g| {
                    let key = g.get("key")?.as_str()?;
                    let id = key.rsplit('/').next()?.to_string();
                    (id.starts_with('S') && id.len() > 1).then(|| (id, g.get("count").and_then(Value::as_i64).unwrap_or(0)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Facts from an OpenAlex `/sources` response, by source id.
pub fn parse_sources(resp: &Value) -> BTreeMap<String, SourceFacts> {
    let mut out = BTreeMap::new();
    for r in resp.get("results").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = r.get("id").and_then(Value::as_str).and_then(|u| u.rsplit('/').next()) else {
            continue;
        };
        out.insert(
            id.to_string(),
            SourceFacts {
                name: r.get("display_name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                kind: r.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
                works: r.get("works_count").and_then(Value::as_i64).unwrap_or(0),
                publisher: r.get("host_organization_name").and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty()),
                homepage: r.get("homepage_url").and_then(|u| clean_url(u, 500)),
            },
        );
    }
    out
}

/// Rank venues over all interests (see the section comment). Only journals and conference proceedings.
pub fn rank(groups: &[(String, Vec<(String, i64)>)], facts: &BTreeMap<String, SourceFacts>) -> Vec<Suggestion> {
    let mut by_id: BTreeMap<String, Suggestion> = BTreeMap::new();
    for (term, list) in groups {
        let scored: Vec<(&String, i64, f64)> = list
            .iter()
            .filter(|(_, n)| *n >= MIN_HITS)
            .filter_map(|(id, n)| {
                let f = facts.get(id)?;
                (matches!(f.kind.as_str(), "journal" | "conference") && !f.name.is_empty())
                    .then(|| (id, *n, *n as f64 / (f.works.max(*n).max(1) as f64).sqrt()))
            })
            .collect();
        let top = scored.iter().map(|s| s.2).fold(0.0_f64, f64::max);
        if top <= 0.0 {
            continue;
        }
        for (id, n, sc) in scored {
            let f = &facts[id];
            let e = by_id.entry(id.clone()).or_insert_with(|| Suggestion {
                openalex_id: id.clone(),
                name: f.name.clone(),
                kind: f.kind.clone(),
                publisher: f.publisher.clone(),
                website_url: f.homepage.clone(),
                ..Default::default()
            });
            e.score += sc / top;
            e.hits.push((term.clone(), n));
        }
    }
    let mut out: Vec<Suggestion> = by_id.into_values().collect();
    for s in &mut out {
        s.hits.sort_by_key(|h| std::cmp::Reverse(h.1));
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Suggest journals and conferences for the interests. Returns `(suggestions, interests that failed)`.
pub async fn suggest(interests: &str) -> (Vec<Suggestion>, Vec<String>) {
    let terms = interest_terms(interests);
    let from = (today() - chrono::Duration::days(3 * 365)).format("%Y-%m-%d").to_string();
    let mut set = tokio::task::JoinSet::new();
    for t in terms.clone() {
        let url = group_url(&t, &from);
        set.spawn(async move { (t, fetcher().json(&url).await) });
    }
    let mut groups: Vec<(String, Vec<(String, i64)>)> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    while let Some(r) = set.join_next().await {
        match r {
            Ok((t, Ok(v))) => groups.push((t, parse_groups(&v))),
            Ok((t, Err(_))) => failed.push(t),
            Err(_) => {}
        }
    }
    // Keep the interests in the order the user wrote them.
    groups.sort_by_key(|(t, _)| terms.iter().position(|x| x == t).unwrap_or(usize::MAX));
    let mut ids: Vec<String> = Vec::new();
    for (_, list) in &groups {
        for (id, n) in list {
            if *n >= MIN_HITS && !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    let mut facts = BTreeMap::new();
    for chunk in ids.chunks(50) {
        let url = format!(
            "https://api.openalex.org/sources?filter=ids.openalex:{}&per-page=50&select=id,display_name,type,works_count,host_organization_name,homepage_url",
            chunk.join("|")
        );
        if let Ok(v) = fetcher().json(&url).await {
            facts.extend(parse_sources(&v));
        }
    }
    (rank(&groups, &facts), failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_fields() {
        let next_year = format!("{}-03-01", chrono::Datelike::year(&today()) + 1);
        assert_eq!(clean_field("deadline_on", &json!(next_year)).unwrap(), json!(next_year));
        assert!(clean_field("deadline_on", &json!("2020-01-01")).unwrap_err().contains("passed"));
        assert!(clean_field("deadline_on", &json!("soon")).is_err());
        // Rankings are never extracted or proposed.
        assert!(clean_field("quartile", &json!("Q2")).is_err());
        assert!(clean_field("rank", &json!("CORE A*")).is_err());
        assert!(!extraction_schema("journal", None)["properties"].as_object().unwrap().contains_key("quartile"));
        assert!(!extraction_schema("conference", None)["properties"].as_object().unwrap().contains_key("rank"));
        assert_eq!(clean_field("review_type", &json!("Double-blind")).unwrap(), "double");
        assert!(clean_field("acceptance_rate", &json!("selective")).is_err());
        assert!(clean_field("website_url", &json!("javascript:x")).is_err());
        assert_eq!(clean_field("deadline_expected", &json!(true)).unwrap(), true);
    }

    #[test]
    fn openalex_pick_and_facts() {
        let resp = json!({ "results": [
            { "id": "https://openalex.org/S1", "display_name": "Information Systems", "host_organization_name": "X" },
            { "id": "https://openalex.org/S92260147", "display_name": "Information Systems Journal", "host_organization_name": "Wiley",
              "homepage_url": "http://onlinelibrary.wiley.com/journal/10.1111/(ISSN)1365-2575", "issn_l": "1350-1917", "issn": ["1350-1917", "1365-2575"],
              "apc_usd": 5110, "apc_prices": [{ "price": 5110, "currency": "USD" }], "is_oa": false, "is_in_doaj": false, "summary_stats": { "h_index": 164 }, "works_count": 1395 }
        ]});
        let s = pick_source(&resp, "Information Systems Journal").unwrap();
        assert_eq!(s.issns.len(), 2);
        assert_eq!(s.h_index, Some(164));
        let f = source_evidence(&s);
        assert_eq!(f["fee"].value, "APC US$5,110 (optional open access)");
        assert_eq!(f["publisher"].value, "Wiley");
        assert!(f["publisher"].verified);
        assert!(pick_source(&resp, "Journal of Documentation").is_none());
    }

    #[test]
    fn wikicfp_pages() {
        let html = r#"<html><head><title>MobileHCI 2027 : The ACM International Conference on Mobile Human-Computer Interaction</title></head><body>
          <table><tr><td><table class="gglu"><tr><th>When</th><td>Sep 20, 2027 - Sep 23, 2027</td></tr>
          <tr><th>Where</th><td>Limassol, Cyprus</td></tr>
          <tr><th>Abstract Registration Due</th><td><span>Feb 10, 2027</span></td></tr>
          <tr><th>Submission Deadline</th><td><span>Feb 17, 2027</span></td></tr></table></td></tr></table>
          <div>Link: <a href="https://mobilehci.acm.org/2027/">https://mobilehci.acm.org/2027/</a></div>
          <h5><a href="../call?conference=HCI">HCI</a></h5></body></html>"#;
        let doc = extract::html(html, "http://www.wikicfp.com/cfp/servlet/event.showcfp?eventid=203910");
        let e = parse_wikicfp_event(&doc);
        assert_eq!(e.deadline, NaiveDate::from_ymd_opt(2027, 2, 17));
        assert_eq!(e.abstract_due, NaiveDate::from_ymd_opt(2027, 2, 10));
        assert_eq!(e.link.as_deref(), Some("https://mobilehci.acm.org/2027/"));
        assert_eq!(edition_text(&e).as_deref(), Some("20–23 Sep 2027, Limassol, Cyprus"));
        assert_eq!(e.categories, vec!["HCI"]);
        assert!(quote_in(e.deadline_line.as_deref().unwrap(), &doc.text()));

        let search = r#"<table><tr><td><a href="/cfp/servlet/event.showcfp?eventid=1">CHI 2027</a></td></tr>
          <tr><td><a href="/cfp/servlet/event.showcfp?eventid=2">CHI 2026</a></td></tr>
          <tr><td><a href="/cfp/servlet/event.showcfp?eventid=3">CHIIR 2027</a></td></tr>
          <tr><td><a href="/cfp/servlet/event.showcfp?eventid=4">FAccT 2027</a></td></tr></table>"#;
        let d = extract::html(search, "http://www.wikicfp.com/cfp/servlet/tool.search?q=CHI");
        let hits = wikicfp_search_hits(&d, "CHI");
        assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![2026, 2027]);
        assert_eq!(wikicfp_search_hits(&d, "ACM FAccT").len(), 1);
        assert_eq!(acronym("CSCW (ACM Conference on Computer-Supported Cooperative Work)"), "CSCW");
    }

    #[test]
    fn links_and_verification() {
        let home = r#"<a href="/cfp">Call for Papers</a><a href="/dates.html">Important Dates</a><a href="https://twitter.com/x">Twitter</a><a href="/program">Program</a>"#;
        let doc = extract::html(home, "https://chi2027.acm.org/");
        let links = follow_links(&doc, "conference", 4);
        assert_eq!(links, vec!["https://chi2027.acm.org/cfp", "https://chi2027.acm.org/dates.html"]);
        let src = Source {
            url: "https://chi2027.acm.org/cfp".into(),
            doc: extract::html("<p>Papers due: January 21, 2027 (AoE)</p>", "https://chi2027.acm.org/cfp"),
        };
        let mut e = Evidence {
            value: json!("2027-01-21"),
            source_url: None,
            quote: Some("Papers due: January 21, 2027".into()),
            source_name: "Website".into(),
            verified: false,
        };
        verify_against(&mut e, std::slice::from_ref(&src));
        assert!(e.verified);
        assert_eq!(e.source_url.as_deref(), Some("https://chi2027.acm.org/cfp"));
        assert!(value_supported("deadline_on", &e.value, e.quote.as_deref()));
        assert!(!value_supported("deadline_on", &json!("2027-01-22"), e.quote.as_deref()));
        let mut bad = Evidence { quote: Some("Papers due: 3 March 2027".into()), verified: false, ..e.clone() };
        verify_against(&mut bad, &[src]);
        assert!(!bad.verified);
    }

    #[test]
    fn schemas_are_strict() {
        let s = extraction_schema("conference", None);
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"].as_array().unwrap().len(), CONF_FIELDS.len());
        assert_eq!(s["properties"]["deadline_on"]["required"], json!(["value", "source_url", "quote"]));
        let only = extraction_schema("journal", Some(&["fee"]));
        assert_eq!(only["required"], json!(["fee"]));
    }

    #[test]
    fn suggestions_rank_specialised_venues() {
        assert_eq!(interest_terms("polymer chemistry; Catalysis,\ncatalysis , x"), vec!["polymer chemistry", "Catalysis"]);
        assert_eq!(phrase("C-H activation: \"new\""), "C-H activation new");
        let groups = vec![
            ("polymer chemistry".to_string(), vec![("S1".into(), 40), ("S2".into(), 300), ("S3".into(), 2), ("S4".into(), 50)]),
            ("catalysis".to_string(), vec![("S1".into(), 10), ("S5".into(), 90)]),
        ];
        let f = |name: &str, kind: &str, works: i64| SourceFacts { name: name.into(), kind: kind.into(), works, ..Default::default() };
        let facts: BTreeMap<String, SourceFacts> = [
            ("S1".to_string(), f("Polymer Chemistry", "journal", 1600)),
            ("S2".to_string(), f("Scientific Reports", "journal", 250000)),
            ("S3".to_string(), f("Tiny", "journal", 10)),
            ("S4".to_string(), f("arXiv", "repository", 2000000)),
            ("S5".to_string(), f("ACS Catalysis", "journal", 8100)),
        ]
        .into_iter()
        .collect();
        let r = rank(&groups, &facts);
        let names: Vec<&str> = r.iter().map(|s| s.name.as_str()).collect();
        // Specialised journals first, the mega-journal last, repositories and venues below MIN_HITS left out.
        assert_eq!(names, vec!["Polymer Chemistry", "ACS Catalysis", "Scientific Reports"]);
        assert_eq!(r[0].hits, vec![("polymer chemistry".to_string(), 40), ("catalysis".to_string(), 10)]);
        let g = parse_groups(&json!({ "group_by": [{ "key": "https://openalex.org/S9", "count": 7 }, { "key": "unknown", "count": 3 }] }));
        assert_eq!(g, vec![("S9".to_string(), 7)]);
    }
}
