use std::future::Future;

use anyhow::Result;
use serde_json::Value;
use sqlx::PgPool;

/// Records a job in `runs`. Failures are stored, then returned so the process exits non-zero.
pub async fn with_run<F>(pool: &PgPool, job: &str, work: F) -> Result<Value>
where
    F: Future<Output = Result<Value>>,
{
    let id: i64 = sqlx::query_scalar("INSERT INTO runs (job) VALUES ($1) RETURNING id")
        .bind(job)
        .fetch_one(pool)
        .await?;

    let outcome = work.await;
    let (stats, error) = match &outcome {
        Ok(stats) => (Some(stats), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };
    sqlx::query("UPDATE runs SET finished_at = now(), stats_json = $2, error = $3 WHERE id = $1")
        .bind(id)
        .bind(stats)
        .bind(error)
        .execute(pool)
        .await?;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use serde_json::json;

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn stores_stats_on_success(pool: PgPool) {
        with_run(&pool, "ok", async { Ok(json!({ "inserted": 3 })) })
            .await
            .unwrap();
        let (stats, error): (Option<Value>, Option<String>) =
            sqlx::query_as("SELECT stats_json, error FROM runs WHERE job = 'ok'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stats, Some(json!({ "inserted": 3 })));
        assert_eq!(error, None);
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn stores_the_error_and_returns_it(pool: PgPool) {
        let result = with_run(&pool, "bad", async { Err(anyhow!("boom")) }).await;
        assert!(result.is_err());
        let error: Option<String> = sqlx::query_scalar("SELECT error FROM runs WHERE job = 'bad'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(error.as_deref(), Some("boom"));
    }
}
