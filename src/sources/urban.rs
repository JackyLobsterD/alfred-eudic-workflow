use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{DictEntry, DictionarySource, SourceError, SourceKind};

const BASE_URL: &str = "https://api.urbandictionary.com/v0/define";
const TOP_N: usize = 3;
const MAX_DEF_CHARS: usize = 220;

pub struct UrbanClient {
    http: Arc<Client>,
    base_url: String,
}

impl UrbanClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }
}

#[derive(Deserialize)]
struct UrbanResponse {
    list: Vec<UrbanItem>,
}

#[derive(Deserialize)]
struct UrbanItem {
    definition: String,
    #[serde(default)]
    example: String,
    thumbs_up: i64,
    thumbs_down: i64,
}

fn clean(s: &str) -> String {
    // Urban wraps cross-refs in [brackets]; strip them and collapse whitespace.
    let stripped: String = s.chars().filter(|c| *c != '[' && *c != ']').collect();
    let collapsed: String = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > MAX_DEF_CHARS {
        let truncated: String = collapsed.chars().take(MAX_DEF_CHARS).collect();
        format!("{}…", truncated)
    } else {
        collapsed
    }
}

#[async_trait]
impl DictionarySource for UrbanClient {
    fn kind(&self) -> SourceKind { SourceKind::Urban }

    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError> {
        let resp = self.http.get(&self.base_url).query(&[("term", spell)]).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::RateLimited);
        }
        if !status.is_success() {
            return Err(SourceError::BadResponse(format!("HTTP {}", status)));
        }
        let body: UrbanResponse = resp.json().await
            .map_err(|e| SourceError::BadResponse(e.to_string()))?;
        let mut items = body.list;
        items.sort_by_key(|i| -i.thumbs_up);
        let entries = items.into_iter().take(TOP_N).map(|i| {
            let mut def = clean(&i.definition);
            if !i.example.trim().is_empty() {
                def.push_str("  e.g. ");
                def.push_str(&clean(&i.example));
            }
            DictEntry {
                headword: spell.to_string(),
                definition: def,
                extra: Some(format!("👍 {}  👎 {}", i.thumbs_up, i.thumbs_down)),
            }
        }).collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_response_and_sorts_by_thumbs_up() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("term", "rizz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "list": [
                    {"definition": "low one", "example": "", "thumbs_up": 5, "thumbs_down": 0},
                    {"definition": "high one [charisma]", "example": "He's got rizz", "thumbs_up": 500, "thumbs_down": 3},
                    {"definition": "mid one", "example": "", "thumbs_up": 100, "thumbs_down": 0}
                ]
            })))
            .mount(&server)
            .await;

        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let r = client.fetch("rizz").await.unwrap();
        assert_eq!(r.len(), 3);
        assert!(r[0].definition.starts_with("high one"));
        assert!(r[0].definition.contains("charisma"));
        assert!(!r[0].definition.contains('['));
        assert_eq!(r[0].extra.as_deref(), Some("👍 500  👎 3"));
    }

    #[tokio::test]
    async fn empty_list_returns_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"list": []})))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let r = client.fetch("asdfqwer").await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn http_500_is_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let err = client.fetch("x").await.unwrap_err();
        assert!(matches!(err, SourceError::BadResponse(_)));
    }

    #[tokio::test]
    async fn rate_limit_is_distinct_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let err = client.fetch("x").await.unwrap_err();
        assert!(matches!(err, SourceError::RateLimited));
    }
}
