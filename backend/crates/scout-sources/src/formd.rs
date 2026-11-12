use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use reqwest::StatusCode;
use serde_json::json;

use crate::{RawSignal, SignalKind, SourceType};

const SEC_BASE: &str = "https://www.sec.gov/Archives";
// One request every 200 ms stays well under SEC's limit of 10 per second.
const REQUEST_GAP: Duration = Duration::from_millis(200);
const TECH_GROUPS: &[&str] = &["Computers", "Telecommunications", "Other Technology"];
const FUND_WORDS: &[&str] = &[
    " fund", " funds", " l.p.", " lp", " l.p", " trust", " reit", " a series of", " spv",
    " capital partners", " ventures", " partners",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filing {
    pub company: String,
    pub filed: NaiveDate,
    pub primary_doc_url: String,
}

/// New Form D filings from the daily indexes of the last `days` calendar days.
pub async fn list_filings(
    http: &reqwest::Client,
    today: NaiveDate,
    days: u32,
) -> Result<Vec<Filing>> {
    let mut filings = Vec::new();
    for back in 1..=days {
        let date = today - chrono::Days::new(back.into());
        pause().await;
        // SEC publishes no index on weekends and holidays.
        let response = http.get(index_url(date)).send().await?;
        if matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::FORBIDDEN) {
            continue;
        }
        let body = response
            .error_for_status()
            .with_context(|| format!("Could not fetch the EDGAR index for {date}"))?
            .text()
            .await?;
        filings.extend(parse_index(&body));
    }
    Ok(filings)
}

async fn pause() {
    tokio::time::sleep(REQUEST_GAP).await;
}

fn index_url(date: NaiveDate) -> String {
    format!(
        "{SEC_BASE}/edgar/daily-index/{}/QTR{}/form.{}.idx",
        date.year(),
        date.quarter(),
        date.format("%Y%m%d"),
    )
}

/// Index rows are fixed-width text: form, company, CIK, date, path.
fn parse_index(body: &str) -> Vec<Filing> {
    body.lines().filter_map(parse_index_row).collect()
}

fn parse_index_row(line: &str) -> Option<Filing> {
    let rest = line.strip_prefix("D ")?;
    let mut fields = rest.split_whitespace().rev();
    let path = fields.next()?.strip_suffix(".txt")?;
    let filed = NaiveDate::parse_from_str(fields.next()?, "%Y%m%d").ok()?;
    fields.next()?; // CIK
    let company = fields.rev().collect::<Vec<_>>().join(" ");
    let (folder, accession) = path.rsplit_once('/')?;
    Some(Filing {
        company,
        filed,
        primary_doc_url: format!(
            "{SEC_BASE}/{folder}/{}/primary_doc.xml",
            accession.replace('-', "")
        ),
    })
}

/// Spares a request for names that are plainly investment vehicles.
pub fn may_be_operating_company(filing: &Filing) -> bool {
    let name = format!("{} ", filing.company.to_lowercase());
    !FUND_WORDS.iter().any(|word| name.contains(&format!("{word} ")) || name.contains(&format!("{word},")))
}

/// Downloads a filing and returns a signal if it is a young tech company that sold securities.
pub async fn fetch_signal(http: &reqwest::Client, filing: &Filing) -> Result<Option<RawSignal>> {
    pause().await;
    let xml = http
        .get(&filing.primary_doc_url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("Could not fetch {}", filing.primary_doc_url))?
        .text()
        .await?;
    let published = Utc
        .from_utc_datetime(&filing.filed.and_hms_opt(0, 0, 0).expect("midnight is valid"));
    parse_filing(&xml, &filing.primary_doc_url, published)
}

fn parse_filing(xml: &str, url: &str, published_at: DateTime<Utc>) -> Result<Option<RawSignal>> {
    let doc = roxmltree::Document::parse(xml).context("Invalid Form D XML")?;
    let text = |tag: &str| {
        doc.descendants()
            .find(|node| node.has_tag_name(tag))
            .and_then(|node| node.text())
            .map(str::trim)
    };

    let industry = text("industryGroupType").unwrap_or_default();
    let is_amendment = text("isAmendment") == Some("true");
    let is_young = text("withinFiveYears") == Some("true");
    let amount_sold = text("totalAmountSold").and_then(|amount| amount.parse::<i64>().ok());
    if !TECH_GROUPS.contains(&industry) || is_amendment || !is_young {
        return Ok(None);
    }
    let Some(amount) = amount_sold.filter(|amount| *amount > 0) else {
        return Ok(None);
    };
    let name = text("entityName").context("Form D has no entityName")?.to_owned();

    let people: Vec<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("relatedPersonInfo"))
        .map(|person| {
            let part = |tag: &str| {
                person
                    .descendants()
                    .find(|node| node.has_tag_name(tag))
                    .and_then(|node| node.text())
                    .unwrap_or_default()
            };
            let roles: Vec<_> = person
                .descendants()
                .filter(|node| node.has_tag_name("relationship"))
                .filter_map(|node| node.text())
                .collect();
            json!({
                "name": format!("{} {}", part("firstName"), part("lastName")),
                "roles": roles,
            })
        })
        .collect();

    Ok(Some(RawSignal {
        source_type: SourceType::Formd,
        source_url: url.to_owned(),
        title: format!("{name} filed a Form D reporting ${amount} sold"),
        legal_name: Some(name.clone()),
        company_name: name,
        domain: None,
        one_liner: None,
        website: None,
        hiring: None,
        kind: SignalKind::Raised,
        published_at,
        amount_usd: Some(amount),
        raw: json!({
            "industry": industry,
            "state": text("stateOrCountry"),
            "year_of_inc": year_of_inc(&doc),
            "founders": people,
        }),
    }))
}

fn year_of_inc<'a>(doc: &'a roxmltree::Document) -> Option<&'a str> {
    doc.descendants()
        .find(|node| node.has_tag_name("yearOfInc"))?
        .children()
        .find(|node| node.has_tag_name("value"))?
        .text()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = "D                511 Therapeutics, Inc.                                        1946264     20261030    edgar/data/1946264/0001946264-26-000003.txt    ";
    const TECH: &str = include_str!("../fixtures/formd_tech.xml");
    const BIOTECH: &str = include_str!("../fixtures/formd_biotech.xml");

    fn filing(company: &str) -> Filing {
        Filing {
            company: company.into(),
            filed: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            primary_doc_url: String::new(),
        }
    }

    #[test]
    fn parses_an_index_row() {
        let parsed = parse_index_row(ROW).unwrap();
        assert_eq!(parsed.company, "511 Therapeutics, Inc.");
        assert_eq!(parsed.filed, NaiveDate::from_ymd_opt(2026, 10, 30).unwrap());
        assert_eq!(
            parsed.primary_doc_url,
            "https://www.sec.gov/Archives/edgar/data/1946264/000194626426000003/primary_doc.xml"
        );
    }

    #[test]
    fn index_skips_amendments_and_other_forms() {
        assert!(parse_index_row(&ROW.replacen("D  ", "D/A", 1)).is_none());
        assert!(parse_index_row("10-K  Acme  1  20261030  edgar/data/1/1.txt").is_none());
    }

    #[test]
    fn builds_the_index_url_for_the_right_quarter() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        assert!(index_url(date).ends_with("/daily-index/2026/QTR4/form.20261030.idx"));
    }

    #[test]
    fn name_filter_drops_investment_vehicles() {
        assert!(!may_be_operating_company(&filing("25Madison Ventures Fund II, L.P.")));
        assert!(!may_be_operating_company(&filing("NC-2026 a Series of CGF2021 LLC")));
        assert!(!may_be_operating_company(&filing("Ripple Effect Onshore Fund LP")));
        assert!(may_be_operating_company(&filing("Acme Robotics Inc.")));
        assert!(may_be_operating_company(&filing("Fundly Inc.")));
    }

    #[test]
    fn keeps_a_young_tech_company_and_its_people() {
        let signal = parse_filing(TECH, "https://sec.test/a", Utc::now()).unwrap().unwrap();
        assert_eq!(signal.company_name, "Acme Robotics Inc.");
        assert_eq!(signal.amount_usd, Some(2_500_000));
        assert_eq!(signal.domain, None);
        assert_eq!(signal.raw["year_of_inc"], "2024");
        assert_eq!(signal.raw["founders"][0]["name"], "Ada Stone");
        assert_eq!(signal.raw["founders"][0]["roles"][0], "Executive Officer");
    }

    #[test]
    fn drops_other_industries_and_unsold_offerings() {
        assert!(parse_filing(BIOTECH, "u", Utc::now()).unwrap().is_none());
        let unsold = TECH.replace("<totalAmountSold>2500000", "<totalAmountSold>0");
        assert!(parse_filing(&unsold, "u", Utc::now()).unwrap().is_none());
    }

    #[test]
    fn drops_amendments_and_older_companies() {
        let amended = TECH.replace("<isAmendment>false", "<isAmendment>true");
        assert!(parse_filing(&amended, "u", Utc::now()).unwrap().is_none());
        let old = TECH.replace("<withinFiveYears>true", "<withinFiveYears>false");
        assert!(parse_filing(&old, "u", Utc::now()).unwrap().is_none());
    }
}
