use anyhow::Result;
use scout_config::Env;
use scout_db::Pool;
use scout_mail::Gmail;
use serde_json::{Value, json};

/// Reads Gmail to learn which drafts were sent, answered or bounced.
pub async fn run(env: &Env, pool: &Pool) -> Result<Value> {
    let (user, password) = env.require_gmail()?;
    let mut gmail = Gmail::connect(user, password).await?;
    let (mut sent, mut replied, mut bounced) = (0, 0, 0);

    for draft in scout_db::open_drafts(pool).await? {
        let since = draft.created_at;
        if draft.queued {
            if !gmail.was_sent(&draft.email, since).await? {
                continue;
            }
            scout_db::mark_sent(pool, draft.id).await?;
            sent += 1;
        }
        // A bounce also mentions the address, so it is checked before any reply.
        if gmail.has_bounced(&draft.email, since).await? {
            scout_db::mark_bounced(pool, draft.id, &draft.email).await?;
            bounced += 1;
        } else if gmail.has_reply(&draft.email, &draft.domain, since).await? {
            scout_db::mark_replied(pool, draft.id).await?;
            replied += 1;
        }
    }
    gmail.logout().await?;
    Ok(json!({ "sent": sent, "replied": replied, "bounced": bounced }))
}
