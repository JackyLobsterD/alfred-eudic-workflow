use async_trait::async_trait;

pub mod sqlite;

pub const CACHE_TTL_SECS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Urban,
    Wordnik,
    Llm,
}

impl CacheKind {
    pub fn table(self) -> &'static str {
        match self {
            CacheKind::Urban => "cache_urban",
            CacheKind::Wordnik => "cache_wordnik",
            CacheKind::Llm => "cache_llm",
        }
    }
}

#[async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, kind: CacheKind, key: &str) -> Option<Vec<u8>>;
    async fn put(&self, kind: CacheKind, key: &str, value: &[u8]);
}
