use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::Ats;

/// Hosts that serve careers boards, for searching the web for boards directly.
pub const BOARD_HOSTS: &[&str] =
    &["jobs.ashbyhq.com", "jobs.lever.co", "job-boards.greenhouse.io", "boards.greenhouse.io"];

// Only boards a company links to itself count. Guessing slugs would match namesakes.
const PAGES: &[&str] = &["/", "/careers", "/jobs"];
const NOT_SLUGS: &[&str] = &["embed", "js", "api", "v1", "jobs"];

static BOARD_LINKS: LazyLock<Vec<(Ats, Regex)>> = LazyLock::new(|| {
    [
        (Ats::Greenhouse, r"greenhouse\.io/embed/job_board(?:/js)?\?for=([A-Za-z0-9_-]+)"),
        (Ats::Greenhouse, r"(?:boards|job-boards)\.(?:eu\.)?greenhouse\.io/([A-Za-z0-9_-]+)"),
        (Ats::Lever, r"jobs\.(?:eu\.)?lever\.co/([A-Za-z0-9_.-]+)"),
        (Ats::Ashby, r"jobs\.ashbyhq\.com/([A-Za-z0-9_.%-]+)"),
        (Ats::Yc, r"(?:www\.)?ycombinator\.com/companies/([a-z0-9][a-z0-9-]*)"),
    ]
    .into_iter()
    .map(|(ats, pattern)| (ats, Regex::new(pattern).expect("board regex is valid")))
    .collect()
});
static CAREERS_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)href="(/[^"#?]*(?:career|jobs|join-us|work-with-us|hiring)[^"#?]*)""##)
        .expect("careers regex is valid")
});

/// Careers boards linked from a page.
pub fn find_boards(html: &str) -> BTreeSet<(Ats, String)> {
    BOARD_LINKS
        .iter()
        .flat_map(|(ats, regex)| regex.captures_iter(html).map(|caps| (*ats, caps[1].to_owned())))
        .filter(|(_, slug)| !NOT_SLUGS.contains(&slug.to_lowercase().as_str()))
        .collect()
}

/// Boards a company links to from its homepage or careers page, read under its robots.txt.
pub async fn discover_boards(http: &reqwest::Client, domain: &str) -> BTreeSet<(Ats, String)> {
    let pages = scout_enrich::read_pages(http, domain, PAGES).await;
    let mut boards: BTreeSet<_> = pages.iter().flat_map(|page| find_boards(&page.html)).collect();
    if boards.is_empty() {
        // Careers pages often live at a path of their own, such as /company/careers.
        let extra: BTreeSet<String> = pages
            .iter()
            .flat_map(|page| careers_paths(&page.html))
            .filter(|path| !PAGES.contains(&path.as_str()))
            .collect();
        if let Some(path) = extra.first() {
            for page in scout_enrich::read_pages(http, domain, &[path.as_str()]).await {
                boards.extend(find_boards(&page.html));
            }
        }
    }
    boards
}

fn careers_paths(html: &str) -> Vec<String> {
    CAREERS_LINK.captures_iter(html).map(|caps| caps[1].to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_provider_and_ignores_embed_paths() {
        let html = r#"
            <a href="https://jobs.ashbyhq.com/Acme">Jobs</a>
            <script src="https://boards.greenhouse.io/embed/job_board/js?for=acmeai"></script>
            <a href="https://job-boards.greenhouse.io/acme/jobs/1">Role</a>
            <a href="https://jobs.lever.co/acme-labs/123">Role</a>
            <a href="https://www.ycombinator.com/companies/acme-ai/jobs/X1-engineer">YC</a>
            <a href="https://news.ycombinator.com/item?id=1">HN</a>"#;
        let found = find_boards(html);
        let expected = [
            (Ats::Greenhouse, "acme"),
            (Ats::Greenhouse, "acmeai"),
            (Ats::Lever, "acme-labs"),
            (Ats::Ashby, "Acme"),
            (Ats::Yc, "acme-ai"),
        ];
        assert_eq!(found, expected.map(|(ats, slug)| (ats, slug.to_owned())).into());
    }

    #[test]
    fn finds_internal_careers_paths_only() {
        let html = r#"<a href="/company/careers">C</a><a href="https://x.com/jobs">X</a><a href="/blog">B</a>"#;
        assert_eq!(careers_paths(html), ["/company/careers"]);
    }
}
