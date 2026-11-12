use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use futures::{StreamExt, stream};
use scout_config::{Env, Profile, Sources};
use scout_db::{Board, BoardSave, Closing, NewBoard, NewJob, Pool};
use scout_jobs::{Ats, BOARD_HOSTS, Posting, RoleFilter};
use scout_llm::{JobInput, score_job};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::matching::{MAX_REASON_WORDS, limit_words, profile_input};

const CONCURRENCY: usize = 4;
const MAX_REPORTED_ERRORS: usize = 10;
const TOP_SHOWN: usize = 15;
// Feeds only show recent results; a role unseen this long is treated as closed.
const FEED_ROLE_DAYS: i32 = 14;

#[derive(Default)]
struct FetchStats {
    boards: usize,
    errors: Vec<String>,
    matching: usize,
    new: u64,
    closed: u64,
}

impl FetchStats {
    fn merge(&mut self, other: Self) {
        self.boards += other.boards;
        self.errors.extend(other.errors);
        self.matching += other.matching;
        self.new += other.new;
        self.closed += other.closed;
    }
}

pub async fn run(
    env: &Env,
    profile: &Profile,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
) -> Result<Value> {
    if profile.job_titles.include.is_empty() {
        bail!("profile.yaml has no job_titles.include, so no role would match");
    }
    let manual = add_manual_boards(pool).await?;
    let from_yc = scout_db::add_yc_boards(pool, profile.must_be_ai).await?;
    let from_websites = discover(http, pool, profile, sources).await?;
    let (board_searches, from_search) = search_boards(env, http, pool, sources).await?;

    let filter = RoleFilter::new(
        &profile.job_titles.include,
        &profile.job_titles.exclude,
        &profile.onsite_regions,
    );
    let fetched = fetch_all(http, pool, sources, &filter).await?;
    let (scored, score_errors) = score_jobs(env, profile, sources, pool, http).await?;
    print_top(&scout_db::job_listing(pool).await?);

    let error_count = fetched.errors.len();
    Ok(json!({
        "boards_added": { "manual": manual, "yc": from_yc, "websites": from_websites, "search": from_search },
        "board_searches": board_searches,
        "boards_read": fetched.boards,
        "boards_failed": error_count,
        "matching_roles": fetched.matching,
        "new_roles": fetched.new,
        "closed_roles": fetched.closed,
        "scored": scored,
        "errors": fetched.errors.into_iter().chain(score_errors).take(MAX_REPORTED_ERRORS).collect::<Vec<_>>(),
    }))
}

async fn add_manual_boards(pool: &Pool) -> Result<usize> {
    let boards = scout_config::load_job_boards()?;
    for board in &boards {
        let ats: Ats = board.ats.parse().with_context(|| format!("job_boards.yaml: {}", board.name))?;
        let new = NewBoard {
            company_id: None,
            name: &board.name,
            ats: ats.as_str(),
            slug: &board.slug,
            source: "manual",
        };
        scout_db::upsert_board(pool, &new).await?;
    }
    Ok(boards.len())
}

/// Reads company websites for links to a careers board. Returns how many boards were found.
async fn discover(
    http: &reqwest::Client,
    pool: &Pool,
    profile: &Profile,
    sources: &Sources,
) -> Result<usize> {
    let queue = scout_db::careers_to_check(
        pool,
        profile.must_be_ai,
        sources.career_recheck_days,
        sources.max_career_checks_per_run,
    )
    .await?;
    let found: Vec<_> = stream::iter(queue)
        .map(|company| async move {
            let boards = scout_jobs::discover_boards(http, &company.domain).await;
            (company, boards)
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    let mut added = 0;
    for (company, boards) in found {
        for (ats, slug) in &boards {
            let board = NewBoard {
                company_id: Some(company.id),
                name: &company.name,
                ats: ats.as_str(),
                slug,
                source: "website",
            };
            scout_db::upsert_board(pool, &board).await?;
        }
        added += boards.len();
        scout_db::mark_careers_checked(pool, company.id).await?;
    }
    Ok(added)
}

/// Searches the hosts of careers boards for new ones, once every `board_search_days`.
/// Returns how many searches ran and how many new boards they found.
async fn search_boards(
    env: &Env,
    http: &reqwest::Client,
    pool: &Pool,
    sources: &Sources,
) -> Result<(usize, usize)> {
    let Some(api_key) = env.tavily_api_key.as_deref() else {
        return Ok((0, 0));
    };
    if !scout_db::board_search_due(pool, sources.board_search_days).await? {
        return Ok((0, 0));
    }
    let (mut searches, mut added) = (0, 0);
    for term in &sources.board_search_terms {
        let urls = scout_enrich::search_urls(http, api_key, term, BOARD_HOSTS).await?;
        searches += 1;
        for (ats, slug) in urls.iter().flat_map(|url| scout_jobs::find_boards(url)) {
            let name = board_name(&slug);
            let board = NewBoard { company_id: None, name: &name, ats: ats.as_str(), slug: &slug, source: "search" };
            added += usize::from(scout_db::upsert_board(pool, &board).await?);
        }
    }
    Ok((searches, added))
}

/// A readable name from a board slug, such as "acme-labs" to "Acme Labs".
pub(crate) fn board_name(slug: &str) -> String {
    slug.split(['-', '_', '.'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Providers are read in parallel; boards of one provider one at a time, at its pace.
async fn fetch_all(
    http: &reqwest::Client,
    pool: &Pool,
    sources: &Sources,
    filter: &RoleFilter,
) -> Result<FetchStats> {
    let now = Utc::now();
    let mut groups: BTreeMap<Ats, Vec<Board>> = BTreeMap::new();
    for board in scout_db::boards_due(pool, sources.yc_recheck_days).await? {
        groups.entry(board.ats.parse()?).or_default().push(board);
    }
    let results = join_all(
        groups
            .into_iter()
            .map(|(ats, boards)| fetch_provider(http, pool, filter, ats, boards, now)),
    )
    .await;

    let mut stats = FetchStats::default();
    for result in results {
        stats.merge(result?);
    }
    Ok(stats)
}

async fn fetch_provider(
    http: &reqwest::Client,
    pool: &Pool,
    filter: &RoleFilter,
    ats: Ats,
    boards: Vec<Board>,
    now: DateTime<Utc>,
) -> Result<FetchStats> {
    let mut stats = FetchStats::default();
    for board in boards {
        sleep(ats.request_gap()).await;
        stats.boards += 1;
        match fetch_board(http, pool, filter, ats, &board, now).await {
            Ok((matching, saved)) => {
                stats.matching += matching;
                stats.new += saved.new;
                stats.closed += saved.closed;
            }
            // A board that fails keeps its stored roles; the next run tries again.
            Err(error) => {
                let message = format!("{} ({ats}/{}): {error:#}", board.name, board.slug);
                scout_db::mark_board_failed(pool, board.id, &message).await?;
                stats.errors.push(message);
            }
        }
    }
    Ok(stats)
}

async fn fetch_board(
    http: &reqwest::Client,
    pool: &Pool,
    filter: &RoleFilter,
    ats: Ats,
    board: &Board,
    now: DateTime<Utc>,
) -> Result<(usize, BoardSave)> {
    let mut postings: Vec<Posting> = scout_jobs::fetch_board(http, ats, &board.slug, now)
        .await?
        .into_iter()
        .filter(|posting| filter.keeps(posting))
        .collect();
    if ats.lists_without_descriptions() {
        let described = scout_db::described_jobs(pool, board.id).await?;
        for posting in postings.iter_mut().filter(|posting| !described.contains(&posting.external_id)) {
            sleep(ats.request_gap()).await;
            // A role without a description is still worth listing.
            posting.description =
                scout_jobs::fetch_description(http, ats, &board.slug, &posting.external_id)
                    .await
                    .ok()
                    .flatten();
        }
    }
    let jobs: Vec<NewJob<'_>> = postings.iter().map(new_job).collect();
    let closing = if ats.lists_every_role() {
        Closing::WhenMissing
    } else {
        Closing::AfterDaysUnseen(FEED_ROLE_DAYS)
    };
    let saved = scout_db::save_board_jobs(pool, board.id, &jobs, closing).await?;
    Ok((postings.len(), saved))
}

fn new_job(posting: &Posting) -> NewJob<'_> {
    NewJob {
        external_id: &posting.external_id,
        title: &posting.title,
        company: posting.company.as_deref(),
        location: &posting.location,
        remote: posting.remote,
        department: posting.department.as_deref(),
        url: &posting.url,
        posted_at: posting.posted_at,
        description: posting.description.as_deref(),
        notes: posting.notes.as_deref(),
    }
}

async fn score_jobs(
    env: &Env,
    profile: &Profile,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
) -> Result<(usize, Vec<String>)> {
    let queue = scout_db::jobs_to_score(pool, sources.max_job_scores_per_run).await?;
    if queue.is_empty() {
        return Ok((0, Vec::new()));
    }
    let llm = crate::llm_client(env, http)?;
    let candidate = profile_input(profile);
    let results: Vec<_> = stream::iter(queue)
        .map(|job| {
            let (llm, candidate) = (&llm, &candidate);
            async move {
                let input = JobInput {
                    title: &job.title,
                    company: &job.company,
                    company_about: job.company_about.as_deref(),
                    location: &job.location,
                    remote: job.remote,
                    department: job.department.as_deref(),
                    notes: job.notes.as_deref(),
                    description: job.description.as_deref(),
                };
                (job.id, score_job(llm, candidate, &input).await)
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    let (mut scored, mut errors) = (0, Vec::new());
    for (id, result) in results {
        match result {
            Ok(output) => {
                let reason = limit_words(&output.reason, MAX_REASON_WORDS);
                scout_db::save_job_fit(pool, id, output.fit.clamp(0, 100), &reason, output.startup).await?;
                scored += 1;
            }
            Err(error) => errors.push(format!("job {id}: {error:#}")),
        }
    }
    Ok((scored, errors))
}

fn print_top(listing: &Value) {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_owned();
    for (rank, row) in listing.as_array().into_iter().flatten().take(TOP_SHOWN).enumerate() {
        println!(
            "{:>2}. [{}] {} at {} ({}{})\n    {}\n    {}",
            rank + 1,
            row["fit"].as_i64().map_or("-".to_owned(), |fit| fit.to_string()),
            text(row, "title"),
            text(row, "company"),
            text(row, "location"),
            if row["remote"].as_bool() == Some(true) { ", remote" } else { "" },
            text(row, "fit_reason"),
            text(row, "url"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_names_read_as_words() {
        assert_eq!(board_name("acme-labs"), "Acme Labs");
        assert_eq!(board_name("openai"), "Openai");
        assert_eq!(board_name("cal.com"), "Cal Com");
    }
}
