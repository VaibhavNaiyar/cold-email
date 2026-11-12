use anyhow::Result;
use chrono::DateTime;
use serde::Deserialize;
use serde::de::IgnoredAny;

use super::{DESCRIPTION_CHARS, Posting, trim_description};

// RemoteOK's terms: link back to the role on Remote OK and name it as the source.
// The stored URL is theirs and the page shows "via RemoteOK".
const API_URL: &str = "https://remoteok.com/api";
const ANYWHERE: &str = "Worldwide";

// The first element is a legal notice, not a job.
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Job(Job),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
struct Job {
    id: String,
    position: String,
    company: String,
    url: String,
    epoch: Option<i64>,
    #[serde(default)]
    location: String,
    description: Option<String>,
}

pub(super) fn list_url(tag: &str) -> String {
    format!("{API_URL}?tag={tag}")
}

pub(super) fn parse(body: &str) -> Result<Vec<Posting>> {
    let entries: Vec<Entry> = serde_json::from_str(body)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Job(job) => Some(posting(job)),
            Entry::Other(_) => None,
        })
        .collect())
}

fn posting(job: Job) -> Posting {
    let location = if job.location.trim().is_empty() { ANYWHERE.to_owned() } else { job.location };
    Posting {
        company: Some(job.company),
        external_id: job.id,
        title: job.position,
        location,
        remote: true,
        department: None,
        url: job.url,
        posted_at: job.epoch.and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
        description: trim_description(
            job.description.map(|html| scout_enrich::html_text(&html, DESCRIPTION_CHARS)),
        ),
        notes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_the_notice_and_reads_jobs() {
        let jobs = parse(include_str!("../../fixtures/remoteok.json")).unwrap();
        assert_eq!(jobs.len(), 2);
        let job = &jobs[0];
        assert_eq!(job.title, "DevOps Engineer");
        assert_eq!(job.company.as_deref(), Some("Acme"));
        assert!(job.remote);
        assert_eq!(job.location, "Worldwide");
        assert_eq!(job.url, "https://remoteOK.com/remote-jobs/devops-engineer-acme-1137620");
        assert_eq!(job.posted_at.unwrap().to_rfc3339(), "2026-11-02T16:00:08+00:00");
        assert_eq!(job.description.as_deref(), Some("Run our Kubernetes clusters."));
        assert_eq!(jobs[1].location, "India");
    }
}
