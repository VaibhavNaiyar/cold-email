use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use scout_config::{Env, Sources};
use scout_db::{NewSignal, Pool};
use scout_llm::Llm;
use scout_sources::domain::name_key;
use scout_sources::{RawSignal, formd, resolver, rss, yc};
use serde_json::{Value, json};

#[derive(Default)]
struct Stats {
    yc_signals: usize,
    feed_items: usize,
    llm_extractions: usize,
    funding_signals: usize,
    formd_filings_read: usize,
    formd_signals: usize,
    new_signals: usize,
    duplicate_signals: usize,
    domain_searches: i64,
    domains_resolved: i64,
    errors: Vec<String>,
}

pub async fn run(
    env: &Env,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
    dry: bool,
) -> Result<Value> {
    if env.contact_user_agent().is_none() {
        eprintln!("warning: SEC_USER_AGENT has no real contact email; requests omit one");
    }
    let llm = crate::llm_client(env, http)?;
    let mut stats = Stats::default();

    // One source failing must not lose the others.
    match yc::fetch(http, sources.yc_batches).await {
        Ok(signals) => {
            stats.yc_signals = signals.len();
            store(pool, &signals, dry, &mut stats).await?;
        }
        Err(error) => stats.errors.push(format!("yc: {error:#}")),
    }

    let items = unseen_funding_items(sources, pool, http, &mut stats).await?;
    stats.llm_extractions = items.len();
    let (extracted, errors) = rss::extract(&llm, items).await;
    stats.errors.extend(errors);
    for item in extracted {
        if let Some(signal) = item.signal {
            stats.funding_signals += 1;
            store(pool, std::slice::from_ref(&signal), dry, &mut stats).await?;
        }
        if !dry {
            scout_db::mark_seen(pool, &item.url).await?;
        }
    }

    if env.contact_user_agent().is_some() {
        ingest_form_d(sources, pool, http, dry, &mut stats).await?;
    } else {
        stats.errors.push("Form D skipped: SEC requires a contact email".into());
    }

    if !dry {
        resolve_domains(env, sources, pool, http, &llm, &mut stats).await?;
    }

    Ok(json!({
        "dry": dry,
        "yc_signals": stats.yc_signals,
        "feed_items": stats.feed_items,
        "llm_extractions": stats.llm_extractions,
        "funding_signals": stats.funding_signals,
        "formd_filings_read": stats.formd_filings_read,
        "formd_signals": stats.formd_signals,
        "new_signals": stats.new_signals,
        "duplicate_signals": stats.duplicate_signals,
        "domain_searches": stats.domain_searches,
        "domains_resolved": stats.domains_resolved,
        "errors": stats.errors,
    }))
}

async fn ingest_form_d(
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
    dry: bool,
    stats: &mut Stats,
) -> Result<()> {
    let today = Utc::now().date_naive();
    let mut filings: HashMap<String, formd::Filing> =
        match formd::list_filings(http, today, sources.formd_days).await {
            Ok(filings) => filings
                .into_iter()
                .filter(formd::may_be_operating_company)
                .map(|filing| (filing.primary_doc_url.clone(), filing))
                .collect(),
            Err(error) => {
                stats.errors.push(format!("formd: {error:#}"));
                return Ok(());
            }
        };

    let urls: Vec<String> = filings.keys().cloned().collect();
    for url in scout_db::filter_unseen(pool, &urls).await? {
        let Some(filing) = filings.remove(&url) else { continue };
        match formd::fetch_signal(http, &filing).await {
            Ok(signal) => {
                stats.formd_filings_read += 1;
                if let Some(signal) = signal {
                    stats.formd_signals += 1;
                    store(pool, std::slice::from_ref(&signal), dry, stats).await?;
                }
                if !dry {
                    scout_db::mark_seen(pool, &url).await?;
                }
            }
            Err(error) => stats.errors.push(format!("{error:#}")),
        }
    }
    Ok(())
}

async fn unseen_funding_items(
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
    stats: &mut Stats,
) -> Result<Vec<rss::FeedItem>> {
    // Keyed by URL because one article can appear in two feeds.
    let mut items = HashMap::new();
    for feed in &sources.feeds {
        match rss::fetch_feed(http, feed).await {
            Ok(fetched) => {
                stats.feed_items += fetched.len();
                items.extend(fetched.into_iter().map(|item| (item.url.clone(), item)));
            }
            Err(error) => stats.errors.push(format!("{error:#}")),
        }
    }
    let urls: Vec<String> = items.keys().cloned().collect();
    let unseen = scout_db::filter_unseen(pool, &urls).await?;
    Ok(unseen
        .iter()
        .filter_map(|url| items.remove(url))
        .filter(rss::may_be_funding)
        .collect())
}

async fn store(pool: &Pool, signals: &[RawSignal], dry: bool, stats: &mut Stats) -> Result<()> {
    if dry {
        return Ok(());
    }
    for signal in signals {
        let key = name_key(&signal.company_name);
        let row = NewSignal {
            company_name: &signal.company_name,
            legal_name: signal.legal_name.as_deref(),
            name_key: &key,
            domain: signal.domain.as_deref(),
            domain_source: signal.domain.as_ref().map(|_| signal.source_type.as_str()),
            one_liner: signal.one_liner.as_deref(),
            website: signal.website.as_deref(),
            hiring: signal.hiring,
            source_type: signal.source_type.as_str(),
            source_url: &signal.source_url,
            title: &signal.title,
            published_at: signal.published_at,
            kind: signal.kind.as_str(),
            amount_usd: signal.amount_usd,
            raw: &signal.raw,
        };
        if scout_db::upsert_signal(pool, &row).await? {
            stats.new_signals += 1;
        } else {
            stats.duplicate_signals += 1;
        }
    }
    Ok(())
}

async fn resolve_domains(
    env: &Env,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
    llm: &Llm,
    stats: &mut Stats,
) -> Result<()> {
    let Some(api_key) = env.tavily_api_key.as_deref() else {
        stats.errors.push("TAVILY_API_KEY unset: domain resolution skipped".into());
        return Ok(());
    };
    let used = scout_db::domain_searches_today(pool).await?;
    let budget = (sources.max_domain_searches_per_day - used).max(0);
    let companies =
        scout_db::unresolved_companies(pool, sources.max_domain_attempts, budget).await?;

    for company in companies {
        // Recorded first, so a failed search still counts against the quota.
        scout_db::mark_domain_attempt(pool, company.id).await?;
        stats.domain_searches += 1;
        match resolver::resolve(http, llm, api_key, &company.name, &company.signal_title).await {
            Ok(Some(domain)) => {
                scout_db::resolve_domain(pool, company.id, &domain, "tavily").await?;
                stats.domains_resolved += 1;
            }
            Ok(None) => {}
            Err(error) => stats.errors.push(format!("{}: {error:#}", company.name)),
        }
    }
    Ok(())
}
