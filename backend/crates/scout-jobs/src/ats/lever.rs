use anyhow::Result;
use chrono::DateTime;
use serde::Deserialize;

use super::{Posting, is_remote, join_notes, trim_description};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    id: String,
    text: String,
    hosted_url: String,
    #[serde(default)]
    categories: Categories,
    created_at: Option<i64>,
    workplace_type: Option<String>,
    description_plain: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Categories {
    location: Option<String>,
    team: Option<String>,
    commitment: Option<String>,
    #[serde(default)]
    all_locations: Vec<String>,
}

pub(super) fn list_url(slug: &str) -> String {
    format!("https://api.lever.co/v0/postings/{slug}?mode=json")
}

pub(super) fn parse(body: &str) -> Result<Vec<Posting>> {
    let jobs: Vec<Job> = serde_json::from_str(body)?;
    Ok(jobs.into_iter().map(posting).collect())
}

fn posting(job: Job) -> Posting {
    let Categories { location, team, commitment, all_locations } = job.categories;
    let location = if all_locations.is_empty() {
        location.unwrap_or_default()
    } else {
        all_locations.join(" | ")
    };
    let remote_flag = job.workplace_type.as_deref() == Some("remote");
    Posting {
        company: None,
        external_id: job.id,
        remote: is_remote(remote_flag, &location),
        location,
        title: job.text,
        department: team,
        url: job.hosted_url,
        posted_at: job.created_at.and_then(DateTime::from_timestamp_millis),
        description: trim_description(job.description_plain),
        notes: join_notes([commitment.as_deref()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_postings_fixture() {
        let jobs = parse(include_str!("../../fixtures/lever.json")).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].title, "Forward Deployed Engineer");
        assert_eq!(jobs[0].location, "Bengaluru, India | Remote");
        assert!(jobs[0].remote);
        assert_eq!(jobs[0].notes.as_deref(), Some("Full-time"));
        assert_eq!(jobs[0].posted_at.unwrap().to_rfc3339(), "2026-08-11T08:18:11.368+00:00");
        assert!(jobs[0].description.as_deref().unwrap().starts_with("Ship agents"));
        assert!(!jobs[1].remote);
    }
}
