use anyhow::{Context, Result};
use scout_llm::{Llm, choose_domain};
use serde::Deserialize;
use serde_json::json;

use crate::domain::company_domain;

const TAVILY_URL: &str = "https://api.tavily.com/search";
const MAX_RESULTS: u32 = 8;
const SNIPPET_CHARS: usize = 300;

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    title: String,
    url: String,
    content: String,
}

/// Finds a company's root domain, or `None` when no result is confidently its own site.
pub async fn resolve(
    http: &reqwest::Client,
    llm: &Llm,
    api_key: &str,
    company: &str,
    signal: &str,
) -> Result<Option<String>> {
    let response: SearchResponse = http
        .post(TAVILY_URL)
        .bearer_auth(api_key)
        .json(&json!({
            "query": format!("\"{company}\" startup official website"),
            "search_depth": "basic",
            "max_results": MAX_RESULTS,
        }))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .context("Tavily search failed")?
        .json()
        .await
        .context("Unexpected Tavily response shape")?;

    let candidates: Vec<(String, &SearchResult)> = response
        .results
        .iter()
        .filter_map(|result| Some((company_domain(&result.url)?, result)))
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }

    let listing = candidates
        .iter()
        .map(|(domain, result)| format_candidate(domain, result))
        .collect::<Vec<_>>()
        .join("\n");
    let choice = choose_domain(llm, company, signal, &listing).await?;
    Ok(accepted(choice.domain, &candidates))
}

fn format_candidate(domain: &str, result: &SearchResult) -> String {
    let snippet: String = result.content.chars().take(SNIPPET_CHARS).collect();
    format!("- {domain} | {} | {} | {snippet}", result.title, result.url)
}

// The model can only pick from what the search returned.
fn accepted(choice: Option<String>, candidates: &[(String, &SearchResult)]) -> Option<String> {
    let choice = company_domain(&choice?)?;
    candidates
        .iter()
        .any(|(domain, _)| *domain == choice)
        .then_some(choice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_tavily_shape() {
        let body = r#"{"query":"q","results":[{"title":"Acme","url":"https://acme.ai",
            "content":"Home","score":0.9,"favicon":null}],"response_time":1.2}"#;
        let parsed: SearchResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.results[0].url, "https://acme.ai");
    }

    #[test]
    fn refuses_a_domain_outside_the_search_results() {
        let result = SearchResult {
            title: "Acme".into(),
            url: "https://acme.ai".into(),
            content: String::new(),
        };
        let candidates = vec![("acme.ai".to_string(), &result)];
        assert_eq!(accepted(Some("acme.ai".into()), &candidates).as_deref(), Some("acme.ai"));
        assert_eq!(accepted(Some("invented.ai".into()), &candidates), None);
        assert_eq!(accepted(None, &candidates), None);
    }
}
