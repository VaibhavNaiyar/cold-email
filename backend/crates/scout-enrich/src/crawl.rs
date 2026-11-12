use std::collections::BTreeSet;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use reqwest::StatusCode;
use texting_robots::Robot;
use tokio::time::sleep;

use crate::homepage::extract_text;

const AGENT: &str = "startup-scout";
const TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_GAP: Duration = Duration::from_secs(1);
const MAX_PAGES: usize = 12;
const STANDARD_PATHS: &[&str] = &[
    "/about", "/team", "/company", "/contact", "/careers", "/jobs", "/imprint", "/impressum",
    "/privacy", "/terms",
];
const USEFUL_WORDS: &[&str] = &[
    "about", "team", "company", "contact", "career", "jobs", "hiring", "founder", "imprint",
    "impressum", "legal", "privacy", "terms",
];

static LOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<loc>\s*([^<\s]+)\s*</loc>").expect("loc regex is valid"));
static HREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r##"href="([^"#?]+)"##).expect("href regex is valid"));

pub struct Page {
    pub url: String,
    pub html: String,
    pub text: String,
}

/// Reads the pages of a site most likely to name people or publish an address.
pub async fn read_site(http: &reqwest::Client, domain: &str) -> Vec<Page> {
    let Some(robots) = fetch_robots(http, domain).await else {
        return Vec::new();
    };
    let base = format!("https://{domain}");
    let mut pages = Vec::new();
    let mut queue: BTreeSet<String> = BTreeSet::new();
    if let Some(home) = fetch_page(http, &robots, &format!("{base}/")).await {
        queue.extend(useful_links(&home.html, &base));
        pages.push(home);
    }
    if let Some(sitemap) = fetch_page(http, &robots, &format!("{base}/sitemap.xml")).await {
        queue.extend(sitemap_links(&sitemap.html, domain));
    }
    queue.extend(STANDARD_PATHS.iter().map(|path| format!("{base}{path}")));

    for url in queue {
        if pages.len() >= MAX_PAGES {
            break;
        }
        if let Some(page) = fetch_page(http, &robots, &url).await {
            pages.push(page);
        }
    }
    pages
}

/// Reads only the given paths of a site, under the same rules as `read_site`.
pub async fn read_pages(http: &reqwest::Client, domain: &str, paths: &[&str]) -> Vec<Page> {
    let Some(robots) = fetch_robots(http, domain).await else {
        return Vec::new();
    };
    let mut pages = Vec::new();
    for path in paths {
        if let Some(page) = fetch_page(http, &robots, &format!("https://{domain}{path}")).await {
            pages.push(page);
        }
    }
    pages
}

enum Robots {
    Open,
    Rules(Robot),
}

impl Robots {
    fn allows(&self, url: &str) -> bool {
        match self {
            Self::Open => true,
            Self::Rules(robot) => robot.allowed(url),
        }
    }
}

// Returns None when the site is unreachable or its rules cannot be read: stay out.
async fn fetch_robots(http: &reqwest::Client, domain: &str) -> Option<Robots> {
    let url = format!("https://{domain}/robots.txt");
    let response = http.get(&url).timeout(TIMEOUT).send().await.ok()?;
    match response.status() {
        StatusCode::OK => {
            let body = response.bytes().await.ok()?;
            Robot::new(AGENT, &body).ok().map(Robots::Rules)
        }
        status if status.is_client_error() => Some(Robots::Open),
        _ => None,
    }
}

async fn fetch_page(http: &reqwest::Client, robots: &Robots, url: &str) -> Option<Page> {
    if !robots.allows(url) {
        return None;
    }
    sleep(REQUEST_GAP).await;
    let response = http.get(url).timeout(TIMEOUT).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html = response.text().await.ok()?;
    let text = extract_text(&html).unwrap_or_default();
    Some(Page { url: url.to_owned(), html, text })
}

fn is_useful(url: &str) -> bool {
    let url = url.to_lowercase();
    USEFUL_WORDS.iter().any(|word| url.contains(word))
}

fn sitemap_links(xml: &str, domain: &str) -> Vec<String> {
    LOC.captures_iter(xml)
        .map(|caps| caps[1].to_owned())
        .filter(|url| url.contains(domain) && is_useful(url))
        .collect()
}

fn useful_links(html: &str, base: &str) -> Vec<String> {
    HREF.captures_iter(html)
        .filter_map(|caps| {
            let link = &caps[1];
            if let Some(path) = link.strip_prefix('/') {
                return Some(format!("{base}/{path}"));
            }
            link.starts_with(base).then(|| link.to_owned())
        })
        .filter(|url| is_useful(url))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_rules_are_honoured() {
        let robots = Robots::Rules(Robot::new(AGENT, b"User-agent: *\nDisallow: /team").unwrap());
        assert!(!robots.allows("https://acme.ai/team"));
        assert!(robots.allows("https://acme.ai/about"));
        assert!(Robots::Open.allows("https://acme.ai/team"));
    }

    #[test]
    fn sitemap_keeps_useful_pages_on_the_same_site() {
        let xml = "<url><loc>https://acme.ai/about</loc></url>
                   <url><loc>https://acme.ai/blog/post</loc></url>
                   <url><loc>https://other.ai/team</loc></url>";
        assert_eq!(sitemap_links(xml, "acme.ai"), ["https://acme.ai/about"]);
    }

    #[test]
    fn homepage_links_keep_useful_internal_paths() {
        let html = r#"<a href="/team">T</a><a href="/pricing">P</a><a href="https://x.com/about">X</a>
                      <a href="https://acme.ai/legal/privacy">L</a>"#;
        assert_eq!(
            useful_links(html, "https://acme.ai"),
            ["https://acme.ai/team", "https://acme.ai/legal/privacy"]
        );
    }
}
