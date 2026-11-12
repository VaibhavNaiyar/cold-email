use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;
use scraper::{Html, Selector};
use texting_robots::Robot;

const AGENT: &str = "startup-scout";
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CHARS: usize = 3000;

/// Reads a company's homepage. `None` means robots.txt forbids it or it has no readable text.
pub async fn homepage_text(http: &reqwest::Client, domain: &str) -> Result<Option<String>> {
    let url = format!("https://{domain}/");
    if !robots_allow(http, domain, &url).await {
        return Ok(None);
    }
    let response = http.get(&url).timeout(TIMEOUT).send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let html = response.text().await?;
    Ok(extract_text(&html))
}

async fn robots_allow(http: &reqwest::Client, domain: &str, url: &str) -> bool {
    let robots_url = format!("https://{domain}/robots.txt");
    let Ok(response) = http.get(&robots_url).timeout(TIMEOUT).send().await else {
        return false; // Unknown rules: stay out.
    };
    match response.status() {
        StatusCode::OK => match response.bytes().await {
            Ok(body) => is_allowed(&body, url),
            Err(_) => false,
        },
        status => status.is_client_error(), // No robots.txt means no restrictions.
    }
}

fn is_allowed(robots_txt: &[u8], url: &str) -> bool {
    Robot::new(AGENT, robots_txt).is_ok_and(|robot| robot.allowed(url))
}

pub(crate) fn extract_text(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let mut parts = Vec::new();
    if let Some(title) = first_text(&document, "title") {
        parts.push(title);
    }
    let description = Selector::parse("meta[name=description]").expect("selector is valid");
    if let Some(meta) = document.select(&description).next() {
        parts.extend(meta.value().attr("content").map(str::to_owned));
    }
    if let Some(body) = first_text(&document, "body") {
        parts.push(body);
    }
    let text = parts.join(" ");
    (!text.trim().is_empty()).then(|| text.chars().take(MAX_CHARS).collect())
}

/// The readable text of an HTML fragment, whitespace collapsed, at most `max_chars` long.
pub fn html_text(html: &str, max_chars: usize) -> String {
    let fragment = Html::parse_fragment(html);
    let mut text = String::new();
    for word in fragment.root_element().text().flat_map(str::split_whitespace) {
        if text.len() >= max_chars {
            break;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
    }
    text.chars().take(max_chars).collect()
}

fn first_text(document: &Html, tag: &str) -> Option<String> {
    let selector = Selector::parse(tag).expect("selector is valid");
    let element = document.select(&selector).next()?;
    let words: Vec<_> = element
        .text()
        .flat_map(str::split_whitespace)
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honours_robots_rules() {
        let closed = b"User-agent: *\nDisallow: /";
        let open = b"User-agent: *\nDisallow: /private";
        assert!(!is_allowed(closed, "https://acme.ai/"));
        assert!(is_allowed(open, "https://acme.ai/"));
    }

    #[test]
    fn extracts_title_description_and_body() {
        let html = r#"<html><head><title>Acme</title>
            <meta name="description" content="Agents for support"></head>
            <body><h1>Hello   world</h1></body></html>"#;
        assert_eq!(extract_text(html).unwrap(), "Acme Agents for support Hello world");
    }

    #[test]
    fn html_text_unescapes_and_caps_length() {
        assert_eq!(html_text("<p>Build <b>agents</b> &amp; tools</p>", 100), "Build agents & tools");
        assert_eq!(html_text("<p>one two three</p>", 7), "one two");
    }

    #[test]
    fn empty_pages_give_nothing() {
        assert_eq!(extract_text("<html><body></body></html>"), None);
    }
}
