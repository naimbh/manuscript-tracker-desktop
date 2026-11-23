//! HTML to readable text (scraper), date detection, and RSS/Atom feed parsing (roxmltree).
//!
//! The text keeps what matters for calls for papers and author guidelines: headings (`# `), list items
//! (`- `), table rows as `cell | cell` lines, and links with their anchor text. Scripts, styles, navigation,
//! footers and forms are dropped.

use chrono::NaiveDate;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub url: String,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Doc {
    pub url: String,
    pub title: String,
    pub lines: Vec<String>,
    pub links: Vec<Link>,
}

const SKIP: &[&str] = &[
    "script", "style", "noscript", "svg", "nav", "footer", "template", "iframe", "form", "button", "select", "textarea", "head", "canvas",
    "video", "audio", "picture", "object",
];
const BLOCK: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "main",
    "header",
    "aside",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "table",
    "tbody",
    "thead",
    "tfoot",
    "blockquote",
    "pre",
    "hr",
    "figure",
    "figcaption",
    "address",
    "center",
    "details",
    "summary",
    "body",
    "html",
    "caption",
    "fieldset",
];

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Walker {
    base: Option<url::Url>,
    cur: String,
    lines: Vec<String>,
    links: Vec<Link>,
}

impl Walker {
    fn flush(&mut self) {
        let line = collapse(&self.cur);
        self.cur.clear();
        if !line.is_empty() && line != "-" && !line.chars().all(|c| c == '#' || c == ' ') {
            self.lines.push(line);
        }
    }

    fn push_text(&mut self, t: &str) {
        if t.trim().is_empty() {
            if !self.cur.ends_with(' ') && !self.cur.is_empty() {
                self.cur.push(' ');
            }
            return;
        }
        if t.starts_with(char::is_whitespace) && !self.cur.ends_with(' ') && !self.cur.is_empty() {
            self.cur.push(' ');
        }
        self.cur.push_str(&collapse(t));
        if t.ends_with(char::is_whitespace) {
            self.cur.push(' ');
        }
    }

    fn resolve(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("javascript:")
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
        {
            return None;
        }
        let u = match &self.base {
            Some(b) => b.join(href).ok()?,
            None => url::Url::parse(href).ok()?,
        };
        if !matches!(u.scheme(), "http" | "https") {
            return None;
        }
        let mut u = u;
        u.set_fragment(None);
        Some(u.to_string())
    }

    fn skip(el: &ElementRef) -> bool {
        let v = el.value();
        SKIP.contains(&v.name())
            || v.attr("role").is_some_and(|r| r == "navigation" || r == "search")
            || v.attr("aria-hidden").is_some_and(|a| a == "true")
            || v.attr("hidden").is_some()
            || v.attr("id").or(v.attr("class")).is_some_and(|c| {
                let c = c.to_ascii_lowercase();
                c.contains("cookie") || c.contains("skip-link")
            })
    }

    fn walk(&mut self, el: ElementRef) {
        if Self::skip(&el) {
            return;
        }
        let name = el.value().name();
        match name {
            "br" => self.flush(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                let level = name[1..].parse::<usize>().unwrap_or(2);
                self.cur.push_str(&"#".repeat(level));
                self.cur.push(' ');
                self.children(el);
                self.flush();
            }
            "li" => {
                self.flush();
                self.cur.push_str("- ");
                self.children(el);
                self.flush();
            }
            "tr" => {
                self.flush();
                let cells: Vec<ElementRef> =
                    el.children().filter_map(ElementRef::wrap).filter(|c| matches!(c.value().name(), "td" | "th")).collect();
                let nested = cells.iter().any(|c| c.descendants().filter_map(ElementRef::wrap).any(|d| d.value().name() == "table"));
                if nested || cells.is_empty() {
                    self.children(el);
                    self.flush();
                    return;
                }
                let texts: Vec<String> = cells
                    .iter()
                    .map(|c| {
                        let mut w = Walker { base: self.base.clone(), cur: String::new(), lines: Vec::new(), links: Vec::new() };
                        w.children(*c);
                        w.flush();
                        self.links.append(&mut w.links);
                        w.lines.join(" ")
                    })
                    .filter(|t| !t.is_empty())
                    .collect();
                if !texts.is_empty() {
                    self.lines.push(texts.join(" | "));
                }
            }
            "a" => {
                let start = self.cur.len();
                self.children(el);
                if let Some(url) = el.value().attr("href").and_then(|h| self.resolve(h)) {
                    let mut text = collapse(self.cur.get(start..).unwrap_or(""));
                    if text.is_empty() {
                        text = collapse(el.value().attr("title").or(el.value().attr("aria-label")).unwrap_or(""));
                    }
                    self.links.push(Link { url, text });
                }
            }
            "img" => {
                if let Some(alt) = el.value().attr("alt") {
                    if !alt.trim().is_empty() && alt.len() < 120 {
                        self.push_text(&format!(" {} ", alt.trim()));
                    }
                }
            }
            "td" | "th" => {
                self.children(el);
                self.push_text(" ");
            }
            _ if BLOCK.contains(&name) => {
                self.flush();
                self.children(el);
                self.flush();
            }
            _ => self.children(el),
        }
    }

    fn children(&mut self, el: ElementRef) {
        for child in el.children() {
            match child.value() {
                Node::Text(t) => self.push_text(t),
                Node::Element(_) => {
                    if let Some(e) = ElementRef::wrap(child) {
                        self.walk(e);
                    }
                }
                _ => {}
            }
        }
    }
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|_| Selector::parse("*").expect("valid selector"))
}

/// Readable text, title and links of an HTML page. `base` resolves relative links.
pub fn html(src: &str, base: &str) -> Doc {
    let doc = Html::parse_document(src);
    let title = doc.select(&sel("title")).next().map(|t| collapse(&t.text().collect::<String>())).unwrap_or_default();
    let mut w = Walker { base: url::Url::parse(base).ok(), cur: String::new(), lines: Vec::new(), links: Vec::new() };
    w.walk(doc.root_element());
    w.flush();
    // Drop consecutive duplicates (menus repeated for mobile, etc.).
    w.lines.dedup();
    let mut seen = std::collections::HashSet::new();
    w.links.retain(|l| seen.insert(l.url.clone()));
    Doc { url: base.to_string(), title, lines: w.lines, links: w.links }
}

impl Doc {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Text for a prompt, at most `max` characters: the whole page when it fits, otherwise the lines that
    /// mention a keyword or a date, with one line of context around each, in page order.
    pub fn excerpt(&self, keywords: &[&str], max: usize) -> String {
        let full = self.text();
        if full.chars().count() <= max {
            return full;
        }
        let kws: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();
        let mut keep = vec![false; self.lines.len()];
        for (i, line) in self.lines.iter().enumerate() {
            let l = line.to_lowercase();
            if line.starts_with('#') || kws.iter().any(|k| l.contains(k.as_str())) || !dates_in(line).is_empty() {
                let end = (i + 1).min(self.lines.len() - 1);
                keep[i.saturating_sub(1)..=end].iter_mut().for_each(|k| *k = true);
            }
        }
        let mut out = String::new();
        let mut gap = false;
        for (i, line) in self.lines.iter().enumerate() {
            if !keep[i] {
                gap = true;
                continue;
            }
            let add = if gap && !out.is_empty() { format!("…\n{line}\n") } else { format!("{line}\n") };
            gap = false;
            if out.chars().count() + add.chars().count() > max {
                break;
            }
            out.push_str(&add);
        }
        if out.is_empty() {
            full.chars().take(max).collect()
        } else {
            out
        }
    }

    /// Links whose text or URL contains one of the needles (case-insensitive).
    pub fn links_matching(&self, needles: &[&str]) -> Vec<&Link> {
        self.links
            .iter()
            .filter(|l| {
                let t = format!("{} {}", l.text, l.url).to_lowercase();
                needles.iter().any(|n| t.contains(n))
            })
            .collect()
    }
}

// ---- Dates --------------------------------------------------------------------------------------------

const MONTHS: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

fn month_num(m: &str) -> Option<u32> {
    let m = m.to_lowercase();
    let i = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"].iter().position(|p| m.starts_with(p))?;
    Some(i as u32 + 1)
}

fn date_res() -> &'static [Regex; 3] {
    static R: OnceLock<[Regex; 3]> = OnceLock::new();
    R.get_or_init(|| {
        [
            Regex::new(r"\b(20\d{2})-(\d{1,2})-(\d{1,2})\b").expect("re"),
            Regex::new(&format!(r"(?i)\b(\d{{1,2}})(?:st|nd|rd|th)?\.?\s+(?:of\s+)?({MONTHS})\.?,?\s+(20\d{{2}})\b")).expect("re"),
            Regex::new(&format!(r"(?i)\b({MONTHS})\.?\s+(\d{{1,2}})(?:st|nd|rd|th)?,?\s+(20\d{{2}})\b")).expect("re"),
        ]
    })
}

/// Every date found in the text, in order (ISO `2027-01-21`, `21 January 2027`, `Jan 21, 2027`, ...).
pub fn dates_in(s: &str) -> Vec<NaiveDate> {
    let [iso, dmy, mdy] = date_res();
    let mut out: Vec<(usize, NaiveDate)> = Vec::new();
    for c in iso.captures_iter(s) {
        let (y, m, d) = (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0), c[3].parse().unwrap_or(0));
        if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
            out.push((c.get(0).map_or(0, |m| m.start()), date));
        }
    }
    for c in dmy.captures_iter(s) {
        if let (Ok(d), Some(m), Ok(y)) = (c[1].parse::<u32>(), month_num(&c[2]), c[3].parse::<i32>()) {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                out.push((c.get(0).map_or(0, |m| m.start()), date));
            }
        }
    }
    for c in mdy.captures_iter(s) {
        if let (Some(m), Ok(d), Ok(y)) = (month_num(&c[1]), c[2].parse::<u32>(), c[3].parse::<i32>()) {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                out.push((c.get(0).map_or(0, |m| m.start()), date));
            }
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out.dedup_by_key(|(_, d)| *d);
    out.into_iter().map(|(_, d)| d).collect()
}

/// The first date in the text, or a strict `YYYY-MM-DD` value.
pub fn parse_date_loose(s: &str) -> Option<NaiveDate> {
    dates_in(s).into_iter().next()
}

/// `10–14 May 2027`, `30 Jul–3 Aug 2027` or `30 Dec 2027–2 Jan 2028`.
pub fn format_range(a: NaiveDate, b: NaiveDate) -> String {
    use chrono::Datelike;
    if a == b {
        return a.format("%-d %b %Y").to_string();
    }
    if a.year() != b.year() {
        return format!("{}–{}", a.format("%-d %b %Y"), b.format("%-d %b %Y"));
    }
    if a.month() != b.month() {
        return format!("{}–{}", a.format("%-d %b"), b.format("%-d %b %Y"));
    }
    format!("{}–{}", a.day(), b.format("%-d %b %Y"))
}

// ---- Feeds -------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub description: String,
    pub published: Option<String>,
}

/// Make feed XML parseable: HTML entities such as `&nbsp;` and bare `&` are common in CFP feeds.
fn sanitize_xml(xml: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"&(#\d+;|#x[0-9a-fA-F]+;|[a-zA-Z][a-zA-Z0-9]*;)?").expect("re"));
    re.replace_all(xml, |c: &regex::Captures| match c.get(1).map(|m| m.as_str()) {
        None => "&amp;".to_string(),
        Some(e) if e.starts_with('#') => format!("&{e}"),
        Some("amp;") | Some("lt;") | Some("gt;") | Some("quot;") | Some("apos;") => {
            format!("&{}", &c[1])
        }
        Some("nbsp;") => "&#160;".into(),
        Some("ndash;") => "&#8211;".into(),
        Some("mdash;") => "&#8212;".into(),
        Some("rsquo;") => "&#8217;".into(),
        Some("lsquo;") => "&#8216;".into(),
        Some("ldquo;") => "&#8220;".into(),
        Some("rdquo;") => "&#8221;".into(),
        Some(_) => " ".into(),
    })
    .into_owned()
}

/// RSS 2.0 `<item>` or Atom `<entry>` elements.
pub fn feed(xml: &str) -> Vec<FeedItem> {
    let clean = sanitize_xml(xml);
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = roxmltree::Document::parse_with_options(&clean, opts) else {
        return Vec::new();
    };
    let child_text = |n: roxmltree::Node, name: &str| -> Option<String> {
        n.children().find(|c| c.is_element() && c.tag_name().name() == name).map(|c| {
            let t: String = c.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect();
            collapse(&t)
        })
    };
    doc.descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry"))
        .filter_map(|n| {
            let title = child_text(n, "title").unwrap_or_default();
            let link = n
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "link")
                .and_then(|l| l.attribute("href").map(str::to_string).or_else(|| l.text().map(|t| t.trim().to_string())))
                .unwrap_or_default();
            if title.is_empty() || link.is_empty() {
                return None;
            }
            let description = child_text(n, "description").or_else(|| child_text(n, "summary")).unwrap_or_default();
            // Descriptions are often HTML: keep only the text.
            let description =
                if description.contains('<') { html(&format!("<p>{description}</p>"), "").text().replace('\n', " ") } else { description };
            let published = child_text(n, "pubDate").or_else(|| child_text(n, "updated"));
            Some(FeedItem { title, link, description, published })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r##"<html><head><title> CHI 2027 | Call for Papers </title><style>.x{}</style>
      <script>var deadline = "2020-01-01";</script></head>
      <body><nav><a href="/">Home</a><a href="/about">About</a></nav>
      <header><h1>CHI 2027</h1><p>10–14 May 2027,   Pittsburgh</p></header>
      <main><h2>Important dates</h2>
        <ul><li>Papers due: <b>January 21, 2027</b> (AoE)</li><li>Notification: 15 Mar 2027</li></ul>
        <table><tr><th>Track</th><th>Deadline</th></tr><tr><td>Posters</td><td>2027-02-01</td></tr></table>
        <p>See the <a href="guide.html#top">author guidelines</a> and <a href="javascript:void(0)">nothing</a>.</p>
        <div hidden>secret</div>
      </main><footer>© ACM <a href="https://acm.org">ACM</a></footer></body></html>"##;

    #[test]
    fn html_to_text() {
        let d = html(PAGE, "https://chi2027.acm.org/cfp/");
        assert_eq!(d.title, "CHI 2027 | Call for Papers");
        let t = d.text();
        assert!(t.contains("# CHI 2027"), "{t}");
        assert!(t.contains("10–14 May 2027, Pittsburgh"));
        assert!(t.contains("## Important dates"));
        assert!(t.contains("- Papers due: January 21, 2027 (AoE)"));
        assert!(t.contains("Track | Deadline"));
        assert!(t.contains("Posters | 2027-02-01"));
        assert!(!t.contains("var deadline"));
        assert!(!t.contains("About"), "nav dropped");
        assert!(!t.contains("© ACM"), "footer dropped");
        assert!(!t.contains("secret"));
        assert_eq!(d.links, vec![Link { url: "https://chi2027.acm.org/cfp/guide.html".into(), text: "author guidelines".into() }]);
        assert_eq!(d.links_matching(&["guideline"]).len(), 1);
    }

    #[test]
    fn nested_tables_stay_readable() {
        let h = "<table><tr><td><table><tr><th>When</th><td>Sep 20, 2027 - Sep 23, 2027</td></tr><tr><th>Submission Deadline</th><td><span>Feb 17, 2027</span></td></tr></table></td></tr></table>";
        let t = html(h, "http://www.wikicfp.com/").text();
        assert!(t.contains("When | Sep 20, 2027 - Sep 23, 2027"), "{t}");
        assert!(t.contains("Submission Deadline | Feb 17, 2027"));
    }

    #[test]
    fn dates() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        assert_eq!(
            dates_in("Papers due January 21, 2027 and 15th March 2027; camera-ready 2027-04-02."),
            vec![d(2027, 1, 21), d(2027, 3, 15), d(2027, 4, 2)]
        );
        assert_eq!(parse_date_loose("Sept. 3, 2027"), Some(d(2027, 9, 3)));
        assert_eq!(parse_date_loose("31 February 2027"), None);
        assert_eq!(format_range(d(2027, 5, 10), d(2027, 5, 14)), "10–14 May 2027");
        assert_eq!(format_range(d(2027, 7, 30), d(2027, 8, 3)), "30 Jul–3 Aug 2027");
    }

    #[test]
    fn excerpt_keeps_relevant_lines() {
        let mut doc = Doc::default();
        for i in 0..400 {
            doc.lines.push(format!("Filler line number {i} about nothing in particular"));
        }
        doc.lines[200] = "Submission deadline: 21 January 2027".into();
        let ex = doc.excerpt(&["deadline"], 500);
        assert!(ex.contains("Submission deadline"));
        assert!(ex.chars().count() <= 500);
    }

    #[test]
    fn rss_items() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title>CFPs on HCI : WikiCFP</title>
          <item><title>MobileHCI 2027 : The ACM International Conference on Mobile Human-Computer Interaction</title>
          <link>http://www.wikicfp.com/cfp/servlet/event.showcfp?eventid=203910&amp;copyownerid=49757</link>
          <description>The ACM Conference [Limassol &amp; Cyprus&nbsp;] [Sep 20, 2027 - Sep 23, 2027]</description></item>
          <item><title>Bad & broken</title><link>http://x.org/a</link></item>
          <item><title></title><link>http://x.org/b</link></item>
          </channel></rss>"#;
        let items = feed(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].link, "http://www.wikicfp.com/cfp/servlet/event.showcfp?eventid=203910&copyownerid=49757");
        assert!(items[0].description.contains("[Sep 20, 2027 - Sep 23, 2027]"));
        assert_eq!(items[1].title, "Bad & broken");
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Special issue</title><link href="https://j.org/si"/><summary>&lt;p&gt;Due 1 June 2027&lt;/p&gt;</summary></entry></feed>"#;
        let a = feed(atom);
        assert_eq!(a[0].link, "https://j.org/si");
        assert!(a[0].description.contains("Due 1 June 2027"));
        assert!(feed("not xml <").is_empty());
    }
}
