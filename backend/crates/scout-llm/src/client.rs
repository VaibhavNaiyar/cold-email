use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Clone)]
pub struct Llm {
    http: reqwest::Client,
    api_key: String,
    pub fast_model: String,
    pub smart_model: String,
}

#[derive(Deserialize)]
struct MessageResponse {
    stop_reason: Option<String>,
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl Llm {
    pub fn new(http: reqwest::Client, api_key: &str, fast_model: &str, smart_model: &str) -> Self {
        Self {
            http,
            api_key: api_key.to_owned(),
            fast_model: fast_model.to_owned(),
            smart_model: smart_model.to_owned(),
        }
    }

    /// Calls `model` with a JSON schema and deserializes the constrained reply.
    pub(crate) async fn structured<T: DeserializeOwned>(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<T> {
        let mut body = json!({
            "model": model,
            "max_tokens": 2048,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            "output_config": {
                "effort": "low",
                "format": { "type": "json_schema", "schema": schema },
            },
        });
        // Haiku has no server-side refusal fallback; Sonnet takes the default route.
        if model == self.smart_model {
            body["fallbacks"] = json!("default");
        }
        parse_reply(self.send(&body).await?)
    }

    async fn send(&self, body: &Value) -> Result<MessageResponse> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = self
                .http
                .post(MESSAGES_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", API_VERSION)
                .header("anthropic-beta", FALLBACK_BETA)
                .json(body)
                .send()
                .await;

            let retryable = match result {
                Ok(response) if response.status().is_success() => {
                    return response.json().await.context("Unreadable Claude response");
                }
                Ok(response) => {
                    let status = response.status();
                    let detail = response.text().await.unwrap_or_default();
                    if !is_retryable(status) {
                        bail!("Claude API returned {status}: {detail}");
                    }
                    anyhow!("Claude API returned {status}: {detail}")
                }
                Err(error) => anyhow!(error).context("Could not reach the Claude API"),
            };
            if attempt >= MAX_ATTEMPTS {
                return Err(retryable);
            }
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
        }
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn parse_reply<T: DeserializeOwned>(reply: MessageResponse) -> Result<T> {
    match reply.stop_reason.as_deref() {
        Some("end_turn") => {}
        other => bail!("Claude stopped early: {other:?}"),
    }
    let text = reply
        .content
        .iter()
        .find(|block| block.kind == "text")
        .and_then(|block| block.text.as_deref())
        .context("Claude reply had no text")?;
    serde_json::from_str(text).context("Claude reply did not match the schema")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(stop_reason: &str, text: &str) -> MessageResponse {
        MessageResponse {
            stop_reason: Some(stop_reason.into()),
            content: vec![
                ContentBlock { kind: "thinking".into(), text: None },
                ContentBlock { kind: "text".into(), text: Some(text.into()) },
            ],
        }
    }

    #[test]
    fn reads_the_text_block_after_thinking() {
        let value: Value = parse_reply(reply("end_turn", r#"{"ok":true}"#)).unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn rejects_refusals_and_truncated_replies() {
        assert!(parse_reply::<Value>(reply("refusal", "{}")).is_err());
        assert!(parse_reply::<Value>(reply("max_tokens", "{")).is_err());
    }
}
