use std::sync::LazyLock;

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use regex::Regex;
use serde::Deserialize;

use super::{DESCRIPTION_CHARS, Posting, get_text, is_remote, join_notes, trim_description};

const SEARCH_URL: &str =
    "https://hn.algolia.com/api/v1/search_by_date?tags=story,author_whoishiring&hitsPerPage=5";
const ITEM_URL: &str = "https://hn.algolia.com/api/v1/items/";
const COMMENT_URL: &str = "https://news.ycombinator.com/item?id=";
const THREAD_TITLE: &str = "Ask HN: Who is hiring?";
const HEADER_CHARS: usize = 400;
const PLACE_WORDS: &[&str] = &["remote", "onsite", "on-site", "hybrid", "office", "relocation"];

static CITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z][A-Za-z .'-]+, [A-Z][A-Za-z]+").expect("city regex is valid"));
static PAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[$€£₹]|\d+\s?[kK]\b").expect("pay regex is valid"));

#[derive(Deserialize)]
struct Search {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(rename = "objectID")]
    object_id: String,
    title: String,
}

#[derive(Deserialize)]
struct Thread {
    #[serde(default)]
    children: Vec<Comment>,
}

#[derive(Deserialize)]
struct Comment {
    id: u64,
    created_at: Option<DateTime<FixedOffset>>,
    text: Option<String>,
}

/// Every top-level post in the latest "Who is hiring?" thread.
pub(super) async fn fetch(http: &reqwest::Client) -> Result<Vec<Posting>> {
    let search: Search = serde_json::from_str(&get_text(http, SEARCH_URL).await?)?;
    let thread = search
        .hits
        .iter()
        .find(|hit| hit.title.starts_with(THREAD_TITLE))
        .context("No \"Who is hiring?\" thread found")?;
    parse_thread(&get_text(http, &format!("{ITEM_URL}{}", thread.object_id)).await?)
}

fn parse_thread(body: &str) -> Result<Vec<Posting>> {
    let thread: Thread = serde_json::from_str(body)?;
    Ok(thread.children.into_iter().filter_map(posting).collect())
}

// Posts open with a header line such as "Acme | Backend Engineer | Remote (EU) | $150k".
fn posting(comment: Comment) -> Option<Posting> {
    let html = comment.text?;
    let header_html = html.split("<p>").next().unwrap_or(&html);
    let header = scout_enrich::html_text(header_html, HEADER_CHARS);
    let fields: Vec<&str> = header.split('|').map(str::trim).filter(|field| !field.is_empty()).collect();
    let (company, rest) = fields.split_first()?;
    let title = rest.iter().find(|field| kind(field) == Field::Role)?;
    let places: Vec<&str> = rest.iter().copied().filter(|field| kind(field) == Field::Place).collect();
    let terms: Vec<&str> = rest.iter().copied().filter(|field| kind(field) == Field::Terms).collect();
    let location = places.join(" | ");
    Some(Posting {
        company: Some((*company).to_owned()),
        external_id: comment.id.to_string(),
        title: (*title).to_owned(),
        remote: is_remote(false, &location),
        location,
        department: None,
        url: format!("{COMMENT_URL}{}", comment.id),
        posted_at: comment.created_at.map(|date| date.with_timezone(&Utc)),
        description: trim_description(Some(scout_enrich::html_text(&html, DESCRIPTION_CHARS))),
        notes: join_notes(terms.iter().map(|term| Some(*term))),
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Field {
    Role,
    Place,
    Terms,
    Link,
}

fn kind(field: &str) -> Field {
    let lower = field.to_lowercase();
    if lower.contains("http") || lower.contains("www.") {
        Field::Link
    } else if PLACE_WORDS.iter().any(|word| lower.contains(word)) || CITY.is_match(field) {
        Field::Place
    } else if PAY.is_match(field) || lower.contains("full-time") || lower.contains("full time")
        || lower.contains("part-time") || lower.contains("contract") || lower.contains("visa")
    {
        Field::Terms
    } else {
        Field::Role
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_job_posts_and_skips_other_comments() {
        let jobs = parse_thread(include_str!("../../fixtures/hn_thread.json")).unwrap();
        assert_eq!(jobs.len(), 2, "the chat reply and the deleted comment are skipped");

        let job = &jobs[0];
        assert_eq!(job.company.as_deref(), Some("Simple Analytics"));
        assert_eq!(job.title, "DevOps Engineer");
        assert_eq!(job.location, "REMOTE (worldwide, any timezone)");
        assert!(job.remote);
        assert_eq!(job.notes.as_deref(), Some("Full-time, 32h; €5,000/month"));
        assert_eq!(job.url, "https://news.ycombinator.com/item?id=50338732");
        assert!(job.description.as_deref().unwrap().contains("Hi HN"));

        assert_eq!(jobs[1].title, "Founding Full-Stack Engineer");
        assert_eq!(jobs[1].location, "Bengaluru, India");
        assert!(!jobs[1].remote);
    }

    #[test]
    fn fields_are_classified() {
        assert_eq!(kind("Senior Backend Engineer"), Field::Role);
        assert_eq!(kind("San Francisco, CA"), Field::Place);
        assert_eq!(kind("ONSITE"), Field::Place);
        assert_eq!(kind("$150k-$200k"), Field::Terms);
        assert_eq!(kind("https://acme.dev/jobs"), Field::Link);
    }
}
