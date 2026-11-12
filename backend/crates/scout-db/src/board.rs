use anyhow::Result;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::types::Json;

/// Every scored company with its funding, best address and draft, as the web page shows it.
pub async fn board(pool: &PgPool) -> Result<Value> {
    let Json(rows): Json<Value> = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(row ORDER BY (row->>'score')::int DESC, row->>'name'), '[]')
         FROM (
           SELECT jsonb_build_object(
             'id', c.id,
             'name', c.name,
             'domain', c.domain,
             'one_liner', c.one_liner,
             'stage', c.stage,
             'region', c.region,
             'remote', c.remote,
             'score', c.score,
             'score_reason', c.score_reason,
             'last_signal_at', c.last_signal_at,
             'first_seen_at', c.first_seen_at,
             'funding', jsonb_build_object(
               'amount_usd', (SELECT max(s.amount_usd) FROM signals s WHERE s.company_id = c.id),
               'round', (SELECT s.raw_json->>'round' FROM signals s
                         WHERE s.company_id = c.id AND s.raw_json->>'round' IS NOT NULL
                         ORDER BY s.published_at DESC LIMIT 1),
               'investors', (SELECT s.raw_json->'investors' FROM signals s
                             WHERE s.company_id = c.id
                               AND jsonb_array_length(COALESCE(s.raw_json->'investors', '[]')) > 0
                             ORDER BY s.published_at DESC LIMIT 1),
               'yc_batch', (SELECT s.raw_json->>'batch' FROM signals s
                            WHERE s.company_id = c.id AND s.source_type = 'yc' LIMIT 1),
               'form_d', EXISTS (SELECT 1 FROM signals s
                                 WHERE s.company_id = c.id AND s.source_type = 'formd')),
             'signals', (SELECT COALESCE(jsonb_agg(jsonb_build_object(
                           'title', s.title, 'url', s.source_url, 'source', s.source_type,
                           'kind', s.kind, 'published_at', s.published_at)
                           ORDER BY s.published_at DESC), '[]')
                         FROM signals s WHERE s.company_id = c.id),
             'contact', (SELECT jsonb_build_object(
                           'email', k.email, 'name', k.full_name, 'role', k.role,
                           'is_generic', k.is_generic, 'source_url', k.source_url)
                         FROM contacts k WHERE k.company_id = c.id
                         ORDER BY k.is_technical DESC, k.confidence DESC NULLS LAST, k.id
                         LIMIT 1),
             'contacts_checked', c.contacts_checked_at IS NOT NULL,
             'draft', (SELECT jsonb_build_object(
                         'subject', d.subject, 'body', d.body, 'status', d.status)
                       FROM drafts d WHERE d.company_id = c.id)
           ) AS row
           FROM companies c
           WHERE c.score IS NOT NULL AND c.is_ai
         ) rows",
    )
    .fetch_one(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn shows_scored_ai_companies_with_their_funding(pool: PgPool) {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO companies (name, name_key, domain, is_ai, score)
             VALUES ('Acme', 'acme', 'acme.ai', true, 70) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO signals (company_id, source_type, source_url, title, published_at, kind,
                                  amount_usd, raw_json)
             VALUES ($1, 'rss', 'https://n/1', 'Acme raises $3M', now(), 'raised', 3000000,
                     '{\"round\": \"Seed\", \"investors\": [\"Fund\"]}')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO companies (name, name_key, is_ai) VALUES ('Unscored', 'u', true)")
            .execute(&pool)
            .await
            .unwrap();

        let rows = board(&pool).await.unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        let funding = &rows[0]["funding"];
        assert_eq!(funding["amount_usd"], 3_000_000);
        assert_eq!(funding["round"], "Seed");
        assert_eq!(funding["investors"][0], "Fund");
        assert!(rows[0]["contact"].is_null());
    }
}
