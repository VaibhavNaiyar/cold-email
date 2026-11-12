mod contacts;
mod draft;
mod import;
mod ingest;
mod jobs;
mod matching;
mod refresh;
mod serve;
mod sync;

use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use draft::Mode;
use scout_config::{Env, Profile, Sources};

#[derive(Parser)]
#[command(name = "scout", about = "Finds funded AI startups and drafts outreach")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Apply pending database migrations
    Migrate,
    /// Pull sources, resolve domains and store signals
    Ingest {
        /// Fetch and extract, but store nothing except the run log
        #[arg(long)]
        dry: bool,
    },
    /// Classify companies, score candidates and print the top matches
    Match,
    /// Find published email addresses for the top-scored companies
    Contacts,
    /// Find open roles on startup careers boards and rate them
    Jobs,
    /// Write emails and put them in Gmail Drafts. Never sends
    Draft {
        /// Print the emails, touching neither Gmail nor the drafts table
        #[arg(long, conflicts_with = "test")]
        dry: bool,
        /// Put one email addressed to yourself in Gmail Drafts
        #[arg(long)]
        test: bool,
    },
    /// Read Gmail and record which drafts were sent, answered or bounced
    Sync,
    /// Add the careers boards found in a file of links (`URL` or `Name<TAB>URL` per line)
    ImportBoards {
        file: std::path::PathBuf,
    },
    /// Never contact an address again
    Suppress {
        email: String,
        #[arg(long, default_value = "opt-out")]
        reason: String,
    },
    /// The daily job: sync, ingest, match, contacts, jobs, draft
    Run,
    /// Serve the read API for the web page, on this machine only
    Serve {
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let env = Env::load()?;
    let pool = scout_db::connect(&env.database_url).await?;

    match cli.command {
        Command::Migrate => {
            scout_db::migrate(&pool).await?;
            println!("Migrations applied");
        }
        Command::Ingest { dry } => {
            Profile::load()?;
            let sources = Sources::load()?;
            let http = http_client(&env)?;
            let job = ingest::run(&env, &sources, &pool, &http, dry);
            let stats = scout_db::with_run(&pool, "ingest", job).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Match => {
            let profile = Profile::load()?;
            let http = http_client(&env)?;
            let job = matching::run(&env, &profile, &pool, &http);
            let stats = scout_db::with_run(&pool, "match", job).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Contacts => {
            let profile = Profile::load()?;
            let sources = Sources::load()?;
            let http = http_client(&env)?;
            let job = contacts::run(&env, &profile, &sources, &pool, &http);
            let stats = scout_db::with_run(&pool, "contacts", job).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Jobs => {
            let profile = Profile::load()?;
            let sources = Sources::load()?;
            let http = http_client(&env)?;
            let job = jobs::run(&env, &profile, &sources, &pool, &http);
            let stats = scout_db::with_run(&pool, "jobs", job).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Draft { dry, test } => {
            let profile = Profile::load()?;
            let sources = Sources::load()?;
            let http = http_client(&env)?;
            let mode = match (dry, test) {
                (true, _) => Mode::Dry,
                (_, true) => Mode::Test,
                _ => Mode::Live,
            };
            let job = draft::run(&env, &profile, &sources, &pool, &http, mode);
            let stats = scout_db::with_run(&pool, "draft", job).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Sync => {
            let stats = scout_db::with_run(&pool, "sync", sync::run(&env, &pool)).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::ImportBoards { file } => {
            let stats = scout_db::with_run(&pool, "import-boards", import::run(&pool, &file)).await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Suppress { email, reason } => {
            let cancelled = scout_db::suppress(&pool, &email, &reason).await?;
            println!("Suppressed {email}. {cancelled} unsent draft(s) cancelled; delete any from Gmail.");
        }
        Command::Serve { port } => {
            let http = http_client(&env)?;
            let writer = draft::Writer::new(&env, Profile::load()?, &http)?;
            let refresh = refresh::JobRefresh::new(env.clone(), http, pool.clone());
            serve::run(pool, writer, refresh, port).await?;
        }
        Command::Run => {
            env.require_gmail()?;
            let profile = Profile::load()?;
            let sources = Sources::load()?;
            let http = http_client(&env)?;
            let stats = serde_json::json!({
                "sync": scout_db::with_run(&pool, "sync", sync::run(&env, &pool)).await?,
                "ingest": scout_db::with_run(&pool, "ingest", ingest::run(&env, &sources, &pool, &http, false)).await?,
                "match": scout_db::with_run(&pool, "match", matching::run(&env, &profile, &pool, &http)).await?,
                "contacts": scout_db::with_run(&pool, "contacts", contacts::run(&env, &profile, &sources, &pool, &http)).await?,
                "jobs": scout_db::with_run(&pool, "jobs", jobs::run(&env, &profile, &sources, &pool, &http)).await?,
                "draft": scout_db::with_run(&pool, "draft", draft::run(&env, &profile, &sources, &pool, &http, Mode::Live)).await?,
            });
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
    }
    Ok(())
}

fn llm_client(env: &Env, http: &reqwest::Client) -> Result<scout_llm::Llm> {
    Ok(scout_llm::Llm::new(
        http.clone(),
        env.require_anthropic_key()?,
        &env.llm_fast_model,
        &env.llm_smart_model,
    ))
}

fn http_client(env: &Env) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(env.user_agent())
        .timeout(Duration::from_secs(60))
        .build()?)
}
