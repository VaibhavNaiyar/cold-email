use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result, anyhow, bail};

use crate::files::repo_root;

const BASE_USER_AGENT: &str = "startup-scout/0.1";
const DEFAULT_FAST_MODEL: &str = "claude-haiku-5-5";
const DEFAULT_SMART_MODEL: &str = "claude-sonnet-5-5";

#[derive(Clone)]
pub struct Env {
    pub database_url: String,
    pub anthropic_api_key: Option<String>,
    pub llm_fast_model: String,
    pub llm_smart_model: String,
    pub tavily_api_key: Option<String>,
    pub sec_user_agent: Option<String>,
    pub gmail_user: Option<String>,
    pub gmail_app_password: Option<String>,
}

// Hand-written so that a stray `{:?}` can never print a secret.
impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Env").finish_non_exhaustive()
    }
}

impl Env {
    pub fn load() -> Result<Self> {
        if let Ok(root) = repo_root() {
            // A missing .env is fine: CI passes real environment variables.
            let _ = dotenvy::from_path(root.join(".env"));
        }
        Self::parse(&std::env::vars().collect())
    }

    pub fn parse(vars: &HashMap<String, String>) -> Result<Self> {
        // Keys left blank in a copied .env.example count as unset.
        let get = |key: &str| {
            vars.get(key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };

        let database_url = get("DATABASE_URL").context("DATABASE_URL is required")?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            bail!("DATABASE_URL must start with postgres://");
        }

        let env = Self {
            database_url,
            anthropic_api_key: get("ANTHROPIC_API_KEY"),
            llm_fast_model: get("LLM_FAST_MODEL").unwrap_or_else(|| DEFAULT_FAST_MODEL.into()),
            llm_smart_model: get("LLM_SMART_MODEL").unwrap_or_else(|| DEFAULT_SMART_MODEL.into()),
            tavily_api_key: get("TAVILY_API_KEY"),
            sec_user_agent: get("SEC_USER_AGENT"),
            gmail_user: get("GMAIL_USER"),
            // Google displays these in groups of four; the spaces are not part of it.
            gmail_app_password: get("GMAIL_APP_PASSWORD").map(|value| value.replace(' ', "")),
        };
        if env.gmail_user.is_some() != env.gmail_app_password.is_some() {
            bail!("GMAIL_USER and GMAIL_APP_PASSWORD must be set together");
        }
        Ok(env)
    }

    /// The configured agent, once it carries a real contact address.
    pub fn contact_user_agent(&self) -> Option<&str> {
        self.sec_user_agent
            .as_deref()
            .filter(|agent| agent.contains('@') && !agent.contains("example.com"))
    }

    pub fn user_agent(&self) -> &str {
        self.contact_user_agent().unwrap_or(BASE_USER_AGENT)
    }

    pub fn require_gmail(&self) -> Result<(&str, &str)> {
        self.gmail_user
            .as_deref()
            .zip(self.gmail_app_password.as_deref())
            .ok_or_else(|| anyhow!("GMAIL_USER and GMAIL_APP_PASSWORD are required. Set them in .env"))
    }

    pub fn require_anthropic_key(&self) -> Result<&str> {
        self.anthropic_api_key
            .as_deref()
            .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY is required for this job. Set it in .env"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        let base = [("DATABASE_URL", "postgres://localhost/x")];
        base.iter()
            .chain(pairs)
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn blank_keys_are_unset() {
        let env = Env::parse(&vars(&[("ANTHROPIC_API_KEY", "  ")])).unwrap();
        assert!(env.anthropic_api_key.is_none());
        assert!(env.require_anthropic_key().is_err());
    }

    #[test]
    fn rejects_non_postgres_url() {
        let mut map = vars(&[]);
        map.insert("DATABASE_URL".into(), "mysql://user:hunter2@host".into());
        let error = Env::parse(&map).unwrap_err().to_string();
        assert!(error.contains("DATABASE_URL"));
        assert!(!error.contains("hunter2"));
    }

    #[test]
    fn gmail_credentials_come_as_a_pair() {
        assert!(Env::parse(&vars(&[("GMAIL_USER", "a@b.c")])).is_err());
    }

    #[test]
    fn app_password_spaces_are_removed() {
        let pairs = [("GMAIL_USER", "a@b.c"), ("GMAIL_APP_PASSWORD", "abcd efgh")];
        let env = Env::parse(&vars(&pairs)).unwrap();
        assert_eq!(env.require_gmail().unwrap(), ("a@b.c", "abcdefgh"));
    }

    #[test]
    fn placeholder_agent_is_not_a_contact() {
        let placeholder = "startup-scout/0.1 your.email@example.com";
        let env = Env::parse(&vars(&[("SEC_USER_AGENT", placeholder)])).unwrap();
        assert!(env.contact_user_agent().is_none());
        assert_eq!(env.user_agent(), BASE_USER_AGENT);
    }

    #[test]
    fn debug_output_hides_secrets() {
        let env = Env::parse(&vars(&[("ANTHROPIC_API_KEY", "sk-secret")])).unwrap();
        assert!(!format!("{env:?}").contains("sk-secret"));
    }

    #[test]
    fn model_defaults_can_be_overridden() {
        let env = Env::parse(&vars(&[("LLM_FAST_MODEL", "other")])).unwrap();
        assert_eq!(env.llm_fast_model, "other");
        assert_eq!(env.llm_smart_model, DEFAULT_SMART_MODEL);
    }
}
