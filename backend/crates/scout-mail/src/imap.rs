use std::collections::HashSet;

use anyhow::{Context, Result, anyhow};
use async_imap::{Client, Session};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio_native_tls::{TlsConnector, TlsStream, native_tls};

const HOST: &str = "imap.gmail.com";
const PORT: u16 = 993;
const INBOX: &str = "INBOX";
const BOUNCE_SENDER: &str = "mailer-daemon";

/// A logged-in Gmail session. It can read mail and add drafts, nothing else.
pub struct Gmail {
    session: Session<TlsStream<TcpStream>>,
    drafts: String,
    sent: String,
}

impl Gmail {
    pub async fn connect(user: &str, app_password: &str) -> Result<Self> {
        let tcp = TcpStream::connect((HOST, PORT)).await.context("Could not reach Gmail IMAP")?;
        let tls = TlsConnector::from(native_tls::TlsConnector::new()?);
        let stream = tls.connect(HOST, tcp).await.context("TLS to Gmail failed")?;
        let mut client = Client::new(stream);
        client
            .read_response()
            .await
            .context("Gmail sent no greeting")?;
        let mut session = client.login(user, app_password).await.map_err(|(error, _)| {
            anyhow!("Gmail login failed: {error}. Check GMAIL_USER and GMAIL_APP_PASSWORD")
        })?;
        let (drafts, sent) = special_folders(&mut session).await?;
        Ok(Self { session, drafts, sent })
    }

    /// Adds a message to Drafts, unless an earlier run already did.
    pub async fn append_draft(&mut self, raw: &[u8], message_id: &str) -> Result<bool> {
        self.session.select(&self.drafts).await?;
        let query = format!("HEADER Message-ID \"<{message_id}>\"");
        if !self.session.search(&query).await?.is_empty() {
            return Ok(false);
        }
        self.session
            .append(&self.drafts, Some("(\\Draft)"), None, raw)
            .await
            .context("Could not add the draft")?;
        Ok(true)
    }

    pub async fn was_sent(&mut self, email: &str, since: DateTime<Utc>) -> Result<bool> {
        let sent = self.sent.clone();
        self.any(&sent, &format!("TO \"{email}\" SINCE {}", imap_date(since))).await
    }

    pub async fn has_reply(
        &mut self,
        email: &str,
        domain: &str,
        since: DateTime<Utc>,
    ) -> Result<bool> {
        let query = format!("OR FROM \"{email}\" FROM \"@{domain}\" SINCE {}", imap_date(since));
        self.any(INBOX, &query).await
    }

    pub async fn has_bounced(&mut self, email: &str, since: DateTime<Utc>) -> Result<bool> {
        let query = format!("FROM \"{BOUNCE_SENDER}\" TEXT \"{email}\" SINCE {}", imap_date(since));
        self.any(INBOX, &query).await
    }

    pub async fn logout(mut self) -> Result<()> {
        self.session.logout().await?;
        Ok(())
    }

    async fn any(&mut self, folder: &str, query: &str) -> Result<bool> {
        self.session.select(folder).await?;
        let found: HashSet<_> = self.session.search(query).await?;
        Ok(!found.is_empty())
    }
}

// Found by flag, because the folder names change with the account's language.
async fn special_folders(session: &mut Session<TlsStream<TcpStream>>) -> Result<(String, String)> {
    let folders: Vec<_> = session.list(None, Some("*")).await?.try_collect().await?;
    let find = |flag: &str| {
        folders
            .iter()
            .find(|folder| has_flag(folder.attributes(), flag))
            .map(|folder| folder.name().to_owned())
            .with_context(|| format!("Gmail has no \\{flag} folder"))
    };
    Ok((find("Drafts")?, find("Sent")?))
}

fn has_flag<T: std::fmt::Debug>(attributes: &[T], flag: &str) -> bool {
    attributes.iter().any(|attribute| format!("{attribute:?}").contains(flag))
}

fn imap_date(date: DateTime<Utc>) -> String {
    date.format("%d-%b-%Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_use_the_imap_format() {
        let date = DateTime::parse_from_rfc3339("2026-11-02T10:00:00Z").unwrap();
        assert_eq!(imap_date(date.with_timezone(&Utc)), "02-Nov-2026");
    }
}
