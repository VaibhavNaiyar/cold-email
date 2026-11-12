use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::emails::extract_emails;
use crate::published::Published;

const TAVILY_URL: &str = "https://api.tavily.com/search";
const MAX_RESULTS: u32 = 8;
const MAX_URL_RESULTS: u32 = 20;
// Brokers sell inferred addresses, and GitHub's policy bans using its data for this.
const EXCLUDED: &[&str] = &[
    "apollo.io", "contactout.com", "crunchbase.com", "github.com", "githubusercontent.com",
    "hunter.io", "instantly.ai", "leadiq.com", "linkedin.com", "lusha.com", "rocketreach.co",
    "signalhire.com", "snov.io", "theorg.com", "tomba.io", "zoominfo.com",
];

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    url: String,
    content: String,
    raw_content: Option<String>,
}

/// Searches the web for a published address, keeping only those present word for word on a page.
pub async fn search_emails(
    http: &reqwest::Client,
    api_key: &str,
    query: &str,
) -> Result<Vec<Published>> {
    let body = json!({
        "query": query,
        "search_depth": "basic",
        "max_results": MAX_RESULTS,
        "include_raw_content": true,
        "exclude_domains": EXCLUDED,
    });
    Ok(emails_in_results(&tavily(http, api_key, &body).await?.results))
}

/// Result URLs of a search limited to the given hosts.
pub async fn search_urls(
    http: &reqwest::Client,
    api_key: &str,
    query: &str,
    hosts: &[&str],
) -> Result<Vec<String>> {
    let body = json!({
        "query": query,
        "search_depth": "basic",
        "max_results": MAX_URL_RESULTS,
        "include_domains": hosts,
    });
    let response = tavily(http, api_key, &body).await?;
    Ok(response.results.into_iter().map(|result| result.url).collect())
}

async fn tavily(http: &reqwest::Client, api_key: &str, body: &Value) -> Result<SearchResponse> {
    http.post(TAVILY_URL)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .context("Tavily search failed")?
        .json()
        .await
        .context("Unexpected Tavily response shape")
}

fn emails_in_results(results: &[SearchResult]) -> Vec<Published> {
    results
        .iter()
        .filter(|result| !EXCLUDED.iter().any(|host| result.url.contains(host)))
        .flat_map(|result| {
            let text = format!("{} {}", result.content, result.raw_content.as_deref().unwrap_or(""));
            extract_emails(&text).into_iter().map(|email| Published {
                email,
                source_type: "search",
                source_url: result.url.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_page_addresses_and_skips_excluded_hosts() {
        let results = vec![
            SearchResult {
                url: "https://podcast.test/ep1".into(),
                content: "Write to ada@acme.ai".into(),
                raw_content: None,
            },
            SearchResult {
                url: "https://rocketreach.co/ada".into(),
                content: "guess@acme.ai".into(),
                raw_content: None,
            },
        ];
        let found = emails_in_results(&results);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_url, "https://podcast.test/ep1");
    }
}
