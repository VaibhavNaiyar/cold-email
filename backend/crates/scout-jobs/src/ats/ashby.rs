use anyhow::Result;
use chrono::{DateTime, FixedOffset, Utc};
use serde::Deserialize;

use super::{Posting, is_remote, join_notes, trim_description};

#[derive(Deserialize)]
struct Board {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    id: String,
    title: String,
    department: Option<String>,
    location: Option<String>,
    #[serde(default)]
    secondary_locations: Vec<Secondary>,
    published_at: Option<DateTime<FixedOffset>>,
    is_listed: Option<bool>,
    is_remote: Option<bool>,
    workplace_type: Option<String>,
    employment_type: Option<String>,
    job_url: String,
    description_plain: Option<String>,
}

#[derive(Deserialize)]
struct Secondary {
    location: String,
}

pub(super) fn list_url(slug: &str) -> String {
    format!("https://api.ashbyhq.com/posting-api/job-board/{slug}")
}

pub(super) fn parse(body: &str) -> Result<Vec<Posting>> {
    let board: Board = serde_json::from_str(body)?;
    Ok(board
        .jobs
        .into_iter()
        .filter(|job| job.is_listed != Some(false))
        .map(posting)
        .collect())
}

fn posting(job: Job) -> Posting {
    let location = job
        .location
        .into_iter()
        .chain(job.secondary_locations.into_iter().map(|secondary| secondary.location))
        .collect::<Vec<_>>()
        .join(" | ");
    let remote_flag = job.is_remote == Some(true)
        || job.workplace_type.as_deref().is_some_and(|kind| kind.eq_ignore_ascii_case("remote"));
    Posting {
        company: None,
        external_id: job.id,
        remote: is_remote(remote_flag, &location),
        location,
        title: job.title,
        department: job.department,
        url: job.job_url,
        posted_at: job.published_at.map(|date| date.with_timezone(&Utc)),
        description: trim_description(job.description_plain),
        notes: join_notes([job.employment_type.as_deref().map(employment_label).as_deref()]),
    }
}

/// Ashby sends "FullTime", "PartTime", "Intern"; show them as words.
fn employment_label(kind: &str) -> String {
    let mut label = String::with_capacity(kind.len() + 2);
    for (index, c) in kind.chars().enumerate() {
        if index > 0 && c.is_uppercase() {
            label.push('-');
        }
        label.push(if index > 0 { c.to_ascii_lowercase() } else { c });
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_board_fixture_and_skips_unlisted_roles() {
        let jobs = parse(include_str!("../../fixtures/ashby.json")).unwrap();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.title, "AI Engineer, New Grad");
        assert_eq!(job.location, "London | Remote - Europe");
        assert!(job.remote);
        assert_eq!(job.url, "https://jobs.ashbyhq.com/acme/240d459b");
        assert_eq!(job.notes.as_deref(), Some("Full-time"));
    }

    #[test]
    fn employment_types_read_as_words() {
        assert_eq!(employment_label("FullTime"), "Full-time");
        assert_eq!(employment_label("PartTime"), "Part-time");
        assert_eq!(employment_label("Intern"), "Intern");
    }
}
