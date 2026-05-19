use std::fmt;
use std::sync::Arc;

use reqwest::Client;
use serde::Deserialize;

use crate::cache::{Cache, CacheKind};

pub mod prompt;
pub mod response;

pub use response::LlmResult;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-haiku-4-5";
// Room for 1-3 Chinese translations plus 6 English example sentences
// (~20 words each), with headroom for the JSON envelope.
const MAX_TOKENS: u32 = 1000;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct LlmClient {
    http: Arc<Client>,
    api_key: String,
    endpoint: String,
}

impl LlmClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, api_key, endpoint: ENDPOINT.to_string() }
    }

    #[doc(hidden)]
    pub fn with_endpoint(http: Arc<Client>, api_key: String, endpoint: String) -> Self {
        Self { http, api_key, endpoint }
    }

    pub async fn translate(&self, spell: &str) -> Result<LlmResult, LlmError> {
        if self.api_key.is_empty() {
            return Err(LlmError::NoApiKey);
        }
        let body = serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "system": prompt::SYSTEM,
            "messages": [{"role": "user", "content": prompt::user(spell)}]
        });
        let resp = self.http.post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send().await
            .map_err(LlmError::from)?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status: status.as_u16(), body: body_text });
        }
        let api_resp: AnthropicResponse = resp.json().await
            .map_err(|e| LlmError::BadJson(format!("envelope: {}", e)))?;
        if api_resp.stop_reason.as_deref() == Some("refusal") {
            return Err(LlmError::Refused);
        }
        let text = api_resp.content.iter()
            .find_map(|c| if c.kind == "text" { Some(c.text.as_str()) } else { None })
            .ok_or_else(|| LlmError::BadJson("no text block".to_string()))?;
        response::parse_llm_json(text).map_err(LlmError::BadJson)
    }
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    stop_reason: Option<String>,
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug)]
pub enum LlmError {
    Http(String),
    Timeout,
    BadJson(String),
    ApiError { status: u16, body: String },
    Refused,
    NoApiKey,
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::Http(e) => write!(f, "HTTP error: {}", e),
            LlmError::Timeout => write!(f, "request timeout"),
            LlmError::BadJson(s) => write!(f, "bad JSON: {}", s),
            LlmError::ApiError { status, .. } => write!(f, "API error {}", status),
            LlmError::Refused => write!(f, "refused by model"),
            LlmError::NoApiKey => write!(f, "no API key configured"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<reqwest::Error> for LlmError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() { LlmError::Timeout } else { LlmError::Http(e.to_string()) }
    }
}

/// Cache-aware LLM call. Only caches successful results.
pub async fn fetch_with_cache_llm(
    client: &LlmClient,
    cache: Arc<dyn Cache>,
    spell: &str,
    bypass: bool,
) -> Result<LlmResult, LlmError> {
    let key = spell.trim().to_lowercase();
    if !bypass {
        if let Some(bytes) = cache.get(CacheKind::Llm, &key).await {
            if let Ok(result) = serde_json::from_slice::<LlmResult>(&bytes) {
                return Ok(result);
            }
        }
    }
    let result = client.translate(spell).await?;
    if let Ok(bytes) = serde_json::to_vec(&result) {
        cache.put(CacheKind::Llm, &key, &bytes).await;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::llm_client;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ok_envelope(text: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "msg_1",
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": text}]
        })
    }

    #[tokio::test]
    async fn happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                ok_envelope(r#"{"translations":["机缘"],"examples":[{"scenario":"casual","sentence":"What serendipity!"}]}"#)
            ))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        let r = client.translate("serendipity").await.unwrap();
        assert_eq!(r.translations, vec!["机缘"]);
        assert_eq!(r.examples.len(), 1);
        assert_eq!(r.examples[0].sentence, "What serendipity!");
    }

    #[tokio::test]
    async fn no_api_key() {
        let client = LlmClient::new(llm_client(), "".into());
        assert!(matches!(client.translate("x").await, Err(LlmError::NoApiKey)));
    }

    #[tokio::test]
    async fn non_json_text_is_bad_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope("I don't know this word")))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        assert!(matches!(client.translate("x").await, Err(LlmError::BadJson(_))));
    }

    #[tokio::test]
    async fn refusal_stop_reason() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "stop_reason": "refusal",
                "content": [{"type": "text", "text": ""}]
            })))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        assert!(matches!(client.translate("x").await, Err(LlmError::Refused)));
    }

    #[tokio::test]
    async fn http_400_is_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid_request"))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        let err = client.translate("x").await.unwrap_err();
        match err {
            LlmError::ApiError { status, .. } => assert_eq!(status, 400),
            _ => panic!("expected ApiError"),
        }
    }
}
