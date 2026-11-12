use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::MatchFilter;
use crate::matching::CANDIDATE_RULES;

// Each company once, with the address most worth writing to.
const BEST_CONTACT: &str = "
    SELECT DISTINCT ON (c.id)
           c.id AS company_id, c.name AS company, c.domain, c.score, c.one_liner,
           c.best_proof_point, ct.id AS contact_id, ct.email, ct.full_name, ct.role,
           ct.is_generic, ct.source_url, s.id AS signal_id, s.title AS signal_title
    FROM companies c
    JOIN contacts ct ON ct.company_id = c.id
    JOIN LATERAL (SELECT id, title FROM signals
                  WHERE company_id = c.id
                  ORDER BY (kind = 'raised') DESC, published_at DESC LIMIT 1) s ON true";
const BEST_CONTACT_ORDER: &str =
    "ORDER BY c.id, ct.is_technical DESC, ct.confidence DESC NULLS LAST, ct.id";

#[derive(Debug, sqlx::FromRow)]
pub struct DraftCandidate {
    pub company_id: i64,
    pub company: String,
    pub domain: String,
    pub score: i32,
    pub one_liner: Option<String>,
    pub best_proof_point: Option<i32>,
    pub contact_id: i64,
    pub email: String,
    pub full_name: Option<String>,
    pub role: Option<String>,
    pub is_generic: bool,
    pub source_url: String,
    pub signal_id: i64,
    pub signal_title: String,
}

#[derive(Debug)]
pub struct NewDraft<'a> {
    pub company_id: i64,
    pub contact_id: i64,
    pub subject: &'a str,
    pub body: &'a str,
    pub hook_signal_id: i64,
    pub message_id: &'a str,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PendingDraft {
    pub id: i64,
    pub email: String,
    pub subject: String,
    pub body: String,
    pub message_id: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OpenDraft {
    pub id: i64,
    pub email: String,
    pub domain: String,
    pub queued: bool,
    pub created_at: DateTime<Utc>,
}

/// One row per company, each with its best address, best-scored company first.
pub async fn draft_candidates(
    pool: &PgPool,
    filter: &MatchFilter<'_>,
    min_score: i32,
    limit: i64,
) -> Result<Vec<DraftCandidate>> {
    let sql = format!(
        "SELECT * FROM (
           {BEST_CONTACT}
           WHERE c.score >= $5 AND {CANDIDATE_RULES}
             AND NOT EXISTS (SELECT 1 FROM drafts d JOIN contacts used ON used.id = d.contact_id
                             WHERE used.email = ct.email
                               AND d.created_at > now() - interval '90 days')
           {BEST_CONTACT_ORDER}
         ) best
         ORDER BY score DESC, company_id
         LIMIT $6"
    );
    let rows = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(filter.must_be_ai)
        .bind(filter.stages)
        .bind(filter.excluded_domains)
        .bind(filter.onsite_regions)
        .bind(min_score)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// The company with its best address, ignoring the daily rules. Used for previews.
pub async fn draft_candidate(pool: &PgPool, company_id: i64) -> Result<Option<DraftCandidate>> {
    let sql = format!("{BEST_CONTACT} WHERE c.id = $1 {BEST_CONTACT_ORDER}");
    let row = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(company_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn drafts_today(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar("SELECT count(*) FROM drafts WHERE created_at >= current_date")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

pub async fn save_draft(pool: &PgPool, draft: &NewDraft<'_>) -> Result<i64> {
    let id = sqlx::query_scalar(
        "INSERT INTO drafts (company_id, contact_id, subject, body, hook_signal_id, message_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(draft.company_id)
    .bind(draft.contact_id)
    .bind(draft.subject)
    .bind(draft.body)
    .bind(draft.hook_signal_id)
    .bind(draft.message_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Written but not yet confirmed in Gmail, including any left by a failed run.
pub async fn pending_drafts(pool: &PgPool) -> Result<Vec<PendingDraft>> {
    let rows = sqlx::query_as(
        "SELECT d.id, ct.email, d.subject, d.body, d.message_id
         FROM drafts d JOIN contacts ct ON ct.id = d.contact_id
         WHERE d.status = 'pending' ORDER BY d.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_queued(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE drafts SET status = 'queued', pushed_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Drafts whose fate can still change: waiting for Send, or waiting for an answer.
pub async fn open_drafts(pool: &PgPool) -> Result<Vec<OpenDraft>> {
    let rows = sqlx::query_as(
        "SELECT d.id, ct.email, c.domain, d.status = 'queued' AS queued, d.created_at
         FROM drafts d
         JOIN contacts ct ON ct.id = d.contact_id
         JOIN companies c ON c.id = d.company_id
         WHERE d.status IN ('queued', 'sent')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_sent(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE drafts SET status = 'sent', sent_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_replied(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE drafts SET status = 'replied' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_bounced(pool: &PgPool, id: i64, email: &str) -> Result<()> {
    sqlx::query("UPDATE drafts SET status = 'bounced' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    suppress(pool, email, "bounced").await?;
    Ok(())
}

/// Blocks an address for good. Returns how many unsent drafts it cancelled.
pub async fn suppress(pool: &PgPool, email: &str, reason: &str) -> Result<u64> {
    let email = email.trim().to_lowercase();
    sqlx::query("INSERT INTO suppression (email, reason) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&email)
        .bind(reason)
        .execute(pool)
        .await?;
    let cancelled = sqlx::query(
        "UPDATE drafts SET status = 'skipped'
         WHERE status IN ('pending', 'queued')
           AND contact_id IN (SELECT id FROM contacts WHERE email = $1)",
    )
    .bind(&email)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGES: &[String] = &[];

    async fn seed(pool: &PgPool, domain: &str, email: &str, score: i32) -> (i64, i64, i64) {
        let company: i64 = sqlx::query_scalar(
            "INSERT INTO companies (name, name_key, domain, is_ai, stage, score)
             VALUES ($1, $1, $1, true, 'unknown', $2) RETURNING id",
        )
        .bind(domain)
        .bind(score)
        .fetch_one(pool)
        .await
        .unwrap();
        let signal: i64 = sqlx::query_scalar(
            "INSERT INTO signals (company_id, source_type, source_url, title, published_at, kind)
             VALUES ($1, 'rss', $2, 'Raised a seed round', now(), 'raised') RETURNING id",
        )
        .bind(company)
        .bind(format!("https://news/{domain}"))
        .fetch_one(pool)
        .await
        .unwrap();
        let contact = add_contact(pool, company, email, false, 0.5).await;
        (company, contact, signal)
    }

    async fn add_contact(
        pool: &PgPool,
        company: i64,
        email: &str,
        technical: bool,
        confidence: f32,
    ) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO contacts (company_id, email, is_technical, source_type, source_url,
                                   provenance, confidence)
             VALUES ($1, $2, $3, 'website', 'https://x/team', 'page', $4) RETURNING id",
        )
        .bind(company)
        .bind(email)
        .bind(technical)
        .bind(confidence)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn filter() -> MatchFilter<'static> {
        MatchFilter { must_be_ai: true, stages: STAGES, excluded_domains: &[], onsite_regions: &[] }
    }

    async fn draft(pool: &PgPool, seeded: (i64, i64, i64), message_id: &str) -> i64 {
        let draft = NewDraft {
            company_id: seeded.0,
            contact_id: seeded.1,
            subject: "hi",
            body: "body",
            hook_signal_id: seeded.2,
            message_id,
        };
        save_draft(pool, &draft).await.unwrap()
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn picks_the_technical_founder_and_best_score_first(pool: PgPool) {
        let (low, ..) = seed(&pool, "low.ai", "hello@low.ai", 60).await;
        let (high, ..) = seed(&pool, "high.ai", "hello@high.ai", 80).await;
        add_contact(&pool, high, "cto@high.ai", true, 0.9).await;

        let rows = draft_candidates(&pool, &filter(), 50, 10).await.unwrap();
        assert_eq!(rows.iter().map(|row| row.company_id).collect::<Vec<_>>(), [high, low]);
        assert_eq!(rows[0].email, "cto@high.ai");
        assert!(draft_candidates(&pool, &filter(), 90, 10).await.unwrap().is_empty());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_preview_ignores_the_daily_rules(pool: PgPool) {
        let seeded = seed(&pool, "acme.ai", "ada@acme.ai", 10).await;
        let row = draft_candidate(&pool, seeded.0).await.unwrap().unwrap();
        assert_eq!(row.email, "ada@acme.ai");
        assert!(draft_candidate(&pool, seeded.0 + 1).await.unwrap().is_none());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_company_is_only_drafted_once(pool: PgPool) {
        let seeded = seed(&pool, "acme.ai", "ada@acme.ai", 70).await;
        draft(&pool, seeded, "1@scout").await;
        assert!(draft_candidates(&pool, &filter(), 50, 10).await.unwrap().is_empty());
        assert_eq!(drafts_today(&pool).await.unwrap(), 1);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn drafts_move_through_their_states(pool: PgPool) {
        let seeded = seed(&pool, "acme.ai", "ada@acme.ai", 70).await;
        let id = draft(&pool, seeded, "1@scout").await;
        assert_eq!(pending_drafts(&pool).await.unwrap().len(), 1);

        mark_queued(&pool, id).await.unwrap();
        assert!(pending_drafts(&pool).await.unwrap().is_empty());
        assert!(open_drafts(&pool).await.unwrap()[0].queued);

        mark_sent(&pool, id).await.unwrap();
        assert!(!open_drafts(&pool).await.unwrap()[0].queued);

        mark_replied(&pool, id).await.unwrap();
        assert!(open_drafts(&pool).await.unwrap().is_empty());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn suppressing_cancels_unsent_drafts(pool: PgPool) {
        let seeded = seed(&pool, "acme.ai", "ada@acme.ai", 70).await;
        let id = draft(&pool, seeded, "1@scout").await;
        mark_queued(&pool, id).await.unwrap();

        assert_eq!(suppress(&pool, " Ada@Acme.ai ", "opt-out").await.unwrap(), 1);
        assert!(open_drafts(&pool).await.unwrap().is_empty());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_bounce_blocks_the_address(pool: PgPool) {
        let seeded = seed(&pool, "acme.ai", "ada@acme.ai", 70).await;
        let id = draft(&pool, seeded, "1@scout").await;
        mark_bounced(&pool, id, "ada@acme.ai").await.unwrap();
        let blocked: i64 = sqlx::query_scalar("SELECT count(*) FROM suppression")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(blocked, 1);
    }
}
