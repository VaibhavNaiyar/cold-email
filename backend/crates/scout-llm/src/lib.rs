//! Claude Messages API client, prompts and output schemas.
//!
//! There is no official Rust SDK, so this speaks HTTP directly.

mod client;
mod prompts;

pub use client::Llm;
pub use prompts::{
    CompanyClass, CompanyInput, CompanyScore, DomainChoice, EmailInput, EmailSlots, Founder,
    FundingExtraction, JobFit, JobInput, ProfileInput, choose_domain, classify, extract_funding,
    extract_team, score, score_job, write_email,
};
