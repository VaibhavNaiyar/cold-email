use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::emails::extract_emails;
use crate::published::Published;

const SEARCH_URL: &str = "https://hn.algolia.com/api/v1/search";
const USER_URL: &str = "https://hn.algolia.com/api/v1/users";
const MAX_AUTHORS: usize = 3;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(rename = "objectID")]
    id: String,
    author: Option<String>,
    comment_text: Option<String>,
    story_text: Option<String>,
    #[serde(rename = "_tags")]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct User {
    about: Option<String>,
}

/// Addresses in Hacker News posts that mention the domain, and in the profiles of those who
/// submitted them. Launch and "Who is hiring" posts are published to be answered.
pub async fn hn_emails(http: &reqwest::Client, domain: &str) -> Result<Vec<Published>> {
    let url = format!("{SEARCH_URL}?query=%22{domain}%22&tags=(story,comment)&hitsPerPage=30");
    let response: SearchResponse = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .context("Hacker News search failed")?
        .json()
        .await
        .context("Unexpected Hacker News response shape")?;

    let mut found = emails_in_hits(&response.hits);
    for author in story_authors(&response.hits) {
        let user: User = http
            .get(format!("{USER_URL}/{author}"))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .context("Hacker News profile failed")?
            .json()
            .await
            .context("Unexpected Hacker News profile shape")?;
        let profile = format!("https://news.ycombinator.com/user?id={author}");
        found.extend(extract_emails(user.about.as_deref().unwrap_or_default()).into_iter().map(
            |email| Published { email, source_type: "hn", source_url: profile.clone() },
        ));
    }
    Ok(found)
}

fn emails_in_hits(hits: &[Hit]) -> Vec<Published> {
    hits.iter()
        .flat_map(|hit| {
            let text = [hit.comment_text.as_deref(), hit.story_text.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let url = format!("https://news.ycombinator.com/item?id={}", hit.id);
            extract_emails(&text)
                .into_iter()
                .map(move |email| Published { email, source_type: "hn", source_url: url.clone() })
        })
        .collect()
}

fn story_authors(hits: &[Hit]) -> BTreeSet<&str> {
    hits.iter()
        .filter(|hit| hit.tags.iter().any(|tag| tag == "story"))
        .filter_map(|hit| hit.author.as_deref())
        .take(MAX_AUTHORS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"hits":[
        {"objectID":"1","author":"ada","title":"Launch HN: Acme","story_text":"Reach me at ada&#x40;acme.ai","comment_text":null,"_tags":["story","author_ada"]},
        {"objectID":"2","author":"bob","comment_text":"nice <a href=\"mailto:bob@acme.ai\">x</a>","_tags":["comment"]}
    ]}"#;

    #[test]
    fn finds_addresses_with_their_post_as_source() {
        let parsed: SearchResponse = serde_json::from_str(BODY).unwrap();
        let found = emails_in_hits(&parsed.hits);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].email, "ada@acme.ai");
        assert_eq!(found[0].source_url, "https://news.ycombinator.com/item?id=1");
    }

    #[test]
    fn only_story_submitters_are_profile_checked() {
        let parsed: SearchResponse = serde_json::from_str(BODY).unwrap();
        assert_eq!(story_authors(&parsed.hits).into_iter().collect::<Vec<_>>(), ["ada"]);
    }
}
