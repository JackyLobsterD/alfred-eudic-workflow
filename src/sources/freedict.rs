//! Card-only enrichment source. Intentionally NOT a `DictionarySource`
//! (that trait yields a single inline-list definition row); FreeDict
//! returns structured definitions/examples/syn/ant/audio consumed by the
//! card via `fetch_json_cached`.

use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://api.dictionaryapi.dev/api/v2/entries/en";

pub struct FreeDictClient {
    http: Arc<Client>,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FreeDictData {
    pub phonetic: Option<String>,
    pub audio: Option<String>,
    pub origin: Option<String>,
    pub meanings: Vec<FdMeaning>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FdMeaning {
    pub pos: String,
    pub definitions: Vec<String>,
    pub examples: Vec<String>,
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)] phonetic: Option<String>,
    #[serde(default)] phonetics: Vec<RawPhonetic>,
    #[serde(default)] origin: Option<String>,
    #[serde(default)] meanings: Vec<RawMeaning>,
}
#[derive(Deserialize)]
struct RawPhonetic { #[serde(default)] text: Option<String>, #[serde(default)] audio: Option<String> }
#[derive(Deserialize)]
struct RawMeaning {
    #[serde(rename = "partOfSpeech", default)] pos: String,
    #[serde(default)] definitions: Vec<RawDef>,
    #[serde(default)] synonyms: Vec<String>,
    #[serde(default)] antonyms: Vec<String>,
}
#[derive(Deserialize)]
struct RawDef { #[serde(default)] definition: String, #[serde(default)] example: Option<String> }

impl FreeDictClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// `None` on any error or 404 (word not found).
    pub async fn fetch(&self, spell: &str) -> Option<FreeDictData> {
        let url = format!("{}/{}", self.base_url, spell.trim());
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let entries: Vec<RawEntry> = resp.json().await.ok()?;
        let mut data = FreeDictData::default();
        for e in entries {
            if data.phonetic.is_none() {
                data.phonetic = e.phonetic.clone();
            }
            for p in &e.phonetics {
                if data.phonetic.is_none() {
                    data.phonetic = p.text.clone();
                }
                if data.audio.is_none() {
                    if let Some(a) = &p.audio {
                        if !a.is_empty() {
                            data.audio = Some(a.clone());
                        }
                    }
                }
            }
            if data.origin.is_none() {
                data.origin = e.origin.clone();
            }
            for m in e.meanings {
                let mut definitions = Vec::new();
                let mut examples = Vec::new();
                for d in m.definitions {
                    if !d.definition.is_empty() {
                        definitions.push(d.definition);
                    }
                    if let Some(ex) = d.example {
                        if !ex.is_empty() {
                            examples.push(ex);
                        }
                    }
                }
                data.meanings.push(FdMeaning {
                    pos: m.pos,
                    definitions,
                    examples,
                    synonyms: m.synonyms,
                    antonyms: m.antonyms,
                });
            }
        }
        if data.meanings.is_empty() && data.phonetic.is_none() {
            return None;
        }
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_entry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/en/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "phonetic": "/tɛst/",
                "phonetics": [{"text": "/tɛst/", "audio": "https://x/test.mp3"}],
                "origin": "late Middle English",
                "meanings": [{
                    "partOfSpeech": "noun",
                    "definitions": [{"definition": "a procedure", "example": "a blood test"}],
                    "synonyms": ["trial"], "antonyms": []
                }]
            }])))
            .mount(&server).await;
        let c = FreeDictClient::with_base_url(dict_client(), format!("{}/en", server.uri()));
        let d = c.fetch("test").await.unwrap();
        assert_eq!(d.phonetic.as_deref(), Some("/tɛst/"));
        assert_eq!(d.audio.as_deref(), Some("https://x/test.mp3"));
        assert_eq!(d.meanings[0].pos, "noun");
        assert_eq!(d.meanings[0].definitions, vec!["a procedure"]);
        assert_eq!(d.meanings[0].examples, vec!["a blood test"]);
        assert_eq!(d.meanings[0].synonyms, vec!["trial"]);
    }

    #[tokio::test]
    async fn not_found_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let c = FreeDictClient::with_base_url(dict_client(), format!("{}/en", server.uri()));
        assert!(c.fetch("zzzzzz").await.is_none());
    }
}
