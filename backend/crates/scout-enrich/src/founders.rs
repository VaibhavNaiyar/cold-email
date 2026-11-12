use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::emails::Person;

const TECHNICAL_WORDS: &[&str] = &["cto", "technical", "technology", "engineer", "developer"];

#[derive(Deserialize)]
struct YcPage {
    props: YcProps,
}

#[derive(Deserialize)]
struct YcProps {
    company: YcCompany,
}

#[derive(Deserialize)]
struct YcCompany {
    founders: Vec<YcFounder>,
}

#[derive(Deserialize)]
struct YcFounder {
    full_name: String,
    title: Option<String>,
}

pub fn person(name: &str, role: Option<&str>) -> Person {
    let technical = role.is_some_and(|role| {
        let role = role.to_lowercase();
        TECHNICAL_WORDS.iter().any(|word| role.contains(word))
    });
    Person { name: name.trim().to_owned(), role: role.map(str::to_owned), technical }
}

pub async fn yc_founders(http: &reqwest::Client, yc_url: &str) -> Result<Vec<Person>> {
    let html = http
        .get(yc_url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("Could not fetch {yc_url}"))?
        .text()
        .await?;
    parse_yc_page(&html)
}

/// The JSON payload YC pages embed in their `data-page` attribute.
pub fn yc_data_page<T: DeserializeOwned>(html: &str) -> Result<T> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("[data-page]").expect("selector is valid");
    let payload = document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr("data-page"))
        .context("YC page has no data-page")?;
    serde_json::from_str(payload).context("Unexpected YC page shape")
}

fn parse_yc_page(html: &str) -> Result<Vec<Person>> {
    let page: YcPage = yc_data_page(html)?;
    Ok(page
        .props
        .company
        .founders
        .iter()
        .map(|founder| person(&founder.full_name, founder.title.as_deref()))
        .collect())
}

/// Technical people first, duplicates removed.
pub fn prioritise(mut people: Vec<Person>) -> Vec<Person> {
    people.sort_by_key(|person| !person.technical);
    let mut seen = Vec::new();
    people.retain(|person| {
        let key = person.name.to_lowercase();
        let fresh = !person.name.is_empty() && !seen.contains(&key);
        seen.push(key);
        fresh
    });
    people
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_founders_from_the_yc_page_payload() {
        let html = r#"<div data-page="{&quot;props&quot;:{&quot;company&quot;:{&quot;founders&quot;:[
            {&quot;full_name&quot;:&quot;Ada Stone&quot;,&quot;title&quot;:&quot;Founder/CEO&quot;},
            {&quot;full_name&quot;:&quot;Linus Bo&quot;,&quot;title&quot;:&quot;CTO&quot;}]}}}"></div>"#;
        let people = parse_yc_page(html).unwrap();
        assert_eq!(people[0].name, "Ada Stone");
        assert!(!people[0].technical);
        assert!(people[1].technical);
    }

    #[test]
    fn prioritise_puts_technical_founders_first_and_dedupes() {
        let people = prioritise(vec![
            person("Ada Stone", Some("CEO")),
            person("Linus Bo", Some("Co-founder & CTO")),
            person("ada stone", None),
        ]);
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].name, "Linus Bo");
    }
}
