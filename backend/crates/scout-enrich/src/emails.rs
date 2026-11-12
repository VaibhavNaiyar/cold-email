use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[a-z0-9][a-z0-9._+-]*@[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,24}\b")
        .expect("email regex is valid")
});
static SPELLED_OUT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b([a-z0-9._+-]+)\s*[\[(]\s*at\s*[\])]\s*([a-z0-9-]+)\s*[\[(]\s*dot\s*[\])]\s*([a-z]{2,24})\b",
    )
    .expect("spelled-out regex is valid")
});
static CLOUDFLARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:data-cfemail="|email-protection#)([0-9a-fA-F]+)"#)
        .expect("cloudflare regex is valid")
});

const FILE_ENDINGS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp", "avif", "js", "css"];
const PLACEHOLDER_DOMAINS: &[&str] = &[
    "example.com", "example.org", "domain.com", "email.com", "yourcompany.com", "company.com",
    "sentry.io", "wixpress.com",
];
const PERSONAL_MAIL: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "proton.me", "protonmail.com",
    "icloud.com", "fastmail.com", "hey.com",
];
// Inboxes nobody reads for a job approach.
const UNSUITABLE: &[&str] = &[
    "abuse", "accounts", "admin", "billing", "compliance", "dmca", "dpo", "dsar", "finance", "gdpr",
    "help", "info-privacy", "invoices", "legal", "marketing", "newsletter", "no-reply", "noreply",
    "partners", "partnership", "partnerships", "postmaster", "press", "privacy", "refunds",
    "report", "reports", "sales", "security", "support", "unsubscribe", "webmaster",
];
// Longer than this with no separator is page text run together, not a mailbox.
const MAX_UNBROKEN_LOCAL: usize = 20;
const FOUNDER_INBOXES: &[&str] = &["ceo", "cto", "founder", "founders", "cofounders"];
const GENERAL_INBOXES: &[&str] = &[
    "careers", "contact", "hello", "hey", "hi", "hiring", "info", "jobs", "join", "mail", "office",
    "team", "talent", "work",
];

/// All addresses published in HTML or text, in any of the usual disguises.
pub fn extract_emails(content: &str) -> BTreeSet<String> {
    let content = content
        .replace("&#x40;", "@")
        .replace("&#64;", "@")
        .replace("&#x2F;", "/")
        .replace("%40", "@");
    let mut found: BTreeSet<String> = EMAIL
        .find_iter(&content)
        .map(|hit| hit.as_str().to_lowercase())
        .collect();
    found.extend(SPELLED_OUT.captures_iter(&content).map(|caps| {
        format!("{}@{}.{}", &caps[1], &caps[2], &caps[3]).to_lowercase()
    }));
    found.extend(CLOUDFLARE.captures_iter(&content).filter_map(|caps| decode_cloudflare(&caps[1])));
    found.retain(|email| is_plausible(email));
    found
}

fn decode_cloudflare(hex: &str) -> Option<String> {
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect::<Option<_>>()?;
    let (key, rest) = bytes.split_first()?;
    let decoded: Vec<u8> = rest.iter().map(|byte| byte ^ key).collect();
    let email = String::from_utf8(decoded).ok()?.to_lowercase();
    EMAIL.is_match(&email).then_some(email)
}

fn is_plausible(email: &str) -> bool {
    let Some((local, domain)) = email.rsplit_once('@') else {
        return false;
    };
    let ending = domain.rsplit('.').next().unwrap_or_default();
    let run_together =
        local.len() > MAX_UNBROKEN_LOCAL && !local.contains(['.', '_', '-', '+']);
    !run_together && !FILE_ENDINGS.contains(&ending) && !PLACEHOLDER_DOMAINS.contains(&domain)
}

#[derive(Debug, Clone)]
pub struct Person {
    pub name: String,
    pub role: Option<String>,
    pub technical: bool,
}

/// How good an address is. Lower variants are tried first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    TechnicalFounder,
    Founder,
    FounderInbox,
    NamedPerson,
    GeneralInbox,
}

impl Rank {
    pub fn is_generic(self) -> bool {
        matches!(self, Self::FounderInbox | Self::GeneralInbox)
    }

    pub fn is_founder(self) -> bool {
        matches!(self, Self::TechnicalFounder | Self::Founder)
    }
}

#[derive(Debug, Clone)]
pub struct Judged {
    pub rank: Rank,
    pub person: Option<Person>,
}

/// Decides whether an address may be used for this company, and how good it is.
pub fn judge(email: &str, company_domain: &str, founders: &[Person]) -> Option<Judged> {
    let (local, domain) = email.rsplit_once('@')?;
    let founder = founders.iter().find(|person| name_matches(local, &person.name));
    let on_company_domain =
        domain == company_domain || domain.ends_with(&format!(".{company_domain}"));

    if !on_company_domain {
        // A private address only counts when it is plainly the founder's own.
        let founder = founder.filter(|_| PERSONAL_MAIL.contains(&domain))?;
        return Some(founder_result(founder));
    }
    if let Some(founder) = founder {
        return Some(founder_result(founder));
    }
    let base = local.split('+').next().unwrap_or(local);
    if UNSUITABLE.contains(&base) {
        return None;
    }
    let rank = if FOUNDER_INBOXES.contains(&base) {
        Rank::FounderInbox
    } else if GENERAL_INBOXES.contains(&base) {
        Rank::GeneralInbox
    } else {
        Rank::NamedPerson
    };
    Some(Judged { rank, person: None })
}

fn founder_result(founder: &Person) -> Judged {
    let rank = if founder.technical { Rank::TechnicalFounder } else { Rank::Founder };
    Judged { rank, person: Some(founder.clone()) }
}

fn name_matches(local: &str, full_name: &str) -> bool {
    let parts: Vec<String> = full_name
        .split_whitespace()
        .map(|part| part.to_lowercase().replace(|c: char| !c.is_alphanumeric(), ""))
        .filter(|part| !part.is_empty())
        .collect();
    let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
        return false;
    };
    let squeezed = local.replace(['.', '_', '-'], "");
    let initial = first.chars().next().map(String::from).unwrap_or_default();
    [
        first.clone(),
        last.clone(),
        format!("{first}{last}"),
        format!("{last}{first}"),
        format!("{initial}{last}"),
    ]
    .contains(&squeezed)
        && first.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<Person> {
        vec![
            Person { name: "Ada Stone".into(), role: Some("CEO".into()), technical: false },
            Person { name: "Linus Bo".into(), role: Some("CTO".into()), technical: true },
        ]
    }

    fn rank(email: &str) -> Option<Rank> {
        judge(email, "acme.ai", &people()).map(|judged| judged.rank)
    }

    #[test]
    fn finds_plain_mailto_and_spelled_out_addresses() {
        let found = extract_emails(
            r#"<a href="mailto:Ada@Acme.ai?subject=x">Mail</a> jobs [at] acme [dot] ai, x&#x40;acme.ai"#,
        );
        assert!(found.contains("ada@acme.ai"));
        assert!(found.contains("jobs@acme.ai"));
        assert!(found.contains("x@acme.ai"));
    }

    #[test]
    fn decodes_cloudflare_protected_addresses() {
        let key = 0x42u8;
        let hex: String = std::iter::once(key)
            .chain(b"ada@acme.ai".iter().map(|byte| byte ^ key))
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let found = extract_emails(&format!(r#"<span data-cfemail="{hex}">[protected]</span>"#));
        assert!(found.contains("ada@acme.ai"));
    }

    #[test]
    fn ignores_assets_and_placeholders() {
        let found = extract_emails("logo@2x.png you@example.com a@sentry.io");
        assert!(found.is_empty());
    }

    #[test]
    fn ignores_page_text_run_together() {
        assert!(extract_emails("aboutcontactgithubcontact@caveman.so").is_empty());
        assert!(!extract_emails("firstname.lastname.longer@acme.ai").is_empty());
    }

    #[test]
    fn ranks_founders_above_inboxes() {
        assert_eq!(rank("linus@acme.ai"), Some(Rank::TechnicalFounder));
        assert_eq!(rank("a.stone@acme.ai"), Some(Rank::Founder));
        assert_eq!(rank("astone@acme.ai"), Some(Rank::Founder));
        assert_eq!(rank("founders@acme.ai"), Some(Rank::FounderInbox));
        assert_eq!(rank("priya@acme.ai"), Some(Rank::NamedPerson));
        assert_eq!(rank("hello@acme.ai"), Some(Rank::GeneralInbox));
        assert!(Rank::TechnicalFounder < Rank::GeneralInbox);
    }

    #[test]
    fn refuses_unsuitable_and_unrelated_addresses() {
        assert_eq!(rank("privacy@acme.ai"), None);
        assert_eq!(rank("support@acme.ai"), None);
        assert_eq!(rank("report@acme.ai"), None);
        assert_eq!(rank("partnership@acme.ai"), None);
        assert_eq!(rank("someone@other.ai"), None);
        assert_eq!(rank("priya@gmail.com"), None);
    }

    #[test]
    fn accepts_a_founders_private_address_and_subdomains() {
        assert_eq!(rank("ada.stone@gmail.com"), Some(Rank::Founder));
        assert_eq!(rank("hello@mail.acme.ai"), Some(Rank::GeneralInbox));
    }
}
