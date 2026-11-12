use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;
use chrono::DateTime;
use serde::Deserialize;
use tokio::time::sleep;

use super::{DESCRIPTION_CHARS, Posting, get_text, join_notes, trim_description};

const SEARCH_URL: &str = "https://himalayas.app/jobs/api/search";
// Search returns 20 per page; the first pages hold the newest matches.
const PAGE_SIZE: usize = 20;
const PAGES: usize = 3;
const ANYWHERE: &str = "Worldwide";

#[derive(Deserialize)]
struct Page {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    title: String,
    company_name: String,
    guid: String,
    #[serde(default)]
    location_restrictions: Vec<String>,
    #[serde(default)]
    seniority: Vec<String>,
    employment_type: Option<String>,
    pub_date: Option<i64>,
    description: Option<String>,
}

/// The newest roles for one search query, such as "devops engineer".
pub(super) async fn fetch(http: &reqwest::Client, query: &str, gap: Duration) -> Result<Vec<Posting>> {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let mut seen = HashSet::new();
    let mut postings = Vec::new();
    for page in 1..=PAGES {
        if page > 1 {
            sleep(gap).await;
        }
        let batch = parse(&get_text(http, &format!("{SEARCH_URL}?q={encoded}&page={page}")).await?)?;
        let last_page = batch.len() < PAGE_SIZE;
        postings.extend(batch.into_iter().filter(|posting| seen.insert(posting.external_id.clone())));
        if last_page {
            break;
        }
    }
    Ok(postings)
}

fn parse(body: &str) -> Result<Vec<Posting>> {
    let page: Page = serde_json::from_str(body)?;
    Ok(page.jobs.into_iter().map(posting).collect())
}

fn posting(job: Job) -> Posting {
    let location = if job.location_restrictions.is_empty() {
        ANYWHERE.to_owned()
    } else {
        job.location_restrictions.join(" | ")
    };
    let seniority = (!job.seniority.is_empty()).then(|| job.seniority.join(", "));
    Posting {
        company: Some(job.company_name),
        external_id: job.guid.clone(),
        title: job.title,
        location,
        remote: true,
        department: None,
        url: job.guid,
        posted_at: job.pub_date.and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
        description: trim_description(
            job.description.map(|html| scout_enrich::html_text(&html, DESCRIPTION_CHARS)),
        ),
        notes: join_notes([job.employment_type.as_deref(), seniority.as_deref()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_search_page() {
        let jobs = parse(include_str!("../../fixtures/himalayas.json")).unwrap();
        assert_eq!(jobs.len(), 2);
        let job = &jobs[0];
        assert_eq!(job.title, "Devops Engineer");
        assert_eq!(job.company.as_deref(), Some("Miratech"));
        assert_eq!(job.location, "India | Poland");
        assert!(job.remote);
        assert_eq!(job.url, "https://himalayas.app/companies/miratech/jobs/devops-engineer-1");
        assert_eq!(job.notes.as_deref(), Some("Full Time; Mid-level, Senior"));
        assert_eq!(job.posted_at.unwrap().to_rfc3339(), "2026-11-03T15:52:55+00:00");
        assert_eq!(jobs[1].location, "Worldwide");
    }
}
