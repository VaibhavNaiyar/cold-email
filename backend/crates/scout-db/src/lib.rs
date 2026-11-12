//! Postgres access: connection, migrations, the run logger and ingest queries.

mod board;
mod contacts;
mod drafts;
mod ingest;
mod jobs;
mod matching;
mod runs;

use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub use board::board;
pub use contacts::{
    ContactCandidate, NewContact, contact_candidates, mark_contacts_checked, save_contact,
};
pub use drafts::{
    DraftCandidate, NewDraft, OpenDraft, PendingDraft, draft_candidate, draft_candidates, drafts_today, mark_bounced,
    mark_queued, mark_replied, mark_sent, open_drafts, pending_drafts, save_draft, suppress,
};
pub use ingest::{
    NewSignal, UnresolvedCompany, domain_searches_today, filter_unseen, mark_seen,
    mark_domain_attempt, resolve_domain, unresolved_companies, upsert_signal,
};
pub use jobs::{
    Board, BoardSave, CareersCheck, Closing, JobToScore, NewBoard, NewJob, add_yc_boards,
    board_search_due, boards, boards_due,
    careers_to_check, described_jobs, job_listing, jobs_to_score, mark_board_failed,
    mark_careers_checked, save_board_jobs, save_job_fit, upsert_board,
};
pub use matching::{
    Classification, CompanyBrief, Match, MatchFilter, save_classification, save_score, top_matches,
    unclassified, unscored_candidates,
};
pub use runs::with_run;
pub use sqlx::PgPool as Pool;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await
        .context("Could not connect to Postgres. Is `docker compose up -d` running?")
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR.run(pool).await.context("Migration failed")
}
