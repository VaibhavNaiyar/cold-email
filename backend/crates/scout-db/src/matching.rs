use anyhow::Result;
use sqlx::PgPool;

/// What the classifier and scorer know about a company.
#[derive(Debug, sqlx::FromRow)]
pub struct CompanyBrief {
    pub id: i64,
    pub name: String,
    pub domain: String,
    pub one_liner: Option<String>,
    pub signals: String,
    pub details: String,
}

#[derive(Debug)]
pub struct Classification<'a> {
    pub is_ai: bool,
    pub stage: &'a str,
    pub region: &'a str,
    pub remote: Option<bool>,
    pub hiring: Option<bool>,
}

/// Which companies count as candidates, taken from `profile.yaml`.
#[derive(Debug)]
pub struct MatchFilter<'a> {
    pub must_be_ai: bool,
    pub stages: &'a [String],
    pub excluded_domains: &'a [String],
    /// Where an office job works; anywhere else the company must allow remote work.
    pub onsite_regions: &'a [String],
}

#[derive(Debug, sqlx::FromRow)]
pub struct Match {
    pub name: String,
    pub domain: String,
    pub stage: Option<String>,
    pub score: i32,
    pub reason: String,
    pub signal: String,
}

const BRIEF_COLUMNS: &str = "
    SELECT c.id, c.name, c.domain, c.one_liner,
           (SELECT string_agg(s.title, ' | ') FROM signals s WHERE s.company_id = c.id) AS signals,
           COALESCE((SELECT string_agg(s.raw_json::text, ' ') FROM signals s
                     WHERE s.company_id = c.id AND s.source_type = 'yc'), '') AS details
    FROM companies c";

// A stage the classifier could not determine stays eligible; so does a missing remote answer.
// Every query binds the filter as $1-$4 and numbers its own parameters from $5.
pub(crate) const CANDIDATE_RULES: &str = "
    c.domain IS NOT NULL
    AND (c.is_ai OR NOT $1)
    AND (c.stage = ANY($2) OR c.stage = 'unknown')
    AND c.domain <> ALL($3)
    AND (c.remote IS NOT FALSE
         OR EXISTS (SELECT 1 FROM unnest($4::text[]) r WHERE c.region ILIKE '%' || r || '%'))
    AND NOT EXISTS (SELECT 1 FROM drafts d WHERE d.company_id = c.id)
    AND NOT EXISTS (SELECT 1 FROM contacts k JOIN suppression x ON x.email = k.email
                    WHERE k.company_id = c.id)";

pub async fn unclassified(pool: &PgPool, limit: i64) -> Result<Vec<CompanyBrief>> {
    let sql = format!(
        "{BRIEF_COLUMNS}
         WHERE c.domain IS NOT NULL AND c.is_ai IS NULL
         ORDER BY c.last_signal_at DESC NULLS LAST
         LIMIT $1"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(limit).fetch_all(pool).await?)
}

pub async fn save_classification(
    pool: &PgPool,
    company_id: i64,
    result: &Classification<'_>,
) -> Result<()> {
    sqlx::query(
        "UPDATE companies
         SET is_ai = $2, stage = $3, region = $4, remote = $5, hiring = COALESCE($6, hiring),
             status = 'classified'
         WHERE id = $1",
    )
    .bind(company_id)
    .bind(result.is_ai)
    .bind(result.stage)
    .bind(result.region)
    .bind(result.remote)
    .bind(result.hiring)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unscored_candidates(
    pool: &PgPool,
    filter: &MatchFilter<'_>,
    limit: i64,
) -> Result<Vec<CompanyBrief>> {
    let sql = format!(
        "{BRIEF_COLUMNS}
         WHERE c.score IS NULL AND {CANDIDATE_RULES}
         ORDER BY EXISTS (SELECT 1 FROM signals s WHERE s.company_id = c.id AND s.kind = 'raised') DESC,
                  c.last_signal_at DESC NULLS LAST
         LIMIT $5"
    );
    let rows = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(filter.must_be_ai)
        .bind(filter.stages)
        .bind(filter.excluded_domains)
        .bind(filter.onsite_regions)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn save_score(
    pool: &PgPool,
    company_id: i64,
    score: i32,
    reason: &str,
    best_proof_point: Option<i32>,
) -> Result<()> {
    sqlx::query(
        "UPDATE companies
         SET score = $2, score_reason = $3, best_proof_point = $4, status = 'scored'
         WHERE id = $1",
    )
    .bind(company_id)
    .bind(score)
    .bind(reason)
    .bind(best_proof_point)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn top_matches(
    pool: &PgPool,
    filter: &MatchFilter<'_>,
    limit: i64,
) -> Result<Vec<Match>> {
    let sql = format!(
        "SELECT c.name, c.domain, c.stage, c.score, c.score_reason AS reason,
                (SELECT s.title FROM signals s WHERE s.company_id = c.id
                 ORDER BY s.published_at DESC LIMIT 1) AS signal
         FROM companies c
         WHERE c.score IS NOT NULL AND {CANDIDATE_RULES}
         ORDER BY c.score DESC, c.last_signal_at DESC NULLS LAST
         LIMIT $5"
    );
    let rows = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(filter.must_be_ai)
        .bind(filter.stages)
        .bind(filter.excluded_domains)
        .bind(filter.onsite_regions)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewSignal, resolve_domain, unresolved_companies, upsert_signal};
    use chrono::Utc;
    use serde_json::json;

    async fn company(pool: &PgPool, name: &str, domain: &str) -> i64 {
        let raw = json!({});
        let url = format!("https://a.test/{name}");
        let signal = NewSignal {
            company_name: name,
            legal_name: None,
            name_key: name,
            domain: None,
            domain_source: None,
            one_liner: None,
            website: None,
            hiring: None,
            source_type: "rss",
            source_url: &url,
            title: "raised",
            published_at: Utc::now(),
            kind: "raised",
            amount_usd: None,
            raw: &raw,
        };
        upsert_signal(pool, &signal).await.unwrap();
        let open = unresolved_companies(pool, 3, 10).await.unwrap();
        let id = open.iter().find(|c| c.name == name).unwrap().id;
        resolve_domain(pool, id, domain, "test").await.unwrap();
        id
    }

    async fn classify(pool: &PgPool, id: i64, is_ai: bool, stage: &str, remote: Option<bool>) {
        let result = Classification { is_ai, stage, region: "United States", remote, hiring: None };
        save_classification(pool, id, &result).await.unwrap();
    }

    async fn names(pool: &PgPool, filter: &MatchFilter<'_>) -> Vec<String> {
        let mut names: Vec<_> = unscored_candidates(pool, filter, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|company| company.name)
            .collect();
        names.sort();
        names
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn filters_apply_the_profile(pool: PgPool) {
        let keep = company(&pool, "keep", "keep.ai").await;
        classify(&pool, keep, true, "seed", None).await;
        let unknown = company(&pool, "unknown", "unknown.ai").await;
        classify(&pool, unknown, true, "unknown", Some(true)).await;
        let not_ai = company(&pool, "notai", "notai.com").await;
        classify(&pool, not_ai, false, "seed", None).await;
        let late = company(&pool, "late", "late.ai").await;
        classify(&pool, late, true, "series-b-plus", None).await;
        let onsite = company(&pool, "onsite", "onsite.ai").await;
        classify(&pool, onsite, true, "seed", Some(false)).await;
        let excluded = company(&pool, "excluded", "excluded.ai").await;
        classify(&pool, excluded, true, "seed", None).await;

        let stages = vec!["seed".to_string()];
        let excluded_domains = vec!["excluded.ai".to_string()];
        let filter =
            MatchFilter { must_be_ai: true, stages: &stages, excluded_domains: &excluded_domains, onsite_regions: &[] };
        assert_eq!(names(&pool, &filter).await, ["keep", "unknown"]);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn scored_companies_rank_and_leave_the_queue(pool: PgPool) {
        let stages = vec!["seed".to_string()];
        let filter = MatchFilter {
            must_be_ai: true,
            stages: &stages,
            excluded_domains: &[],
            onsite_regions: &[],
        };
        let low = company(&pool, "low", "low.ai").await;
        classify(&pool, low, true, "seed", None).await;
        let high = company(&pool, "high", "high.ai").await;
        classify(&pool, high, true, "seed", None).await;

        save_score(&pool, low, 40, "weak", Some(0)).await.unwrap();
        save_score(&pool, high, 90, "strong", Some(1)).await.unwrap();

        assert!(names(&pool, &filter).await.is_empty());
        let top = top_matches(&pool, &filter, 10).await.unwrap();
        assert_eq!(top[0].name, "high");
        assert_eq!(top[1].score, 40);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn unclassified_returns_only_new_companies(pool: PgPool) {
        let done = company(&pool, "done", "done.ai").await;
        classify(&pool, done, true, "seed", None).await;
        company(&pool, "fresh", "fresh.ai").await;
        let queue = unclassified(&pool, 10).await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].name, "fresh");
        assert_eq!(queue[0].signals, "raised");
    }
}
