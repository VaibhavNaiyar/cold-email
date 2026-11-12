use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const MAX_EMAILS_PER_DAY: u32 = 10;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    /// Who I am and what I have shipped, in my own words. The scorer reads it.
    #[serde(default)]
    pub summary: String,
    pub roles: Vec<String>,
    pub stack: Vec<String>,
    pub locations: Vec<String>,
    /// Countries where an office job works; elsewhere a company must allow remote work.
    #[serde(default)]
    pub onsite_regions: Vec<String>,
    pub stage: Vec<String>,
    pub must_be_ai: bool,
    pub exclude_domains: Vec<String>,
    pub max_emails_per_day: u32,
    pub proof_points: Vec<String>,
    /// Words that make a job title worth a look, and words that rule it out.
    #[serde(default)]
    pub job_titles: JobTitles,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTitles {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// A careers board to read even though no ingested company links to it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualBoard {
    pub name: String,
    pub ats: String,
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub feeds: Vec<String>,
    pub yc_batches: usize,
    pub formd_days: u32,
    pub max_domain_searches_per_day: i64,
    pub max_domain_attempts: i32,
    pub max_contact_searches_per_run: i64,
    pub max_contact_companies_per_run: i64,
    pub min_contact_score: i32,
    pub max_career_checks_per_run: i64,
    pub career_recheck_days: i32,
    pub yc_recheck_days: i32,
    pub max_job_scores_per_run: i64,
    /// Searches of careers-board hosts for new boards, one per term.
    pub board_search_terms: Vec<String>,
    pub board_search_days: i32,
}

impl Profile {
    pub fn load() -> Result<Self> {
        Self::parse(&read_root_file("profile.yaml")?)
    }

    pub fn parse(yaml: &str) -> Result<Self> {
        let profile: Self = serde_yaml_ng::from_str(yaml).context("Invalid profile.yaml")?;
        if !(1..=MAX_EMAILS_PER_DAY).contains(&profile.max_emails_per_day) {
            bail!("max_emails_per_day must be between 1 and {MAX_EMAILS_PER_DAY}");
        }
        if profile.proof_points.is_empty() {
            bail!("proof_points must not be empty");
        }
        Ok(profile)
    }
}

impl Sources {
    pub fn load() -> Result<Self> {
        let yaml = read_root_file("sources.yaml")?;
        serde_yaml_ng::from_str(&yaml).context("Invalid sources.yaml")
    }
}

/// Extra boards from `job_boards.yaml`.
pub fn load_job_boards() -> Result<Vec<ManualBoard>> {
    let yaml = read_root_file("job_boards.yaml")?;
    serde_yaml_ng::from_str(&yaml).context("Invalid job_boards.yaml")
}

/// Walks up from the working directory, so `cargo test` works inside a crate.
pub fn repo_root() -> Result<PathBuf> {
    let start = env::current_dir()?;
    start
        .ancestors()
        .find(|dir| is_root(dir))
        .map(Path::to_path_buf)
        .with_context(|| format!("No profile.yaml above {}", start.display()))
}

fn is_root(dir: &Path) -> bool {
    dir.join("profile.yaml").exists()
}

fn read_root_file(name: &str) -> Result<String> {
    let path = repo_root()?.join(name);
    fs::read_to_string(&path).with_context(|| format!("Could not read {}", path.display()))
}

/// A file from `templates/`.
pub fn read_template(name: &str) -> Result<String> {
    read_root_file(&format!("templates/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_files_are_valid() {
        assert!(Profile::load().unwrap().max_emails_per_day <= MAX_EMAILS_PER_DAY);
        assert!(!Sources::load().unwrap().feeds.is_empty());
        assert!(!Profile::load().unwrap().job_titles.include.is_empty());
        load_job_boards().unwrap();
        assert!(read_template("outreach.md").unwrap().starts_with("Subject:"));
    }

    #[test]
    fn rejects_a_daily_cap_above_the_limit() {
        let yaml = "
name: A
roles: [a]
stack: [a]
locations: [a]
stage: [seed]
must_be_ai: true
exclude_domains: []
max_emails_per_day: 50
proof_points: [a]
";
        assert!(Profile::parse(yaml).is_err());
    }

    #[test]
    fn rejects_unknown_profile_keys() {
        assert!(Profile::parse("nmae: typo").is_err());
    }
}
