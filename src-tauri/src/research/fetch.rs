//! Polite HTTP fetching for research: one shared client (rustls, gzip/brotli, 20 s timeout, honest user agent),
//! at most 6 requests at a time, at least 1 s between requests to the same host, 2 retries with backoff on
//! 429/5xx and network errors, robots.txt for web pages, a 3 MB size cap and a 24 h on-disk cache in
//! `<app data>/cache`. No personal data is ever put in a request (no `mailto`, no names in headers).

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;

pub const USER_AGENT: &str = "ManuscriptTracker/1.0 (+desktop)";
pub const MAX_BYTES: usize = 3 * 1024 * 1024;
pub const CACHE_TTL_SECS: u64 = 24 * 3600;
pub const CONCURRENCY: usize = 6;
pub const HOST_GAP: Duration = Duration::from_millis(1000);
pub const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    /// Not an http(s) URL.
    Invalid,
    /// robots.txt asks automated clients not to read it.
    Disallowed,
    /// The site refused automated reading (captcha, 429 that does not clear).
    Blocked(String),
    Status(u16),
    TooLarge,
    Network(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Invalid => write!(f, "not a web address"),
            FetchError::Disallowed => write!(f, "the site asks apps not to read this page"),
            FetchError::Blocked(why) => write!(f, "blocked by the site ({why})"),
            FetchError::Status(s) => write!(f, "HTTP {s}"),
            FetchError::TooLarge => write!(f, "page larger than 3 MB"),
            FetchError::Network(e) => write!(f, "{e}"),
        }
    }
}

impl From<FetchError> for crate::error::ApiError {
    fn from(e: FetchError) -> Self {
        crate::error::ApiError::msg(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    pub url: String,
    pub final_url: String,
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub cached: bool,
}

impl Page {
    pub fn is_html(&self) -> bool {
        self.content_type.is_empty() || self.content_type.contains("html") || self.body.trim_start().starts_with('<')
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Opts {
    /// Use and fill the on-disk cache.
    pub cache: bool,
    /// Check robots.txt first (web pages; not for APIs).
    pub robots: bool,
    pub accept: &'static str,
    pub retries: u32,
    /// Treat 429 as "blocked" at once (no retries), e.g. Google Scholar.
    pub no_retry_429: bool,
}

impl Opts {
    pub const PAGE: Opts =
        Opts { cache: true, robots: true, accept: "text/html,application/xhtml+xml;q=0.9,*/*;q=0.5", retries: 2, no_retry_429: false };
    pub const API: Opts = Opts { cache: true, robots: false, accept: "application/json", retries: 2, no_retry_429: false };
    pub const FEED: Opts = Opts {
        cache: true,
        robots: true,
        accept: "application/rss+xml,application/xml,text/xml;q=0.9,*/*;q=0.5",
        retries: 2,
        no_retry_429: false,
    };
}

#[derive(Debug, Default)]
struct Robots {
    rules: Vec<(bool, String)>,
}

pub struct Fetcher {
    client: reqwest::Client,
    permits: Semaphore,
    hosts: Mutex<HashMap<String, Instant>>,
    robots: Mutex<HashMap<String, Arc<Robots>>>,
    cache_dir: Mutex<Option<PathBuf>>,
}

/// The shared fetcher.
pub fn fetcher() -> &'static Fetcher {
    static F: OnceLock<Fetcher> = OnceLock::new();
    F.get_or_init(|| Fetcher {
        client: reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .gzip(true)
            .brotli(true)
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()
            .unwrap_or_default(),
        permits: Semaphore::new(CONCURRENCY),
        hosts: Mutex::new(HashMap::new()),
        robots: Mutex::new(HashMap::new()),
        cache_dir: Mutex::new(None),
    })
}

/// Point the cache at `<app data>/cache` (no-op without an app, e.g. in tests).
pub fn init_cache(ctx: &crate::api::Ctx) {
    if let Ok(dir) = ctx.data_dir() {
        let dir = dir.join("cache");
        if std::fs::create_dir_all(&dir).is_ok() {
            *fetcher().cache_dir.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn hash(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `path` (path + query) matches a robots.txt pattern (`*` wildcard, `$` end anchor).
pub fn robots_match(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
            continue;
        }
        if part.is_empty() {
            continue;
        }
        match path[pos..].find(part) {
            Some(j) => pos += j + part.len(),
            None => return false,
        }
    }
    !anchored || pos == path.len() || pat.ends_with('*')
}

/// Parse robots.txt rules for `User-agent: *` (or our own name).
fn parse_robots(txt: &str) -> Robots {
    let mut rules = Vec::new();
    let mut applies = false;
    let mut in_agents = false;
    for line in txt.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "user-agent" => {
                let ours = v == "*" || v.to_ascii_lowercase().contains("manuscripttracker");
                applies = if in_agents { applies || ours } else { ours };
                in_agents = true;
            }
            "allow" | "disallow" => {
                in_agents = false;
                if applies && !v.is_empty() {
                    rules.push((k == "allow", v.to_string()));
                }
            }
            _ => in_agents = false,
        }
    }
    Robots { rules }
}

/// robots.txt decision: the longest matching rule wins; ties go to Allow.
fn robots_allows(robots: &Robots, path: &str) -> bool {
    let mut best: Option<(usize, bool)> = None;
    for (allow, pat) in &robots.rules {
        if robots_match(pat, path) {
            let len = pat.len();
            best = match best {
                Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                _ => Some((len, *allow)),
            };
        }
    }
    best.is_none_or(|(_, allow)| allow)
}

/// Signs that a page is a captcha or "unusual traffic" wall.
pub fn looks_blocked(final_url: &str, body: &str) -> bool {
    let b = body.get(..body.len().min(60_000)).unwrap_or(body).to_ascii_lowercase();
    final_url.contains("/sorry/")
        || b.contains("gs_captcha")
        || b.contains("g-recaptcha")
        || b.contains("unusual traffic from your computer")
        || b.contains("please show you&#39;re not a robot")
        || b.contains("cf-challenge")
        || b.contains("challenge-platform")
}

impl Fetcher {
    async fn wait_for_host(&self, host: &str) {
        let wait = {
            let mut map = self.hosts.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let next = map.get(host).map(|t| *t + HOST_GAP).unwrap_or(now).max(now);
            map.insert(host.to_string(), next);
            next - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    fn cache_path(&self, url: &str) -> Option<PathBuf> {
        self.cache_dir.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|d| d.join(format!("{}.json", hash(url))))
    }

    fn cache_read(&self, url: &str) -> Option<Page> {
        let path = self.cache_path(url)?;
        let raw = std::fs::read_to_string(&path).ok()?;
        let v: Value = serde_json::from_str(&raw).ok()?;
        let at = v.get("at").and_then(Value::as_u64)?;
        if now_secs().saturating_sub(at) > CACHE_TTL_SECS {
            let _ = std::fs::remove_file(path);
            return None;
        }
        Some(Page {
            url: url.to_string(),
            final_url: v.get("final_url").and_then(Value::as_str).unwrap_or(url).to_string(),
            status: v.get("status").and_then(Value::as_u64).unwrap_or(200) as u16,
            content_type: v.get("content_type").and_then(Value::as_str).unwrap_or("").to_string(),
            body: v.get("body").and_then(Value::as_str).unwrap_or("").to_string(),
            cached: true,
        })
    }

    fn cache_write(&self, page: &Page) {
        if let Some(path) = self.cache_path(&page.url) {
            let v = json!({ "url": page.url, "final_url": page.final_url, "status": page.status,
                            "content_type": page.content_type, "at": now_secs(), "body": page.body });
            let tmp = path.with_extension("tmp");
            if std::fs::write(&tmp, v.to_string()).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }

    /// Remove cache files older than the TTL (called occasionally).
    pub fn prune_cache(&self) {
        let dir = self.cache_dir.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(dir) = dir else { return };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > CACHE_TTL_SECS);
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    async fn robots_for(&self, base: &url::Url) -> Arc<Robots> {
        let key = format!("{}://{}", base.scheme(), base.host_str().unwrap_or(""));
        if let Some(r) = self.robots.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return r.clone();
        }
        let robots_url = format!("{key}/robots.txt");
        let opts = Opts { cache: true, robots: false, accept: "text/plain", retries: 1, no_retry_429: false };
        let robots = match Box::pin(self.get(&robots_url, &opts)).await {
            Ok(p) if p.status == 200 && !p.body.trim_start().starts_with('<') => parse_robots(&p.body),
            _ => Robots::default(),
        };
        let robots = Arc::new(robots);
        self.robots.lock().unwrap_or_else(|e| e.into_inner()).insert(key, robots.clone());
        robots
    }

    /// GET a URL with the given options.
    pub async fn get(&self, url: &str, opts: &Opts) -> Result<Page, FetchError> {
        let parsed = url::Url::parse(url).map_err(|_| FetchError::Invalid)?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(FetchError::Invalid);
        }
        if opts.cache {
            if let Some(p) = self.cache_read(url) {
                return Ok(p);
            }
        }
        if opts.robots {
            let robots = self.robots_for(&parsed).await;
            let path = match parsed.query() {
                Some(q) => format!("{}?{q}", parsed.path()),
                None => parsed.path().to_string(),
            };
            if !robots_allows(&robots, &path) {
                return Err(FetchError::Disallowed);
            }
        }
        let host = parsed.host_str().unwrap_or("").to_string();
        let mut attempt = 0u32;
        loop {
            self.wait_for_host(&host).await;
            let permit = self.permits.acquire().await.map_err(|e| FetchError::Network(e.to_string()))?;
            let res = self.client.get(url).header(reqwest::header::ACCEPT, opts.accept).send().await;
            let retry_after = |r: &reqwest::Response| {
                r.headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(|s| Duration::from_secs(s.min(10)))
            };
            let backoff = |attempt: u32| Duration::from_millis(1000 * 3u64.pow(attempt));
            match res {
                Err(e) => {
                    drop(permit);
                    if attempt < opts.retries {
                        tokio::time::sleep(backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    let msg = if e.is_timeout() { "timed out".to_string() } else { "could not connect".to_string() };
                    return Err(FetchError::Network(msg));
                }
                Ok(mut resp) => {
                    let status = resp.status().as_u16();
                    if status == 429 || status >= 500 {
                        let wait = retry_after(&resp).unwrap_or_else(|| backoff(attempt));
                        drop(permit);
                        if status == 429 && opts.no_retry_429 {
                            return Err(FetchError::Blocked("too many requests".into()));
                        }
                        if attempt < opts.retries {
                            tokio::time::sleep(wait).await;
                            attempt += 1;
                            continue;
                        }
                        return Err(if status == 429 {
                            FetchError::Blocked("too many requests".into())
                        } else {
                            FetchError::Status(status)
                        });
                    }
                    if resp.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
                        return Err(FetchError::TooLarge);
                    }
                    let final_url = resp.url().to_string();
                    let content_type =
                        resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
                    let mut buf: Vec<u8> = Vec::new();
                    loop {
                        match resp.chunk().await {
                            Ok(Some(chunk)) => {
                                if buf.len() + chunk.len() > MAX_BYTES {
                                    return Err(FetchError::TooLarge);
                                }
                                buf.extend_from_slice(&chunk);
                            }
                            Ok(None) => break,
                            Err(_) => return Err(FetchError::Network("connection dropped".into())),
                        }
                    }
                    drop(permit);
                    let body = String::from_utf8_lossy(&buf).into_owned();
                    if looks_blocked(&final_url, &body) {
                        return Err(FetchError::Blocked("captcha".into()));
                    }
                    if !(200..300).contains(&status) {
                        return Err(FetchError::Status(status));
                    }
                    let page = Page { url: url.to_string(), final_url, status, content_type, body, cached: false };
                    if opts.cache {
                        self.cache_write(&page);
                    }
                    return Ok(page);
                }
            }
        }
    }

    /// A web page (robots.txt respected, cached).
    pub async fn page(&self, url: &str) -> Result<Page, FetchError> {
        self.get(url, &Opts::PAGE).await
    }

    /// A JSON API response (cached).
    pub async fn json(&self, url: &str) -> Result<Value, FetchError> {
        let p = self.get(url, &Opts::API).await?;
        serde_json::from_str(&p.body).map_err(|_| FetchError::Network("the answer was not JSON".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_rules() {
        let txt = "User-agent: *\nDisallow: /search\nDisallow: /citations?\nAllow: /citations?user=\nDisallow: /citations?*cstart=\nDisallow: /citations?user=*@\n\nUser-agent: PetalBot\nDisallow: /\n";
        let r = parse_robots(txt);
        assert!(robots_allows(&r, "/citations?user=abc&hl=en&view_op=list_works&pagesize=100"));
        assert!(!robots_allows(&r, "/citations?user=abc&cstart=100"));
        assert!(!robots_allows(&r, "/citations?view_op=x"));
        assert!(!robots_allows(&r, "/search?q=x"));
        assert!(robots_allows(&r, "/"));
        assert!(robots_match("/a*b$", "/axxb"));
        assert!(!robots_match("/a*b$", "/axxbc"));
        let none = parse_robots("User-agent: Gigabot\nDisallow: /\n");
        assert!(robots_allows(&none, "/cfp/rss"));
    }

    #[test]
    fn blocked_pages() {
        assert!(looks_blocked("https://www.google.com/sorry/index?continue=x", ""));
        assert!(looks_blocked("https://scholar.google.com/x", "<div id=\"gs_captcha_ccl\">"));
        assert!(!looks_blocked("https://example.org", "<html>fine</html>"));
    }

    #[test]
    fn rejects_bad_urls() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert_eq!(rt.block_on(fetcher().page("file:///etc/passwd")).unwrap_err(), FetchError::Invalid);
        assert_eq!(rt.block_on(fetcher().page("not a url")).unwrap_err(), FetchError::Invalid);
    }
}
