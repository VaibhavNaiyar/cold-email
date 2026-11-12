use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::types::Json;

#[derive(Debug)]
pub struct NewBoard<'a> {
    pub company_id: Option<i64>,
    pub name: &'a str,
    pub ats: &'a str,
    pub slug: &'a str,
    pub source: &'a str,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub ats: String,
    pub slug: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CareersCheck {
    pub id: i64,
    pub name: String,
    pub domain: String,
}

#[derive(Debug)]
pub struct NewJob<'a> {
    pub external_id: &'a str,
    pub title: &'a str,
    /// Set by feeds; a company's own board leaves it to the board's name.
    pub company: Option<&'a str>,
    pub location: &'a str,
    pub remote: bool,
    pub department: Option<&'a str>,
    pub url: &'a str,
    pub posted_at: Option<DateTime<Utc>>,
    pub description: Option<&'a str>,
    pub notes: Option<&'a str>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BoardSave {
    pub new: u64,
    pub closed: u64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct JobToScore {
    pub id: i64,
    pub title: String,
    pub location: String,
    pub remote: bool,
    pub department: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub company: String,
    pub company_about: Option<String>,
}

/// Adds a board, or links an existing one to its company. Returns true when it is new.
pub async fn upsert_board(pool: &PgPool, board: &NewBoard<'_>) -> Result<bool> {
    let inserted = sqlx::query_scalar(
        "INSERT INTO job_boards (company_id, name, ats, slug, source)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (ats, slug)
         DO UPDATE SET company_id = COALESCE(job_boards.company_id, EXCLUDED.company_id)
         RETURNING (xmax = 0)",
    )
    .bind(board.company_id)
    .bind(board.name)
    .bind(board.ats)
    .bind(board.slug)
    .bind(board.source)
    .fetch_one(pool)
    .await?;
    Ok(inserted)
}

/// Whether the weekly search for new boards is due: none has run within `days`.
pub async fn board_search_due(pool: &PgPool, days: i32) -> Result<bool> {
    let due = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM runs
                            WHERE job = 'jobs' AND error IS NULL
                              AND (stats_json->>'board_searches')::int > 0
                              AND started_at > now() - make_interval(days => $1))",
    )
    .bind(days)
    .fetch_one(pool)
    .await?;
    Ok(due)
}

/// Every YC company already in the database gets its YC jobs page as a board.
pub async fn add_yc_boards(pool: &PgPool, must_be_ai: bool) -> Result<u64> {
    let added = sqlx::query(
        "INSERT INTO job_boards (company_id, name, ats, slug, source)
         SELECT DISTINCT ON (slug) company_id, name, 'yc', slug, 'yc'
         FROM (SELECT c.id AS company_id, c.name,
                      substring(s.source_url FROM 'ycombinator\\.com/companies/([^/?#]+)') AS slug
               FROM signals s JOIN companies c ON c.id = s.company_id
               WHERE s.source_type = 'yc' AND (c.is_ai OR NOT $1)) yc
         WHERE slug IS NOT NULL
         ON CONFLICT (ats, slug) DO NOTHING",
    )
    .bind(must_be_ai)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(added)
}

/// Companies whose website has not been read for a careers board lately, best first.
pub async fn careers_to_check(
    pool: &PgPool,
    must_be_ai: bool,
    recheck_days: i32,
    limit: i64,
) -> Result<Vec<CareersCheck>> {
    let rows = sqlx::query_as(
        "SELECT c.id, c.name, c.domain FROM companies c
         WHERE c.domain IS NOT NULL AND (c.is_ai OR NOT $1)
           AND (c.careers_checked_at IS NULL
                OR c.careers_checked_at < now() - make_interval(days => $2))
         ORDER BY c.score DESC NULLS LAST, c.id
         LIMIT $3",
    )
    .bind(must_be_ai)
    .bind(recheck_days)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_careers_checked(pool: &PgPool, company_id: i64) -> Result<()> {
    sqlx::query("UPDATE companies SET careers_checked_at = now() WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn boards(pool: &PgPool) -> Result<Vec<Board>> {
    let rows = sqlx::query_as("SELECT id, name, ats, slug FROM job_boards ORDER BY ats, id")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// Boards worth reading this run. A YC page is slow to read and usually empty, so it is read
/// only when YC marks the company as hiring, when it has open roles, or once a week.
/// A board found by search that failed before it ever worked was a dead link and is skipped.
pub async fn boards_due(pool: &PgPool, yc_recheck_days: i32) -> Result<Vec<Board>> {
    let rows = sqlx::query_as(
        "SELECT b.id, b.name, b.ats, b.slug FROM job_boards b
         LEFT JOIN companies c ON c.id = b.company_id
         WHERE (b.ats <> 'yc'
                OR b.last_fetched_at IS NULL
                OR b.last_fetched_at < now() - make_interval(days => $1)
                OR c.hiring IS TRUE
                OR EXISTS (SELECT 1 FROM jobs j WHERE j.board_id = b.id AND j.closed_at IS NULL))
           AND NOT (b.source = 'search' AND b.last_fetched_at IS NULL AND b.last_error IS NOT NULL)
         ORDER BY b.ats, b.id",
    )
    .bind(yc_recheck_days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Roles of a board already stored with a description, so it is not fetched again.
pub async fn described_jobs(pool: &PgPool, board_id: i64) -> Result<HashSet<String>> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT external_id FROM jobs WHERE board_id = $1 AND description IS NOT NULL",
    )
    .bind(board_id)
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().collect())
}

/// How a board's roles close once they stop showing up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    /// The board lists every open role: one it no longer lists has closed.
    WhenMissing,
    /// The board shows only recent results: a role closes after this many days unseen.
    AfterDaysUnseen(i32),
}

/// Stores a board's current roles in one transaction, then closes roles by `closing`.
pub async fn save_board_jobs(
    pool: &PgPool,
    board_id: i64,
    jobs: &[NewJob<'_>],
    closing: Closing,
) -> Result<BoardSave> {
    let mut tx = pool.begin().await?;
    // Never read before: everything listed now was there before Scout looked.
    let first_read: bool =
        sqlx::query_scalar("SELECT last_fetched_at IS NULL FROM job_boards WHERE id = $1")
            .bind(board_id)
            .fetch_one(&mut *tx)
            .await?;
    let mut new = 0;
    for job in jobs {
        let inserted: bool = sqlx::query_scalar(
            "INSERT INTO jobs (board_id, external_id, title, location, remote, department, url,
                               posted_at, description, notes, baseline, company)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (board_id, external_id) DO UPDATE SET
                 title = EXCLUDED.title, company = EXCLUDED.company,
                 location = EXCLUDED.location, remote = EXCLUDED.remote,
                 department = EXCLUDED.department, url = EXCLUDED.url,
                 posted_at = COALESCE(jobs.posted_at, EXCLUDED.posted_at),
                 description = COALESCE(EXCLUDED.description, jobs.description),
                 notes = EXCLUDED.notes, last_seen_at = now(), closed_at = NULL
             RETURNING (xmax = 0)",
        )
        .bind(board_id)
        .bind(job.external_id)
        .bind(job.title)
        .bind(job.location)
        .bind(job.remote)
        .bind(job.department)
        .bind(job.url)
        .bind(job.posted_at)
        .bind(job.description)
        .bind(job.notes)
        .bind(first_read)
        .bind(job.company)
        .fetch_one(&mut *tx)
        .await?;
        new += u64::from(inserted);
    }
    let closed = match closing {
        Closing::WhenMissing => {
            let listed: Vec<&str> = jobs.iter().map(|job| job.external_id).collect();
            sqlx::query(
                "UPDATE jobs SET closed_at = now()
                 WHERE board_id = $1 AND closed_at IS NULL AND external_id <> ALL($2)",
            )
            .bind(board_id)
            .bind(listed)
            .execute(&mut *tx)
            .await?
        }
        Closing::AfterDaysUnseen(days) => {
            sqlx::query(
                "UPDATE jobs SET closed_at = now()
                 WHERE board_id = $1 AND closed_at IS NULL
                   AND last_seen_at < now() - make_interval(days => $2)",
            )
            .bind(board_id)
            .bind(days)
            .execute(&mut *tx)
            .await?
        }
    }
    .rows_affected();
    sqlx::query("UPDATE job_boards SET last_fetched_at = now(), last_error = NULL WHERE id = $1")
        .bind(board_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(BoardSave { new, closed })
}

/// Keeps the board's roles as they were and records why it could not be read.
pub async fn mark_board_failed(pool: &PgPool, board_id: i64, error: &str) -> Result<()> {
    sqlx::query("UPDATE job_boards SET last_error = $2 WHERE id = $1")
        .bind(board_id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn jobs_to_score(pool: &PgPool, limit: i64) -> Result<Vec<JobToScore>> {
    let rows = sqlx::query_as(
        "SELECT j.id, j.title, j.location, j.remote, j.department, j.description, j.notes,
                COALESCE(j.company, b.name) AS company, c.one_liner AS company_about
         FROM jobs j
         JOIN job_boards b ON b.id = j.board_id
         LEFT JOIN companies c ON c.id = b.company_id
         WHERE j.closed_at IS NULL AND j.fit IS NULL
         ORDER BY j.first_seen_at DESC, j.id
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn save_job_fit(
    pool: &PgPool,
    job_id: i64,
    fit: i32,
    reason: &str,
    is_startup: Option<bool>,
) -> Result<()> {
    sqlx::query("UPDATE jobs SET fit = $2, fit_reason = $3, is_startup = $4 WHERE id = $1")
        .bind(job_id)
        .bind(fit)
        .bind(reason)
        .bind(is_startup)
        .execute(pool)
        .await?;
    Ok(())
}

/// Every open role with its company, best fit first, as the web page shows it.
pub async fn job_listing(pool: &PgPool) -> Result<Value> {
    let Json(rows): Json<Value> = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(row ORDER BY fit DESC NULLS LAST, first_seen_at DESC), '[]')
         FROM (
           SELECT j.fit, j.first_seen_at, jsonb_build_object(
             'id', j.id,
             'title', j.title,
             'url', j.url,
             'location', j.location,
             'remote', j.remote,
             'department', j.department,
             'notes', j.notes,
             'posted_at', j.posted_at,
             'first_seen_at', j.first_seen_at,
             'baseline', j.baseline,
             'fit', j.fit,
             'fit_reason', j.fit_reason,
             'is_startup', j.is_startup,
             'board', b.ats,
             'company', COALESCE(j.company, b.name),
             'company_domain', c.domain,
             'company_about', c.one_liner,
             'company_stage', c.stage
           ) AS row
           FROM jobs j
           JOIN job_boards b ON b.id = j.board_id
           LEFT JOIN companies c ON c.id = b.company_id
           WHERE j.closed_at IS NULL
         ) open",
    )
    .fetch_one(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn board(pool: &PgPool) -> i64 {
        let board = NewBoard { company_id: None, name: "Acme", ats: "ashby", slug: "acme", source: "manual" };
        upsert_board(pool, &board).await.unwrap();
        upsert_board(pool, &board).await.unwrap();
        boards(pool).await.unwrap()[0].id
    }

    fn job(id: &str) -> NewJob<'_> {
        NewJob {
            external_id: id,
            title: "AI Engineer",
            company: None,
            location: "Remote",
            remote: true,
            department: None,
            url: "https://jobs.ashbyhq.com/acme/1",
            posted_at: None,
            description: None,
            notes: None,
        }
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn new_roles_are_counted_and_missing_ones_closed(pool: PgPool) {
        let id = board(&pool).await;
        assert!(board_search_due(&pool, 7).await.unwrap(), "no search has run yet");
        assert_eq!(boards(&pool).await.unwrap().len(), 1, "upsert keeps one board");

        let first = save_board_jobs(&pool, id, &[job("a"), job("b")], Closing::WhenMissing).await.unwrap();
        assert_eq!(first, BoardSave { new: 2, closed: 0 });
        let baseline: Vec<bool> = sqlx::query_scalar("SELECT baseline FROM jobs ORDER BY external_id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(baseline, [true, true], "the first read only sets the baseline");

        let second = save_board_jobs(&pool, id, &[job("b"), job("c")], Closing::WhenMissing).await.unwrap();
        assert_eq!(second, BoardSave { new: 1, closed: 1 });
        let fresh: bool = sqlx::query_scalar("SELECT baseline FROM jobs WHERE external_id = 'c'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!fresh, "a role that appears later is new");

        let listing = job_listing(&pool).await.unwrap();
        assert_eq!(listing.as_array().unwrap().len(), 2);
        assert_eq!(listing[0]["company"], "Acme");
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn feed_roles_close_by_age_and_carry_their_company(pool: PgPool) {
        let id = board(&pool).await;
        let feed_job = NewJob { company: Some("Beta"), ..job("a") };
        save_board_jobs(&pool, id, &[feed_job], Closing::AfterDaysUnseen(14)).await.unwrap();

        // Missing from a feed's latest page is not closed...
        let save = save_board_jobs(&pool, id, &[], Closing::AfterDaysUnseen(14)).await.unwrap();
        assert_eq!(save.closed, 0);
        assert_eq!(job_listing(&pool).await.unwrap()[0]["company"], "Beta");

        // ...until it has gone unseen for the whole window.
        sqlx::query("UPDATE jobs SET last_seen_at = now() - interval '15 days'")
            .execute(&pool)
            .await
            .unwrap();
        let save = save_board_jobs(&pool, id, &[], Closing::AfterDaysUnseen(14)).await.unwrap();
        assert_eq!(save.closed, 1);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn quiet_yc_pages_wait_for_the_weekly_check(pool: PgPool) {
        let quiet = NewBoard { company_id: None, name: "Quiet", ats: "yc", slug: "quiet", source: "yc" };
        upsert_board(&pool, &quiet).await.unwrap();
        assert_eq!(boards_due(&pool, 7).await.unwrap().len(), 1, "never read yet");

        let id = boards(&pool).await.unwrap()[0].id;
        save_board_jobs(&pool, id, &[], Closing::WhenMissing).await.unwrap();
        assert!(boards_due(&pool, 7).await.unwrap().is_empty(), "read today, nothing open");
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn dead_boards_from_search_are_skipped(pool: PgPool) {
        let found = NewBoard { company_id: None, name: "Gone", ats: "ashby", slug: "gone", source: "search" };
        let listed = NewBoard { name: "Typo", slug: "typo", source: "manual", ..found };
        upsert_board(&pool, &found).await.unwrap();
        upsert_board(&pool, &listed).await.unwrap();
        for board in boards(&pool).await.unwrap() {
            mark_board_failed(&pool, board.id, "404").await.unwrap();
        }
        let due = boards_due(&pool, 7).await.unwrap();
        assert_eq!(due.len(), 1, "a hand-listed board keeps being tried so its error stays visible");
        assert_eq!(due[0].slug, "typo");
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn scored_roles_leave_the_queue(pool: PgPool) {
        let id = board(&pool).await;
        save_board_jobs(&pool, id, &[job("a")], Closing::WhenMissing).await.unwrap();
        let queue = jobs_to_score(&pool, 10).await.unwrap();
        assert_eq!(queue.len(), 1);

        save_job_fit(&pool, queue[0].id, 70, "fits", Some(true)).await.unwrap();
        assert!(jobs_to_score(&pool, 10).await.unwrap().is_empty());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn yc_signals_become_boards(pool: PgPool) {
        let company: i64 = sqlx::query_scalar(
            "INSERT INTO companies (name, name_key, domain, is_ai) VALUES ('Acme', 'acme', 'acme.ai', true)
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO signals (company_id, source_type, source_url, title, published_at, kind)
             VALUES ($1, 'yc', 'https://www.ycombinator.com/companies/acme', 'Acme launched', now(), 'launched')",
        )
        .bind(company)
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(add_yc_boards(&pool, true).await.unwrap(), 1);
        assert_eq!(add_yc_boards(&pool, true).await.unwrap(), 0);
        assert_eq!(boards(&pool).await.unwrap()[0].slug, "acme");

        assert_eq!(careers_to_check(&pool, true, 14, 10).await.unwrap().len(), 1);
        mark_careers_checked(&pool, company).await.unwrap();
        assert!(careers_to_check(&pool, true, 14, 10).await.unwrap().is_empty());
    }
}
