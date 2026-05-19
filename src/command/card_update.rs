//! Background card refresh: invoked as a detached subprocess by the
//! main `search` command when the LLM cache is empty. Re-fetches every
//! source (all non-LLM sources hit the 7-day cache populated by the
//! caller, so they're effectively instant) and the LLM (slow path),
//! then overwrites `<cache_dir>/preview.html` with the finished card.
//! The webview meta-refresh in the loading-state HTML picks up the
//! updated file automatically.

use std::env;
use std::sync::Arc;

use alfred::core::AlfredUtils;

use crate::cache::{Cache, sqlite::SqliteCache};
use crate::card::{CardKeys, gather_card_data};
use crate::dictionary::{DictionaryConfig, DictionaryManager};
use crate::http::{dict_client, llm_client};
use crate::llm::{LlmClient, fetch_with_cache_llm};
use crate::preview;
use crate::sources::{
    DictionarySource, fetch_with_cache, urban::UrbanClient, wordnik::WordnikClient,
};
use crate::{SEARCH_LIMIT, SearchArgs};

pub async fn run_card_update(args: SearchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let trimmed = args.spell.trim();
    let spell = trimmed.strip_prefix('!').unwrap_or(trimmed).trim().to_string();
    if spell.len() <= 1 {
        return Ok(());
    }

    let wordnik_key = env::var("WORDNIK_API_KEY").unwrap_or_default();
    let anthropic_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let mw_learners_key = env::var("MW_LEARNERS_API_KEY").unwrap_or_default();
    let mw_thesaurus_key = env::var("MW_THESAURUS_API_KEY").unwrap_or_default();
    let card_keys = CardKeys { mw_learners: mw_learners_key, mw_thesaurus: mw_thesaurus_key };

    let dir = cache_dir();
    let _ = std::fs::create_dir_all(&dir);

    let cache: Arc<dyn Cache> = match SqliteCache::open(dir.join("lookup_cache.db")) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            AlfredUtils::log(format!("card-update cache open failed: {}", e));
            Arc::new(SqliteCache::in_memory()?)
        }
    };

    // ECDICT (sync, local).
    let manager = DictionaryManager::new(DictionaryConfig::new(
        args.completion_file.clone(),
        args.db_file.clone(),
    ));
    let ecdict_entries = if let Some(ref db_file) = args.db_file {
        if !db_file.is_empty() && std::path::Path::new(db_file).exists() {
            let spell_norm: String = spell.split_whitespace().collect();
            manager.find_matches_in_db(&spell_norm, SEARCH_LIMIT)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Wordnik + Urban + the seven card-only sources — all should hit
    // the cache from the main process. Then the slow LLM.
    let urban = UrbanClient::new(dict_client());
    let wordnik = WordnikClient::new(dict_client(), wordnik_key);
    let (urban_res, wordnik_res, card_extra) = tokio::join!(
        fetch_with_cache(&urban as &dyn DictionarySource, cache.clone(), &spell, false),
        fetch_with_cache(&wordnik as &dyn DictionarySource, cache.clone(), &spell, false),
        gather_card_data(cache.clone(), &spell, false, &card_keys),
    );

    let llm_result = if !anthropic_key.is_empty() && spell.chars().count() <= 50 {
        let llm = LlmClient::new(llm_client(), anthropic_key);
        match fetch_with_cache_llm(&llm, cache.clone(), &spell, false).await {
            Ok(r) => Some(r),
            Err(e) => {
                AlfredUtils::log(format!("card-update LLM failed: {}", e));
                None
            }
        }
    } else {
        None
    };

    let wordnik_slice: &[crate::sources::DictEntry] =
        wordnik_res.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
    let urban_slice: &[crate::sources::DictEntry] =
        urban_res.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);

    let _ = preview::write_preview(
        &dir,
        &spell,
        ecdict_entries.first(),
        wordnik_slice,
        urban_slice,
        llm_result.as_ref(),
        &card_extra,
        false, // not loading anymore
    );

    Ok(())
}

fn cache_dir() -> std::path::PathBuf {
    env::var("alfred_workflow_cache")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("alfred-eudic-cache"))
}
