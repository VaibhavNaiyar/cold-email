use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use roxmltree::Node;

use super::{DESCRIPTION_CHARS, Posting, join_notes, trim_description};

// We Work Remotely publishes one public RSS feed per category; robots.txt allows reading them.
const FEED_URL: &str = "https://weworkremotely.com/categories/";
const ANYWHERE: &str = "Anywhere in the World";

pub(super) fn list_url(category: &str) -> String {
    format!("{FEED_URL}{category}.rss")
}

pub(super) fn parse(xml: &str) -> Result<Vec<Posting>> {
    let document = roxmltree::Document::parse(xml).context("Feed is not valid XML")?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|item| posting(&item))
        .collect())
}

// Item titles read "Company: Role"; anything else is not a job.
fn posting(item: &Node<'_, '_>) -> Option<Posting> {
    let text = |tag: &str| {
        item.children()
            .find(|child| child.has_tag_name(tag))
            .and_then(|child| child.text())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let (company, title) = text("title")?.split_once(": ")?;
    let link = text("link")?;
    Some(Posting {
        company: Some(company.trim().to_owned()),
        external_id: text("guid").unwrap_or(link).to_owned(),
        title: title.trim().to_owned(),
        location: text("region").unwrap_or(ANYWHERE).to_owned(),
        remote: true,
        department: text("category").map(str::to_owned),
        url: link.to_owned(),
        posted_at: text("pubDate")
            .and_then(|date| DateTime::parse_from_rfc2822(date).ok())
            .map(|date| date.with_timezone(&Utc)),
        description: trim_description(
            text("description").map(|html| scout_enrich::html_text(html, DESCRIPTION_CHARS)),
        ),
        notes: join_notes([text("type")]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_feed_items() {
        let jobs = parse(include_str!("../../fixtures/wwr.rss")).unwrap();
        assert_eq!(jobs.len(), 1, "an item without \"Company: Role\" is skipped");
        let job = &jobs[0];
        assert_eq!(job.company.as_deref(), Some("Acme"));
        assert_eq!(job.title, "Full-Stack Engineer");
        assert_eq!(job.location, ANYWHERE);
        assert!(job.remote);
        assert_eq!(job.url, "https://weworkremotely.com/remote-jobs/acme-full-stack-engineer");
        assert_eq!(job.notes.as_deref(), Some("Full-Time"));
        assert_eq!(job.posted_at.unwrap().to_rfc3339(), "2026-11-03T10:00:00+00:00");
        assert_eq!(job.description.as_deref(), Some("Build the agent console."));
    }
}
