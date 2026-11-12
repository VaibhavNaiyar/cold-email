//! Runs the job finder when the web page asks, one run at a time, in the background.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::Result;
use chrono::{DateTime, Utc};
use scout_config::{Env, Profile, Sources};
use scout_db::Pool;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefreshStatus {
    pub running: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub new_roles: Option<u64>,
    pub closed_roles: Option<u64>,
}

pub struct JobRefresh {
    env: Env,
    http: reqwest::Client,
    pool: Pool,
    status: Mutex<RefreshStatus>,
}

impl JobRefresh {
    pub fn new(env: Env, http: reqwest::Client, pool: Pool) -> Arc<Self> {
        Arc::new(Self { env, http, pool, status: Mutex::default() })
    }

    pub fn status(&self) -> RefreshStatus {
        self.lock().clone()
    }

    /// Starts a run unless one is already going. Returns whether this call started it.
    pub fn start(self: &Arc<Self>) -> bool {
        {
            let mut status = self.lock();
            if status.running {
                return false;
            }
            *status = RefreshStatus { running: true, started_at: Some(Utc::now()), ..RefreshStatus::default() };
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this.run_once().await;
            let mut status = this.lock();
            status.running = false;
            status.finished_at = Some(Utc::now());
            match outcome {
                Ok(stats) => {
                    status.new_roles = stats["new_roles"].as_u64();
                    status.closed_roles = stats["closed_roles"].as_u64();
                }
                Err(error) => status.error = Some(format!("{error:#}")),
            }
        });
        true
    }

    // Re-reads the profile and limits, so edits apply without restarting the server.
    async fn run_once(&self) -> Result<Value> {
        let profile = Profile::load()?;
        let sources = Sources::load()?;
        let job = crate::jobs::run(&self.env, &profile, &sources, &self.pool, &self.http);
        scout_db::with_run(&self.pool, "jobs", job).await
    }

    // A panic elsewhere must not leave the status locked for good.
    fn lock(&self) -> MutexGuard<'_, RefreshStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
