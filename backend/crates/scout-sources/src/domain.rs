use url::Url;

// Sites that show up for a company without being the company.
const NOT_A_COMPANY_SITE: &[&str] = &[
    "angel.co", "apple.com", "bit.ly", "bloomberg.com", "businessinsider.com", "crunchbase.com",
    "dealroom.co", "eu-startups.com", "facebook.com", "forbes.com", "github.com", "github.io",
    "google.com", "inc42.com", "instagram.com", "linkedin.com", "medium.com", "notion.site",
    "pitchbook.com", "producthunt.com", "reuters.com", "sifted.eu", "substack.com",
    "tavily.com", "techcrunch.com", "tracxn.com", "twitter.com", "vercel.app", "wellfound.com", "wikipedia.org",
    "x.com", "ycombinator.com", "yourstory.com", "youtube.com",
];

/// Reduces a URL or bare host to its registrable domain: `https://www.acme.co.uk/blog` → `acme.co.uk`.
pub fn registrable_domain(input: &str) -> Option<String> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    psl::domain_str(&host).map(str::to_owned)
}

/// Like [`registrable_domain`], but refuses news sites, directories and shared hosts.
pub fn company_domain(input: &str) -> Option<String> {
    registrable_domain(input).filter(|domain| !NOT_A_COMPANY_SITE.contains(&domain.as_str()))
}

/// Comparison key for names: `Acme, Inc.` and `ACME` match.
pub fn name_key(name: &str) -> String {
    const SUFFIXES: &[&str] = &["inc", "llc", "ltd", "limited", "gmbh", "corp", "co", "ai"];
    let cleaned = name.to_lowercase().replace(|c: char| !c.is_alphanumeric(), " ");
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    while words.len() > 1 && words.last().is_some_and(|word| SUFFIXES.contains(word)) {
        words.pop();
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_urls_to_the_registrable_domain() {
        assert_eq!(registrable_domain("https://www.acme.ai/blog").unwrap(), "acme.ai");
        assert_eq!(registrable_domain("http://app.acme.co.uk").unwrap(), "acme.co.uk");
        assert_eq!(registrable_domain("Acme.IO").unwrap(), "acme.io");
    }

    #[test]
    fn rejects_inputs_that_are_not_domains() {
        assert_eq!(registrable_domain("not a url"), None);
        assert_eq!(registrable_domain(""), None);
    }

    #[test]
    fn company_domain_rejects_news_and_shared_hosts() {
        assert_eq!(company_domain("https://techcrunch.com/2026/acme"), None);
        assert_eq!(company_domain("https://www.linkedin.com/company/acme"), None);
        assert_eq!(company_domain("https://acme.ai").unwrap(), "acme.ai");
    }

    #[test]
    fn name_key_ignores_case_and_legal_suffixes() {
        assert_eq!(name_key("Acme, Inc."), name_key("ACME"));
        assert_eq!(name_key("Acme Robotics GmbH"), "acme robotics");
        assert_eq!(name_key("AI"), "ai");
    }
}
