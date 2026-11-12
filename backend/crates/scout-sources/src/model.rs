use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Yc,
    Rss,
    Formd,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yc => "yc",
            Self::Rss => "rss",
            Self::Formd => "formd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Raised,
    Launched,
    Hiring,
}

impl SignalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raised => "raised",
            Self::Launched => "launched",
            Self::Hiring => "hiring",
        }
    }
}

/// What every source adapter produces.
#[derive(Debug, Clone)]
pub struct RawSignal {
    pub source_type: SourceType,
    pub source_url: String,
    pub company_name: String,
    pub legal_name: Option<String>,
    pub domain: Option<String>,
    pub one_liner: Option<String>,
    pub website: Option<String>,
    pub hiring: Option<bool>,
    pub kind: SignalKind,
    pub title: String,
    pub published_at: DateTime<Utc>,
    pub amount_usd: Option<i64>,
    pub raw: Value,
}
