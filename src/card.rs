//! Aggregates the card-only network sources in parallel, each cached and
//! independently degraded. ECDICT/Wordnik/Urban/LLM are NOT fetched here
//! (the orchestrator already has them); they are rendered by preview.rs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheKind};
use crate::http::dict_client;
use crate::sources::datamuse::{DatamuseClient, DatamuseData};
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
    let http = dict_client();
    let yd = YoudaoClient::new(http.clone());
    let wp = WikipediaClient::new(http.clone());
    let dm = DatamuseClient::new(http.clone());
    let wk = WiktionaryClient::new(http.clone());
    let ml = MwLearnersClient::new(http.clone(), keys.mw_learners.clone());
    let mt = MwThesaurusClient::new(http.clone(), keys.mw_thesaurus.clone());

    let (youdao, wikipedia, datamuse, wiktionary, mw_learners, mw_thesaurus) = tokio::join!(
        fetch_json_cached(cache.clone(), CacheKind::Youdao, spell, bypass, || yd.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Wikipedia, spell, bypass, || wp.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Datamuse, spell, bypass, || dm.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::Wiktionary, spell, bypass, || wk.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::MwLearners, spell, bypass, || ml.fetch(spell)),
        fetch_json_cached(cache.clone(), CacheKind::MwThesaurus, spell, bypass, || mt.fetch(spell)),
    );

    CardSources { youdao, wikipedia, datamuse, wiktionary, mw_learners, mw_thesaurus }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::sqlite::SqliteCache;

    #[tokio::test]
    async fn gather_degrades_to_all_none_offline() {
        let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
        let keys = CardKeys { mw_learners: String::new(), mw_thesaurus: String::new() };
        let r = gather_card_data(cache, "zzzznotaword", false, &keys).await;
        assert_eq!(r, CardSources::default());
    }
}
