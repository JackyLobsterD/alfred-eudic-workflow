//! Card-only enrichment source. Intentionally NOT a `DictionarySource`
//! (that trait yields a single inline-list definition row); the two M-W
//! clients return structured data consumed by the card via
//! `fetch_json_cached`. Both require an API key; an empty key yields
//! `None` (the source is silently skipped).

use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::sources::util::encode_path_segment;

const LEARNERS_BASE: &str = "https://dictionaryapi.com/api/v3/references/learners/json";
const THESAURUS_BASE: &str = "https://dictionaryapi.com/api/v3/references/thesaurus/json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MwLearnersData {
    pub pos: Option<String>,
    pub phonetic: Option<String>,
    pub audio_url: Option<String>,
    pub short_defs: Vec<String>,
}

/// M-W Thesaurus data preserved in its native sense-grouped shape.
/// Each inner `Vec<String>` is one sub-sense group; the preview layer
/// renders each on its own labelled line so the structure is visible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MwThesaurusData {
    pub synonym_groups: Vec<Vec<String>>,
    pub antonym_groups: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct LearnersEntry {
    #[serde(default)] fl: Option<String>,
    #[serde(default)] hwi: Option<Hwi>,
    #[serde(default)] shortdef: Vec<String>,
}
#[derive(Deserialize)]
struct Hwi { #[serde(default)] prs: Vec<Prs> }
#[derive(Deserialize)]
struct Prs { #[serde(default)] ipa: Option<String>, #[serde(default)] sound: Option<Sound> }
#[derive(Deserialize)]
struct Sound { #[serde(default)] audio: Option<String> }

#[derive(Deserialize)]
struct ThesaurusEntry { #[serde(default)] meta: Option<ThMeta> }
#[derive(Deserialize)]
struct ThMeta {
    #[serde(default)] syns: Vec<Vec<String>>,
    #[serde(default)] ants: Vec<Vec<String>>,
}

/// M-W audio subdirectory rule.
fn mw_audio_url(audio: &str) -> Option<String> {
    if audio.is_empty() {
        return None;
    }
    let subdir = if audio.starts_with("bix") {
        "bix"
    } else if audio.starts_with("gg") {
        "gg"
    } else if audio.chars().next().map(|c| !c.is_ascii_alphabetic()).unwrap_or(true) {
        "number"
    } else {
        &audio[0..1]
    };
    Some(format!(
        "https://media.merriam-webster.com/audio/prons/en/us/mp3/{}/{}.mp3",
        subdir, audio
    ))
}

pub struct MwLearnersClient {
    http: Arc<Client>,
    base_url: String,
    api_key: String,
}

impl MwLearnersClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, base_url: LEARNERS_BASE.to_string(), api_key }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, api_key: String, base_url: String) -> Self {
        Self { http, base_url, api_key }
    }

    /// `None` if no key, on any error, or on a no-match (string array) result.
    pub async fn fetch(&self, spell: &str) -> Option<MwLearnersData> {
        if self.api_key.is_empty() {
            return None;
        }
        let url = format!("{}/{}", self.base_url, encode_path_segment(spell.trim()));
        let resp = self
            .http
            .get(&url)
            .query(&[("key", self.api_key.as_str())])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let entries: Vec<LearnersEntry> = match resp.json().await {
            Ok(v) => v,
            Err(_) => return None, // no-match returns ["sug","gest"] -> type mismatch -> None
        };
        let e = entries.into_iter().next()?;
        let prs = e.hwi.and_then(|h| h.prs.into_iter().next());
        let (phonetic, audio_url) = match prs {
            Some(p) => (
                p.ipa,
                p.sound.and_then(|s| s.audio).and_then(|a| mw_audio_url(&a)),
            ),
            None => (None, None),
        };
        let data = MwLearnersData { pos: e.fl, phonetic, audio_url, short_defs: e.shortdef.iter().map(|d| crate::sources::util::strip_tags(d)).collect() };
        if data.short_defs.is_empty() && data.phonetic.is_none() {
            None
        } else {
            Some(data)
        }
    }
}

pub struct MwThesaurusClient {
    http: Arc<Client>,
    base_url: String,
    api_key: String,
}

impl MwThesaurusClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, base_url: THESAURUS_BASE.to_string(), api_key }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, api_key: String, base_url: String) -> Self {
        Self { http, base_url, api_key }
    }

    pub async fn fetch(&self, spell: &str) -> Option<MwThesaurusData> {
        if self.api_key.is_empty() {
            return None;
        }
        let url = format!("{}/{}", self.base_url, encode_path_segment(spell.trim()));
        let resp = self
            .http
            .get(&url)
            .query(&[("key", self.api_key.as_str())])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let entries: Vec<ThesaurusEntry> = match resp.json().await {
            Ok(v) => v,
            Err(_) => return None,
        };
        // Preserve M-W's sense-grouped structure (one Vec per sub-sense).
        // Empty inner groups are dropped; the preview renders each group
        // as a separate labelled line so the user can see the structure.
        let mut synonym_groups: Vec<Vec<String>> = Vec::new();
        let mut antonym_groups: Vec<Vec<String>> = Vec::new();
        for e in entries {
            if let Some(m) = e.meta {
                for g in m.syns {
                    if !g.is_empty() {
                        synonym_groups.push(g);
                    }
                }
                for g in m.ants {
                    if !g.is_empty() {
                        antonym_groups.push(g);
                    }
                }
            }
        }
        if synonym_groups.is_empty() && antonym_groups.is_empty() {
            None
        } else {
            Some(MwThesaurusData { synonym_groups, antonym_groups })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn audio_subdir_rule() {
        assert_eq!(mw_audio_url("test0001").unwrap(), "https://media.merriam-webster.com/audio/prons/en/us/mp3/t/test0001.mp3");
        assert_eq!(mw_audio_url("bixxx").unwrap(), "https://media.merriam-webster.com/audio/prons/en/us/mp3/bix/bixxx.mp3");
        assert_eq!(mw_audio_url("_3test").unwrap(), "https://media.merriam-webster.com/audio/prons/en/us/mp3/number/_3test.mp3");
        assert_eq!(mw_audio_url("3d").unwrap(), "https://media.merriam-webster.com/audio/prons/en/us/mp3/number/3d.mp3");
        assert!(mw_audio_url("").is_none());
    }

    #[tokio::test]
    async fn learners_parses_entry() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/learners/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "fl": "noun",
                "hwi": {"hw": "test", "prs": [{"ipa": "ˈtɛst", "sound": {"audio": "test0001"}}]},
                "shortdef": ["a set of questions", "a careful study"]
            }])))
            .mount(&server).await;
        let c = MwLearnersClient::with_base_url(dict_client(), "k".into(), format!("{}/learners", server.uri()));
        let d = c.fetch("test").await.unwrap();
        assert_eq!(d.pos.as_deref(), Some("noun"));
        assert_eq!(d.phonetic.as_deref(), Some("ˈtɛst"));
        assert!(d.audio_url.unwrap().ends_with("/t/test0001.mp3"));
        assert_eq!(d.short_defs.len(), 2);
    }

    #[tokio::test]
    async fn learners_no_key_is_none() {
        let c = MwLearnersClient::with_base_url(dict_client(), String::new(), "http://unused".into());
        assert!(c.fetch("test").await.is_none());
    }

    #[tokio::test]
    async fn learners_no_match_string_array_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/learners/tesst"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["tester", "tested"])))
            .mount(&server).await;
        let c = MwLearnersClient::with_base_url(dict_client(), "k".into(), format!("{}/learners", server.uri()));
        assert!(c.fetch("tesst").await.is_none());
    }

    #[tokio::test]
    async fn thesaurus_collects_syns_ants() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/thes/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "meta": {"syns": [["essay", "experiment"], ["exam"]], "ants": [["proof"]]}
            }])))
            .mount(&server).await;
        let c = MwThesaurusClient::with_base_url(dict_client(), "k".into(), format!("{}/thes", server.uri()));
        let d = c.fetch("test").await.unwrap();
        // Sense-grouped structure is preserved — every group survives.
        assert_eq!(d.synonym_groups, vec![vec!["essay".to_string(), "experiment".into()], vec!["exam".into()]]);
        assert_eq!(d.antonym_groups, vec![vec!["proof".to_string()]]);
    }

    #[tokio::test]
    async fn thesaurus_keeps_all_sense_groups_across_entries() {
        // Two entries, each with multiple sense-groups: every non-empty
        // group is kept in source order.
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/thes/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"meta": {"syns": [["a", "b"], ["c", "d"]], "ants": [["bad"], ["worse"]]}},
                {"meta": {"syns": [["e"], ["f", "g"], []], "ants": []}}
            ])))
            .mount(&server).await;
        let c = MwThesaurusClient::with_base_url(dict_client(), "k".into(), format!("{}/thes", server.uri()));
        let d = c.fetch("x").await.unwrap();
        assert_eq!(
            d.synonym_groups,
            vec![
                vec!["a".to_string(), "b".into()],
                vec!["c".into(), "d".into()],
                vec!["e".into()],
                vec!["f".into(), "g".into()],
            ],
            "empty groups dropped; non-empty groups preserved in order"
        );
        assert_eq!(d.antonym_groups, vec![vec!["bad".to_string()], vec!["worse".into()]]);
    }

    #[tokio::test]
    async fn thesaurus_no_key_is_none() {
        let c = MwThesaurusClient::with_base_url(dict_client(), String::new(), "http://unused".into());
        assert!(c.fetch("test").await.is_none());
    }
}
