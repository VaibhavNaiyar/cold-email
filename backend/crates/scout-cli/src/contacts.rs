use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::Result;
use futures::{StreamExt, stream};
use scout_config::{Env, Profile, Sources};
use scout_db::{ContactCandidate, MatchFilter, NewContact, Pool};
use scout_enrich::{Judged, Person, Published, Rank, extract_emails, judge, person, prioritise};
use scout_llm::{Llm, extract_team};
use serde_json::{Value, json};

const CONCURRENCY: usize = 4;

struct Found {
    published: Published,
    judged: Judged,
}

struct Outcome {
    company: ContactCandidate,
    found: Vec<Found>,
    founders: usize,
    errors: Vec<String>,
}

pub async fn run(
    env: &Env,
    profile: &Profile,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
) -> Result<Value> {
    let llm = crate::llm_client(env, http)?;
    let filter = MatchFilter {
        must_be_ai: profile.must_be_ai,
        stages: &profile.stage,
        excluded_domains: &profile.exclude_domains,
        onsite_regions: &profile.onsite_regions,
    };
    let queue = scout_db::contact_candidates(
        pool,
        &filter,
        sources.min_contact_score,
        sources.max_contact_companies_per_run,
    )
    .await?;
    let target = i64::from(profile.max_emails_per_day) * 2;
    let searches = AtomicI64::new(sources.max_contact_searches_per_run);
    let api_key = env.tavily_api_key.as_deref();

    let mut checked = 0;
    let (mut reached, mut personal, mut founders_known, mut stored) = (0i64, 0, 0, 0);
    let mut by_source: BTreeMap<&'static str, i64> = BTreeMap::new();
    let mut errors = Vec::new();

    // Keep going down the ranking until enough companies have an address.
    let mut queue = queue.into_iter().peekable();
    while reached < target && queue.peek().is_some() {
        let batch: Vec<_> = queue.by_ref().take(CONCURRENCY).collect();
        let outcomes: Vec<_> = stream::iter(batch)
            .map(|company| find_contacts(http, &llm, api_key, &searches, company))
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

        for outcome in outcomes {
            checked += 1;
            founders_known += i64::from(outcome.founders > 0);
            errors.extend(outcome.errors.iter().cloned());
            if !outcome.found.is_empty() {
                reached += 1;
            }
            if outcome.found.iter().any(|found| !found.judged.rank.is_generic()) {
                personal += 1;
            }
            for found in &outcome.found {
                if save(pool, outcome.company.id, found).await? {
                    stored += 1;
                    *by_source.entry(found.published.source_type).or_default() += 1;
                }
            }
            scout_db::mark_contacts_checked(pool, outcome.company.id).await?;
            print_outcome(&outcome);
        }
    }

    Ok(json!({
        "companies_checked": checked,
        "founders_known": founders_known,
        "companies_with_email": reached,
        "companies_with_person_email": personal,
        "emails_stored": stored,
        "by_source": by_source,
        "searches_used": sources.max_contact_searches_per_run - searches.load(Ordering::Relaxed).max(0),
        "errors": errors,
    }))
}

async fn find_contacts(
    http: &reqwest::Client,
    llm: &Llm,
    api_key: Option<&str>,
    searches: &AtomicI64,
    company: ContactCandidate,
) -> Outcome {
    let mut errors = Vec::new();
    let mut people = signal_people(&company);
    if let Some(url) = &company.yc_url {
        match scout_enrich::yc_founders(http, url).await {
            Ok(found) => people.extend(found),
            Err(error) => errors.push(format!("{}: {error:#}", company.name)),
        }
    }

    let pages = scout_enrich::read_site(http, &company.domain).await;
    if people.is_empty() {
        people.extend(team_from_pages(llm, &company, &pages, &mut errors).await);
    }
    let people = prioritise(people);

    let mut published: Vec<Published> = pages
        .iter()
        .flat_map(|page| {
            extract_emails(&page.html).into_iter().map(|email| Published {
                email,
                source_type: "website",
                source_url: page.url.clone(),
            })
        })
        .collect();
    match scout_enrich::hn_emails(http, &company.domain).await {
        Ok(found) => published.extend(found),
        Err(error) => errors.push(format!("{}: {error:#}", company.name)),
    }

    let mut found = judge_all(published, &company.domain, &people);
    if !found.iter().any(|item| item.judged.rank.is_founder())
        && let Some(api_key) = api_key.filter(|_| take_search(searches)) {
            let query = search_query(&company, &people);
            match scout_enrich::search_emails(http, api_key, &query).await {
                Ok(results) => {
                    found.extend(judge_all(results, &company.domain, &people));
                    found = keep_best(found);
                }
                Err(error) => errors.push(format!("{}: {error:#}", company.name)),
            }
        }
    Outcome { company, found, founders: people.len(), errors }
}

fn take_search(searches: &AtomicI64) -> bool {
    searches.fetch_sub(1, Ordering::Relaxed) > 0
}

fn signal_people(company: &ContactCandidate) -> Vec<Person> {
    let Some(Value::Array(founders)) = &company.signal_founders else {
        return Vec::new();
    };
    founders
        .iter()
        .filter_map(|founder| {
            let name = founder["name"].as_str()?;
            let role = founder["role"].as_str().or_else(|| founder["roles"][0].as_str());
            Some(person(name, role))
        })
        .collect()
}

async fn team_from_pages(
    llm: &Llm,
    company: &ContactCandidate,
    pages: &[scout_enrich::Page],
    errors: &mut Vec<String>,
) -> Vec<Person> {
    let text: String = pages
        .iter()
        .map(|page| page.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(12_000)
        .collect();
    if text.is_empty() {
        return Vec::new();
    }
    match extract_team(llm, &company.name, &text).await {
        Ok(team) => team
            .iter()
            .map(|member| person(&member.name, member.role.as_deref()))
            .collect(),
        Err(error) => {
            errors.push(format!("{}: {error:#}", company.name));
            Vec::new()
        }
    }
}

fn judge_all(published: Vec<Published>, domain: &str, people: &[Person]) -> Vec<Found> {
    keep_best(
        published
            .into_iter()
            .filter_map(|published| {
                let judged = judge(&published.email, domain, people)?;
                // Off the company's own site, an unnamed address could be anyone's.
                let unattributed =
                    judged.rank == Rank::NamedPerson && published.source_type != "website";
                (!unattributed).then_some(Found { published, judged })
            })
            .collect(),
    )
}

fn keep_best(mut found: Vec<Found>) -> Vec<Found> {
    found.sort_by_key(|item| item.judged.rank);
    let mut seen = HashSet::new();
    found.retain(|item| seen.insert(item.published.email.clone()));
    found
}

fn search_query(company: &ContactCandidate, people: &[Person]) -> String {
    match people.first() {
        Some(founder) => format!("\"{}\" {} founder email contact", founder.name, company.domain),
        None => format!("\"@{}\" email founder contact", company.domain),
    }
}

async fn save(pool: &Pool, company_id: i64, found: &Found) -> Result<bool> {
    let person = found.judged.person.as_ref();
    let rank = found.judged.rank;
    let contact = NewContact {
        company_id,
        full_name: person.map(|person| person.name.as_str()),
        role: person.and_then(|person| person.role.as_deref()),
        is_technical: rank == Rank::TechnicalFounder,
        is_generic: rank.is_generic(),
        email: &found.published.email,
        source_type: found.published.source_type,
        source_url: &found.published.source_url,
        confidence: confidence(rank),
    };
    scout_db::save_contact(pool, &contact).await
}

fn confidence(rank: Rank) -> f32 {
    match rank {
        Rank::TechnicalFounder | Rank::Founder => 0.9,
        Rank::FounderInbox => 0.7,
        Rank::NamedPerson => 0.6,
        Rank::GeneralInbox => 0.5,
    }
}

fn print_outcome(outcome: &Outcome) {
    let best = outcome.found.first();
    println!(
        "[{}] {} ({}): {}",
        outcome.company.score,
        outcome.company.name,
        outcome.company.domain,
        best.map_or("no address".to_owned(), |item| {
            format!("{} ({:?}, {})", item.published.email, item.judged.rank, item.published.source_url)
        }),
    );
}
