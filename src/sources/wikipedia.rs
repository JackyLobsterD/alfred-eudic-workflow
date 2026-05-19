use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::sources::util::encode_path_segment;

const BASE_URL: &str = "https://en.wikipedia.org/api/rest_v1/page/summary";

pub struct WikipediaClient {
    http: Arc<Client>,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WikipediaSummary {
    pub title: String,
    pub extract: String,
    pub url: Option<String>,
}

#[derive(Deserialize)]
struct RawSummary {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    extract: String,
    #[serde(default)]
    content_urls: Option<ContentUrls>,
}
#[derive(Deserialize)]
struct ContentUrls { #[serde(default)] desktop: Option<Desktop> }
#[derive(Deserialize)]
struct Desktop { #[serde(default)] page: Option<String> }

impl WikipediaClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// Returns `None` on any error or for disambiguation/no-page results.
    pub async fn fetch(&self, spell: &str) -> Option<WikipediaSummary> {
        let url = format!("{}/{}", self.base_url, encode_path_segment(spell));
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let raw: RawSummary = resp.json().await.ok()?;
        if raw.kind == "disambiguation" || raw.kind == "no-extract" || raw.extract.trim().is_empty() {
            return None;
        }
        Some(WikipediaSummary {
            title: raw.title,
            extract: raw.extract,
            url: raw.content_urls.and_then(|c| c.desktop).and_then(|d| d.page),
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_standard_summary() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/summary/serendipity"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "standard",
                "title": "Serendipity",
                "extract": "Serendipity is an unplanned fortunate discovery.",
                "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/Serendipity"}}
            })))
            .mount(&server).await;
        let c = WikipediaClient::with_base_url(dict_client(), format!("{}/summary", server.uri()));
        let r = c.fetch("serendipity").await.unwrap();
        assert_eq!(r.title, "Serendipity");
        assert!(r.extract.contains("unplanned"));
        assert_eq!(r.url.as_deref(), Some("https://en.wikipedia.org/wiki/Serendipity"));
    }

    #[tokio::test]
    async fn disambiguation_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "disambiguation", "title": "Mercury", "extract": "x"
            })))
            .mount(&server).await;
        let c = WikipediaClient::with_base_url(dict_client(), format!("{}/summary", server.uri()));
        assert!(c.fetch("mercury").await.is_none());
    }

    #[tokio::test]
    async fn http_404_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let c = WikipediaClient::with_base_url(dict_client(), format!("{}/summary", server.uri()));
        assert!(c.fetch("zzzzzz").await.is_none());
    }

    #[tokio::test]
    async fn no_extract_type_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "no-extract", "title": "File:X.svg", "extract": "some residual text"
            })))
            .mount(&server).await;
        let c = WikipediaClient::with_base_url(dict_client(), format!("{}/summary", server.uri()));
        assert!(c.fetch("File:X.svg").await.is_none());
    }
}
