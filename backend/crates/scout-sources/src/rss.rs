use std::sync::LazyLock;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use feed_rs::parser;
use futures::{StreamExt, stream};
use regex::Regex;
use scout_llm::{Llm, extract_funding};
use serde_json::json;

use crate::domain::company_domain;
use crate::{RawSignal, SignalKind, SourceType};

const EXTRACTION_CONCURRENCY: usize = 4;

static FUNDING_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(raise[sd]?|raising|funding|funded|seed|pre-seed|series [a-e]|secures?|secured|bags?|lands?|nabs?|closes?|backed|round|\$\d+|€\d+|£\d+|₹\s?\d+|\d+\s?(m|mn|million|crore|cr)\b)",
    )
    .expect("funding regex is valid")
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("tag regex is valid"));

#[derive(Debug, Clone)]
pub struct FeedItem {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub published_at: DateTime<Utc>,
}

pub async fn fetch_feed(http: &reqwest::Client, feed_url: &str) -> Result<Vec<FeedItem>> {
    let body = http
        .get(feed_url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("Could not fetch {feed_url}"))?
        .bytes()
        .await?;
    parse_feed(&body).with_context(|| format!("Could not parse {feed_url}"))
}

pub fn parse_feed(body: &[u8]) -> Result<Vec<FeedItem>> {
    let feed = parser::parse(body)?;
    let items = feed.entries.into_iter().filter_map(|entry| {
        Some(FeedItem {
            url: entry.links.first()?.href.clone(),
            title: entry.title?.content,
            summary: entry
                .summary
                .map(|text| plain_text(&text.content))
                .unwrap_or_default(),
            published_at: entry.published.or(entry.updated)?,
        })
    });
    Ok(items.collect())
}

fn plain_text(html: &str) -> String {
    TAGS.replace_all(html, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cheap check that spares the LLM anything that cannot be a funding story.
pub fn may_be_funding(item: &FeedItem) -> bool {
    FUNDING_WORDS.is_match(&item.title)
}

pub struct Extracted {
    pub url: String,
    pub signal: Option<RawSignal>,
}

/// Runs extraction over `items`. Items that fail are left out so a later run retries them.
pub async fn extract(llm: &Llm, items: Vec<FeedItem>) -> (Vec<Extracted>, Vec<String>) {
    let outcomes: Vec<_> = stream::iter(items)
        .map(|item| async move {
            let result = extract_funding(llm, &item.title, &item.summary).await;
            (item, result)
        })
        .buffer_unordered(EXTRACTION_CONCURRENCY)
        .collect()
        .await;

    let mut extracted = Vec::new();
    let mut errors = Vec::new();
    for (item, result) in outcomes {
        match result {
            Ok(funding) => extracted.push(Extracted {
                url: item.url.clone(),
                signal: to_signal(&item, funding),
            }),
            Err(error) => errors.push(format!("{}: {error:#}", item.url)),
        }
    }
    (extracted, errors)
}

fn to_signal(item: &FeedItem, funding: scout_llm::FundingExtraction) -> Option<RawSignal> {
    if !funding.is_funding_announcement {
        return None;
    }
    let company = funding.company.filter(|name| !name.trim().is_empty())?;
    // A domain only counts if the article really contains it.
    let domain = funding
        .domain
        .filter(|domain| item.summary.to_lowercase().contains(&domain.to_lowercase()))
        .and_then(|domain| company_domain(&domain));
    Some(RawSignal {
        source_type: SourceType::Rss,
        source_url: item.url.clone(),
        company_name: company,
        legal_name: None,
        domain,
        one_liner: None,
        website: None,
        hiring: None,
        kind: SignalKind::Raised,
        title: item.title.clone(),
        published_at: item.published_at,
        amount_usd: funding.amount_usd,
        raw: json!({
            "round": funding.round,
            "investors": funding.investors,
            "founders": funding
                .founders
                .iter()
                .map(|founder| json!({ "name": founder.name, "role": founder.role }))
                .collect::<Vec<_>>(),
            "summary": item.summary,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scout_llm::FundingExtraction;

    const FIXTURE: &[u8] = include_bytes!("../fixtures/feed.xml");

    fn item(title: &str) -> FeedItem {
        FeedItem {
            url: "https://news.test/a".into(),
            title: title.into(),
            summary: "Acme (acme.ai) raised $3M.".into(),
            published_at: Utc::now(),
        }
    }

    fn funding(domain: Option<&str>) -> FundingExtraction {
        FundingExtraction {
            is_funding_announcement: true,
            company: Some("Acme".into()),
            domain: domain.map(String::from),
            round: Some("Seed".into()),
            amount_usd: Some(3_000_000),
            investors: vec![],
            founders: vec![],
        }
    }

    #[test]
    fn parses_items_and_strips_html() {
        let items = parse_feed(FIXTURE).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://news.test/acme-raises");
        assert_eq!(items[0].summary, "Acme raised a seed round.");
    }

    #[test]
    fn prefilter_keeps_funding_headlines_only() {
        assert!(may_be_funding(&item("Acme raises $3M seed from Index")));
        assert!(may_be_funding(&item("Bengaluru's Acme bags ₹40 crore")));
        assert!(may_be_funding(&item("Acme secures Series A")));
        assert!(!may_be_funding(&item("Why founders should sleep more")));
    }

    #[test]
    fn keeps_a_domain_found_in_the_article() {
        let signal = to_signal(&item("t"), funding(Some("acme.ai"))).unwrap();
        assert_eq!(signal.domain.as_deref(), Some("acme.ai"));
    }

    #[test]
    fn drops_a_domain_the_article_does_not_contain() {
        let signal = to_signal(&item("t"), funding(Some("invented.ai"))).unwrap();
        assert_eq!(signal.domain, None);
    }

    #[test]
    fn non_funding_items_produce_no_signal() {
        let mut result = funding(None);
        result.is_funding_announcement = false;
        assert!(to_signal(&item("t"), result).is_none());
    }
}
