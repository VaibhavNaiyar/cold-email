use anyhow::Result;
use serde_json::Value;
use sqlx::PgPool;

use crate::MatchFilter;
use crate::matching::CANDIDATE_RULES;

#[derive(Debug, sqlx::FromRow)]
pub struct ContactCandidate {
    pub id: i64,
    pub name: String,
    pub domain: String,
    pub score: i32,
    pub yc_url: Option<String>,
    pub signal_founders: Option<Value>,
}

#[derive(Debug)]
pub struct NewContact<'a> {
    pub company_id: i64,
    pub full_name: Option<&'a str>,
    pub role: Option<&'a str>,
    pub is_technical: bool,
    pub is_generic: bool,
    pub email: &'a str,
    pub source_type: &'a str,
    pub source_url: &'a str,
    pub confidence: f32,
}

/// The best-scored candidates not yet searched for an address.
pub async fn contact_candidates(
    pool: &PgPool,
    filter: &MatchFilter<'_>,
    min_score: i32,
    limit: i64,
) -> Result<Vec<ContactCandidate>> {
    let sql = format!(
        "SELECT c.id, c.name, c.domain, c.score,
                (SELECT s.source_url FROM signals s
                 WHERE s.company_id = c.id AND s.source_type = 'yc' LIMIT 1) AS yc_url,
                (SELECT jsonb_agg(f) FROM signals s,
                        jsonb_array_elements(s.raw_json->'founders') f
                 WHERE s.company_id = c.id
                   AND jsonb_typeof(s.raw_json->'founders') = 'array') AS signal_founders
         FROM companies c
         WHERE c.score >= $6 AND c.contacts_checked_at IS NULL AND {CANDIDATE_RULES}
         ORDER BY c.score DESC, c.last_signal_at DESC NULLS LAST
         LIMIT $5"
    );
    let rows = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(filter.must_be_ai)
        .bind(filter.stages)
        .bind(filter.excluded_domains)
        .bind(filter.onsite_regions)
        .bind(limit)
        .bind(min_score)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// Stores an address unless it is suppressed. Returns true when it is new.
pub async fn save_contact(pool: &PgPool, contact: &NewContact<'_>) -> Result<bool> {
    let inserted = sqlx::query(
        "INSERT INTO contacts
           (company_id, full_name, role, is_technical, is_generic, email,
            source_type, source_url, provenance, confidence)
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, 'page', $9
         WHERE NOT EXISTS (SELECT 1 FROM suppression WHERE email = $6)
         ON CONFLICT (company_id, email) DO NOTHING",
    )
    .bind(contact.company_id)
    .bind(contact.full_name)
    .bind(contact.role)
    .bind(contact.is_technical)
    .bind(contact.is_generic)
    .bind(contact.email)
    .bind(contact.source_type)
    .bind(contact.source_url)
    .bind(contact.confidence)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(inserted == 1)
}

pub async fn mark_contacts_checked(pool: &PgPool, company_id: i64) -> Result<()> {
    sqlx::query("UPDATE companies SET contacts_checked_at = now() WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn company(pool: &PgPool) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO companies (name, name_key, domain) VALUES ('Acme', 'acme', 'acme.ai')
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn contact(company_id: i64) -> NewContact<'static> {
        NewContact {
            company_id,
            full_name: Some("Ada Stone"),
            role: Some("CEO"),
            is_technical: false,
            is_generic: false,
            email: "ada@acme.ai",
            source_type: "website",
            source_url: "https://acme.ai/team",
            confidence: 0.9,
        }
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn saves_once(pool: PgPool) {
        let id = company(&pool).await;
        assert!(save_contact(&pool, &contact(id)).await.unwrap());
        assert!(!save_contact(&pool, &contact(id)).await.unwrap());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn suppressed_addresses_are_never_stored(pool: PgPool) {
        let id = company(&pool).await;
        sqlx::query("INSERT INTO suppression (email, reason) VALUES ('ada@acme.ai', 'opt-out')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!save_contact(&pool, &contact(id)).await.unwrap());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn checked_companies_leave_the_queue(pool: PgPool) {
        let id = company(&pool).await;
        sqlx::query(
            "UPDATE companies SET is_ai = true, stage = 'seed', score = 80 WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let stages = vec!["seed".to_string()];
        let filter = MatchFilter {
            must_be_ai: true,
            stages: &stages,
            excluded_domains: &[],
            onsite_regions: &[],
        };

        let queue = contact_candidates(&pool, &filter, 50, 10).await.unwrap();
        assert_eq!(queue.len(), 1);
        assert!(contact_candidates(&pool, &filter, 90, 10).await.unwrap().is_empty());

        mark_contacts_checked(&pool, id).await.unwrap();
        assert!(contact_candidates(&pool, &filter, 50, 10).await.unwrap().is_empty());
    }
}
