use anyhow::Result;
use chrono::{DateTime, FixedOffset, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::{DESCRIPTION_CHARS, Posting, is_remote, trim_description};

#[derive(Deserialize)]
struct Board {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
struct Job {
    id: u64,
    title: String,
    absolute_url: String,
    location: Option<Named>,
    first_published: Option<DateTime<FixedOffset>>,
    updated_at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    metadata: Option<Vec<Metadata>>,
    #[serde(default)]
    departments: Vec<Named>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct Detail {
    content: Option<String>,
}

pub(super) fn list_url(slug: &str) -> String {
    format!("https://boards-api.greenhouse.io/v1/boards/{slug}/jobs")
}

pub(super) fn parse(body: &str) -> Result<Vec<Posting>> {
    let board: Board = serde_json::from_str(body)?;
    Ok(board.jobs.into_iter().map(posting).collect())
}

pub(super) fn detail_url(slug: &str, id: &str) -> String {
    format!("https://boards-api.greenhouse.io/v1/boards/{slug}/jobs/{id}")
}

/// Greenhouse sends the description as escaped HTML: unescape once, then read the text.
pub(super) fn parse_description(body: &str) -> Result<Option<String>> {
    let detail: Detail = serde_json::from_str(body)?;
    Ok(trim_description(detail.content.map(|escaped| {
        let html = scout_enrich::html_text(&escaped, usize::MAX);
        scout_enrich::html_text(&html, DESCRIPTION_CHARS)
    })))
}

fn posting(job: Job) -> Posting {
    let location = job.location.map(|location| location.name).unwrap_or_default();
    let remote_flag = job.metadata.iter().flatten().any(|meta| {
        meta.name.to_lowercase().contains("location type")
            && meta.value.as_str().is_some_and(|value| value.eq_ignore_ascii_case("remote"))
    });
    Posting {
        company: None,
        external_id: job.id.to_string(),
        remote: is_remote(remote_flag, &location),
        location,
        title: job.title,
        department: job.departments.into_iter().next().map(|department| department.name),
        url: job.absolute_url,
        posted_at: job.first_published.or(job.updated_at).map(|date| date.with_timezone(&Utc)),
        description: None,
        notes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_board_fixture() {
        let jobs = parse(include_str!("../../fixtures/greenhouse.json")).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].external_id, "5456619008");
        assert_eq!(jobs[0].url, "https://job-boards.greenhouse.io/acme/jobs/5456619008");
        assert!(!jobs[0].remote);
        assert_eq!(jobs[0].department.as_deref(), Some("Engineering"));
        assert_eq!(jobs[0].posted_at.unwrap().to_rfc3339(), "2026-10-19T15:17:24+00:00");
        assert!(jobs[1].remote, "metadata says Remote");
    }

    #[test]
    fn unescapes_the_description() {
        let body = r#"{"content": "&lt;p&gt;Build &lt;b&gt;agents&lt;/b&gt;&lt;/p&gt;"}"#;
        assert_eq!(parse_description(body).unwrap().as_deref(), Some("Build agents"));
    }
}
