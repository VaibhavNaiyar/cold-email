use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;

use super::{Posting, is_remote, join_notes};

const BASE_URL: &str = "https://www.ycombinator.com";

#[derive(Deserialize)]
struct Page {
    props: Props,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Props {
    job_postings: Option<Vec<Job>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    id: u64,
    title: String,
    url: String,
    location: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    role_specific_type: Option<String>,
    min_experience: Option<String>,
    visa: Option<String>,
    salary_range: Option<String>,
    created_at: Option<String>,
}

pub(super) fn list_url(slug: &str) -> String {
    format!("{BASE_URL}/companies/{slug}/jobs")
}

pub(super) fn parse(html: &str, now: DateTime<Utc>) -> Result<Vec<Posting>> {
    let page: Page = scout_enrich::yc_data_page(html)?;
    Ok(page
        .props
        .job_postings
        .unwrap_or_default()
        .into_iter()
        .map(|job| posting(job, now))
        .collect())
}

fn posting(job: Job, now: DateTime<Utc>) -> Posting {
    let location = job.location.unwrap_or_default();
    let experience = job.min_experience.map(|value| format!("Experience: {value}"));
    let visa = job.visa.map(|value| format!("Visa: {value}"));
    Posting {
        company: None,
        external_id: job.id.to_string(),
        remote: is_remote(false, &location),
        location,
        title: job.title,
        url: format!("{BASE_URL}{}", job.url),
        posted_at: job.created_at.as_deref().and_then(|age| posted_at(age, now)),
        description: None,
        notes: join_notes([
            job.kind.as_deref(),
            experience.as_deref(),
            visa.as_deref(),
            job.salary_range.as_deref(),
        ]),
        department: job.role_specific_type,
    }
}

/// YC shows ages such as "4 days" or "about 1 month"; turn them back into a date.
fn posted_at(age: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let words: Vec<&str> = age.split_whitespace().collect();
    let count = words.iter().find_map(|word| word.parse::<i32>().ok()).unwrap_or(1);
    let unit = words.iter().rev().find_map(|word| unit(word))?;
    Some(now - unit * count)
}

fn unit(word: &str) -> Option<TimeDelta> {
    match word.trim_end_matches('s') {
        "minute" => Some(TimeDelta::minutes(1)),
        "hour" => Some(TimeDelta::hours(1)),
        "day" => Some(TimeDelta::days(1)),
        "week" => Some(TimeDelta::weeks(1)),
        "month" => Some(TimeDelta::days(30)),
        "year" => Some(TimeDelta::days(365)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-02T00:00:00Z").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn reads_postings_from_the_page() {
        let jobs = parse(include_str!("../../fixtures/yc_jobs.html"), now()).unwrap();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.title, "Founding AI Engineer");
        assert_eq!(job.url, "https://www.ycombinator.com/companies/acme/jobs/FX4w-founding-ai-engineer");
        assert!(job.remote);
        assert_eq!(job.department.as_deref(), Some("AI engineer"));
        assert_eq!(
            job.notes.as_deref(),
            Some("Full-time; Experience: Any (new grads ok); Visa: Will sponsor; $80K - $140K")
        );
        assert_eq!(job.posted_at.unwrap().to_rfc3339(), "2026-10-29T00:00:00+00:00");
    }

    #[test]
    fn a_page_without_postings_is_empty() {
        let html = r#"<div data-page="{&quot;props&quot;:{&quot;jobPostings&quot;:null}}"></div>"#;
        assert!(parse(html, now()).unwrap().is_empty());
    }

    #[test]
    fn ages_become_dates() {
        let day = TimeDelta::days(1);
        assert_eq!(posted_at("4 days", now()), Some(now() - day * 4));
        assert_eq!(posted_at("about 1 month", now()), Some(now() - day * 30));
        assert_eq!(posted_at("an hour", now()), Some(now() - TimeDelta::hours(1)));
        assert_eq!(posted_at("recently", now()), None);
    }
}
