use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{DictEntry, DictionarySource, SourceError, SourceKind};

const BASE_URL: &str = "https://api.wordnik.com/v4/word.json";
const SOURCE_DICTS: &str = "ahd-5,wiktionary,wordnet";
const FETCH_LIMIT: u32 = 10;

pub struct WordnikClient {
    http: Arc<Client>,
    api_key: String,
    base_url: String,
}

impl WordnikClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, api_key, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, api_key: String, base_url: String) -> Self {
        Self { http, api_key, base_url }
    }
}

#[derive(Deserialize)]
struct WordnikDef {
    #[serde(default)]
    text: Option<String>,
    #[serde(rename = "partOfSpeech", default)]
    part_of_speech: Option<String>,
    #[serde(rename = "sourceDictionary", default)]
    source_dictionary: Option<String>,
}

#[async_trait]
impl DictionarySource for WordnikClient {
    fn kind(&self) -> SourceKind { SourceKind::Wordnik }

    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError> {
        if self.api_key.is_empty() {
            return Err(SourceError::NoApiKey);
        }
        // URL-encode the word into the path segment.
        let encoded: String = spell
            .chars()
            .flat_map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '\'' {
                    vec![c]
                } else if c == ' ' {
                    vec!['+']
                } else {
                    // percent-encode other bytes
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    s.bytes().flat_map(|b| format!("%{:02X}", b).chars().collect::<Vec<_>>()).collect()
                }
            })
            .collect();
        let url = format!("{}/{}/definitions", self.base_url, encoded);
        let resp = self.http.get(&url)
            .query(&[
                ("limit", FETCH_LIMIT.to_string()),
                ("includeRelated", "false".to_string()),
                ("sourceDictionaries", SOURCE_DICTS.to_string()),
                ("useCanonical", "true".to_string()),
                ("api_key", self.api_key.clone()),
            ])
            .send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::RateLimited);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(SourceError::BadResponse(format!("HTTP {}", status)));
        }
        let defs: Vec<WordnikDef> = resp.json().await
            .map_err(|e| SourceError::BadResponse(e.to_string()))?;
        let entries = defs.into_iter().filter_map(|d| {
            let text = d.text?.trim().to_string();
            if text.is_empty() { return None; }
            let pos = d.part_of_speech.unwrap_or_default();
            let src = d.source_dictionary.unwrap_or_default();
            let extra = match (pos.is_empty(), src.is_empty()) {
                (false, false) => Some(format!("{} · {}", pos, src)),
                (false, true)  => Some(pos),
                (true, false)  => Some(src),
                (true, true)   => None,
            };
            Some(DictEntry {
                headword: spell.to_string(),
                definition: text,
                extra,
            })
        }).collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_definitions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/word.json/serendipity/definitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"text": "good luck in making unexpected discoveries", "partOfSpeech": "noun", "sourceDictionary": "ahd-5"},
                {"text": "the fact of finding interesting things by chance", "partOfSpeech": "noun", "sourceDictionary": "wiktionary"}
            ])))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "test-key".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("serendipity").await.unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[0].definition.contains("good luck"));
        assert_eq!(r[0].extra.as_deref(), Some("noun · ahd-5"));
    }

    #[tokio::test]
    async fn no_api_key_returns_error() {
        let client = WordnikClient::new(dict_client(), "".to_string());
        assert!(matches!(client.fetch("x").await, Err(SourceError::NoApiKey)));
    }

    #[tokio::test]
    async fn http_404_returns_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "test-key".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("asdfqwer").await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn drops_entries_with_empty_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"text": "good"},
                {"text": ""},
                {"text": "   "}
            ])))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "k".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("x").await.unwrap();
        assert_eq!(r.len(), 1);
    }
}
