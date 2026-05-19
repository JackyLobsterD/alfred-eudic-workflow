use async_trait::async_trait;

pub mod sqlite;

pub const CACHE_TTL_SECS: i64 = 7 * 24 * 3600;

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

#[async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, kind: CacheKind, key: &str) -> Option<Vec<u8>>;
    async fn put(&self, kind: CacheKind, key: &str, value: &[u8]);
}
