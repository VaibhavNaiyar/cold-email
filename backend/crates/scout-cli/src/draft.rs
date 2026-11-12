use anyhow::Result;
use chrono::Utc;
use scout_config::{Env, Profile, Sources, read_template};
use scout_db::{DraftCandidate, MatchFilter, NewDraft, Pool};
use scout_llm::{EmailInput, EmailSlots, Llm, write_email};
use scout_mail::{Email, Gmail, Slots, build_message, lint, render, unsupported_facts};
use serde_json::{Value, json};

const TEMPLATE: &str = "outreach.md";
const GUIDE: &str = "outreach-guide.md";
const MAX_WRITES: usize = 2;
// Extra companies to read, so a few rejected emails do not shrink the day's batch.
const SPARE_CANDIDATES: i64 = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Writes to Gmail Drafts and records the draft.
    Live,
    /// Prints emails. Touches neither Gmail nor the drafts table.
    Dry,
    /// Puts one email, addressed to the owner, in Gmail Drafts. Records nothing.
    Test,
}

pub struct Writer {
    llm: Llm,
    profile: Profile,
    template: String,
    guide: String,
}

pub async fn run(
    env: &Env,
    profile: &Profile,
    sources: &Sources,
    pool: &Pool,
    http: &reqwest::Client,
    mode: Mode,
) -> Result<Value> {
    // Log in before spending on the model, so a bad password fails at once.
    let gmail = env.require_gmail();
    let mut session = match mode {
        Mode::Dry => None,
        _ => {
            let (user, password) = gmail?;
            Some(Gmail::connect(user, password).await?)
        }
    };

    let writer = Writer::new(env, profile.clone(), http)?;
    let filter = MatchFilter {
        must_be_ai: profile.must_be_ai,
        stages: &profile.stage,
        excluded_domains: &profile.exclude_domains,
        onsite_regions: &profile.onsite_regions,
    };

    let mut pushed = 0;
    if mode == Mode::Live {
        pushed += push_pending(env, profile, pool, session.as_mut()).await?;
    }
    let cap = i64::from(profile.max_emails_per_day);
    let budget = match mode {
        Mode::Live => cap - scout_db::drafts_today(pool).await?,
        Mode::Dry => cap,
        Mode::Test => 1,
    }
    .max(0);

    let candidates = if budget == 0 {
        Vec::new()
    } else {
        let limit = budget + SPARE_CANDIDATES;
        scout_db::draft_candidates(pool, &filter, sources.min_contact_score, limit).await?
    };

    let (mut written, mut rejected, mut errors) = (0, 0, Vec::new());
    for company in &candidates {
        if written >= budget {
            break;
        }
        let email = match writer.write(company).await {
            Ok(Ok(email)) => email,
            Ok(Err(problems)) => {
                rejected += 1;
                println!("REJECTED {}: {}\n", company.company, problems.join("; "));
                continue;
            }
            Err(error) => {
                errors.push(format!("{}: {error:#}", company.company));
                continue;
            }
        };
        written += 1;
        print_email(company, &email);

        match mode {
            Mode::Dry => {}
            Mode::Test => {
                let (user, _) = env.require_gmail()?;
                let id = message_id(0, user);
                let raw = build_message(&profile.name, user, user, &email, &id)?;
                if let Some(gmail) = session.as_mut() {
                    gmail.append_draft(&raw, &id).await?;
                    pushed += 1;
                }
            }
            Mode::Live => {
                let (user, _) = env.require_gmail()?;
                let id = message_id(company.company_id, user);
                let draft = NewDraft {
                    company_id: company.company_id,
                    contact_id: company.contact_id,
                    subject: &email.subject,
                    body: &email.body,
                    hook_signal_id: company.signal_id,
                    message_id: &id,
                };
                scout_db::save_draft(pool, &draft).await?;
                pushed += push_pending(env, profile, pool, session.as_mut()).await?;
            }
        }
    }
    if let Some(gmail) = session {
        gmail.logout().await?;
    }

    Ok(json!({
        "written": written,
        "rejected_by_lint": rejected,
        "pushed_to_gmail_drafts": pushed,
        "errors": errors,
    }))
}

impl Writer {
    pub fn new(env: &Env, profile: Profile, http: &reqwest::Client) -> Result<Self> {
        Ok(Self {
            llm: crate::llm_client(env, http)?,
            profile,
            template: read_template(TEMPLATE)?,
            guide: read_template(GUIDE)?,
        })
    }

    /// The finished email, or the reasons no acceptable one could be written.
    pub async fn write(&self, company: &DraftCandidate) -> Result<Result<Email, Vec<String>>> {
        let index = usize::try_from(company.best_proof_point.unwrap_or(0)).unwrap_or(0);
        let proof = self
            .profile
            .proof_points
            .get(index)
            .unwrap_or(&self.profile.proof_points[0]);
        let input = EmailInput {
            company: &company.company,
            one_liner: company.one_liner.as_deref(),
            signal: &company.signal_title,
            recipient_role: company.role.as_deref(),
            roles: &self.profile.roles,
            stack: &self.profile.stack,
            proof,
        };
        let source = format!(
            "{} {} {} {} {} {proof}",
            company.company,
            company.domain,
            company.one_liner.as_deref().unwrap_or_default(),
            company.signal_title,
            self.profile.stack.join(" "),
        );
        let first_name = first_name(company);

        let mut problems = Vec::new();
        for _ in 0..MAX_WRITES {
            let slots = write_email(&self.llm, &self.guide, &input, &problems).await?;
            let email = render(
                &self.template,
                &Slots {
                    subject: &slots.subject,
                    first_name: &first_name,
                    hook: &slots.hook,
                    proof,
                    offer: &slots.offer,
                },
            )?;
            problems = review(&email, &slots, &source);
            if problems.is_empty() {
                return Ok(Ok(email));
            }
        }
        Ok(Err(problems))
    }
}

fn review(email: &Email, slots: &EmailSlots, source: &str) -> Vec<String> {
    let mut problems = lint(email);
    let written = format!("{}. {}. {}", slots.subject, slots.hook, slots.offer);
    problems.extend(
        unsupported_facts(&written, source)
            .into_iter()
            .map(|fact| format!("\"{fact}\" is not in the input")),
    );
    problems
}

// A shared inbox has no name to greet.
fn first_name(company: &DraftCandidate) -> String {
    company
        .full_name
        .as_deref()
        .filter(|_| !company.is_generic)
        .and_then(|name| name.split_whitespace().next())
        .map_or_else(|| format!("{} team", company.company), str::to_owned)
}

async fn push_pending(
    env: &Env,
    profile: &Profile,
    pool: &Pool,
    session: Option<&mut Gmail>,
) -> Result<i64> {
    let (Some(gmail), (user, _)) = (session, env.require_gmail()?) else {
        return Ok(0);
    };
    let mut pushed = 0;
    for draft in scout_db::pending_drafts(pool).await? {
        let email = Email { subject: draft.subject, body: draft.body };
        let raw = build_message(&profile.name, user, &draft.email, &email, &draft.message_id)?;
        gmail.append_draft(&raw, &draft.message_id).await?;
        scout_db::mark_queued(pool, draft.id).await?;
        pushed += 1;
    }
    Ok(pushed)
}

fn message_id(company_id: i64, user: &str) -> String {
    let domain = user.rsplit('@').next().unwrap_or("scout");
    format!("scout-{company_id}-{}@{domain}", Utc::now().timestamp_millis())
}

fn print_email(company: &DraftCandidate, email: &Email) {
    println!(
        "--- [{}] {} -> {}\n    source: {}\nSubject: {}\n\n{}\n",
        company.score, company.company, company.email, company.source_url, email.subject, email.body,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn company(full_name: Option<&str>, is_generic: bool) -> DraftCandidate {
        DraftCandidate {
            company_id: 1,
            company: "Acme".into(),
            domain: "acme.ai".into(),
            score: 70,
            one_liner: None,
            best_proof_point: None,
            contact_id: 1,
            email: "x@acme.ai".into(),
            full_name: full_name.map(str::to_owned),
            role: None,
            is_generic,
            source_url: "https://acme.ai".into(),
            signal_id: 1,
            signal_title: "Raised".into(),
        }
    }

    #[test]
    fn greets_a_person_by_first_name_and_an_inbox_by_company() {
        assert_eq!(first_name(&company(Some("Ada Stone"), false)), "Ada");
        assert_eq!(first_name(&company(None, true)), "Acme team");
    }

    #[test]
    fn invented_facts_fail_the_review() {
        let slots = EmailSlots {
            subject: "hi".into(),
            hook: "Congrats on the $9M.".into(),
            offer: "I could help.".into(),
        };
        let email = Email { subject: "hi".into(), body: "I won't follow up.".into() };
        assert_eq!(review(&email, &slots, "Acme raised a seed").len(), 1);
    }
}
