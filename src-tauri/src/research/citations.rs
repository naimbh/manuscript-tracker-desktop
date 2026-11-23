//! Publication details and citation counts from free indexes (port of CitationUpdater and the
//! publications:refresh command), no AI needed: OpenAlex by DOI or title, Crossref (DOI, volume, issue,
//! pages, publisher, ISSN), Semantic Scholar as a fallback. Title matches must be near-exact (and within a
//! year when both years are known).
//!
//! Citation counts are a tracked metric: the highest count among the indexes is applied when it is higher
//! than the stored one (Google Scholar counts, from the profile refresh, are usually higher and are never
//! lowered). Other fields become proposals (empty fields are filled when verified).

use super::fetch::fetcher;
use super::proposals::{self, Draft};
use super::runs::Run;
use super::{similarity, Evidence};
use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use serde_json::{json, Value};

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// `https://doi.org/10.1/X` -> `10.1/x`-style DOI as typed (prefix removed, trimmed).
pub fn clean_doi(s: &str) -> Option<String> {
    let s = s.trim();
    let lower = s.to_lowercase();
    let mut d = s;
    for p in ["https://doi.org/", "http://doi.org/", "https://dx.doi.org/", "http://dx.doi.org/", "doi:", "doi.org/"] {
        if lower.starts_with(p) {
            d = &s[p.len()..];
            break;
        }
    }
    let d = d.trim().trim_end_matches(['.', ',', ';']);
    (d.starts_with("10.") && d.contains('/') && d.len() <= 255).then(|| d.to_string())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    pub source: &'static str,
    /// Page for people (OpenAlex work, Crossref API record, Semantic Scholar paper).
    pub source_url: String,
    pub title: Option<String>,
    pub doi: Option<String>,
    pub year: Option<i64>,
    pub venue: Option<String>,
    pub publisher: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub url: Option<String>,
    pub citations: Option<i64>,
    pub issns: Vec<String>,
}

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(|x| match x {
        Value::String(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

pub fn openalex_work(v: &Value) -> Option<Work> {
    let id = s(v, "/id")?;
    let pages = match (s(v, "/biblio/first_page"), s(v, "/biblio/last_page")) {
        (Some(a), Some(b)) if a != b => Some(format!("{a}-{b}")),
        (Some(a), _) => Some(a),
        _ => None,
    };
    Some(Work {
        source: "OpenAlex",
        source_url: id,
        title: s(v, "/display_name").or_else(|| s(v, "/title")),
        doi: s(v, "/doi").and_then(|d| clean_doi(&d)),
        year: v.get("publication_year").and_then(Value::as_i64),
        venue: s(v, "/primary_location/source/display_name"),
        publisher: s(v, "/primary_location/source/host_organization_name"),
        volume: s(v, "/biblio/volume"),
        issue: s(v, "/biblio/issue"),
        pages,
        url: s(v, "/primary_location/landing_page_url"),
        citations: v.get("cited_by_count").and_then(Value::as_i64),
        issns: v
            .pointer("/primary_location/source/issn")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
    })
}

pub fn crossref_work(v: &Value) -> Option<Work> {
    let doi = s(v, "/DOI").and_then(|d| clean_doi(&d))?;
    let year = v
        .pointer("/issued/date-parts/0/0")
        .and_then(Value::as_i64)
        .or_else(|| v.pointer("/published/date-parts/0/0").and_then(Value::as_i64));
    Some(Work {
        source: "Crossref",
        source_url: format!("https://api.crossref.org/works/{}", enc(&doi)),
        title: s(v, "/title/0"),
        doi: Some(doi),
        year,
        venue: s(v, "/container-title/0"),
        publisher: s(v, "/publisher"),
        volume: s(v, "/volume"),
        issue: s(v, "/issue"),
        pages: s(v, "/page"),
        url: s(v, "/resource/primary/URL").or_else(|| s(v, "/URL")),
        citations: v.get("is-referenced-by-count").and_then(Value::as_i64),
        issns: v
            .get("ISSN")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
    })
}

pub fn s2_work(v: &Value) -> Option<Work> {
    let id = s(v, "/paperId")?;
    Some(Work {
        source: "Semantic Scholar",
        source_url: format!("https://www.semanticscholar.org/paper/{id}"),
        title: s(v, "/title"),
        doi: s(v, "/externalIds/DOI").and_then(|d| clean_doi(&d)),
        year: v.get("year").and_then(Value::as_i64),
        venue: s(v, "/venue"),
        citations: v.get("citationCount").and_then(Value::as_i64),
        ..Default::default()
    })
}

/// Whether a search hit is the same publication.
pub fn matches(p: &Value, w: &Work) -> bool {
    let title = p["title"].as_str().unwrap_or("");
    let Some(wt) = &w.title else { return false };
    let year_ok = match (p["year"].as_i64(), w.year) {
        (Some(a), Some(b)) => (a - b).abs() <= 1,
        _ => true,
    };
    year_ok && similarity(title, wt) >= 0.92
}

async fn lookup(p: &Value) -> Vec<Work> {
    let doi = p["doi"].as_str().and_then(clean_doi);
    let title = p["title"].as_str().unwrap_or("").to_string();
    let mut works = Vec::new();
    match &doi {
        Some(d) => {
            let (u1, u2, u3) = (
                format!("https://api.openalex.org/works/doi:{d}"),
                format!("https://api.crossref.org/works/{}", enc(d)),
                format!("https://api.semanticscholar.org/graph/v1/paper/DOI:{d}?fields=title,venue,year,citationCount,externalIds"),
            );
            let (oa, cr, ss) = tokio::join!(fetcher().json(&u1), fetcher().json(&u2), fetcher().json(&u3));
            works.extend(oa.ok().and_then(|v| openalex_work(&v)));
            works.extend(cr.ok().and_then(|v| v.get("message").and_then(crossref_work)));
            works.extend(ss.ok().and_then(|v| s2_work(&v)));
        }
        None if title.len() > 8 => {
            let q = enc(&title);
            let (u1, u2) = (
                format!("https://api.openalex.org/works?search={q}&per-page=5"),
                format!("https://api.crossref.org/works?query.bibliographic={q}&rows=5"),
            );
            let (oa, cr) = tokio::join!(fetcher().json(&u1), fetcher().json(&u2));
            let pick = |list: Option<&Vec<Value>>, f: fn(&Value) -> Option<Work>| {
                list.and_then(|l| l.iter().filter_map(f).find(|w| matches(p, w)))
            };
            works.extend(pick(oa.as_ref().ok().and_then(|v| v.get("results")).and_then(Value::as_array), openalex_work));
            works.extend(pick(cr.as_ref().ok().and_then(|v| v.pointer("/message/items")).and_then(Value::as_array), crossref_work));
            if works.is_empty() {
                let ss = fetcher().json(&format!("https://api.semanticscholar.org/graph/v1/paper/search?query={q}&limit=5&fields=title,venue,year,citationCount,externalIds")).await;
                works.extend(pick(ss.as_ref().ok().and_then(|v| v.get("data")).and_then(Value::as_array), s2_work));
            }
        }
        None => {}
    }
    works
}

fn ev(w: &Work, quote: String) -> Evidence {
    Evidence {
        value: Value::Null,
        source_url: Some(w.source_url.clone()),
        quote: Some(quote),
        source_name: w.source.into(),
        verified: true,
    }
}

/// Proposals for one publication from the works found (first source with a value wins; Crossref is
/// preferred for bibliographic details, OpenAlex for the landing page).
pub fn drafts_for(p: &Value, works: &[Work]) -> Vec<Draft> {
    let mut out: Vec<Draft> = Vec::new();
    let mut order: Vec<&Work> = works.iter().collect();
    order.sort_by_key(|w| match w.source {
        "Crossref" => 0,
        "OpenAlex" => 1,
        _ => 2,
    });
    let mut add = |field: &str, value: Option<Value>, w: &Work, quote: String| {
        if let Some(v) = value {
            if !out.iter().any(|d| d.field == field) {
                out.push(Draft::new(field, v, &ev(w, quote)));
            }
        }
    };
    for w in &order {
        add("doi", w.doi.clone().map(Value::String), w, format!("DOI: {}", w.doi.clone().unwrap_or_default()));
        add(
            "volume",
            w.volume.clone().map(|v| json!(super::limit(&v, 40))),
            w,
            format!("volume: {}", w.volume.clone().unwrap_or_default()),
        );
        add("issue", w.issue.clone().map(|v| json!(super::limit(&v, 40))), w, format!("issue: {}", w.issue.clone().unwrap_or_default()));
        add("pages", w.pages.clone().map(|v| json!(super::limit(&v, 40))), w, format!("pages: {}", w.pages.clone().unwrap_or_default()));
        add("year", w.year.filter(|y| (1900..=2100).contains(y)).map(|y| json!(y)), w, format!("year: {}", w.year.unwrap_or(0)));
        add("venue", w.venue.clone().map(Value::String), w, format!("venue: {}", w.venue.clone().unwrap_or_default()));
        add("publisher", w.publisher.clone().map(Value::String), w, format!("publisher: {}", w.publisher.clone().unwrap_or_default()));
    }
    for w in works.iter().filter(|w| w.source == "OpenAlex").chain(works.iter()) {
        if let Some(u) = w.url.clone().filter(|u| super::clean_url(&json!(u), 500).is_some()) {
            add("url", Some(json!(u)), w, format!("landing page: {u}"));
        }
    }
    // Citations: the highest index count, only upwards.
    if let Some(best) = works.iter().filter(|w| w.citations.is_some()).max_by_key(|w| w.citations) {
        let n = best.citations.unwrap_or(0);
        let current = p["citations"].as_i64();
        if current.is_none_or(|c| n > c) {
            let mut d = Draft::new("citations", json!(n), &ev(best, format!("cited by {n}")));
            d.metric = true;
            out.push(d);
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct PubReport {
    pub id: i64,
    pub title: String,
    pub before: Option<i64>,
    pub after: Option<i64>,
    pub applied: Vec<String>,
    pub pending: Vec<String>,
    pub found: bool,
}

impl PubReport {
    pub fn to_json(&self) -> Value {
        json!({ "publication_id": self.id, "title": self.title, "ok": true, "found": self.found, "before": self.before, "after": self.after,
                "applied": self.applied, "pending": self.pending })
    }
}

/// Refresh one publication from the indexes and record proposals.
pub async fn refresh_publication(ctx: &Ctx, run: &Run, id: i64) -> Result<PubReport> {
    let p = ctx.db.find("publications", id)?.ok_or_else(ApiError::not_found)?;
    let title = p["title"].as_str().unwrap_or("").to_string();
    let before = p["citations"].as_i64();
    let works = run.guard(lookup(&p)).await?;
    let drafts = drafts_for(&p, &works);
    let out = proposals::record(ctx, "publication", id, drafts, Some(run.id))?;
    let mut sources: Vec<String> = works.iter().map(|w| w.source_url.clone()).collect();
    if let Some(u) = works.iter().find_map(|w| w.url.clone()) {
        sources.insert(0, u);
    }
    sources.dedup();
    sources.truncate(6);
    let mut upd = json!({ "checked_at": crate::util::now_ts() });
    if !sources.is_empty() {
        upd["sources"] = json!(sources);
    }
    ctx.db.update("publications", id, &upd)?;
    let after = ctx.db.find("publications", id)?.and_then(|r| r["citations"].as_i64());
    Ok(PubReport { id, title, before, after, applied: out.applied, pending: out.pending, found: !works.is_empty() })
}

/// Publications not checked within `stale_days` (all when `None`).
pub fn stale(ctx: &Ctx, stale_days: Option<i64>) -> Result<Vec<i64>> {
    let rows = match stale_days {
        Some(d) => ctx.db.rows(
            "SELECT id FROM publications WHERE user_id = ?1 AND (checked_at IS NULL OR checked_at < ?2) ORDER BY id",
            rusqlite::params![USER_ID, crate::util::ts_minutes_ago(d * 24 * 60)],
        )?,
        None => ctx.db.rows("SELECT id FROM publications WHERE user_id = ?1 ORDER BY id", [USER_ID])?,
    };
    Ok(rows.iter().filter_map(|r| r["id"].as_i64()).collect())
}

/// Refresh several publications as one run (4 at a time). Notifies like the web page's "update all".
pub async fn refresh_many(ctx: Ctx, run: Run, ids: Vec<i64>, notify: bool) -> (usize, usize) {
    super::fetch::init_cache(&ctx);
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut set = tokio::task::JoinSet::new();
    for id in ids {
        let (ctx, run, sem) = (ctx.clone(), run.clone(), sem.clone());
        set.spawn(async move {
            let _permit = sem.acquire().await;
            if run.cancelled() {
                return (id, Err(ApiError::msg(super::runs::CANCELLED)));
            }
            let title: String = ctx.db.scalar("SELECT title FROM publications WHERE id = ?1", [id]).ok().flatten().unwrap_or_default();
            run.step(&ctx, &format!("Checking {}", super::limit(&title, 80)));
            (id, refresh_publication(&ctx, &run, id).await)
        });
    }
    let (mut changed, mut failed) = (0usize, 0usize);
    while let Some(res) = set.join_next().await {
        let Ok((id, r)) = res else { continue };
        match r {
            Ok(rep) => {
                if rep.after != rep.before {
                    changed += 1;
                }
                run.advance(&ctx, None);
                run.partial(&ctx, json!({ "publication": rep.to_json() }));
            }
            Err(e) if e.to_string() == super::runs::CANCELLED => {}
            Err(e) => {
                failed += 1;
                run.note(&ctx, &format!("Error on publication {id}: {e}"));
                run.advance(&ctx, None);
                run.partial(&ctx, json!({ "publication": { "publication_id": id, "ok": false, "error": e.to_string() } }));
            }
        }
    }
    let cancelled = run.cancelled();
    run.finish(&ctx, cancelled.then_some(super::runs::CANCELLED), Some(json!({ "changed": changed, "failed": failed })));
    ctx.changed("publications");
    ctx.changed("proposals");
    if notify && !cancelled {
        let body = format!("{changed} counts changed{}", if failed > 0 { format!(", {failed} failed.") } else { ".".into() });
        let _ = ctx.notify("ai_job_finished", json!({ "title": "Citations updated", "body": body, "status": if failed > 0 { "warning" } else { "success" }, "url": "#/publications" }));
    }
    (changed, failed)
}

/// The `publications:refresh` job: Scholar profile first (when linked), then stale publications (6 days).
pub fn scheduled_refresh(ctx: &Ctx) -> Result<()> {
    let Some(user) = ctx.user()? else {
        return Ok(());
    };
    if user["scholar_url"].as_str().is_some_and(|u| !u.is_empty()) {
        let run = Run::begin(ctx, super::runs::SCHOLAR_KIND, super::runs::SCHOLAR_LABEL, 1, Some("#/publications"))?;
        ctx.block_on(super::scholar::refresh_job(ctx.clone(), run, true));
    }
    let ids = stale(ctx, Some(6))?;
    if !ids.is_empty() {
        let run = Run::begin(ctx, "citations", "Publication details update", ids.len() as i64, Some("#/publications"))?;
        ctx.block_on(refresh_many(ctx.clone(), run, ids, false));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dois() {
        assert_eq!(clean_doi("https://doi.org/10.1145/3544548.3580000.").as_deref(), Some("10.1145/3544548.3580000"));
        assert_eq!(clean_doi("doi:10.1/x").as_deref(), Some("10.1/x"));
        assert!(clean_doi("not a doi").is_none());
    }

    #[test]
    fn parse_and_draft() {
        let oa = json!({ "id": "https://openalex.org/W1", "display_name": "Platform labor in the Global South", "doi": "https://doi.org/10.1/abc",
            "publication_year": 2024, "cited_by_count": 12, "biblio": { "volume": "34", "issue": "2", "first_page": "100", "last_page": "120" },
            "primary_location": { "landing_page_url": "https://journal.org/a", "source": { "display_name": "Information Systems Journal", "host_organization_name": "Wiley", "issn": ["1350-1917"] } } });
        let cr = json!({ "DOI": "10.1/ABC", "title": ["Platform labor in the Global South"], "issued": { "date-parts": [[2024, 3]] }, "container-title": ["Information Systems Journal"],
            "publisher": "Wiley", "volume": "34", "issue": "2", "page": "100-120", "URL": "https://doi.org/10.1/abc", "is-referenced-by-count": 15 });
        let s2 = json!({ "paperId": "abc", "title": "Platform labor in the Global South", "year": 2024, "citationCount": 14, "externalIds": { "DOI": "10.1/abc" } });
        let works = vec![openalex_work(&oa).unwrap(), crossref_work(&cr).unwrap(), s2_work(&s2).unwrap()];
        assert_eq!(works[0].pages.as_deref(), Some("100-120"));
        let p = json!({ "title": "Platform Labor in the Global South", "year": 2024, "citations": 20, "volume": "34" });
        assert!(works.iter().all(|w| matches(&p, w)));
        assert!(!matches(&json!({ "title": "Platform labor in the Global South", "year": 2019 }), &works[0]));
        let drafts = drafts_for(&p, &works);
        let get = |f: &str| drafts.iter().find(|d| d.field == f).map(|d| d.proposed.clone());
        assert_eq!(get("doi"), Some(json!("10.1/ABC")));
        assert_eq!(get("url"), Some(json!("https://journal.org/a")));
        assert_eq!(get("year"), Some(json!(2024)));
        assert!(get("citations").is_none(), "never lowers a higher count");
        let fresh = drafts_for(&json!({ "title": "x", "citations": null }), &works);
        let c = fresh.iter().find(|d| d.field == "citations").unwrap();
        assert_eq!((c.proposed.clone(), c.metric, c.source_name.as_str()), (json!(15), true, "Crossref"));
    }
}
