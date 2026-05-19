//! Card-only enrichment source. Intentionally NOT a `DictionarySource`
//! (that trait yields a single inline-list definition row); Wiktionary
//! returns multi-sense English definitions consumed by the card via
//! `fetch_json_cached`.

use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::sources::util::{encode_path_segment, strip_tags};

const BASE_URL: &str = "https://en.wiktionary.org/api/rest_v1/page/definition";

pub struct WiktionaryClient {
    http: Arc<Client>,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WiktionaryData {
    pub senses: Vec<WkSense>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WkSense {
    pub pos: String,
    pub definitions: Vec<String>,
}

#[derive(Deserialize)]
struct RawDef { #[serde(rename = "partOfSpeech", default)] pos: String, #[serde(default)] definitions: Vec<RawSense> }
#[derive(Deserialize)]
struct RawSense { #[serde(default)] definition: String }


impl WiktionaryClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// Only the English (`en`) section is used. `None` on any error.
    pub async fn fetch(&self, spell: &str) -> Option<WiktionaryData> {
        let url = format!("{}/{}", self.base_url, encode_path_segment(spell.trim()));
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let map: std::collections::HashMap<String, Vec<RawDef>> = resp.json().await.ok()?;
        let en = map.get("en")?;
        let mut senses = Vec::new();
        for d in en {
            let defs: Vec<String> = d
                .definitions
                .iter()
                .map(|s| strip_tags(&mark_transclusions(&s.definition)))
                .filter(|s| !s.is_empty())
                .collect();
            if !defs.is_empty() {
                senses.push(WkSense { pos: d.pos.clone(), definitions: defs });
            }
        }
        if senses.is_empty() {
            None
        } else {
            Some(WiktionaryData { senses })
        }
    }
}

/// Wiktionary often wraps context labels (e.g. "(poker slang)") in
/// `<span typeof="mw:Transclusion">` whose actual text is supplied
/// client-side by a MediaWiki template — the REST API returns an empty
/// span. Replace those empty placeholders with a small `(…)` marker
/// so the user can see that a context label is missing and click
/// through to Wiktionary for the full version (rather than seeing a
/// stripped-down definition that looks more authoritative than it is).
fn mark_transclusions(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut rest = s;
    while let Some(pos) = rest.find("<span") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        // Find the end of this opening tag.
        let close = match tail.find('>') {
            Some(c) => c,
            None => { out.push_str(tail); rest = ""; break; }
        };
        let open_tag = &tail[..=close];
        let after_open = &tail[close + 1..];
        if open_tag.contains("mw:Transclusion") {
            // Look for the matching </span>.
            if let Some(end) = after_open.find("</span>") {
                let inner = &after_open[..end];
                if inner.trim().is_empty() {
                    out.push_str("(…)");
                    rest = &after_open[end + "</span>".len()..];
                    continue;
                }
            }
        }
        out.push_str(open_tag);
        rest = after_open;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn mark_transclusions_replaces_empty_template_span() {
        let raw = "<span typeof=\"mw:Transclusion\"></span> <a>Four of a kind</a>.";
        let r = strip_tags(&mark_transclusions(raw));
        assert!(r.starts_with("(…)"), "expected (…) marker, got: {}", r);
        assert!(r.contains("Four of a kind"));
    }

    #[test]
    fn mark_transclusions_preserves_non_template_spans() {
        let raw = "<span class=\"plain\">regular text</span> body";
        let r = strip_tags(&mark_transclusions(raw));
        assert_eq!(r, "regular text body");
    }

    #[tokio::test]
    async fn parses_english_section_and_strips_tags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/definition/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "en": [{"partOfSpeech": "Noun", "definitions": [
                    {"definition": "A challenge, trial."},
                    {"definition": "A <a href='x'>cupel</a>."}
                ]}],
                "fr": [{"partOfSpeech": "Noun", "definitions": [{"definition": "ignore me"}]}]
            })))
            .mount(&server).await;
        let c = WiktionaryClient::with_base_url(dict_client(), format!("{}/definition", server.uri()));
        let d = c.fetch("test").await.unwrap();
        assert_eq!(d.senses.len(), 1);
        assert_eq!(d.senses[0].pos, "Noun");
        assert_eq!(d.senses[0].definitions, vec!["A challenge, trial.", "A cupel."]);
    }

    #[tokio::test]
    async fn no_english_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/definition/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "fr": [{"partOfSpeech": "Noun", "definitions": [{"definition": "x"}]}]
            })))
            .mount(&server).await;
        let c = WiktionaryClient::with_base_url(dict_client(), format!("{}/definition", server.uri()));
        assert!(c.fetch("test").await.is_none());
    }

    #[tokio::test]
    async fn http_404_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let c = WiktionaryClient::with_base_url(dict_client(), format!("{}/definition", server.uri()));
        assert!(c.fetch("zzzz").await.is_none());
    }

    #[tokio::test]
    async fn multiword_query_is_path_encoded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/definition/ad%20hoc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "en": [{"partOfSpeech": "Adverb", "definitions": [{"definition": "for this purpose"}]}]
            })))
            .mount(&server).await;
        let c = WiktionaryClient::with_base_url(dict_client(), format!("{}/definition", server.uri()));
        assert!(c.fetch("ad hoc").await.is_some());
    }
}
