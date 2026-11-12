use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::domain::company_domain;
use crate::{RawSignal, SignalKind, SourceType};

const META_URL: &str = "https://yc-oss.github.io/api/meta.json";
const BASE_URL: &str = "https://yc-oss.github.io/api/batches";
const SEASONS: [&str; 4] = ["winter", "spring", "summer", "fall"];

#[derive(Deserialize)]
struct Meta {
    batches: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Company {
    name: String,
    website: Option<String>,
    one_liner: Option<String>,
    launched_at: Option<i64>,
    #[serde(rename = "isHiring")]
    is_hiring: bool,
    batch: String,
    status: String,
    url: String,
    tags: Vec<String>,
    industries: Vec<String>,
    regions: Vec<String>,
    team_size: Option<i64>,
}

pub async fn fetch(http: &reqwest::Client, batch_count: usize) -> Result<Vec<RawSignal>> {
    let meta: Meta = get_json(http, META_URL).await?;
    let mut signals = Vec::new();
    for slug in latest_batches(meta.batches.keys(), batch_count) {
        let companies: Vec<Company> = get_json(http, &format!("{BASE_URL}/{slug}.json")).await?;
        signals.extend(companies.into_iter().filter_map(to_signal));
    }
    Ok(signals)
}

async fn get_json<T: serde::de::DeserializeOwned>(http: &reqwest::Client, url: &str) -> Result<T> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("Could not fetch {url}"))?
        .json()
        .await
        .with_context(|| format!("Unexpected response shape from {url}"))
}

/// Newest first. Slugs look like `summer-2026`; `other` and old codes are skipped.
fn latest_batches<'a>(slugs: impl Iterator<Item = &'a String>, count: usize) -> Vec<&'a str> {
    let mut dated: Vec<((i32, usize), &str)> = slugs
        .filter_map(|slug| {
            let (season, year) = slug.split_once('-')?;
            let season = SEASONS.iter().position(|name| *name == season)?;
            Some(((year.parse().ok()?, season), slug.as_str()))
        })
        .collect();
    dated.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    dated.into_iter().take(count).map(|(_, slug)| slug).collect()
}

fn to_signal(company: Company) -> Option<RawSignal> {
    if company.status != "Active" {
        return None;
    }
    let domain = company_domain(company.website.as_deref()?)?;
    let published_at = company
        .launched_at
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .unwrap_or_else(Utc::now);
    let (kind, verb) = if company.is_hiring {
        (SignalKind::Hiring, "is hiring")
    } else {
        (SignalKind::Launched, "launched")
    };
    Some(RawSignal {
        source_type: SourceType::Yc,
        title: format!("{} ({}) {verb}", company.name, company.batch),
        raw: json!({
            "batch": company.batch,
            "tags": company.tags,
            "industries": company.industries,
            "regions": company.regions,
            "team_size": company.team_size,
        }),
        source_url: company.url,
        company_name: company.name,
        legal_name: None,
        domain: Some(domain),
        one_liner: company.one_liner,
        website: company.website,
        hiring: Some(company.is_hiring),
        kind,
        published_at,
        amount_usd: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/yc_batch.json");

    #[test]
    fn parses_a_batch_and_keeps_active_companies_only() {
        let companies: Vec<Company> = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(companies.len(), 3);
        let signals: Vec<_> = companies.into_iter().filter_map(to_signal).collect();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, SignalKind::Hiring);
        assert_eq!(signals[0].domain.as_deref(), Some("cerebralvalley.example"));
    }

    #[test]
    fn picks_the_newest_dated_batches() {
        let slugs = ["summer-2026", "fall-2026", "winter-2027", "other", "w09", "unspecified"]
            .map(String::from);
        assert_eq!(latest_batches(slugs.iter(), 2), ["winter-2027", "fall-2026"]);
    }
}
