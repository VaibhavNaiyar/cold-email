use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

pub struct NewSignal<'a> {
    pub company_name: &'a str,
    pub legal_name: Option<&'a str>,
    pub name_key: &'a str,
    pub domain: Option<&'a str>,
    pub domain_source: Option<&'a str>,
    pub one_liner: Option<&'a str>,
    pub website: Option<&'a str>,
    pub hiring: Option<bool>,
    pub source_type: &'a str,
    pub source_url: &'a str,
    pub title: &'a str,
    pub published_at: DateTime<Utc>,
    pub kind: &'a str,
    pub amount_usd: Option<i64>,
    pub raw: &'a Value,
}

/// Stores a signal and its company. Returns true when the signal is new.
pub async fn upsert_signal(pool: &PgPool, signal: &NewSignal<'_>) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let company_id = upsert_company(&mut tx, signal).await?;
    let inserted = sqlx::query(
        "INSERT INTO signals
           (company_id, source_type, source_url, title, published_at, kind, amount_usd, raw_json)
         VALUES ($1, $2, $3, $4, $5, $6::signal_kind, $7, $8)
         ON CONFLICT (source_url) DO NOTHING",
    )
    .bind(company_id)
    .bind(signal.source_type)
    .bind(signal.source_url)
    .bind(signal.title)
    .bind(signal.published_at)
    .bind(signal.kind)
    .bind(signal.amount_usd)
    .bind(signal.raw)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    if inserted {
        sqlx::query(
            "UPDATE companies SET last_signal_at = GREATEST(last_signal_at, $2) WHERE id = $1",
        )
        .bind(company_id)
        .bind(signal.published_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(inserted)
}

async fn upsert_company(tx: &mut Transaction<'_, Postgres>, s: &NewSignal<'_>) -> Result<i64> {
    let sql = if s.domain.is_some() {
        "INSERT INTO companies
           (name, name_key, domain, domain_source, one_liner, website, hiring, legal_name)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (domain) DO UPDATE SET
           one_liner = COALESCE(companies.one_liner, EXCLUDED.one_liner),
           website   = COALESCE(companies.website, EXCLUDED.website),
           hiring    = COALESCE(EXCLUDED.hiring, companies.hiring)
         RETURNING id"
    } else {
        "INSERT INTO companies
           (name, name_key, domain, domain_source, one_liner, website, hiring, legal_name)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (name_key) WHERE domain IS NULL DO UPDATE SET
           one_liner  = COALESCE(companies.one_liner, EXCLUDED.one_liner),
           legal_name = COALESCE(companies.legal_name, EXCLUDED.legal_name)
         RETURNING id"
    };
    sqlx::query_scalar(sql)
        .bind(s.company_name)
        .bind(s.name_key)
        .bind(s.domain)
        .bind(s.domain_source)
        .bind(s.one_liner)
        .bind(s.website)
        .bind(s.hiring)
        .bind(s.legal_name)
        .fetch_one(&mut **tx)
        .await
        .with_context(|| format!("Could not store company {}", s.company_name))
}

/// Keeps only the URLs that have not been read before.
pub async fn filter_unseen(pool: &PgPool, urls: &[String]) -> Result<Vec<String>> {
    let unseen = sqlx::query_scalar(
        "SELECT candidate.url FROM unnest($1::text[]) AS candidate (url)
         WHERE NOT EXISTS (SELECT 1 FROM seen_urls s WHERE s.url = candidate.url)",
    )
    .bind(urls)
    .fetch_all(pool)
    .await?;
    Ok(unseen)
}

pub async fn mark_seen(pool: &PgPool, url: &str) -> Result<()> {
    sqlx::query("INSERT INTO seen_urls (url) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(url)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct UnresolvedCompany {
    pub id: i64,
    pub name: String,
    pub signal_title: String,
}

pub async fn domain_searches_today(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM companies
         WHERE domain_last_attempt_at >= date_trunc('day', now())",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn unresolved_companies(
    pool: &PgPool,
    max_attempts: i32,
    limit: i64,
) -> Result<Vec<UnresolvedCompany>> {
    let rows = sqlx::query_as(
        "SELECT c.id, c.name, s.title AS signal_title
         FROM companies c
         JOIN LATERAL (
           SELECT title, source_type FROM signals
           WHERE company_id = c.id ORDER BY published_at DESC LIMIT 1
         ) s ON true
         WHERE c.domain IS NULL AND c.domain_attempts < $1
           AND (c.domain_last_attempt_at IS NULL
                OR c.domain_last_attempt_at < date_trunc('day', now()))
         ORDER BY (s.source_type = 'rss') DESC, c.last_signal_at DESC NULLS LAST
         LIMIT $2",
    )
    .bind(max_attempts)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_domain_attempt(pool: &PgPool, company_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE companies
         SET domain_attempts = domain_attempts + 1, domain_last_attempt_at = now()
         WHERE id = $1",
    )
    .bind(company_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Sets a company's domain, merging it into any company that already owns the domain.
pub async fn resolve_domain(
    pool: &PgPool,
    company_id: i64,
    domain: &str,
    source: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM companies WHERE domain = $1")
        .bind(domain)
        .fetch_optional(&mut *tx)
        .await?;

    match existing {
        Some(target) if target != company_id => {
            sqlx::query("UPDATE signals SET company_id = $1 WHERE company_id = $2")
                .bind(target)
                .bind(company_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM companies WHERE id = $1")
                .bind(company_id)
                .execute(&mut *tx)
                .await?;
        }
        _ => {
            sqlx::query("UPDATE companies SET domain = $2, domain_source = $3 WHERE id = $1")
                .bind(company_id)
                .bind(domain)
                .bind(source)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn signal<'a>(url: &'a str, domain: Option<&'a str>, raw: &'a Value) -> NewSignal<'a> {
        NewSignal {
            company_name: "Acme",
            legal_name: None,
            name_key: "acme",
            domain,
            domain_source: domain.map(|_| "test"),
            one_liner: None,
            website: None,
            hiring: None,
            source_type: "rss",
            source_url: url,
            title: "Acme raises $3M",
            published_at: Utc::now(),
            kind: "raised",
            amount_usd: Some(3_000_000),
            raw,
        }
    }

    async fn company_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM companies")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn rerun_inserts_no_duplicates(pool: PgPool) {
        let raw = json!({});
        let first = signal("https://a.test/1", Some("acme.ai"), &raw);
        assert!(upsert_signal(&pool, &first).await.unwrap());
        assert!(!upsert_signal(&pool, &first).await.unwrap());
        assert_eq!(company_count(&pool).await, 1);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn domainless_signals_share_one_company(pool: PgPool) {
        let raw = json!({});
        upsert_signal(&pool, &signal("https://a.test/1", None, &raw)).await.unwrap();
        upsert_signal(&pool, &signal("https://a.test/2", None, &raw)).await.unwrap();
        assert_eq!(company_count(&pool).await, 1);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn domainless_signal_does_not_join_a_namesake(pool: PgPool) {
        let raw = json!({});
        upsert_signal(&pool, &signal("https://a.test/1", Some("acme.ai"), &raw)).await.unwrap();
        upsert_signal(&pool, &signal("https://a.test/2", None, &raw)).await.unwrap();
        assert_eq!(company_count(&pool).await, 2);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn resolving_to_a_known_domain_merges(pool: PgPool) {
        let raw = json!({});
        upsert_signal(&pool, &signal("https://a.test/1", Some("acme.ai"), &raw)).await.unwrap();
        upsert_signal(&pool, &signal("https://a.test/2", None, &raw)).await.unwrap();
        let open = unresolved_companies(&pool, 3, 10).await.unwrap();
        resolve_domain(&pool, open[0].id, "acme.ai", "tavily").await.unwrap();

        assert_eq!(company_count(&pool).await, 1);
        let signals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM signals s JOIN companies c ON c.id = s.company_id
             WHERE c.domain = 'acme.ai'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(signals, 2);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn attempted_companies_wait_until_the_next_day(pool: PgPool) {
        let raw = json!({});
        upsert_signal(&pool, &signal("https://a.test/1", None, &raw)).await.unwrap();
        let open = unresolved_companies(&pool, 3, 10).await.unwrap();
        mark_domain_attempt(&pool, open[0].id).await.unwrap();

        assert_eq!(domain_searches_today(&pool).await.unwrap(), 1);
        assert!(unresolved_companies(&pool, 3, 10).await.unwrap().is_empty());
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn seen_urls_are_filtered(pool: PgPool) {
        mark_seen(&pool, "https://a.test/1").await.unwrap();
        let urls = vec!["https://a.test/1".to_string(), "https://a.test/2".to_string()];
        assert_eq!(filter_unseen(&pool, &urls).await.unwrap(), vec![urls[1].clone()]);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn contact_without_a_source_is_refused(pool: PgPool) {
        let raw = json!({});
        upsert_signal(&pool, &signal("https://a.test/1", Some("acme.ai"), &raw)).await.unwrap();
        let result = sqlx::query(
            "INSERT INTO contacts (company_id, email, source_type, provenance)
             SELECT id, 'a@acme.ai', 'website', 'page' FROM companies",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err());
    }
}
