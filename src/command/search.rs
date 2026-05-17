use std::env;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alfred::core::{AlfredConst, AlfredUtils};
use alfred::script_filter::{Item, ScriptFilter, Variable};
use alfred::updater::{Updater, version_compare};

use crate::cache::{Cache, sqlite::SqliteCache};
use crate::dictionary::{DictionaryConfig, DictionaryManager};
use crate::http::{dict_client, llm_client};
use crate::llm::{LlmClient, LlmError, fetch_with_cache_llm};
use crate::render;
use crate::sources::{
    DictionarySource, SourceError, SourceKind, fetch_with_cache,
    urban::UrbanClient, wordnik::WordnikClient,
};
use crate::{GITHUB_REPO, SEARCH_LIMIT, SearchArgs, WORKFLOW_ASSET_NAME};

const LLM_TRIGGER_THRESHOLD: usize = 5;
const MAX_LLM_SPELL_LEN: usize = 50;

pub async fn run_search(args: SearchArgs) -> Result<(), Box<dyn std::error::Error>> {
    ScriptFilter::reset();

    if args.spell.len() <= 1 {
        ScriptFilter::item(Item::new("Input more than one letter"));
        AlfredUtils::output(ScriptFilter::output());
        return Ok(());
    }

    let t1 = Instant::now();
    let bypass_cache = env::var("BYPASS_CACHE").ok().as_deref() == Some("1");
    let wordnik_key = env::var("WORDNIK_API_KEY").unwrap_or_default();
    let anthropic_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();

    // Open cache (degrade gracefully).
    let cache: Arc<dyn Cache> = match open_cache() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            AlfredUtils::log(format!("cache open failed, no-cache mode: {}", e));
            Arc::new(SqliteCache::in_memory().expect("in-memory cache must work"))
        }
    };

    // ECDICT (synchronous, fast, local).
    let manager = DictionaryManager::new(DictionaryConfig::new(args.completion_file.clone(), args.db_file.clone()));
    let ecdict_entries = if let Some(ref db_file) = args.db_file {
        if !db_file.is_empty() && std::path::Path::new(db_file).exists() {
            let spell_norm: String = args.spell.split_whitespace().collect();
            manager.find_matches_in_db(&spell_norm, SEARCH_LIMIT)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Urban + Wordnik concurrent. WordnikClient.fetch returns NoApiKey
    // when the key is empty, so we don't need a separate short-circuit.
    let urban = UrbanClient::new(dict_client());
    let wordnik = WordnikClient::new(dict_client(), wordnik_key);
    let spell_for_remote = args.spell.trim().to_string();

    let (urban_res, wordnik_res) = tokio::join!(
        fetch_with_cache(&urban as &dyn DictionarySource, cache.clone(), &spell_for_remote, bypass_cache),
        fetch_with_cache(&wordnik as &dyn DictionarySource, cache.clone(), &spell_for_remote, bypass_cache),
    );

    // Decide whether to invoke LLM.
    let llm_should_run = match &wordnik_res {
        Ok(v) => v.len() < LLM_TRIGGER_THRESHOLD,
        Err(_) => true,
    } && spell_for_remote.chars().count() <= MAX_LLM_SPELL_LEN;

    let llm_outcome: Option<Result<crate::llm::LlmResult, LlmError>> = if llm_should_run {
        if anthropic_key.is_empty() {
            Some(Err(LlmError::NoApiKey))
        } else {
            let llm = LlmClient::new(llm_client(), anthropic_key);
            Some(fetch_with_cache_llm(&llm, cache.clone(), &spell_for_remote, bypass_cache).await)
        }
    } else {
        None
    };

    // Build items in order: fallback, ECDICT, Wordnik, Urban, LLM.
    let mut items: Vec<Item> = Vec::new();
    items.push(Item::new(&args.spell).arg(&args.spell).subtitle("Type enter to check in Eudic"));

    for it in render::render_ecdict(&ecdict_entries) { items.push(it); }
    push_source_items(&mut items, "Wordnik", SourceKind::Wordnik, wordnik_res, &args.spell);
    push_source_items(&mut items, "Urban", SourceKind::Urban, urban_res, &args.spell);
    if let Some(outcome) = llm_outcome {
        match outcome {
            Ok(r) => for it in render::render_llm(&r, &args.spell) { items.push(it); },
            Err(LlmError::NoApiKey) => items.push(render::render_no_api_key("Claude")),
            Err(e) => items.push(render::render_error("LLM", &e.to_string(), &args.spell)),
        }
    }

    for item in items { ScriptFilter::item(item); }

    let t2 = Instant::now();
    AlfredUtils::log(format!("search time duration: {:?}", t2 - t1));

    // Update banner (unchanged behavior).
    let updater = Updater::new(GITHUB_REPO, WORKFLOW_ASSET_NAME, Duration::from_secs(60 * 60 * 24));
    let alfred = AlfredConst::shared();
    if let Some(cached) = updater.read_cached_release().await.ok().and_then(|o| o) {
        if updater.cache_valid(&cached) {
            if let Some(ref current_version) = alfred.workflow_version {
                if version_compare(current_version, &cached.tag_name) == std::cmp::Ordering::Less {
                    ScriptFilter::item(
                        Item::new("New version available on GitHub, type [Enter] to update")
                            .subtitle(format!("current version: {}, remote version: {}", current_version, cached.tag_name))
                            .arg("update")
                            .variable(Variable::new(Some("HAS_UPDATE".into()), Some("1".into()))),
                    );
                }
            }
        }
    }

    AlfredUtils::output(ScriptFilter::output());

    if let Some(cached) = updater.read_cached_release().await.ok().and_then(|o| o) {
        if !updater.cache_valid(&cached) {
            AlfredUtils::log("cache invalid");
            check_for_update_silently();
        }
    } else {
        check_for_update_silently();
    }

    Ok(())
}

fn push_source_items(
    items: &mut Vec<Item>,
    name: &str,
    kind: SourceKind,
    res: Result<Vec<crate::sources::DictEntry>, SourceError>,
    spell: &str,
) {
    match res {
        Ok(v) if v.is_empty() => { /* nothing to show; not an error */ }
        Ok(v) => {
            // Cap displayed entries for visual sanity.
            let cap = match kind { SourceKind::Wordnik => 5, SourceKind::Urban => 3 };
            let slice = v.iter().take(cap).cloned().collect::<Vec<_>>();
            for it in render::render_dict(&slice, kind) { items.push(it); }
        }
        Err(SourceError::NoApiKey) => items.push(render::render_no_api_key(name)),
        Err(e) => items.push(render::render_error(name, &e.to_string(), spell)),
    }
}

fn open_cache() -> Result<SqliteCache, Box<dyn std::error::Error>> {
    let dir = env::var("alfred_workflow_cache")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("alfred-eudic-cache"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("lookup_cache.db");
    Ok(SqliteCache::open(path)?)
}

fn check_for_update_silently() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let status = Command::new("/usr/bin/nohup")
        .arg(&exe)
        .args(["update", "--action", "check"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match status {
        Ok(_) => AlfredUtils::log("Update check completed in the background"),
        Err(e) => AlfredUtils::log(format!("Failed to start update process: {}", e)),
    }
}
