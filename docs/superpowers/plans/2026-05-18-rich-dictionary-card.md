# Rich Dictionary Quick Look Card — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aggregate many free dictionary sources into one richly-sectioned Quick Look card (Shift / ⌘Y), leaving the Alfred dropdown list unchanged.

**Architecture:** Each new network source is a standalone client (not a `DictionarySource`) returning a typed, all-optional struct, fetched in parallel and cached via a generic JSON cache helper. An aggregator (`card.rs`) gathers them; `preview.rs` is rewritten to render 10 priority-ordered blocks. The orchestrator passes existing ECDICT/Wordnik/Urban/LLM results plus the new aggregate to the preview builder.

**Tech Stack:** Rust 2024, tokio, reqwest (rustls), serde/serde_json, rusqlite, wiremock (dev), async-trait.

Spec: `docs/superpowers/specs/2026-05-18-rich-dictionary-card-design.md`

---

## File Structure

- Create `src/sources/youdao.rs` — Youdao jsonapi client + `YoudaoData`.
- Create `src/sources/wikipedia.rs` — official Wikipedia summary client.
- Create `src/sources/datamuse.rs` — Datamuse syn/ant/related client.
- Create `src/sources/wiktionary.rs` — Wiktionary REST client.
- Create `src/sources/freedict.rs` — dictionaryapi.dev client.
- Create `src/sources/mw.rs` — Merriam-Webster Learner's + Thesaurus clients.
- Create `src/card.rs` — `CardSources` aggregate + `gather_card_data`.
- Modify `src/cache/mod.rs` — add 7 `CacheKind` variants + `table()`.
- Modify `src/cache/sqlite.rs:34` — extend migrate loop.
- Modify `src/sources/mod.rs` — add generic `fetch_json_cached` helper; declare new modules.
- Rewrite `src/preview.rs` — new signature + 10-block renderer.
- Modify `src/command/search.rs` — gather card sources, pass to preview.
- Modify `src/lib.rs` — `pub mod card;`.
- Modify `info.plist` — 2 optional M-W key config fields.
- Create `tests/card_integration.rs` — wiremock end-to-end.

Conventions to follow (from existing code): client struct holds `Arc<Client>` + `base_url`; `new(http)` uses the real base const; `#[doc(hidden)] pub fn with_base_url(http, base)` is the wiremock seam; HTTP via `crate::http::dict_client()`; graceful = return `None`/empty, never panic; cache key = `spell.trim().to_lowercase()`.

---

## Task 1: Cache plumbing for new sources

**Files:**
- Modify: `src/cache/mod.rs:8-22`
- Modify: `src/cache/sqlite.rs:34`

- [ ] **Step 1: Extend `CacheKind` and `table()`**

In `src/cache/mod.rs` replace the enum and impl:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Urban,
    Wordnik,
    Llm,
    Youdao,
    Wikipedia,
    Datamuse,
    Wiktionary,
    FreeDict,
    MwLearners,
    MwThesaurus,
}

impl CacheKind {
    pub fn table(self) -> &'static str {
        match self {
            CacheKind::Urban => "cache_urban",
            CacheKind::Wordnik => "cache_wordnik",
            CacheKind::Llm => "cache_llm",
            CacheKind::Youdao => "cache_youdao",
            CacheKind::Wikipedia => "cache_wikipedia",
            CacheKind::Datamuse => "cache_datamuse",
            CacheKind::Wiktionary => "cache_wiktionary",
            CacheKind::FreeDict => "cache_freedict",
            CacheKind::MwLearners => "cache_mw_learners",
            CacheKind::MwThesaurus => "cache_mw_thesaurus",
        }
    }

    /// All variants, used by cache migration.
    pub fn all() -> [CacheKind; 10] {
        [
            CacheKind::Urban, CacheKind::Wordnik, CacheKind::Llm,
            CacheKind::Youdao, CacheKind::Wikipedia, CacheKind::Datamuse,
            CacheKind::Wiktionary, CacheKind::FreeDict,
            CacheKind::MwLearners, CacheKind::MwThesaurus,
        ]
    }
}
```

- [ ] **Step 2: Use `all()` in migrate**

In `src/cache/sqlite.rs` line 34 change:

```rust
for kind in CacheKind::all() {
```

- [ ] **Step 3: Run cache tests**

Run: `cargo test --lib cache:: -- --nocolor`
Expected: PASS (existing 5 cache tests still green).

- [ ] **Step 4: Commit**

```bash
git add src/cache/mod.rs src/cache/sqlite.rs
git commit -m "feat(cache): add CacheKind variants for rich-card sources"
```

---

## Task 2: Generic JSON cache helper

**Files:**
- Modify: `src/sources/mod.rs` (add helper + declare modules)

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `src/sources/mod.rs`:

```rust
#[tokio::test]
async fn json_cache_caches_some_and_skips_refetch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Clone)]
    struct D { v: u32 }
    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let calls = AtomicUsize::new(0);
    let mk = || async { calls.fetch_add(1, Ordering::SeqCst); Some(D { v: 7 }) };
    let a = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Youdao, "Hi", false, mk).await;
    let b = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Youdao, "hi", false, mk).await;
    assert_eq!(a, Some(D { v: 7 }));
    assert_eq!(b, Some(D { v: 7 }));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "second call must hit cache");
}

#[tokio::test]
async fn json_cache_does_not_cache_none() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct D { v: u32 }
    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let calls = AtomicUsize::new(0);
    let mk = || async { calls.fetch_add(1, Ordering::SeqCst); None::<D> };
    let _ = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Datamuse, "x", false, mk).await;
    let _ = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Datamuse, "x", false, mk).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "None results are never cached");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib sources::tests::json_cache -- --nocolor`
Expected: FAIL — `fetch_json_cached` not found.

- [ ] **Step 3: Implement the helper**

Add to the top imports of `src/sources/mod.rs`:

```rust
use serde::de::DeserializeOwned;
use std::future::Future;
```

Add the modules below the existing `pub mod` lines:

```rust
pub mod youdao;
pub mod wikipedia;
pub mod datamuse;
pub mod wiktionary;
pub mod freedict;
pub mod mw;
```

Add the helper after `fetch_with_cache`:

```rust
/// Cache-aware fetch for card sources. The fetcher returns `Option<T>`;
/// only `Some` is cached. Any error inside the fetcher must surface as
/// `None` so the card simply omits that block.
pub async fn fetch_json_cached<T, F, Fut>(
    cache: Arc<dyn Cache>,
    kind: CacheKind,
    spell: &str,
    bypass: bool,
    fetch: F,
) -> Option<T>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let key = spell.trim().to_lowercase();
    if !bypass {
        if let Some(bytes) = cache.get(kind, &key).await {
            if let Ok(v) = serde_json::from_slice::<T>(&bytes) {
                return Some(v);
            }
        }
    }
    let value = fetch().await?;
    if let Ok(bytes) = serde_json::to_vec(&value) {
        cache.put(kind, &key, &bytes).await;
    }
    Some(value)
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --lib sources::tests::json_cache -- --nocolor`
Expected: PASS (2 tests). Module-not-found errors for the new `pub mod` lines are expected here — Task 3+ create them; to keep this task green, temporarily create empty files: `touch src/sources/youdao.rs src/sources/wikipedia.rs src/sources/datamuse.rs src/sources/wiktionary.rs src/sources/freedict.rs src/sources/mw.rs` (they will be filled in subsequent tasks; an empty file is a valid empty module).

- [ ] **Step 5: Commit**

```bash
git add src/sources/mod.rs src/sources/youdao.rs src/sources/wikipedia.rs src/sources/datamuse.rs src/sources/wiktionary.rs src/sources/freedict.rs src/sources/mw.rs
git commit -m "feat(sources): generic JSON cache helper + module stubs"
```

---

## Task 3: Wikipedia source

**Files:**
- Create: `src/sources/wikipedia.rs`

- [ ] **Step 1: Write the failing test + implementation together** (file is currently an empty stub)

Write `src/sources/wikipedia.rs`:

```rust
use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

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
        let url = format!("{}/{}", self.base_url, urlencoding(spell));
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let raw: RawSummary = resp.json().await.ok()?;
        if raw.kind == "disambiguation" || raw.extract.trim().is_empty() {
            return None;
        }
        Some(WikipediaSummary {
            title: raw.title,
            extract: raw.extract,
            url: raw.content_urls.and_then(|c| c.desktop).and_then(|d| d.page),
        })
    }
}

/// Minimal path-segment encoding (space → %20, etc.). Wikipedia accepts
/// underscores too but percent-encoding is safest.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", c));
        }
    }
    out
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
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::wikipedia -- --nocolor`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add src/sources/wikipedia.rs
git commit -m "feat(sources): official Wikipedia summary client"
```

---

## Task 4: Datamuse source

**Files:**
- Create: `src/sources/datamuse.rs`

- [ ] **Step 1: Write file with tests**

Write `src/sources/datamuse.rs`:

```rust
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
        Mock::given(method("GET")).and(query_param("rel_syn", "happy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"word":"glad"},{"word":"joyful"}])))
            .mount(&server).await;
        Mock::given(method("GET")).and(query_param("rel_ant", "happy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"word":"sad"}])))
            .mount(&server).await;
        Mock::given(method("GET")).and(query_param("ml", "happy"))
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::datamuse -- --nocolor`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add src/sources/datamuse.rs
git commit -m "feat(sources): Datamuse syn/ant/related client"
```

---

## Task 5: Free Dictionary API source

**Files:**
- Create: `src/sources/freedict.rs`

- [ ] **Step 1: Write file with tests**

Write `src/sources/freedict.rs`:

```rust
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::freedict -- --nocolor`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add src/sources/freedict.rs
git commit -m "feat(sources): Free Dictionary API client"
```

---

## Task 6: Wiktionary source

**Files:**
- Create: `src/sources/wiktionary.rs`

- [ ] **Step 1: Write file with tests**

Write `src/sources/wiktionary.rs`:

```rust
use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

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

/// Remove `<...>` HTML tags Wiktionary embeds in definitions.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
        let url = format!("{}/{}", self.base_url, spell.trim());
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
                .map(|s| strip_tags(&s.definition))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::wiktionary -- --nocolor`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add src/sources/wiktionary.rs
git commit -m "feat(sources): official Wiktionary definitions client"
```

---

## Task 7: Merriam-Webster source (Learner's + Thesaurus)

**Files:**
- Create: `src/sources/mw.rs`

- [ ] **Step 1: Write file with tests**

Write `src/sources/mw.rs`:

```rust
use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

const LEARNERS_BASE: &str = "https://dictionaryapi.com/api/v3/references/learners/json";
const THESAURUS_BASE: &str = "https://dictionaryapi.com/api/v3/references/thesaurus/json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MwLearnersData {
    pub pos: Option<String>,
    pub phonetic: Option<String>,
    pub audio_url: Option<String>,
    pub short_defs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MwThesaurusData {
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
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
        let url = format!("{}/{}", self.base_url, spell.trim());
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
            Err(_) => return None, // no-match returns ["sug","gest"] → type mismatch → None
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
        let data = MwLearnersData { pos: e.fl, phonetic, audio_url, short_defs: e.shortdef };
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
        let url = format!("{}/{}", self.base_url, spell.trim());
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
        let mut synonyms = Vec::new();
        let mut antonyms = Vec::new();
        for e in entries {
            if let Some(m) = e.meta {
                for g in m.syns {
                    synonyms.extend(g);
                }
                for g in m.ants {
                    antonyms.extend(g);
                }
            }
        }
        if synonyms.is_empty() && antonyms.is_empty() {
            None
        } else {
            Some(MwThesaurusData { synonyms, antonyms })
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
        assert_eq!(d.synonyms, vec!["essay", "experiment", "exam"]);
        assert_eq!(d.antonyms, vec!["proof"]);
    }

    #[tokio::test]
    async fn thesaurus_no_key_is_none() {
        let c = MwThesaurusClient::with_base_url(dict_client(), String::new(), "http://unused".into());
        assert!(c.fetch("test").await.is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::mw -- --nocolor`
Expected: PASS (6 tests).

- [ ] **Step 3: Commit**

```bash
git add src/sources/mw.rs
git commit -m "feat(sources): Merriam-Webster Learner's + Thesaurus clients"
```

---

## Task 8: Youdao source

**Files:**
- Create: `src/sources/youdao.rs`

- [ ] **Step 1: Capture a real fixture**

Run (saves a real response to use as the wiremock body):

```bash
curl -s 'https://dict.youdao.com/jsonapi?q=test' -H 'User-Agent: Mozilla/5.0' -o /tmp/youdao_test.json
python3 -c "import json;d=json.load(open('/tmp/youdao_test.json'));print(sorted(d.keys()))"
```
Expected: prints keys including `ee`, `collins`, `syno`, `phrs`, `blng_sents_part`, `web_trans`, `ec`, `wikipedia_digest`, `etym`.

- [ ] **Step 2: Write file with tests**

Write `src/sources/youdao.rs`. Parsing is defensive: every field optional, unknown shapes ignored. Map only the fields the card uses.

```rust
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
    pub ec_zh: Vec<String>,        // 英汉释义行 "pos. 中文"
    pub ee: Vec<String>,           // 英英释义行
    pub syno: Vec<String>,         // 同义词组 "pos. w1, w2"
    pub rel_word: Vec<String>,     // 派生/联想 "pos. w1, w2"
    pub phrs: Vec<String>,         // 词组 "phrase — 中文"
    pub sents: Vec<(String, String)>, // (en, zh)
    pub web_trans: Vec<String>,    // 网络释义
    pub wiki: Option<String>,      // wikipedia_digest summary
    pub etym: Option<String>,      // 词源
}

fn s(v: &Value, p: &str) -> Option<String> {
    let t = v.pointer(p)?.as_str()?.trim();
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

        // ec: word.trs[].tr[].l.i[]
        if let Some(trs) = v.pointer("/ec/word/0/trs").and_then(|x| x.as_array()) {
            for t in trs {
                if let Some(line) = t.pointer("/tr/0/l/i/0").and_then(|x| x.as_str()) {
                    d.ec_zh.push(line.trim().to_string());
                }
            }
        }
        // ee: word.trs[].tr[].l.i (English-English)
        if let Some(trs) = v.pointer("/ee/word/trs").and_then(|x| x.as_array()) {
            for t in trs {
                let pos = t.pointer("/pos").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(tr) = t.pointer("/tr/0/l/i").and_then(|x| x.as_str()) {
                    let line = if pos.is_empty() { tr.to_string() } else { format!("{} {}", pos, tr) };
                    d.ee.push(line.trim().to_string());
                }
            }
        }
        // syno: synos.syno[]
        if let Some(arr) = v.pointer("/syno/synos").and_then(|x| x.as_array()) {
            for g in arr {
                let pos = g.pointer("/syno/pos").and_then(|x| x.as_str()).unwrap_or("");
                let ws: Vec<String> = g
                    .pointer("/syno/ws")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|w| w.pointer("/w").and_then(|x| x.as_str()).map(String::from)).collect())
                    .unwrap_or_default();
                if !ws.is_empty() {
                    d.syno.push(format!("{} {}", pos, ws.join(", ")).trim().to_string());
                }
            }
        }
        // rel_word: rel_word.rels[].rel
        if let Some(rels) = v.pointer("/rel_word/rels").and_then(|x| x.as_array()) {
            for r in rels {
                let pos = r.pointer("/rel/pos").and_then(|x| x.as_str()).unwrap_or("");
                let ws: Vec<String> = r
                    .pointer("/rel/words")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|w| w.pointer("/word").and_then(|x| x.as_str()).map(String::from)).collect())
                    .unwrap_or_default();
                if !ws.is_empty() {
                    d.rel_word.push(format!("{} {}", pos, ws.join(", ")).trim().to_string());
                }
            }
        }
        // phrs: phrs.phrs[].phr
        if let Some(arr) = v.pointer("/phrs/phrs").and_then(|x| x.as_array()) {
            for p in arr {
                let head = p.pointer("/phr/headword/l/i").and_then(|x| x.as_str()).unwrap_or("");
                let tr = p.pointer("/phr/trs/0/tr/l/i").and_then(|x| x.as_str()).unwrap_or("");
                if !head.is_empty() {
                    d.phrs.push(format!("{} — {}", head.trim(), tr.trim()).trim_end_matches(" — ").to_string());
                }
            }
        }
        // blng_sents_part: sentence-pair[]
        if let Some(arr) = v.pointer("/blng_sents_part/sentence-pair").and_then(|x| x.as_array()) {
            for sp in arr {
                let en = sp.pointer("/sentence").and_then(|x| x.as_str()).unwrap_or("");
                let zh = sp.pointer("/sentence-translation").and_then(|x| x.as_str()).unwrap_or("");
                if !en.is_empty() {
                    d.sents.push((en.trim().to_string(), zh.trim().to_string()));
                }
            }
        }
        // web_trans: web_trans.web-translation[].trans[].value
        if let Some(arr) = v.pointer("/web_trans/web-translation").and_then(|x| x.as_array()) {
            for w in arr {
                if let Some(val) = w.pointer("/trans/0/value").and_then(|x| x.as_str()) {
                    d.web_trans.push(val.trim().to_string());
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
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib sources::youdao -- --nocolor`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/sources/youdao.rs
git commit -m "feat(sources): Youdao jsonapi client (defensive parsing)"
```

---

## Task 9: Card aggregator

**Files:**
- Create: `src/card.rs`
- Modify: `src/lib.rs:6` (add `pub mod card;`)

- [ ] **Step 1: Write the failing test**

Create `src/card.rs`:

```rust
//! Aggregates the card-only network sources in parallel, each cached and
//! independently degraded. ECDICT/Wordnik/Urban/LLM are NOT fetched here
//! (the orchestrator already has them); they are rendered by preview.rs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheKind};
use crate::http::dict_client;
use crate::sources::datamuse::{DatamuseClient, DatamuseData};
use crate::sources::freedict::{FreeDictClient, FreeDictData};
use crate::sources::fetch_json_cached;
use crate::sources::mw::{
    MwLearnersClient, MwLearnersData, MwThesaurusClient, MwThesaurusData,
};
use crate::sources::wikipedia::{WikipediaClient, WikipediaSummary};
use crate::sources::wiktionary::{WiktionaryClient, WiktionaryData};
use crate::sources::youdao::{YoudaoClient, YoudaoData};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CardSources {
    pub youdao: Option<YoudaoData>,
    pub wikipedia: Option<WikipediaSummary>,
    pub datamuse: Option<DatamuseData>,
    pub wiktionary: Option<WiktionaryData>,
    pub freedict: Option<FreeDictData>,
    pub mw_learners: Option<MwLearnersData>,
    pub mw_thesaurus: Option<MwThesaurusData>,
}

pub struct CardKeys {
    pub mw_learners: String,
    pub mw_thesaurus: String,
}

/// Fetch all card-only sources concurrently. Each is cached under its own
/// `CacheKind`; a failing/empty source yields `None` and is skipped.
pub async fn gather_card_data(
    cache: Arc<dyn Cache>,
    spell: &str,
    bypass: bool,
    keys: &CardKeys,
) -> CardSources {
    let yd = YoudaoClient::new(dict_client());
    let wp = WikipediaClient::new(dict_client());
    let dm = DatamuseClient::new(dict_client());
    let wk = WiktionaryClient::new(dict_client());
    let fd = FreeDictClient::new(dict_client());
    let ml = MwLearnersClient::new(dict_client(), keys.mw_learners.clone());
    let mt = MwThesaurusClient::new(dict_client(), keys.mw_thesaurus.clone());

    let (youdao, wikipedia, datamuse, wiktionary, freedict, mw_learners, mw_thesaurus) = tokio::join!(
        fetch_json_cached(cache.clone(), CacheKind::Youdao, spell, bypass, || yd.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Wikipedia, spell, bypass, || wp.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Datamuse, spell, bypass, || dm.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Wiktionary, spell, bypass, || wk.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::FreeDict, spell, bypass, || fd.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::MwLearners, spell, bypass, || ml.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::MwThesaurus, spell, bypass, || mt.fetch(spell)),
    );

    CardSources { youdao, wikipedia, datamuse, wiktionary, freedict, mw_learners, mw_thesaurus }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::sqlite::SqliteCache;

    #[tokio::test]
    async fn gather_degrades_to_all_none_offline() {
        // Real clients hitting unroutable hosts via the 2s dict client.
        // We only assert it never panics and returns a struct.
        let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
        let keys = CardKeys { mw_learners: String::new(), mw_thesaurus: String::new() };
        let r = gather_card_data(cache, "zzzznotaword", false, &keys).await;
        assert!(r.mw_learners.is_none() && r.mw_thesaurus.is_none());
    }
}
```

Add to `src/lib.rs` after `pub mod cache;` line (keep alphabetical-ish, any order compiles):

```rust
pub mod card;
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib card:: -- --nocolor`
Expected: PASS. (`gather_degrades...` is network-dependent but only asserts the no-key M-W fields are `None`, which is deterministic.)

- [ ] **Step 3: Commit**

```bash
git add src/card.rs src/lib.rs
git commit -m "feat(card): parallel cached aggregator for card-only sources"
```

---

## Task 10: Rewrite preview.rs (10-block renderer)

**Files:**
- Rewrite: `src/preview.rs`

- [ ] **Step 1: Write the new preview.rs with tests**

Replace the entire contents of `src/preview.rs`:

```rust
//! Builds the rich multi-source Quick Look card (Shift / ⌘Y). Blocks are
//! rendered in the user-approved priority order; a block with no data is
//! skipped entirely. The Alfred dropdown list is built elsewhere and is
//! unaffected by this module.

use std::fmt::Write as _;
use std::path::Path;

use crate::card::CardSources;
use crate::dictionary::entry::StardictEntry;
use crate::llm::LlmResult;
use crate::sources::DictEntry;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Remove `<...>` markup (e.g. Wordnik `<xref>`), keep inner text.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const STYLE: &str = "\
body{font:15px/1.6 -apple-system,Helvetica,sans-serif;background:#1e1e1e;\
color:#e8e8e8;margin:0;padding:22px 26px;}\
h1{font-size:24px;margin:0 0 2px;}\
.ph{color:#999;margin:0 0 14px;}\
h2{font-size:14px;margin:22px 0 8px;padding-bottom:4px;\
border-bottom:1px solid #3a3a3a;color:#9ad;letter-spacing:.3px;}\
ol,ul{margin:0;padding-left:22px;}\
li{margin:0 0 8px;}\
.src{color:#888;font-size:12px;}\
.meta{display:block;color:#888;font-size:12px;margin-top:2px;}\
.tags{color:#c9a;}\
em{color:#c8a;font-style:normal;}\
a{color:#7bf;}\
p{margin:6px 0;}";

/// Render the card to `<cache_dir>/preview.html`; return its path for use
/// as an Alfred `quicklookurl`. `None` if there is nothing to show.
#[allow(clippy::too_many_arguments)]
pub fn write_preview(
    cache_dir: &Path,
    spell: &str,
    ecdict: Option<&StardictEntry>,
    wordnik: &[DictEntry],
    urban: &[DictEntry],
    llm: Option<&LlmResult>,
    extra: &CardSources,
) -> Option<String> {
    let mut body = String::new();

    // Title + phonetic (prefer ECDICT, then FreeDict, then MW Learner's).
    let phonetic = ecdict
        .and_then(|e| e.phonetic.clone())
        .filter(|p| !p.is_empty())
        .or_else(|| extra.freedict.as_ref().and_then(|f| f.phonetic.clone()))
        .or_else(|| extra.mw_learners.as_ref().and_then(|m| m.phonetic.clone()));

    // ---- Block 1: English-English (top priority) ----
    {
        let mut lines: Vec<(String, String)> = Vec::new(); // (text, source)
        for w in wordnik {
            lines.push((strip_tags(&w.definition), format!("Wordnik {}", w.extra.clone().unwrap_or_default())));
        }
        if let Some(m) = &extra.mw_learners {
            let pos = m.pos.clone().unwrap_or_default();
            for d in &m.short_defs {
                lines.push((d.clone(), format!("M-W Learner's {}", pos).trim().to_string()));
            }
        }
        if let Some(y) = &extra.youdao {
            for l in &y.ee {
                lines.push((l.clone(), "有道".to_string()));
            }
        }
        if let Some(w) = &extra.wiktionary {
            for s in &w.senses {
                for d in &s.definitions {
                    lines.push((d.clone(), format!("Wiktionary {}", s.pos)));
                }
            }
        }
        if let Some(f) = &extra.freedict {
            for m in &f.meanings {
                for d in &m.definitions {
                    lines.push((d.clone(), format!("FreeDict {}", m.pos)));
                }
            }
        }
        if !lines.is_empty() {
            let _ = write!(body, "<section><h2>🔤 英英释义 English-English</h2><ol>");
            for (t, src) in lines {
                let _ = write!(body, "<li>{}<span class=\"meta\">{}</span></li>", esc(&t), esc(src.trim()));
            }
            let _ = write!(body, "</ol></section>");
        }
    }

    // ---- Block 2: Synonyms / Antonyms / Related ----
    {
        let mut syn: Vec<String> = Vec::new();
        let mut ant: Vec<String> = Vec::new();
        let mut rel: Vec<String> = Vec::new();
        if let Some(t) = &extra.mw_thesaurus {
            syn.extend(t.synonyms.iter().cloned());
            ant.extend(t.antonyms.iter().cloned());
        }
        if let Some(d) = &extra.datamuse {
            syn.extend(d.synonyms.iter().cloned());
            ant.extend(d.antonyms.iter().cloned());
            rel.extend(d.related.iter().cloned());
        }
        if let Some(f) = &extra.freedict {
            for m in &f.meanings {
                syn.extend(m.synonyms.iter().cloned());
                ant.extend(m.antonyms.iter().cloned());
            }
        }
        let yd_syno = extra.youdao.as_ref().map(|y| (&y.syno, &y.rel_word));
        dedup(&mut syn);
        dedup(&mut ant);
        dedup(&mut rel);
        let has_yd = yd_syno.map(|(s, r)| !s.is_empty() || !r.is_empty()).unwrap_or(false);
        if !syn.is_empty() || !ant.is_empty() || !rel.is_empty() || has_yd {
            let _ = write!(body, "<section><h2>🔄 同义 / 反义 / 联想</h2>");
            if !syn.is_empty() {
                let _ = write!(body, "<p><b>同义</b> {}</p>", esc(&syn.join(", ")));
            }
            if !ant.is_empty() {
                let _ = write!(body, "<p><b>反义</b> {}</p>", esc(&ant.join(", ")));
            }
            if !rel.is_empty() {
                let _ = write!(body, "<p><b>联想</b> {}</p>", esc(&rel.join(", ")));
            }
            if let Some((s, r)) = yd_syno {
                for line in s.iter().chain(r.iter()) {
                    let _ = write!(body, "<p class=\"src\">有道: {}</p>", esc(line));
                }
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 3: Phrases / collocations + examples ----
    {
        let y = extra.youdao.as_ref();
        let fd = extra.freedict.as_ref();
        let has_phr = y.map(|y| !y.phrs.is_empty()).unwrap_or(false);
        let has_sent = y.map(|y| !y.sents.is_empty()).unwrap_or(false);
        let fd_ex: Vec<&String> = fd
            .map(|f| f.meanings.iter().flat_map(|m| m.examples.iter()).collect())
            .unwrap_or_default();
        if has_phr || has_sent || !fd_ex.is_empty() {
            let _ = write!(body, "<section><h2>🧩 词组短语 / 例句</h2>");
            if let Some(y) = y {
                if !y.phrs.is_empty() {
                    let _ = write!(body, "<ul>");
                    for p in &y.phrs {
                        let _ = write!(body, "<li>{}</li>", esc(p));
                    }
                    let _ = write!(body, "</ul>");
                }
                for (en, zh) in &y.sents {
                    let _ = write!(body, "<p>{}<span class=\"meta\">{}</span></p>", esc(en), esc(zh));
                }
            }
            for ex in fd_ex {
                let _ = write!(body, "<p><em>e.g.</em> {}</p>", esc(ex));
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 4: Chinese + POS + web translations ----
    {
        let mut zh: Vec<String> = Vec::new();
        if let Some(e) = ecdict {
            if let Some(t) = e.translation.as_ref().or(e.definition.as_ref()) {
                zh.push(t.replace('\\', "/").replace('\n', "; "));
            }
        }
        if let Some(y) = &extra.youdao {
            zh.extend(y.ec_zh.iter().cloned());
        }
        let web: Vec<String> = extra
            .youdao
            .as_ref()
            .map(|y| y.web_trans.clone())
            .unwrap_or_default();
        dedup(&mut zh);
        if !zh.is_empty() || !web.is_empty() {
            let _ = write!(body, "<section><h2>📕 中文释义</h2>");
            for line in &zh {
                let _ = write!(body, "<p>{}</p>", esc(line));
            }
            if !web.is_empty() {
                let _ = write!(body, "<p class=\"src\">网络释义: {}</p>", esc(&web.join("; ")));
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 5: Inflections + exam tags + collins ----
    if let Some(e) = ecdict {
        let infl = e.exchange_info();
        let tags = e.tag_info();
        let collins = e.collins.filter(|c| *c > 0).map(|c| "⭐️".repeat(c as usize));
        if infl.is_some() || tags.is_some() || collins.is_some() {
            let _ = write!(body, "<section><h2>🔀 词形变化 / 标签</h2>");
            if let Some(i) = infl {
                let _ = write!(body, "<p>{}</p>", esc(&i));
            }
            if let Some(t) = tags {
                let _ = write!(body, "<p class=\"tags\">考试: {}</p>", esc(&t));
            }
            if let Some(c) = collins {
                let _ = write!(body, "<p>Collins {}</p>", c);
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 6: Wikipedia (official; Youdao digest fallback) ----
    {
        let (text, link) = if let Some(w) = &extra.wikipedia {
            (Some(w.extract.clone()), w.url.clone())
        } else if let Some(y) = &extra.youdao {
            (y.wiki.clone(), None)
        } else {
            (None, None)
        };
        if let Some(t) = text.filter(|t| !t.is_empty()) {
            let _ = write!(body, "<section><h2>📖 维基百科</h2><p>{}</p>", esc(&t));
            if let Some(u) = link {
                let _ = write!(body, "<p><a href=\"{}\">{}</a></p>", esc(&u), esc(&u));
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 7: Etymology ----
    {
        let mut et: Vec<String> = Vec::new();
        if let Some(y) = &extra.youdao {
            if let Some(e) = &y.etym {
                et.push(e.clone());
            }
        }
        if let Some(f) = &extra.freedict {
            if let Some(o) = &f.origin {
                et.push(o.clone());
            }
        }
        dedup(&mut et);
        if !et.is_empty() {
            let _ = write!(body, "<section><h2>🌱 词源</h2>");
            for e in et {
                let _ = write!(body, "<p>{}</p>", esc(&e));
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 8: Pronunciation ----
    {
        let audio = extra
            .mw_learners
            .as_ref()
            .and_then(|m| m.audio_url.clone())
            .or_else(|| extra.freedict.as_ref().and_then(|f| f.audio.clone()));
        if phonetic.is_some() || audio.is_some() {
            let _ = write!(body, "<section><h2>🔊 发音</h2>");
            if let Some(p) = &phonetic {
                let _ = write!(body, "<p>/{}/</p>", esc(p));
            }
            if let Some(a) = &audio {
                let _ = write!(body, "<p><a href=\"{}\">▶ play audio</a></p>", esc(a));
            }
            let _ = write!(body, "</section>");
        }
    }

    // ---- Block 9: Urban Dictionary ----
    if !urban.is_empty() {
        let _ = write!(body, "<section><h2>🔥 Urban Dictionary</h2><ol>");
        for u in urban {
            let def = esc(&u.definition).replace("  e.g. ", "<br><em>e.g. ");
            let _ = write!(body, "<li>{}", def);
            if let Some(x) = u.extra.as_deref().filter(|x| !x.is_empty()) {
                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(x));
            }
            let _ = write!(body, "</li>");
        }
        let _ = write!(body, "</ol></section>");
    }

    // ---- Block 10: Claude translation (only when present) ----
    if let Some(r) = llm {
        if !r.translations.is_empty() {
            let _ = write!(body, "<section><h2>🤖 Claude 翻译</h2><p>{}</p>", esc(&r.translations.join("；")));
            if let Some(ex) = r.example.as_deref().filter(|e| !e.is_empty()) {
                let _ = write!(body, "<p><em>e.g.</em> {}</p>", esc(ex));
            }
            let _ = write!(body, "</section>");
        }
    }

    if body.is_empty() {
        return None;
    }

    let ph = phonetic
        .map(|p| format!("<p class=\"ph\">/{}/</p>", esc(&p)))
        .unwrap_or_default();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
<style>{STYLE}</style></head><body><h1>{}</h1>{ph}{body}</body></html>",
        esc(spell)
    );
    let path = cache_dir.join("preview.html");
    std::fs::write(&path, html).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn dedup(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| !s.trim().is_empty() && seen.insert(s.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::datamuse::DatamuseData;
    use crate::sources::wikipedia::WikipediaSummary;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join("eudic-card-test");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn empty_everything_is_none() {
        let cs = CardSources::default();
        assert!(write_preview(&dir(), "x", None, &[], &[], None, &cs).is_none());
    }

    #[test]
    fn renders_blocks_in_priority_order_and_escapes() {
        let mut cs = CardSources::default();
        cs.datamuse = Some(DatamuseData {
            synonyms: vec!["glad".into()],
            antonyms: vec!["sad".into()],
            related: vec![],
        });
        cs.wikipedia = Some(WikipediaSummary {
            title: "Happy".into(),
            extract: "Happiness <is> good".into(),
            url: Some("https://w/Happy".into()),
        });
        let wordnik = vec![DictEntry {
            headword: "happy".into(),
            definition: "feeling <xref>joy</xref>".into(),
            extra: Some("adjective".into()),
        }];
        let p = write_preview(&dir(), "ha<ppy", None, &wordnik, &[], None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        // English-English (block 1) appears before Synonyms (block 2)
        let i_ee = html.find("英英释义").unwrap();
        let i_syn = html.find("同义").unwrap();
        let i_wiki = html.find("维基百科").unwrap();
        assert!(i_ee < i_syn && i_syn < i_wiki, "block order must follow priority");
        assert!(html.contains("feeling joy"), "xref stripped");
        assert!(html.contains("ha&lt;ppy"), "title escaped");
        assert!(html.contains("Happiness &lt;is&gt; good"), "wiki escaped");
    }

    #[test]
    fn skips_blocks_without_data() {
        let mut cs = CardSources::default();
        cs.wikipedia = Some(WikipediaSummary {
            title: "T".into(),
            extract: "Only wiki here".into(),
            url: None,
        });
        let p = write_preview(&dir(), "t", None, &[], &[], None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        assert!(html.contains("维基百科"));
        assert!(!html.contains("英英释义"));
        assert!(!html.contains("同义"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib preview:: -- --nocolor`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add src/preview.rs
git commit -m "feat(preview): 10-block rich card renderer"
```

---

## Task 11: Wire aggregator into the orchestrator

**Files:**
- Modify: `src/command/search.rs`

Current relevant code (search.rs): env keys read ~line 42-44; existing `tokio::join!` for urban+wordnik ~line 74-76; the `quicklook` block ~line 101-118 calls `preview::write_preview(&dir, &args.spell, ecdict_entries.first(), wordnik_slice, urban_slice, llm_ref)`.

- [ ] **Step 1: Read M-W keys**

In `src/command/search.rs`, just after the existing `let anthropic_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();` line add:

```rust
    let mw_learners_key = env::var("MW_LEARNERS_API_KEY").unwrap_or_default();
    let mw_thesaurus_key = env::var("MW_THESAURUS_API_KEY").unwrap_or_default();
```

- [ ] **Step 2: Gather card data alongside existing fetches**

Add an import near the other `use crate::...` lines:

```rust
use crate::card::{gather_card_data, CardKeys};
```

Immediately AFTER the existing `let (urban_res, wordnik_res) = tokio::join!(...)` block, add:

```rust
    let card_extra = gather_card_data(
        cache.clone(),
        &spell_for_remote,
        bypass_cache,
        &CardKeys {
            mw_learners: mw_learners_key,
            mw_thesaurus: mw_thesaurus_key,
        },
    )
    .await;
```

- [ ] **Step 3: Pass it to the preview builder**

In the `let quicklook = { ... }` block, change the `preview::write_preview(` call to pass `&card_extra` as the final argument (after the existing `llm_ref` argument):

```rust
        preview::write_preview(
            &dir,
            &args.spell,
            ecdict_entries.first(),
            wordnik_slice,
            urban_slice,
            llm_ref,
            &card_extra,
        )
```

- [ ] **Step 4: Build + run full suite**

Run: `cargo test --quiet`
Expected: PASS — all lib + integration tests green (existing + new).

- [ ] **Step 5: Commit**

```bash
git add src/command/search.rs
git commit -m "feat(search): fetch and pass card sources to the preview"
```

---

## Task 12: Add optional M-W key config fields to info.plist

**Files:**
- Modify: `info.plist`

- [ ] **Step 1: Add two textfield config entries**

Run this script (mirrors how `WORDNIK_API_KEY`/`ANTHROPIC_API_KEY` were added):

```bash
python3 - <<'PY'
import plistlib
p="info.plist"
d=plistlib.load(open(p,'rb'))
cfg=d["userconfigurationconfig"]
def field(label, var, ph, desc):
    return {"config":{"default":"","placeholder":ph,"required":False,"trim":True},
            "description":desc,"label":label,"type":"textfield","variable":var}
have={c.get("variable") for c in cfg}
if "MW_LEARNERS_API_KEY" not in have:
    cfg.append(field("Merriam-Webster Learner's Key","MW_LEARNERS_API_KEY",
        "dictionaryapi.com 申请 Learner's","可选：M-W Learner's 英英+音频（留空则跳过）"))
if "MW_THESAURUS_API_KEY" not in have:
    cfg.append(field("Merriam-Webster Thesaurus Key","MW_THESAURUS_API_KEY",
        "dictionaryapi.com 申请 Thesaurus","可选：M-W 同义/反义词库（留空则跳过）"))
plistlib.dump(d,open(p,'wb'))
print("variables:",[c.get("variable") for c in d["userconfigurationconfig"]])
PY
plutil -lint info.plist
```
Expected: prints the 5 variables (Database, WORDNIK, ANTHROPIC, MW_LEARNERS, MW_THESAURUS) and `info.plist: OK`.

- [ ] **Step 2: Commit**

```bash
git add info.plist
git commit -m "chore(workflow): add optional Merriam-Webster key config fields"
```

---

## Task 13: Integration test

**Files:**
- Create: `tests/card_integration.rs`

- [ ] **Step 1: Write the integration test**

Create `tests/card_integration.rs`:

```rust
//! End-to-end: aggregator + preview render with mocked card sources.

use std::sync::Arc;

use alfred_eudic::card::{gather_card_data, CardKeys};
use alfred_eudic::cache::Cache;
use alfred_eudic::cache::sqlite::SqliteCache;
use alfred_eudic::preview::write_preview;

#[tokio::test]
async fn aggregator_then_preview_offline_is_safe() {
    // No keys, unroutable lookups → all card sources None, but the
    // pipeline must not panic and preview must handle an empty aggregate.
    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let keys = CardKeys { mw_learners: String::new(), mw_thesaurus: String::new() };
    let extra = gather_card_data(cache, "zzzznotaword", false, &keys).await;
    assert!(extra.mw_learners.is_none());
    assert!(extra.mw_thesaurus.is_none());

    let dir = std::env::temp_dir().join("eudic-card-it");
    std::fs::create_dir_all(&dir).unwrap();
    // Empty aggregate + no other data ⇒ no card.
    let none = write_preview(&dir, "zzzznotaword", None, &[], &[], None, &extra);
    assert!(none.is_none());
}
```

Note: `card`, `cache`, `preview` must be reachable from the `alfred_eudic` lib crate. They already are (`pub mod` in `src/lib.rs`).

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test card_integration -- --nocolor`
Expected: PASS (1 test).

- [ ] **Step 3: Commit**

```bash
git add tests/card_integration.rs
git commit -m "test: card aggregator + preview integration"
```

---

## Task 14: Full verification + deploy

**Files:** none (verification + deploy)

- [ ] **Step 1: Full test suite**

Run: `cargo test --quiet`
Expected: all suites PASS (≈ 38 prior lib tests + ~22 new + integration).

- [ ] **Step 2: Release build**

Run: `cargo build --release 2>&1 | tail -1`
Expected: `Finished \`release\` profile [optimized] target(s)`.

- [ ] **Step 3: Live smoke test (real network, real ECDICT)**

```bash
WF="/Users/jk/Library/Application Support/Alfred/Alfred.alfredpreferences/workflows/user.workflow.4D4E31FF-94A3-4DB7-87CE-ACB783925B51"
AK=$(plutil -extract ANTHROPIC_API_KEY raw "$WF/prefs.plist")
WK=$(plutil -extract WORDNIK_API_KEY raw "$WF/prefs.plist")
alfred_workflow_cache="$WF/cache" MW_LEARNERS_API_KEY=458f94ba-2dd5-4840-8fb8-58773176b4f2 \
MW_THESAURUS_API_KEY=9612cc34-dbee-4999-b181-354af7338f00 \
WORDNIK_API_KEY="$WK" ANTHROPIC_API_KEY="$AK" \
./target/release/alfred-eudic search \
  --completion-file="$WF/resources/words_alpha.txt" \
  --db-file="$HOME/Code/jk/alfred-workflows/ECDICT/stardict.db" "serendipity" >/dev/null 2>&1
python3 -c "import re;h=open([l for l in __import__('glob').glob('$WF/cache/preview.html')][0]).read();print([s for s in ['英英释义','同义','词组','中文释义','词形变化','维基百科','词源','发音','Urban'] if s in h])"
```
Expected: prints a list containing most of the block names (data-dependent; English-English and Wikipedia should appear for `serendipity`).

- [ ] **Step 4: Deploy binary to the installed workflow**

```bash
WF="/Users/jk/Library/Application Support/Alfred/Alfred.alfredpreferences/workflows/user.workflow.4D4E31FF-94A3-4DB7-87CE-ACB783925B51"
cp target/release/alfred-eudic "$WF/bin/alfred-eudic" && chmod +x "$WF/bin/alfred-eudic"
echo deployed
```

- [ ] **Step 5: Rebuild the distributable + commit**

```bash
ROOT="/Users/jk/Code/jk/alfred-workflows/alfred-eudic-workflow"
PKG="$(mktemp -d)/EudicSearch"; mkdir -p "$PKG/bin" "$PKG/script" "$PKG/resources"
cp "$ROOT/info.plist" "$ROOT/icon.png" "$PKG/"
cp "$ROOT/target/release/alfred-eudic" "$PKG/bin/"; cp "$ROOT/script/"*.sh "$PKG/script/"
cp "$ROOT/resources/words_alpha.txt" "$PKG/resources/"
chmod +x "$PKG/bin/alfred-eudic" "$PKG/script/"*.sh
rm -f "$ROOT/EudicSearch.alfredworkflow"
( cd "$PKG" && zip -qr "$ROOT/EudicSearch.alfredworkflow" . -x '.*' )
cd "$ROOT" && git add -A && git commit -m "chore: rebuild distributable with rich card"
```

- [ ] **Step 6: Manual verification in Alfred (user)**

Restart Alfred, type `dic serendipity`, press **Shift** on a result. The card should show, in order: 🔤 英英释义 → 🔄 同义/反义/联想 → 🧩 词组/例句 → 📕 中文 → 🔀 词形变化 → 📖 维基百科 → 🌱 词源 → 🔊 发音 → 🔥 Urban → (🤖 Claude when triggered). Configure `MW_LEARNERS_API_KEY` / `MW_THESAURUS_API_KEY` in the Configure panel (Don't Export) to enable the M-W sources.

---

## Self-Review Notes

- **Spec coverage:** every source in the spec table maps to a task
  (Wikipedia→T3, Datamuse→T4, FreeDict→T5, Wiktionary→T6, M-W→T7,
  Youdao→T8); aggregator→T9; 10-block layout→T10; orchestrator→T11;
  optional M-W config→T12; tests→each task + T13; performance (parallel +
  2s `dict_client` timeout + 7-day cache) is realised by `gather_card_data`
  (tokio::join!) + `fetch_json_cached` + existing cache; degradation =
  every source returns `None` on error.
- **Placeholder scan:** none — every step has concrete code/commands.
- **Type consistency:** `CardSources` field types match each source's
  returned struct (`YoudaoData`, `WikipediaSummary`, `DatamuseData`,
  `WiktionaryData`, `FreeDictData`, `MwLearnersData`, `MwThesaurusData`);
  `write_preview` signature in T10 matches the call site updated in T11;
  `CacheKind` variants added in T1 are the exact ones referenced in T9;
  `fetch_json_cached` signature in T2 matches all T9 call sites.
