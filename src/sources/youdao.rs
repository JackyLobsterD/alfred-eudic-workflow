//! Card-only enrichment source. Intentionally NOT a `DictionarySource`
//! (that trait yields a single inline-list definition row); Youdao's
//! unofficial jsonapi returns a large structured payload consumed by the
//! card via `fetch_json_cached`. Parsed defensively: every field is
//! optional and unknown shapes are ignored.

use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BASE_URL: &str = "https://dict.youdao.com/jsonapi";

pub struct YoudaoClient {
    http: Arc<Client>,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct YoudaoData {
    pub ec_zh: Vec<String>,
    pub ee: Vec<String>,
    pub syno: Vec<String>,
    pub rel_word: Vec<String>,
    pub phrs: Vec<String>,
    pub sents: Vec<(String, String)>,
    pub web_trans: Vec<String>,
    pub wiki: Option<String>,
    pub etym: Option<String>,
}

fn s(v: &Value, p: &str) -> Option<String> {
    let t = v.pointer(p)?.as_str()?.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

fn ne(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

impl YoudaoClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }

    /// Unofficial endpoint — parse defensively, `None` on any error/empty.
    pub async fn fetch(&self, spell: &str) -> Option<YoudaoData> {
        let resp = self
            .http
            .get(&self.base_url)
            .query(&[("q", spell.trim())])
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        let mut d = YoudaoData::default();

        if let Some(trs) = v.pointer("/ec/word/0/trs").and_then(|x| x.as_array()) {
            for t in trs {
                if let Some(line) = t.pointer("/tr/0/l/i/0").and_then(|x| x.as_str()) {
                    if let Some(x) = ne(line) { d.ec_zh.push(x); }
                }
            }
        }
        if let Some(trs) = v.pointer("/ee/word/trs").and_then(|x| x.as_array()) {
            for t in trs {
                let pos = t.pointer("/pos").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(tr) = t.pointer("/tr/0/l/i").and_then(|x| x.as_str()) {
                    let line = if pos.is_empty() { tr.to_string() } else { format!("{} {}", pos, tr) };
                    if let Some(x) = ne(&line) { d.ee.push(x); }
                }
            }
        }
        if let Some(arr) = v.pointer("/syno/synos").and_then(|x| x.as_array()) {
            for g in arr {
                let pos = g.pointer("/syno/pos").and_then(|x| x.as_str()).unwrap_or("");
                let ws: Vec<String> = g
                    .pointer("/syno/ws")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|w| w.pointer("/w").and_then(|x| x.as_str()).map(String::from)).collect())
                    .unwrap_or_default();
                if !ws.is_empty() {
                    if let Some(x) = ne(&format!("{} {}", pos, ws.join(", "))) { d.syno.push(x); }
                }
            }
        }
        if let Some(rels) = v.pointer("/rel_word/rels").and_then(|x| x.as_array()) {
            for r in rels {
                let pos = r.pointer("/rel/pos").and_then(|x| x.as_str()).unwrap_or("");
                let ws: Vec<String> = r
                    .pointer("/rel/words")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|w| w.pointer("/word").and_then(|x| x.as_str()).map(String::from)).collect())
                    .unwrap_or_default();
                if !ws.is_empty() {
                    if let Some(x) = ne(&format!("{} {}", pos, ws.join(", "))) { d.rel_word.push(x); }
                }
            }
        }
        if let Some(arr) = v.pointer("/phrs/phrs").and_then(|x| x.as_array()) {
            for p in arr {
                let head = p.pointer("/phr/headword/l/i").and_then(|x| x.as_str()).unwrap_or("");
                let tr = p.pointer("/phr/trs/0/tr/l/i").and_then(|x| x.as_str()).unwrap_or("");
                if !head.trim().is_empty() {
                    let line = if tr.trim().is_empty() {
                        head.trim().to_string()
                    } else {
                        format!("{} — {}", head.trim(), tr.trim())
                    };
                    d.phrs.push(line);
                }
            }
        }
        if let Some(arr) = v.pointer("/blng_sents_part/sentence-pair").and_then(|x| x.as_array()) {
            for sp in arr {
                let en = sp.pointer("/sentence").and_then(|x| x.as_str()).unwrap_or("");
                let zh = sp.pointer("/sentence-translation").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(en2) = ne(en) {
                    d.sents.push((en2, zh.trim().to_string()));
                }
            }
        }
        if let Some(arr) = v.pointer("/web_trans/web-translation").and_then(|x| x.as_array()) {
            for w in arr {
                if let Some(val) = w.pointer("/trans/0/value").and_then(|x| x.as_str()) {
                    if let Some(x) = ne(val) { d.web_trans.push(x); }
                }
            }
        }
        d.wiki = s(&v, "/wikipedia_digest/summarys/0/summary")
            .or_else(|| s(&v, "/wikipedia_digest/summary/0/summary"));
        d.etym = s(&v, "/etym/etyms/zh/0/value").or_else(|| s(&v, "/etym/etyms/0/value"));

        let empty = d.ec_zh.is_empty() && d.ee.is_empty() && d.syno.is_empty()
            && d.rel_word.is_empty() && d.phrs.is_empty() && d.sents.is_empty()
            && d.web_trans.is_empty() && d.wiki.is_none() && d.etym.is_none();
        if empty { None } else { Some(d) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_core_fields() {
        let body = serde_json::json!({
            "ec": {"word": [{"trs": [{"tr": [{"l": {"i": ["n. 测试"]}}]}]}]},
            "ee": {"word": {"trs": [{"pos": "noun", "tr": [{"l": {"i": "a procedure"}}]}]}},
            "syno": {"synos": [{"syno": {"pos": "n.", "ws": [{"w": "trial"}, {"w": "exam"}]}}]},
            "phrs": {"phrs": [{"phr": {"headword": {"l": {"i": "acid test"}}, "trs": [{"tr": {"l": {"i": "严峻考验"}}}]}}]},
            "blng_sents_part": {"sentence-pair": [{"sentence": "Run the test.", "sentence-translation": "运行测试。"}]},
            "web_trans": {"web-translation": [{"trans": [{"value": "测试；检验"}]}]},
            "wikipedia_digest": {"summarys": [{"summary": "A test is an assessment."}]},
            "etym": {"etyms": {"zh": [{"value": "源自拉丁语"}]}}
        });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server).await;
        let c = YoudaoClient::with_base_url(dict_client(), server.uri());
        let d = c.fetch("test").await.unwrap();
        assert_eq!(d.ec_zh, vec!["n. 测试"]);
        assert_eq!(d.ee, vec!["noun a procedure"]);
        assert_eq!(d.syno, vec!["n. trial, exam"]);
        assert_eq!(d.phrs, vec!["acid test — 严峻考验"]);
        assert_eq!(d.sents, vec![("Run the test.".into(), "运行测试。".into())]);
        assert_eq!(d.web_trans, vec!["测试；检验"]);
        assert_eq!(d.wiki.as_deref(), Some("A test is an assessment."));
        assert_eq!(d.etym.as_deref(), Some("源自拉丁语"));
    }

    #[tokio::test]
    async fn empty_body_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server).await;
        let c = YoudaoClient::with_base_url(dict_client(), server.uri());
        assert!(c.fetch("zzzz").await.is_none());
    }

    #[tokio::test]
    async fn http_error_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&server).await;
        let c = YoudaoClient::with_base_url(dict_client(), server.uri());
        assert!(c.fetch("x").await.is_none());
    }

    #[tokio::test]
    async fn whitespace_only_values_are_dropped() {
        let body = serde_json::json!({
            "ec": {"word": [{"trs": [{"tr": [{"l": {"i": ["   "]}}]}]}]},
            "web_trans": {"web-translation": [{"trans": [{"value": "  "}]}]}
        });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server).await;
        let c = YoudaoClient::with_base_url(dict_client(), server.uri());
        // ec_zh and web_trans would be whitespace-only -> dropped -> all empty -> None
        assert!(c.fetch("x").await.is_none());
    }

    #[tokio::test]
    async fn phrase_without_translation_is_headword_only() {
        let body = serde_json::json!({
            "phrs": {"phrs": [{"phr": {"headword": {"l": {"i": "acid test"}}, "trs": []}}]}
        });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server).await;
        let c = YoudaoClient::with_base_url(dict_client(), server.uri());
        let d = c.fetch("x").await.unwrap();
        assert_eq!(d.phrs, vec!["acid test"]);
    }
}
