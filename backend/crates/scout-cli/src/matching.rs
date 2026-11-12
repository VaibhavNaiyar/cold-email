use anyhow::Result;
use futures::{StreamExt, stream};
use scout_config::{Env, Profile};
use scout_db::{Classification, CompanyBrief, MatchFilter, Pool};
use scout_llm::{CompanyInput, Llm, ProfileInput, classify, score};
use serde_json::{Value, json};

const CONCURRENCY: usize = 4;
const MAX_CLASSIFIED_PER_RUN: i64 = 500;
const MAX_SCORED_PER_RUN: i64 = 60;
const TOP_MATCHES: i64 = 15;
pub(crate) const MAX_REASON_WORDS: usize = 25;

pub async fn run(env: &Env, profile: &Profile, pool: &Pool, http: &reqwest::Client) -> Result<Value> {
    let llm = crate::llm_client(env, http)?;
    let mut errors = Vec::new();

    let queue = scout_db::unclassified(pool, MAX_CLASSIFIED_PER_RUN).await?;
    let classify_count = queue.len();
    let results: Vec<_> = stream::iter(queue)
        .map(|company| classify_company(&llm, http, company))
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    for result in results {
        match result {
            Ok((id, output)) => {
                let classification = Classification {
                    is_ai: output.is_ai,
                    stage: &output.stage,
                    region: &output.region,
                    remote: output.remote_friendly,
                    hiring: output.hiring_engineers,
                };
                scout_db::save_classification(pool, id, &classification).await?;
            }
            Err(error) => errors.push(format!("{error:#}")),
        }
    }

    let filter = MatchFilter {
        must_be_ai: profile.must_be_ai,
        stages: &profile.stage,
        excluded_domains: &profile.exclude_domains,
        onsite_regions: &profile.onsite_regions,
    };
    let candidates = scout_db::unscored_candidates(pool, &filter, MAX_SCORED_PER_RUN).await?;
    let score_count = candidates.len();
    let profile_input = profile_input(profile);
    let results: Vec<_> = stream::iter(candidates)
        .map(|company| {
            let llm = &llm;
            let profile_input = &profile_input;
            async move {
                let result = score(llm, profile_input, &company_input(&company, None)).await;
                (company.id, result)
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    for (id, result) in results {
        match result {
            Ok(output) => {
                let proof = usize::try_from(output.best_proof_point_index)
                    .ok()
                    .filter(|index| *index < profile.proof_points.len())
                    .map(|_| output.best_proof_point_index);
                let reason = limit_words(&output.reason, MAX_REASON_WORDS);
                scout_db::save_score(pool, id, output.score.clamp(0, 100), &reason, proof).await?;
            }
            Err(error) => errors.push(format!("{error:#}")),
        }
    }

    let top = scout_db::top_matches(pool, &filter, TOP_MATCHES).await?;
    for (rank, company) in top.iter().enumerate() {
        println!(
            "{:>2}. [{}] {} ({}) · {}\n    {}\n    {}",
            rank + 1,
            company.score,
            company.name,
            company.domain,
            company.stage.as_deref().unwrap_or("unknown"),
            company.reason,
            company.signal,
        );
    }

    Ok(json!({
        "classified": classify_count,
        "scored": score_count,
        "top": top.len(),
        "errors": errors,
    }))
}

async fn classify_company(
    llm: &Llm,
    http: &reqwest::Client,
    company: CompanyBrief,
) -> Result<(i64, scout_llm::CompanyClass)> {
    // Companies with no description need their site; a failed read just leaves less to go on.
    let homepage = match company.one_liner {
        Some(_) => None,
        None => scout_enrich::homepage_text(http, &company.domain).await.ok().flatten(),
    };
    let output = classify(llm, &company_input(&company, homepage.as_deref())).await?;
    Ok((company.id, output))
}

pub(crate) fn profile_input(profile: &Profile) -> ProfileInput<'_> {
    ProfileInput {
        summary: &profile.summary,
        roles: &profile.roles,
        stack: &profile.stack,
        locations: &profile.locations,
        proof_points: &profile.proof_points,
    }
}

pub(crate) fn limit_words(text: &str, max: usize) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    if words.len() <= max {
        return words.join(" ");
    }
    format!("{}...", words[..max].join(" "))
}

fn company_input<'a>(company: &'a CompanyBrief, homepage: Option<&'a str>) -> CompanyInput<'a> {
    CompanyInput {
        name: &company.name,
        domain: &company.domain,
        one_liner: company.one_liner.as_deref(),
        signals: &company.signals,
        details: &company.details,
        homepage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_words_keeps_short_text_and_trims_long_text() {
        assert_eq!(limit_words("a b  c", 3), "a b c");
        assert_eq!(limit_words("a b c d", 2), "a b...");
    }
}
