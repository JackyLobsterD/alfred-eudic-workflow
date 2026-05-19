//! End-to-end: aggregator + preview render with the offline (no-key,
//! unroutable) path — must never panic and must degrade to "no card".

use std::sync::Arc;

use alfred_eudic::card::{gather_card_data, CardKeys};
use alfred_eudic::cache::Cache;
use alfred_eudic::cache::sqlite::SqliteCache;
use alfred_eudic::preview::write_preview;

#[tokio::test]
async fn aggregator_then_preview_offline_is_safe() {
    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let keys = CardKeys { mw_learners: String::new(), mw_thesaurus: String::new() };
    let extra = gather_card_data(cache, "zzzznotaword", false, &keys).await;
    assert!(extra.mw_learners.is_none());
    assert!(extra.mw_thesaurus.is_none());

    let dir = std::env::temp_dir().join(format!("eudic-card-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Empty aggregate + no other data ⇒ no card.
    let none = write_preview(&dir, "zzzznotaword", None, &[], &[], None, &extra);
    assert!(none.is_none());
}
