use std::fmt;
use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheKind};

pub mod urban;
pub mod wordnik;
pub mod youdao;
pub mod wikipedia;
pub mod datamuse;
pub mod wiktionary;
pub mod freedict;
pub mod mw;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Urban,
    Wordnik,
}

impl SourceKind {
    pub fn cache_kind(self) -> CacheKind {
        match self {
            SourceKind::Urban => CacheKind::Urban,
            SourceKind::Wordnik => CacheKind::Wordnik,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SourceKind::Urban => "Urban",
            SourceKind::Wordnik => "Wordnik",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    pub headword: String,
    pub definition: String,
    pub extra: Option<String>,
}

#[derive(Debug)]
pub enum SourceError {
    Http(String),
    Timeout,
    BadResponse(String),
    RateLimited,
    NoApiKey,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Http(e) => write!(f, "HTTP error: {}", e),
            SourceError::Timeout => write!(f, "request timeout"),
            SourceError::BadResponse(s) => write!(f, "bad response: {}", s),
            SourceError::RateLimited => write!(f, "rate limited"),
            SourceError::NoApiKey => write!(f, "no API key configured"),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<reqwest::Error> for SourceError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            SourceError::Timeout
        } else {
            SourceError::Http(e.to_string())
        }
    }
}

#[async_trait]
pub trait DictionarySource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError>;
}

/// Cache-aware fetch wrapper. Honors `bypass` to force a fresh fetch.
pub async fn fetch_with_cache(
    source: &dyn DictionarySource,
    cache: Arc<dyn Cache>,
    spell: &str,
    bypass: bool,
) -> Result<Vec<DictEntry>, SourceError> {
    let key = spell.trim().to_lowercase();
    let kind = source.kind().cache_kind();
    if !bypass {
        if let Some(bytes) = cache.get(kind, &key).await {
            if let Ok(entries) = serde_json::from_slice::<Vec<DictEntry>>(&bytes) {
                return Ok(entries);
            }
            // corrupt cache value — fall through and refetch
        }
    }
    let entries = source.fetch(spell).await?;
    if let Ok(bytes) = serde_json::to_vec(&entries) {
        cache.put(kind, &key, &bytes).await;
    }
    Ok(entries)
}

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
            // corrupt or schema-changed cache value — fall through and refetch
        }
    }
    let value = fetch().await?;
    if let Ok(bytes) = serde_json::to_vec(&value) {
        cache.put(kind, &key, &bytes).await;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::sqlite::SqliteCache;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeSource {
        kind: SourceKind,
        calls: AtomicUsize,
        result: Vec<DictEntry>,
    }

    #[async_trait]
    impl DictionarySource for FakeSource {
        fn kind(&self) -> SourceKind { self.kind }
        async fn fetch(&self, _spell: &str) -> Result<Vec<DictEntry>, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn cache_hit_skips_fetch() {
        let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
        let src = FakeSource {
            kind: SourceKind::Wordnik,
            calls: AtomicUsize::new(0),
            result: vec![DictEntry { headword: "x".into(), definition: "y".into(), extra: None }],
        };
        let _ = fetch_with_cache(&src, cache.clone(), "Hello", false).await.unwrap();
        let _ = fetch_with_cache(&src, cache.clone(), "hello", false).await.unwrap();
        assert_eq!(src.calls.load(Ordering::SeqCst), 1, "second call should hit cache");
    }

    #[tokio::test]
    async fn bypass_forces_refetch() {
        let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
        let src = FakeSource {
            kind: SourceKind::Wordnik,
            calls: AtomicUsize::new(0),
            result: vec![],
        };
        let _ = fetch_with_cache(&src, cache.clone(), "hello", false).await.unwrap();
        let _ = fetch_with_cache(&src, cache.clone(), "hello", true).await.unwrap();
        assert_eq!(src.calls.load(Ordering::SeqCst), 2);
    }

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

    #[tokio::test]
    async fn json_cache_bypass_forces_refetch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct D { v: u32 }
        let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
        let calls = AtomicUsize::new(0);
        let mk = || async { calls.fetch_add(1, Ordering::SeqCst); Some(D { v: 1 }) };
        let _ = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Wikipedia, "k", false, mk).await;
        let _ = fetch_json_cached::<D, _, _>(cache.clone(), CacheKind::Wikipedia, "k", true, mk).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "bypass=true must skip cache and refetch");
    }
}
