use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::sources::util::encode_path_segment;

const SUMMARY_BASE: &str = "https://en.wikipedia.org/api/rest_v1/page/summary";
const MEDIA_BASE: &str = "https://en.wikipedia.org/api/rest_v1/page/media-list";

pub struct WikipediaClient {
    http: Arc<Client>,
    summary_base: String,
    media_base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WikipediaSummary {
    pub title: String,
    pub extract: String,
    pub url: Option<String>,
    /// Absolute https URLs of the first few images on the Wikipedia
    /// article, in source order. Empty when the article has none.
    #[serde(default)]
    pub images: Vec<String>,
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

#[derive(Deserialize)]
struct RawMediaList { #[serde(default)] items: Vec<RawMediaItem> }
#[derive(Deserialize)]
struct RawMediaItem {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    srcset: Vec<RawSrc>,
}
#[derive(Deserialize)]
struct RawSrc {
    #[serde(default)]
    src: String,
}

const MAX_IMAGES: usize = 6;

fn normalize_image_url(src: &str) -> Option<String> {
    let s = src.trim();
    if s.is_empty() { return None; }
    if let Some(rest) = s.strip_prefix("//") {
        Some(format!("https://{}", rest))
    } else if s.starts_with("https://") || s.starts_with("http://") {
        Some(s.to_string())
    } else {
        None
    }
}

impl WikipediaClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self {
            http,
            summary_base: SUMMARY_BASE.to_string(),
            media_base: MEDIA_BASE.to_string(),
        }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        // Tests pass one base; derive the media base by sibling-replacing
        // the last path segment.
        let media_base = if base_url.ends_with("/summary") {
            format!("{}/media-list", &base_url[..base_url.len() - "/summary".len()])
        } else {
            base_url.replace("/summary", "/media-list")
        };
        Self { http, summary_base: base_url, media_base }
    }

    /// Returns `None` on any error or for disambiguation/no-page results.
    /// The summary and media-list endpoints are fetched in parallel so a
    /// slow media-list call doesn't extend wall time.
    pub async fn fetch(&self, spell: &str) -> Option<WikipediaSummary> {
        let encoded = encode_path_segment(spell);
        let summary_url = format!("{}/{}", self.summary_base, encoded);
        let media_url = format!("{}/{}", self.media_base, encoded);
        let (summary_resp, media_resp) = tokio::join!(
            self.http.get(&summary_url).send(),
            self.http.get(&media_url).send(),
        );

        // Summary is load-bearing — if it fails, no card section.
        let summary_resp = summary_resp.ok()?;
        if !summary_resp.status().is_success() {
            return None;
        }
        let raw: RawSummary = summary_resp.json().await.ok()?;
        if raw.kind == "disambiguation" || raw.kind == "no-extract" || raw.extract.trim().is_empty() {
            return None;
        }

        // Media list is best-effort.
        let images: Vec<String> = match media_resp {
            Ok(r) if r.status().is_success() => match r.json::<RawMediaList>().await {
                Ok(m) => m
                    .items
                    .into_iter()
                    .filter(|i| i.kind == "image")
                    .filter_map(|i| {
                        i.srcset
                            .into_iter()
                            .find_map(|s| normalize_image_url(&s.src))
                    })
                    .take(MAX_IMAGES)
                    .collect(),
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        };

        Some(WikipediaSummary {
            title: raw.title,
            extract: raw.extract,
            url: raw.content_urls.and_then(|c| c.desktop).and_then(|d| d.page),
            images,
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
