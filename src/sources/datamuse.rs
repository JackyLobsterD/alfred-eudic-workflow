//! Card-only enrichment source. Intentionally NOT a `DictionarySource`
//! (that trait yields a single inline-list definition row); Datamuse
//! returns structured syn/ant/related data consumed by the card via
//! `fetch_json_cached`.

use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://api.datamuse.com/words";

pub struct DatamuseClient {
    http: Arc<Client>,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DatamuseData {
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
    pub related: Vec<String>,
}

#[derive(Deserialize)]
struct Word { word: String }

impl DatamuseClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }

    async fn query(&self, param: &str, spell: &str) -> Vec<String> {
        let resp = match self
            .http
            .get(&self.base_url)
            .query(&[(param, spell), ("max", "12")])
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };
        let words: Vec<Word> = resp.json().await.unwrap_or_default();
        words.into_iter().map(|w| w.word).collect()
    }

    /// Returns `None` if every category is empty (so the block is skipped).
    pub async fn fetch(&self, spell: &str) -> Option<DatamuseData> {
        let (synonyms, antonyms, related) = tokio::join!(
            self.query("rel_syn", spell),
            self.query("rel_ant", spell),
            self.query("ml", spell),
        );
        let data = DatamuseData { synonyms, antonyms, related };
        if data.synonyms.is_empty() && data.antonyms.is_empty() && data.related.is_empty() {
            None
        } else {
            Some(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn collects_syn_ant_related() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(query_param("rel_syn", "happy")).and(query_param("max", "12"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"word":"glad"},{"word":"joyful"}])))
            .mount(&server).await;
        Mock::given(method("GET")).and(query_param("rel_ant", "happy")).and(query_param("max", "12"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"word":"sad"}])))
            .mount(&server).await;
        Mock::given(method("GET")).and(query_param("ml", "happy")).and(query_param("max", "12"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"word":"cheerful"}])))
            .mount(&server).await;
        let c = DatamuseClient::with_base_url(dict_client(), server.uri());
        let d = c.fetch("happy").await.unwrap();
        assert_eq!(d.synonyms, vec!["glad", "joyful"]);
        assert_eq!(d.antonyms, vec!["sad"]);
        assert_eq!(d.related, vec!["cheerful"]);
    }

    #[tokio::test]
    async fn all_empty_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server).await;
        let c = DatamuseClient::with_base_url(dict_client(), server.uri());
        assert!(c.fetch("zzzz").await.is_none());
    }
}
