use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use tokio::time::sleep;

mod ashby;
mod greenhouse;
mod hn;
mod himalayas;
mod lever;
mod remoteok;
mod wwr;
mod yc;

const TIMEOUT: Duration = Duration::from_secs(30);
const DESCRIPTION_CHARS: usize = 3000;
// A busy server (429 or 503) gets up to this many tries, waiting as long as it asks.
const MAX_ATTEMPTS: u32 = 3;
const MAX_WAIT: Duration = Duration::from_secs(60);

/// A careers board of one company, or a feed listing many.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ats {
    Greenhouse,
    Lever,
    Ashby,
    Yc,
    RemoteOk,
    Himalayas,
    /// The monthly Hacker News "Who is hiring?" thread.
    Hn,
    /// We Work Remotely category feeds.
    Wwr,
}

/// One open role, normalised across providers.
#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub external_id: String,
    pub title: String,
    /// Set by feeds that list many companies; a company's own board leaves it empty.
    pub company: Option<String>,
    pub location: String,
    pub remote: bool,
    pub department: Option<String>,
    /// The role on the board it came from.
    pub url: String,
    pub posted_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
    /// Terms the board states outright, such as experience, visa or contract type.
    pub notes: Option<String>,
}

impl Ats {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever => "lever",
            Self::Ashby => "ashby",
            Self::Yc => "yc",
            Self::RemoteOk => "remoteok",
            Self::Himalayas => "himalayas",
            Self::Hn => "hn",
            Self::Wwr => "wwr",
        }
    }

    /// Pause between requests to one provider. Pages and shared feeds get the slowest pace.
    pub fn request_gap(self) -> Duration {
        match self {
            Self::Greenhouse | Self::Lever | Self::Ashby => Duration::from_millis(250),
            Self::Yc | Self::RemoteOk | Self::Himalayas | Self::Hn | Self::Wwr => Duration::from_secs(1),
        }
    }

    /// Whether a read lists every open role, so a role missing from it has closed.
    /// Feeds show only the latest results, so their roles close by age instead.
    pub fn lists_every_role(self) -> bool {
        !matches!(self, Self::RemoteOk | Self::Himalayas | Self::Wwr)
    }

    /// Whether the list leaves out descriptions, so new roles need one more request each.
    pub fn lists_without_descriptions(self) -> bool {
        self == Self::Greenhouse
    }

}

impl fmt::Display for Ats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Ats {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "greenhouse" => Ok(Self::Greenhouse),
            "lever" => Ok(Self::Lever),
            "ashby" => Ok(Self::Ashby),
            "yc" => Ok(Self::Yc),
            "remoteok" => Ok(Self::RemoteOk),
            "himalayas" => Ok(Self::Himalayas),
            "hn" => Ok(Self::Hn),
            "wwr" => Ok(Self::Wwr),
            other => bail!("Unknown careers board \"{other}\""),
        }
    }
}

/// Every role currently listed on one board.
pub async fn fetch_board(
    http: &reqwest::Client,
    ats: Ats,
    slug: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Posting>> {
    let postings = match ats {
        Ats::Greenhouse => greenhouse::parse(&get_text(http, &greenhouse::list_url(slug)).await?),
        Ats::Lever => lever::parse(&get_text(http, &lever::list_url(slug)).await?),
        Ats::Ashby => ashby::parse(&get_text(http, &ashby::list_url(slug)).await?),
        Ats::Yc => yc::parse(&get_text(http, &yc::list_url(slug)).await?, now),
        Ats::RemoteOk => remoteok::parse(&get_text(http, &remoteok::list_url(slug)).await?),
        Ats::Himalayas => himalayas::fetch(http, slug, ats.request_gap()).await,
        Ats::Hn => hn::fetch(http).await,
        Ats::Wwr => wwr::parse(&get_text(http, &wwr::list_url(slug)).await?),
    };
    postings.with_context(|| format!("Unexpected {ats} board for {slug}"))
}

/// The description of one role, for providers whose list leaves it out.
pub async fn fetch_description(
    http: &reqwest::Client,
    ats: Ats,
    slug: &str,
    external_id: &str,
) -> Result<Option<String>> {
    match ats {
        Ats::Greenhouse => {
            let body = get_text(http, &greenhouse::detail_url(slug, external_id)).await?;
            greenhouse::parse_description(&body)
        }
        _ => Ok(None),
    }
}

async fn get_text(http: &reqwest::Client, url: &str) -> Result<String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let response = http
            .get(url)
            .timeout(TIMEOUT)
            .send()
            .await
            .with_context(|| format!("Could not fetch {url}"))?;
        let busy = matches!(response.status(), StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE);
        if busy && attempt < MAX_ATTEMPTS {
            let asked = response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value| value.to_str().ok());
            sleep(retry_wait(asked, attempt)).await;
            continue;
        }
        return response
            .error_for_status()
            .with_context(|| format!("Could not fetch {url}"))?
            .text()
            .await
            .with_context(|| format!("Could not read {url}"));
    }
}

/// The server's own Retry-After in seconds, otherwise 2, 4, 8... seconds; never more than a minute.
fn retry_wait(retry_after: Option<&str>, attempt: u32) -> Duration {
    retry_after
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or_else(|| Duration::from_secs(2u64.pow(attempt)), Duration::from_secs)
        .min(MAX_WAIT)
}

/// A board's own flag, or the word "remote" in the location.
fn is_remote(flag: bool, location: &str) -> bool {
    flag || location.to_lowercase().contains("remote")
}

fn trim_description(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().chars().take(DESCRIPTION_CHARS).collect::<String>())
        .filter(|text| !text.is_empty())
}

/// Non-empty parts joined into one line, or nothing.
fn join_notes<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    let parts: Vec<&str> = parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for ats in [Ats::Greenhouse, Ats::Lever, Ats::Ashby, Ats::Yc, Ats::RemoteOk, Ats::Himalayas, Ats::Hn, Ats::Wwr] {
            assert_eq!(ats.as_str().parse::<Ats>().unwrap(), ats);
        }
        assert!("workday".parse::<Ats>().is_err());
    }

    #[test]
    fn busy_servers_are_waited_on_politely() {
        assert_eq!(retry_wait(Some("5"), 1), Duration::from_secs(5));
        assert_eq!(retry_wait(Some("3600"), 1), MAX_WAIT);
        assert_eq!(retry_wait(None, 1), Duration::from_secs(2));
        assert_eq!(retry_wait(Some("soon"), 2), Duration::from_secs(4));
    }

    #[test]
    fn remote_comes_from_the_flag_or_the_location() {
        assert!(is_remote(true, "Berlin"));
        assert!(is_remote(false, "Remote (India)"));
        assert!(!is_remote(false, "San Francisco"));
    }

    #[test]
    fn notes_skip_empty_parts() {
        assert_eq!(join_notes([Some("Intern"), None, Some(" "), Some("Visa: none")]).unwrap(), "Intern; Visa: none");
        assert_eq!(join_notes([None, Some("")]), None);
    }

    #[test]
    fn descriptions_are_capped() {
        let long = "x".repeat(DESCRIPTION_CHARS + 10);
        assert_eq!(trim_description(Some(long)).unwrap().len(), DESCRIPTION_CHARS);
        assert_eq!(trim_description(Some("  ".into())), None);
    }
}
