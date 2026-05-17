# Multi-Source Lookup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Urban Dictionary, Wordnik, and Claude Haiku 4.5 LLM fallback to the existing Alfred-Eudic workflow, with per-source SQLite caching, error-tolerant degradation, ENTER-to-retry, and a couple of security fixes that fall in the same change footprint.

**Architecture:** Rust async (tokio) orchestrator dispatches three concurrent fetches (ECDICT sync + Urban + Wordnik), then conditionally invokes LLM only when Wordnik returns <5 results. A `DictionarySource` trait abstracts Urban/Wordnik; LLM is deliberately a sibling module (not a Source impl). Persistent caching via SQLite at `$alfred_workflow_cache/lookup_cache.db`, 7-day TTL, JSON-serialized values. Output is rendered into Alfred ScriptFilter items grouped by source with emoji prefixes.

**Tech Stack:** Rust 2024, tokio, reqwest (rustls), serde, serde_json, async-trait, once_cell, rusqlite, wiremock (dev). Anthropic API via raw HTTP (no SDK).

---

## File Map

**Create:**

- `src/http.rs` — shared reqwest clients (dict 2s, llm 8s)
- `src/sources/mod.rs` — `DictionarySource` trait, `DictEntry`, `SourceKind`, `SourceError`, `fetch_with_cache` helper
- `src/sources/urban.rs` — `UrbanClient`
- `src/sources/wordnik.rs` — `WordnikClient`
- `src/llm/mod.rs` — `LlmClient`, `LlmError`, `fetch_with_cache_llm` helper
- `src/llm/prompt.rs` — system + user prompt templates
- `src/llm/response.rs` — `LlmResult` struct + JSON parser
- `src/cache/mod.rs` — `Cache` trait, `CacheKind`, `CACHE_TTL_SECS`
- `src/cache/sqlite.rs` — `SqliteCache`
- `src/render.rs` — render functions for each source + errors + no-key prompts
- `tests/search_integration.rs` — end-to-end pipeline tests
- `info.plist` — Alfred workflow definition (extracted from installed copy)

**Modify:**

- `Cargo.toml` — add dependencies
- `src/main.rs` — register new modules; expose `lib.rs` style entry for tests
- `src/lib.rs` (new) — re-export modules so integration tests can call them
- `src/command/search.rs` — full rewrite into orchestrator
- `src/command/mod.rs` — re-exports
- `src/dictionary/database.rs` — fix SQL injection, open read-only
- `src/dictionary/manager.rs` — replace panic with empty-vec + log
- `script/search_eudic.sh` — pass word via env var (fix AppleScript injection)
- `script/speak_eudic.sh` — same fix
- `README.md` — add `WORDNIK_API_KEY` / `ANTHROPIC_API_KEY` configuration section

---

## Task 1: Add dependencies and convert to lib+bin layout

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Update `Cargo.toml`**

Replace the file contents with:

```toml
[package]
name = "alfred-eudic"
version = "0.1.0"
edition = "2024"
description = "Alfred workflow for Eudic dictionary search"

[lib]
name = "alfred_eudic"
path = "src/lib.rs"

[[bin]]
name = "alfred-eudic"
path = "src/main.rs"

[dependencies]
alfred = { git = "https://github.com/hanleylee/alfred-workflow-utils.git", tag = "0.0.5", features = ["script-filter", "updater", "updater-cli"] }
#alfred = { path = "../alfred-workflow-utils/alfred", features = ["script-filter", "updater", "updater-cli"] }
clap = { version = "4", features = ["env", "derive"] }
rusqlite = { version = "0.32", features = ["bundled"] }
tokio = { version = "1", features = ["fs", "rt-multi-thread", "macros", "time", "sync"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
once_cell = "1"

[dev-dependencies]
wiremock = "0.6"
tempfile = "3"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 2: Create `src/lib.rs`**

```rust
pub mod cache;
pub mod command;
pub mod dictionary;
pub mod http;
pub mod llm;
pub mod render;
pub mod sources;
pub mod workflow_utils;

pub const GITHUB_REPO: &str = "hanleylee/alfred-eudic-workflow";
pub const WORKFLOW_ASSET_NAME: &str = "EudicSearch.alfredworkflow";
pub const SEARCH_LIMIT: u32 = 30;

pub struct SearchArgs {
    pub completion_file: Option<String>,
    pub db_file: Option<String>,
    pub spell: String,
}
```

- [ ] **Step 3: Replace `src/main.rs`**

```rust
use alfred::updater_cli::{UpdateAction, run_default_update};
use alfred_eudic::{GITHUB_REPO, SearchArgs, WORKFLOW_ASSET_NAME, command::run_search};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "alfred-eudic")]
#[command(about = "Tool used to quickly search matched words by partial query")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Search {
        #[arg(long, env = "ALFRED_EUDIC_COMPLETION_FILE")]
        completion_file: Option<String>,
        #[arg(long, env = "ALFRED_EUDIC_DATABASE_FILE")]
        db_file: Option<String>,
        #[arg(default_value = "are")]
        spell: String,
    },
    Update {
        #[command(subcommand)]
        action: UpdateAction,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Search { completion_file, db_file, spell } => {
            run_search(SearchArgs { completion_file, db_file, spell }).await?
        }
        Commands::Update { action } => {
            run_default_update(GITHUB_REPO, WORKFLOW_ASSET_NAME, action).await?
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Create placeholder module files so the crate still compiles**

Create `src/cache/mod.rs`:
```rust
// populated in Task 7
```

Create `src/sources/mod.rs`:
```rust
// populated in Task 6
```

Create `src/llm/mod.rs`:
```rust
// populated in Task 11
```

Create `src/http.rs`:
```rust
// populated in Task 5
```

Create `src/render.rs`:
```rust
// populated in Task 12
```

Also update `src/command/mod.rs` to re-export properly. Read it first; current contents are likely `pub mod search; pub use search::run_search;`. Keep that.

- [ ] **Step 5: Verify build**

Run: `cargo build`
Expected: success, no errors. Warnings about unused modules are acceptable at this stage.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/main.rs src/cache/ src/sources/ src/llm/ src/http.rs src/render.rs
git commit -m "chore: add deps and convert to lib+bin layout

Required for integration tests to call orchestrator directly.
Adds reqwest, serde, async-trait, once_cell, wiremock(dev), tempfile(dev)."
```

---

## Task 2: Fix SQL injection in `database.rs`

**Files:**
- Modify: `src/dictionary/database.rs`

- [ ] **Step 1: Add failing test for SQL injection**

Append to `src/dictionary/database.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    fn make_fixture() -> NamedTempFile {
        let f = NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch("
            CREATE TABLE stardict (id INTEGER PRIMARY KEY, word TEXT, sw TEXT,
              phonetic TEXT, definition TEXT, translation TEXT, pos TEXT,
              collins INTEGER, oxford INTEGER, tag TEXT, bnc INTEGER, frq INTEGER,
              exchange TEXT, detail TEXT, audio TEXT);
            INSERT INTO stardict (word, sw) VALUES ('apple','apple'),('appendix','appendix'),('arc','arc');
        ").unwrap();
        f
    }

    #[test]
    fn prefix_match_finds_apple_and_appendix() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("app", 10).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn malicious_quote_does_not_inject() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        // Without parameterization this would close the LIKE string and inject.
        let r = db.search_word("a' OR '1'='1", 10).unwrap();
        assert_eq!(r.len(), 0, "injection attempt must not return all rows");
    }

    #[test]
    fn literal_percent_does_not_act_as_wildcard() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("a%", 10).unwrap();
        assert_eq!(r.len(), 0, "% must be escaped, not treated as wildcard");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib dictionary::database`
Expected: `malicious_quote_does_not_inject` fails (returns 3 instead of 0); `literal_percent_does_not_act_as_wildcard` may also fail.

- [ ] **Step 3: Fix the SQL with parameterized query + LIKE escaping + read-only DB**

Replace `src/dictionary/database.rs` contents:

```rust
use alfred::core::AlfredUtils;
use rusqlite::{Connection, OpenFlags};

use super::entry::StardictEntry;

const TABLE: &str = "stardict";

pub struct StardictDatabase {
    conn: Connection,
}

impl StardictDatabase {
    pub fn new(database_path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_with_flags(
            database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        AlfredUtils::log(format!("Connected to database (read-only) at {}", database_path));
        Ok(Self { conn })
    }

    pub fn search_word(&self, spell: &str, limit: u32) -> Result<Vec<StardictEntry>, rusqlite::Error> {
        if spell.is_empty() {
            return Ok(Vec::new());
        }
        let limit_i = limit.min(1000);
        let escaped = spell
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("{}%", escaped);
        let sql = format!("
        SELECT id, word, sw, phonetic, definition, translation, pos, collins, oxford, tag, bnc, frq, exchange, detail, audio
        FROM {TABLE}
        WHERE sw LIKE ?1 ESCAPE '\\'
        LIMIT ?2
        ");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params![pattern, limit_i])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(row_to_entry(row)?);
        }
        Ok(entries)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<StardictEntry, rusqlite::Error> {
    Ok(StardictEntry {
        id: row.get(0)?,
        word: row.get(1)?,
        sw: row.get(2)?,
        phonetic: row.get(3)?,
        definition: row.get(4)?,
        translation: row.get(5)?,
        pos: row.get(6)?,
        collins: row.get(7)?,
        oxford: row.get(8)?,
        tag: row.get(9)?,
        bnc: row.get(10)?,
        frq: row.get(11)?,
        exchange: row.get(12)?,
        detail: row.get(13)?,
        audio: row.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    fn make_fixture() -> NamedTempFile {
        let f = NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch("
            CREATE TABLE stardict (id INTEGER PRIMARY KEY, word TEXT, sw TEXT,
              phonetic TEXT, definition TEXT, translation TEXT, pos TEXT,
              collins INTEGER, oxford INTEGER, tag TEXT, bnc INTEGER, frq INTEGER,
              exchange TEXT, detail TEXT, audio TEXT);
            INSERT INTO stardict (word, sw) VALUES ('apple','apple'),('appendix','appendix'),('arc','arc');
        ").unwrap();
        f
    }

    #[test]
    fn prefix_match_finds_apple_and_appendix() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("app", 10).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn malicious_quote_does_not_inject() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("a' OR '1'='1", 10).unwrap();
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn literal_percent_does_not_act_as_wildcard() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("a%", 10).unwrap();
        assert_eq!(r.len(), 0);
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --lib dictionary::database`
Expected: all three tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/dictionary/database.rs
git commit -m "fix(security): parameterize stardict SQL and open DB read-only

- Switch from format!-built SQL to ?1/?2 parameters
- Escape LIKE wildcards (%, _) and backslash
- Open SQLite with SQLITE_OPEN_READ_ONLY"
```

---

## Task 3: Fix AppleScript injection in shell scripts

**Files:**
- Modify: `script/search_eudic.sh`
- Modify: `script/speak_eudic.sh`

- [ ] **Step 1: Rewrite `script/search_eudic.sh`**

Replace contents with:

```bash
#!/usr/bin/env bash
set -euo pipefail

Eudic_ID=$(osascript -e 'id of app "Eudb_en_free"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudb_en"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudic"' 2>/dev/null)

if [[ -z "$Eudic_ID" ]]; then
    osascript -e 'display dialog "Please install EuDic"'
    exit
fi

# Pass the word via env var so AppleScript reads it as a literal, not as inline source.
EUDIC_QUERY_WORD="${1:-}" EUDIC_APP_ID="$Eudic_ID" osascript <<'EOF'
tell application "System Events"
    set appId to (system attribute "EUDIC_APP_ID")
    set theWord to (system attribute "EUDIC_QUERY_WORD")
    do shell script "open -b " & quoted form of appId
    tell application id appId
        activate
        show dic with word theWord
    end tell
end tell
EOF
```

- [ ] **Step 2: Rewrite `script/speak_eudic.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

Eudic_ID=$(osascript -e 'id of app "Eudb_en_free"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudb_en"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudic"' 2>/dev/null)

if [[ -z "$Eudic_ID" ]]; then
    osascript -e 'display dialog "Please install EuDic"'
    exit
fi

EUDIC_QUERY_WORD="${1:-}" EUDIC_APP_ID="$Eudic_ID" osascript <<'EOF'
set appId to (system attribute "EUDIC_APP_ID")
set theWord to (system attribute "EUDIC_QUERY_WORD")
tell application id appId
    speak word with word theWord
end tell
EOF
```

- [ ] **Step 3: Manually verify (smoke test)**

Run from inside this repo:
```bash
./script/search_eudic.sh 'serendipity'
```
Expected: Eudic opens and shows "serendipity". No errors.

Then run with a hostile string:
```bash
./script/search_eudic.sh 'foo" & (do shell script "open -a Calculator") & "bar'
```
Expected: Eudic opens (or errors gracefully), but **Calculator does NOT launch**. The literal string is passed as a word.

- [ ] **Step 4: Commit**

```bash
git add script/search_eudic.sh script/speak_eudic.sh
git commit -m "fix(security): pass query word to osascript via env var

Previously \$1 was expanded by bash into a quoted heredoc, allowing
embedded \" and AppleScript metacharacters to break out and execute
arbitrary shell. Now the word is read via 'system attribute' inside
a single-quoted heredoc so AppleScript receives it as a literal string."
```

---

## Task 4: Replace panic in `manager.rs` with graceful empty return

**Files:**
- Modify: `src/dictionary/manager.rs`

- [ ] **Step 1: Locate the panic**

Read `src/dictionary/manager.rs` around line 46-53. The body of `find_matches_in_completion` panics if the completion file can't be read.

- [ ] **Step 2: Add failing test**

Append `#[cfg(test)]` module to `src/dictionary/manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_completion_file_returns_empty_not_panic() {
        let mgr = DictionaryManager::new(DictionaryConfig::new(None, None));
        let r = mgr.find_matches_in_completion("/nonexistent/path/xyz.txt", "ap", 5).await;
        assert!(r.is_empty());
    }
}
```

- [ ] **Step 3: Run test, observe panic**

Run: `cargo test --lib dictionary::manager::tests::missing_completion_file_returns_empty_not_panic`
Expected: FAIL (test panics).

- [ ] **Step 4: Replace the panic**

In `src/dictionary/manager.rs`, change the body of `find_matches_in_completion`:

Find:
```rust
        let content = match tokio::fs::read_to_string(completion_file).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Failed to read completion file: {}", e);
            }
        };
```

Replace with:
```rust
        let content = match tokio::fs::read_to_string(completion_file).await {
            Ok(c) => c,
            Err(e) => {
                AlfredUtils::log(format!("Failed to read completion file: {}", e));
                return Vec::new();
            }
        };
```

- [ ] **Step 5: Run test, verify pass**

Run: `cargo test --lib dictionary::manager`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src/dictionary/manager.rs
git commit -m "fix: gracefully handle missing completion file

Previously panic!() killed the process when completion file was
missing, even though other sources could still produce results."
```

---

## Task 5: Add shared HTTP module

**Files:**
- Modify: `src/http.rs`

- [ ] **Step 1: Replace `src/http.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::Client;

const USER_AGENT: &str = concat!("alfred-eudic/", env!("CARGO_PKG_VERSION"));

static DICT_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    Arc::new(
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(2))
            .build()
            .expect("dict reqwest client must build"),
    )
});

static LLM_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    Arc::new(
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(8))
            .build()
            .expect("llm reqwest client must build"),
    )
});

pub fn dict_client() -> Arc<Client> {
    DICT_CLIENT.clone()
}

pub fn llm_client() -> Arc<Client> {
    LLM_CLIENT.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_are_singletons() {
        let a = dict_client();
        let b = dict_client();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn dict_and_llm_are_distinct() {
        let d = dict_client();
        let l = llm_client();
        assert!(!Arc::ptr_eq(&d, &l));
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --lib http`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/http.rs
git commit -m "feat(http): add shared reqwest clients

Two singletons: dict_client (2s timeout) and llm_client (8s).
All sources reuse the connection pool."
```

---

## Task 6: Add sources trait and shared types

**Files:**
- Modify: `src/sources/mod.rs`

- [ ] **Step 1: Replace `src/sources/mod.rs`**

```rust
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheKind};

pub mod urban;
pub mod wordnik;

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
}
```

- [ ] **Step 2: Create empty submodule files for compilation**

Create `src/sources/urban.rs`:
```rust
// populated in Task 9
```

Create `src/sources/wordnik.rs`:
```rust
// populated in Task 10
```

- [ ] **Step 3: Note**: tests in Step 1 reference `SqliteCache::in_memory()` which is added in Task 7. Until Task 7 is done, `cargo test --lib sources` will fail to compile. That is expected — `cargo build` (no tests) should still pass. If you want to verify Task 6 in isolation, comment out the `#[cfg(test)]` block temporarily, then uncomment after Task 7.

- [ ] **Step 4: Verify build**

Run: `cargo build`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add src/sources/
git commit -m "feat(sources): add DictionarySource trait and fetch_with_cache

Defines DictEntry, SourceKind, SourceError, and a cache-aware
fetch wrapper that normalizes the cache key (trim+lowercase)
and honors a bypass flag."
```

---

## Task 7: Add cache trait and SQLite implementation

**Files:**
- Modify: `src/cache/mod.rs`
- Create: `src/cache/sqlite.rs`

- [ ] **Step 1: Replace `src/cache/mod.rs`**

```rust
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
```

- [ ] **Step 2: Write failing tests in `src/cache/sqlite.rs`**

```rust
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags, params};

use super::{CACHE_TTL_SECS, Cache, CacheKind};

pub struct SqliteCache {
    conn: Mutex<Connection>,
}

impl SqliteCache {
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_millis(500))?;
        Self::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        for kind in [CacheKind::Urban, CacheKind::Wordnik, CacheKind::Llm] {
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {} (\
                    key TEXT PRIMARY KEY, \
                    value BLOB NOT NULL, \
                    fetched_at INTEGER NOT NULL\
                 )",
                kind.table()
            );
            conn.execute(&sql, [])?;
        }
        Ok(())
    }
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[async_trait]
impl Cache for SqliteCache {
    async fn get(&self, kind: CacheKind, key: &str) -> Option<Vec<u8>> {
        let conn = self.conn.lock().ok()?;
        let sql = format!("SELECT value, fetched_at FROM {} WHERE key = ?1", kind.table());
        let mut stmt = conn.prepare(&sql).ok()?;
        let mut rows = stmt.query(params![key]).ok()?;
        let row = rows.next().ok()??;
        let value: Vec<u8> = row.get(0).ok()?;
        let fetched_at: i64 = row.get(1).ok()?;
        if now_secs() - fetched_at > CACHE_TTL_SECS {
            return None;
        }
        Some(value)
    }

    async fn put(&self, kind: CacheKind, key: &str, value: &[u8]) {
        let Ok(conn) = self.conn.lock() else { return };
        let sql = format!(
            "INSERT INTO {} (key, value, fetched_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, fetched_at=excluded.fetched_at",
            kind.table()
        );
        let _ = conn.execute(&sql, params![key, value, now_secs()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "hello", b"world").await;
        let v = c.get(CacheKind::Urban, "hello").await;
        assert_eq!(v.as_deref(), Some(&b"world"[..]));
    }

    #[tokio::test]
    async fn kinds_are_isolated() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "k", b"u").await;
        c.put(CacheKind::Wordnik, "k", b"w").await;
        assert_eq!(c.get(CacheKind::Urban, "k").await.as_deref(), Some(&b"u"[..]));
        assert_eq!(c.get(CacheKind::Wordnik, "k").await.as_deref(), Some(&b"w"[..]));
        assert_eq!(c.get(CacheKind::Llm, "k").await, None);
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let c = SqliteCache::in_memory().unwrap();
        assert_eq!(c.get(CacheKind::Urban, "missing").await, None);
    }

    #[tokio::test]
    async fn put_overwrites() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "k", b"v1").await;
        c.put(CacheKind::Urban, "k", b"v2").await;
        assert_eq!(c.get(CacheKind::Urban, "k").await.as_deref(), Some(&b"v2"[..]));
    }

    #[tokio::test]
    async fn expired_entry_returns_none() {
        let c = SqliteCache::in_memory().unwrap();
        // Insert directly with old timestamp.
        {
            let conn = c.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO cache_urban (key, value, fetched_at) VALUES (?1, ?2, ?3)",
                params!["old", &b"v"[..], now_secs() - CACHE_TTL_SECS - 1],
            ).unwrap();
        }
        assert_eq!(c.get(CacheKind::Urban, "old").await, None);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib cache`
Expected: 5 cache tests pass + the 2 sources tests from Task 6 (`cache_hit_skips_fetch`, `bypass_forces_refetch`) now compile and pass.

Also run: `cargo test --lib sources`
Expected: both Task 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/cache/
git commit -m "feat(cache): add SQLite cache with 7-day TTL

One table per CacheKind. INSERT OR REPLACE semantics on put.
get returns None for entries past CACHE_TTL_SECS."
```

---

## Task 8: Implement Urban Dictionary client

**Files:**
- Modify: `src/sources/urban.rs`

- [ ] **Step 1: Write `src/sources/urban.rs` with embedded tests**

```rust
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{DictEntry, DictionarySource, SourceError, SourceKind};

const BASE_URL: &str = "https://api.urbandictionary.com/v0/define";
const TOP_N: usize = 3;
const MAX_DEF_CHARS: usize = 220;

pub struct UrbanClient {
    http: Arc<Client>,
    base_url: String,
}

impl UrbanClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, base_url: String) -> Self {
        Self { http, base_url }
    }
}

#[derive(Deserialize)]
struct UrbanResponse {
    list: Vec<UrbanItem>,
}

#[derive(Deserialize)]
struct UrbanItem {
    definition: String,
    #[serde(default)]
    example: String,
    thumbs_up: i64,
    thumbs_down: i64,
}

fn clean(s: &str) -> String {
    // Urban wraps cross-refs in [brackets]; strip them and collapse whitespace.
    let stripped: String = s.chars().filter(|c| *c != '[' && *c != ']').collect();
    let collapsed: String = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > MAX_DEF_CHARS {
        let truncated: String = collapsed.chars().take(MAX_DEF_CHARS).collect();
        format!("{}…", truncated)
    } else {
        collapsed
    }
}

#[async_trait]
impl DictionarySource for UrbanClient {
    fn kind(&self) -> SourceKind { SourceKind::Urban }

    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError> {
        let resp = self.http.get(&self.base_url).query(&[("term", spell)]).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::RateLimited);
        }
        if !status.is_success() {
            return Err(SourceError::BadResponse(format!("HTTP {}", status)));
        }
        let body: UrbanResponse = resp.json().await
            .map_err(|e| SourceError::BadResponse(e.to_string()))?;
        let mut items = body.list;
        items.sort_by_key(|i| -i.thumbs_up);
        let entries = items.into_iter().take(TOP_N).map(|i| {
            let mut def = clean(&i.definition);
            if !i.example.trim().is_empty() {
                def.push_str("  e.g. ");
                def.push_str(&clean(&i.example));
            }
            DictEntry {
                headword: spell.to_string(),
                definition: def,
                extra: Some(format!("👍 {}  👎 {}", i.thumbs_up, i.thumbs_down)),
            }
        }).collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_response_and_sorts_by_thumbs_up() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("term", "rizz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "list": [
                    {"definition": "low one", "example": "", "thumbs_up": 5, "thumbs_down": 0},
                    {"definition": "high one [charisma]", "example": "He's got rizz", "thumbs_up": 500, "thumbs_down": 3},
                    {"definition": "mid one", "example": "", "thumbs_up": 100, "thumbs_down": 0}
                ]
            })))
            .mount(&server)
            .await;

        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let r = client.fetch("rizz").await.unwrap();
        assert_eq!(r.len(), 3);
        assert!(r[0].definition.starts_with("high one"));
        assert!(r[0].definition.contains("charisma"));
        assert!(!r[0].definition.contains('['));
        assert_eq!(r[0].extra.as_deref(), Some("👍 500  👎 3"));
    }

    #[tokio::test]
    async fn empty_list_returns_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"list": []})))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let r = client.fetch("asdfqwer").await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn http_500_is_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let err = client.fetch("x").await.unwrap_err();
        assert!(matches!(err, SourceError::BadResponse(_)));
    }

    #[tokio::test]
    async fn rate_limit_is_distinct_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        let client = UrbanClient::with_base_url(dict_client(), format!("{}/", server.uri()));
        let err = client.fetch("x").await.unwrap_err();
        assert!(matches!(err, SourceError::RateLimited));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::urban`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/sources/urban.rs
git commit -m "feat(sources): Urban Dictionary client

GET /v0/define?term=X; takes top 3 by thumbs_up; strips
[bracket] cross-refs; truncates long definitions; appends
example as 'e.g. ...' if present."
```

---

## Task 9: Implement Wordnik client

**Files:**
- Modify: `src/sources/wordnik.rs`

- [ ] **Step 1: Write `src/sources/wordnik.rs`**

```rust
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{DictEntry, DictionarySource, SourceError, SourceKind};

const BASE_URL: &str = "https://api.wordnik.com/v4/word.json";
const SOURCE_DICTS: &str = "ahd-5,wiktionary,wordnet";
const FETCH_LIMIT: u32 = 10;

pub struct WordnikClient {
    http: Arc<Client>,
    api_key: String,
    base_url: String,
}

impl WordnikClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, api_key, base_url: BASE_URL.to_string() }
    }

    #[doc(hidden)]
    pub fn with_base_url(http: Arc<Client>, api_key: String, base_url: String) -> Self {
        Self { http, api_key, base_url }
    }
}

#[derive(Deserialize)]
struct WordnikDef {
    #[serde(default)]
    text: Option<String>,
    #[serde(rename = "partOfSpeech", default)]
    part_of_speech: Option<String>,
    #[serde(rename = "sourceDictionary", default)]
    source_dictionary: Option<String>,
}

#[async_trait]
impl DictionarySource for WordnikClient {
    fn kind(&self) -> SourceKind { SourceKind::Wordnik }

    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError> {
        if self.api_key.is_empty() {
            return Err(SourceError::NoApiKey);
        }
        // URL-encode the word into the path segment.
        let encoded: String = spell
            .chars()
            .flat_map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '\'' {
                    vec![c]
                } else if c == ' ' {
                    vec!['+']
                } else {
                    // percent-encode other bytes
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    s.bytes().flat_map(|b| format!("%{:02X}", b).chars().collect::<Vec<_>>()).collect()
                }
            })
            .collect();
        let url = format!("{}/{}/definitions", self.base_url, encoded);
        let resp = self.http.get(&url)
            .query(&[
                ("limit", FETCH_LIMIT.to_string()),
                ("includeRelated", "false".to_string()),
                ("sourceDictionaries", SOURCE_DICTS.to_string()),
                ("useCanonical", "true".to_string()),
                ("api_key", self.api_key.clone()),
            ])
            .send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::RateLimited);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(SourceError::BadResponse(format!("HTTP {}", status)));
        }
        let defs: Vec<WordnikDef> = resp.json().await
            .map_err(|e| SourceError::BadResponse(e.to_string()))?;
        let entries = defs.into_iter().filter_map(|d| {
            let text = d.text?.trim().to_string();
            if text.is_empty() { return None; }
            let pos = d.part_of_speech.unwrap_or_default();
            let src = d.source_dictionary.unwrap_or_default();
            let extra = match (pos.is_empty(), src.is_empty()) {
                (false, false) => Some(format!("{} · {}", pos, src)),
                (false, true)  => Some(pos),
                (true, false)  => Some(src),
                (true, true)   => None,
            };
            Some(DictEntry {
                headword: spell.to_string(),
                definition: text,
                extra,
            })
        }).collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dict_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_definitions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/word.json/serendipity/definitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"text": "good luck in making unexpected discoveries", "partOfSpeech": "noun", "sourceDictionary": "ahd-5"},
                {"text": "the fact of finding interesting things by chance", "partOfSpeech": "noun", "sourceDictionary": "wiktionary"}
            ])))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "test-key".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("serendipity").await.unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[0].definition.contains("good luck"));
        assert_eq!(r[0].extra.as_deref(), Some("noun · ahd-5"));
    }

    #[tokio::test]
    async fn no_api_key_returns_error() {
        let client = WordnikClient::new(dict_client(), "".to_string());
        assert!(matches!(client.fetch("x").await, Err(SourceError::NoApiKey)));
    }

    #[tokio::test]
    async fn http_404_returns_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "test-key".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("asdfqwer").await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn drops_entries_with_empty_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"text": "good"},
                {"text": ""},
                {"text": "   "}
            ])))
            .mount(&server)
            .await;
        let client = WordnikClient::with_base_url(
            dict_client(),
            "k".to_string(),
            format!("{}/word.json", server.uri()),
        );
        let r = client.fetch("x").await.unwrap();
        assert_eq!(r.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sources::wordnik`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/sources/wordnik.rs
git commit -m "feat(sources): Wordnik client

GET /v4/word.json/<spell>/definitions with ahd-5,wiktionary,wordnet
sources. NoApiKey when key missing; 404 -> empty vec; 429 -> RateLimited."
```

---

## Task 10: Implement LLM client (Anthropic Haiku)

**Files:**
- Modify: `src/llm/mod.rs`
- Create: `src/llm/prompt.rs`
- Create: `src/llm/response.rs`

- [ ] **Step 1: Create `src/llm/prompt.rs`**

```rust
pub const SYSTEM: &str = "You are a concise bilingual English-Chinese dictionary. \
Detect the input language. Output JSON only, no prose, no markdown fences.";

pub fn user(spell: &str) -> String {
    format!(
        "Word: \"{}\". Output exactly:\n\
         {{\"translations\":[\"释义1\",\"释义2\"],\"example\":\"example sentence\"}}\n\
         Rules: 1-3 translations, each ≤20 Chinese chars or ≤8 English words. \
         Example in the opposite language, ≤15 words. No extra text outside the JSON.",
        spell.replace('"', "'")
    )
}
```

- [ ] **Step 2: Create `src/llm/response.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResult {
    pub translations: Vec<String>,
    #[serde(default)]
    pub example: Option<String>,
}

/// Extract a JSON object from possibly-noisy model output.
/// Handles: clean JSON, JSON in code fences, JSON surrounded by prose.
pub fn parse_llm_json(raw: &str) -> Result<LlmResult, String> {
    let trimmed = raw.trim();
    // Strip ```json ... ``` fences if present.
    let stripped = if let Some(rest) = trimmed.strip_prefix("```") {
        let after_lang = rest.splitn(2, '\n').nth(1).unwrap_or(rest);
        after_lang.trim_end_matches("```").trim()
    } else {
        trimmed
    };
    // Find the outermost {...}.
    let start = stripped.find('{').ok_or_else(|| "no '{' in output".to_string())?;
    let end = stripped.rfind('}').ok_or_else(|| "no '}' in output".to_string())?;
    if end < start {
        return Err("malformed JSON braces".to_string());
    }
    let json = &stripped[start..=end];
    serde_json::from_str::<LlmResult>(json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_json() {
        let r = parse_llm_json(r#"{"translations":["A","B"],"example":"x"}"#).unwrap();
        assert_eq!(r.translations, vec!["A", "B"]);
        assert_eq!(r.example.as_deref(), Some("x"));
    }

    #[test]
    fn fenced_json() {
        let r = parse_llm_json("```json\n{\"translations\":[\"A\"]}\n```").unwrap();
        assert_eq!(r.translations, vec!["A"]);
        assert_eq!(r.example, None);
    }

    #[test]
    fn surrounding_prose() {
        let r = parse_llm_json("Here you go: {\"translations\":[\"A\"]} done").unwrap();
        assert_eq!(r.translations, vec!["A"]);
    }

    #[test]
    fn rejects_no_braces() {
        assert!(parse_llm_json("hello").is_err());
    }

    #[test]
    fn rejects_missing_field() {
        assert!(parse_llm_json("{\"example\":\"x\"}").is_err());
    }
}
```

- [ ] **Step 3: Replace `src/llm/mod.rs`**

```rust
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::cache::{Cache, CacheKind};

pub mod prompt;
pub mod response;

pub use response::LlmResult;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-haiku-4-5";
const MAX_TOKENS: u32 = 200;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct LlmClient {
    http: Arc<Client>,
    api_key: String,
    endpoint: String,
}

impl LlmClient {
    pub fn new(http: Arc<Client>, api_key: String) -> Self {
        Self { http, api_key, endpoint: ENDPOINT.to_string() }
    }

    #[doc(hidden)]
    pub fn with_endpoint(http: Arc<Client>, api_key: String, endpoint: String) -> Self {
        Self { http, api_key, endpoint }
    }

    pub async fn translate(&self, spell: &str) -> Result<LlmResult, LlmError> {
        if self.api_key.is_empty() {
            return Err(LlmError::NoApiKey);
        }
        let body = serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "system": prompt::SYSTEM,
            "messages": [{"role": "user", "content": prompt::user(spell)}]
        });
        let resp = self.http.post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send().await
            .map_err(LlmError::from)?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status: status.as_u16(), body: body_text });
        }
        let api_resp: AnthropicResponse = resp.json().await
            .map_err(|e| LlmError::BadJson(format!("envelope: {}", e)))?;
        if api_resp.stop_reason.as_deref() == Some("refusal") {
            return Err(LlmError::Refused);
        }
        let text = api_resp.content.iter()
            .find_map(|c| if c.kind == "text" { Some(c.text.as_str()) } else { None })
            .ok_or_else(|| LlmError::BadJson("no text block".to_string()))?;
        response::parse_llm_json(text).map_err(LlmError::BadJson)
    }
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    stop_reason: Option<String>,
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug)]
pub enum LlmError {
    Http(String),
    Timeout,
    BadJson(String),
    ApiError { status: u16, body: String },
    Refused,
    NoApiKey,
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::Http(e) => write!(f, "HTTP error: {}", e),
            LlmError::Timeout => write!(f, "request timeout"),
            LlmError::BadJson(s) => write!(f, "bad JSON: {}", s),
            LlmError::ApiError { status, .. } => write!(f, "API error {}", status),
            LlmError::Refused => write!(f, "refused by model"),
            LlmError::NoApiKey => write!(f, "no API key configured"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<reqwest::Error> for LlmError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() { LlmError::Timeout } else { LlmError::Http(e.to_string()) }
    }
}

/// Cache-aware LLM call. Only caches successful results.
pub async fn fetch_with_cache_llm(
    client: &LlmClient,
    cache: Arc<dyn Cache>,
    spell: &str,
    bypass: bool,
) -> Result<LlmResult, LlmError> {
    let key = spell.trim().to_lowercase();
    if !bypass {
        if let Some(bytes) = cache.get(CacheKind::Llm, &key).await {
            if let Ok(result) = serde_json::from_slice::<LlmResult>(&bytes) {
                return Ok(result);
            }
        }
    }
    let result = client.translate(spell).await?;
    if let Ok(bytes) = serde_json::to_vec(&result) {
        cache.put(CacheKind::Llm, &key, &bytes).await;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::llm_client;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ok_envelope(text: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "msg_1",
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": text}]
        })
    }

    #[tokio::test]
    async fn happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                ok_envelope(r#"{"translations":["机缘"],"example":"What serendipity!"}"#)
            ))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        let r = client.translate("serendipity").await.unwrap();
        assert_eq!(r.translations, vec!["机缘"]);
    }

    #[tokio::test]
    async fn no_api_key() {
        let client = LlmClient::new(llm_client(), "".into());
        assert!(matches!(client.translate("x").await, Err(LlmError::NoApiKey)));
    }

    #[tokio::test]
    async fn non_json_text_is_bad_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope("I don't know this word")))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        assert!(matches!(client.translate("x").await, Err(LlmError::BadJson(_))));
    }

    #[tokio::test]
    async fn refusal_stop_reason() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "stop_reason": "refusal",
                "content": [{"type": "text", "text": ""}]
            })))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        assert!(matches!(client.translate("x").await, Err(LlmError::Refused)));
    }

    #[tokio::test]
    async fn http_400_is_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid_request"))
            .mount(&server)
            .await;
        let client = LlmClient::with_endpoint(llm_client(), "k".into(), format!("{}/v1/messages", server.uri()));
        let err = client.translate("x").await.unwrap_err();
        match err {
            LlmError::ApiError { status, .. } => assert_eq!(status, 400),
            _ => panic!("expected ApiError"),
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib llm`
Expected: 5 + 5 = 10 tests pass (response.rs has 5; mod.rs has 5).

- [ ] **Step 5: Commit**

```bash
git add src/llm/
git commit -m "feat(llm): Anthropic Haiku 4.5 translator with cache wrapper

Sends a constrained JSON-only prompt; tolerates fenced/prose-wrapped
output via parse_llm_json. Maps Anthropic envelope to LlmError variants
(NoApiKey, Refused, BadJson, ApiError, Timeout, Http)."
```

---

## Task 11: Implement render module

**Files:**
- Modify: `src/render.rs`

- [ ] **Step 1: Replace `src/render.rs`**

```rust
use alfred::script_filter::{Item, Mod, Variable};

use crate::dictionary::entry::StardictEntry;
use crate::llm::LlmResult;
use crate::sources::{DictEntry, SourceKind};
use crate::workflow_utils;

const README_CONFIG_URL: &str = "https://github.com/hanleylee/alfred-eudic-workflow#%E5%AE%89%E8%A3%85";
const MAX_SUBTITLE_CHARS: usize = 220;

fn truncate(s: &str) -> String {
    if s.chars().count() <= MAX_SUBTITLE_CHARS { return s.to_string(); }
    let head: String = s.chars().take(MAX_SUBTITLE_CHARS).collect();
    format!("{}…", head)
}

fn source_emoji(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Urban => "🔥",
        SourceKind::Wordnik => "📘",
    }
}

pub fn render_dict(entries: &[DictEntry], kind: SourceKind) -> Vec<Item> {
    entries.iter().enumerate().map(|(i, e)| {
        let prefix = if i == 0 { source_emoji(kind) } else { "  " };
        let title = format!("{} {}", prefix, e.headword);
        let subtitle = workflow_utils::aligned_text(
            &truncate(&e.definition),
            e.extra.as_deref().unwrap_or(""),
        );
        Item::new(title).subtitle(subtitle).arg(&e.headword)
    }).collect()
}

pub fn render_llm(result: &LlmResult, spell: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for (i, t) in result.translations.iter().enumerate() {
        let prefix = if i == 0 { "🤖" } else { "  " };
        let title = format!("{} {}", prefix, spell);
        let subtitle = if let Some(ex) = &result.example {
            workflow_utils::aligned_text(t, ex)
        } else {
            t.clone()
        };
        items.push(Item::new(title).subtitle(subtitle).arg(spell));
    }
    items
}

pub fn render_ecdict(entries: &[StardictEntry]) -> Vec<Item> {
    entries.iter().enumerate().map(|(i, entry)| {
        let explanation = entry.translation.as_ref()
            .or(entry.definition.as_ref())
            .map(|s| s.replace('\n', "; "))
            .unwrap_or_default();
        let phonetic = entry.phonetic.as_deref().unwrap_or("");
        let collins_rate = "⭐️".repeat(entry.collins.unwrap_or(0) as usize);
        let mut importance: Vec<String> = Vec::new();
        if let Some(c) = entry.collins { importance.push(format!("COLLINS: {}", "⭐️".repeat(c as usize))); }
        if entry.oxford.is_some() { importance.push("OXFORD 3000".into()); }
        if let Some(bnc) = entry.bnc { if bnc != 0 { importance.push(format!("BNC: {}", bnc)); } }
        if let Some(frq) = entry.frq { if frq != 0 { importance.push(format!("COCA: {}", frq)); } }
        if let Some(tag) = entry.tag_info() { importance.push(tag); }
        let prefix = if i == 0 { "📕" } else { "  " };
        let title = workflow_utils::aligned_text(&format!("{} {}", prefix, entry.word), &collins_rate);
        let subtitle = workflow_utils::aligned_text(&truncate(&explanation), phonetic);
        let cmd_subtitle = entry.exchange_info().unwrap_or_default();
        let alt_subtitle = importance.join("; ");
        Item::new(title)
            .subtitle(subtitle)
            .arg(&entry.word)
            .cmd(Mod::new().subtitle(cmd_subtitle))
            .alt(Mod::new().subtitle(alt_subtitle))
    }).collect()
}

pub fn render_no_api_key(source_name: &str) -> Item {
    Item::new(format!("⚙️ {} 未配置 API key", source_name))
        .subtitle("回车查看配置说明")
        .arg(README_CONFIG_URL)
}

pub fn render_error(source_name: &str, err_msg: &str, spell: &str) -> Item {
    Item::new(format!("⚠️ {}: {}", source_name, err_msg))
        .subtitle("回车重试（绕过缓存）")
        .arg(spell)
        .variable(Variable::new(Some("BYPASS_CACHE".into()), Some("1".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_first_item_has_emoji_prefix() {
        let entries = vec![
            DictEntry { headword: "rizz".into(), definition: "charisma".into(), extra: None },
            DictEntry { headword: "rizz".into(), definition: "skill".into(), extra: None },
        ];
        let items = render_dict(&entries, SourceKind::Urban);
        let json = serde_json::to_value(&items[0]).unwrap();
        let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("");
        assert!(title.starts_with("🔥"));
        let json2 = serde_json::to_value(&items[1]).unwrap();
        let title2 = json2.get("title").and_then(|v| v.as_str()).unwrap_or("");
        assert!(!title2.starts_with("🔥"));
    }

    #[test]
    fn llm_renders_translations() {
        let r = LlmResult {
            translations: vec!["机缘".to_string(), "巧合".to_string()],
            example: Some("What a serendipity!".to_string()),
        };
        let items = render_llm(&r, "serendipity");
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn error_item_sets_bypass_variable() {
        let item = render_error("Wordnik", "timeout", "rizz");
        let json = serde_json::to_value(&item).unwrap();
        let vars = json.get("variables").cloned().unwrap_or_default();
        assert_eq!(vars.get("BYPASS_CACHE").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn long_definition_is_truncated() {
        let long = "a".repeat(500);
        let entries = vec![DictEntry { headword: "x".into(), definition: long, extra: None }];
        let items = render_dict(&entries, SourceKind::Wordnik);
        let json = serde_json::to_value(&items[0]).unwrap();
        let subtitle = json.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
        assert!(subtitle.contains('…'));
    }
}
```

- [ ] **Step 2: Make `StardictEntry` and submodules accessible**

The current `src/dictionary/mod.rs` exposes only `DictionaryConfig` and `DictionaryManager`. Tests in render.rs need `StardictEntry` exposed. Update `src/dictionary/mod.rs`:

```rust
pub mod database;
pub mod entry;
pub mod manager;
mod completion_words;

pub use entry::StardictEntry;
pub use manager::{DictionaryConfig, DictionaryManager};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib render`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/render.rs src/dictionary/mod.rs
git commit -m "feat(render): unified rendering for all sources

Per-source emoji prefix on the first item only; truncates long
definitions; error items carry BYPASS_CACHE=1 variable for retry."
```

---

## Task 12: Wire the orchestrator in `command/search.rs`

**Files:**
- Modify: `src/command/search.rs`
- Modify: `src/command/mod.rs`

This is the biggest task. Read each step carefully.

- [ ] **Step 1: Replace `src/command/mod.rs`**

```rust
pub mod search;
pub use search::run_search;
```

- [ ] **Step 2: Replace `src/command/search.rs`**

```rust
use std::env;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alfred::core::{AlfredConst, AlfredUtils};
use alfred::script_filter::{Item, Mod, ScriptFilter, Variable};
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
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: success. Resolve any compilation errors by re-reading affected source files.

- [ ] **Step 4: Run all unit tests**

Run: `cargo test --lib`
Expected: all tests from Tasks 2–11 pass.

- [ ] **Step 5: Commit**

```bash
git add src/command/
git commit -m "feat(search): orchestrate ECDICT + Urban + Wordnik + LLM

Concurrent ECDICT (sync) + Urban + Wordnik via tokio::join!.
LLM invoked only when Wordnik returns <5 OR errored, AND spell ≤50 chars.
Cache opens at \$alfred_workflow_cache/lookup_cache.db; failure degrades
to in-memory cache (effectively no-cache). Per-source error or no-key
rendering. Items ordered: fallback, ECDICT, Wordnik, Urban, LLM."
```

---

## Task 13: Integration test of the full pipeline

**Files:**
- Create: `tests/search_integration.rs`

- [ ] **Step 1: Write the integration test**

```rust
//! End-to-end pipeline test. We do not exercise stdout output — instead,
//! we test the internals of `run_search`'s data assembly via a thin re-export
//! seam. To avoid heavy refactoring, this test spawns mock HTTP servers,
//! injects URLs through env vars (where the real binary doesn't read them),
//! and instead reaches into the underlying source clients directly.

use std::sync::Arc;

use alfred_eudic::cache::Cache;
use alfred_eudic::cache::sqlite::SqliteCache;
use alfred_eudic::llm::{LlmClient, fetch_with_cache_llm};
use alfred_eudic::sources::{
    DictionarySource, SourceKind, fetch_with_cache,
    urban::UrbanClient, wordnik::WordnikClient,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client() -> Arc<reqwest::Client> {
    Arc::new(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build().unwrap())
}

#[tokio::test]
async fn wordnik_with_many_results_does_not_trigger_llm_concept() {
    // We model the concept here: when Wordnik returns >=5, LLM should not be invoked.
    // Since run_search reads env vars and not parameters, this test simulates the
    // decision logic by checking that fetch_with_cache yields >=5 entries.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"text": "a"}, {"text": "b"}, {"text": "c"}, {"text": "d"}, {"text": "e"}, {"text": "f"}
        ])))
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let wn = WordnikClient::with_base_url(client(), "k".into(), format!("{}/word.json", server.uri()));
    let r = fetch_with_cache(&wn as &dyn DictionarySource, cache.clone(), "x", false).await.unwrap();
    assert!(r.len() >= 5, "Wordnik should return >=5 to skip LLM");
}

#[tokio::test]
async fn cache_hit_avoids_second_http_call() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "list": [{"definition": "d", "example": "", "thumbs_up": 1, "thumbs_down": 0}]
        })))
        .expect(1) // CRITICAL: server must be called exactly once
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let ub = UrbanClient::with_base_url(client(), format!("{}/", server.uri()));
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", false).await.unwrap();
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "Hello", false).await.unwrap();
    // wiremock's .expect(1) is verified at MockServer drop.
}

#[tokio::test]
async fn bypass_cache_refetches() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "list": [{"definition": "d", "example": "", "thumbs_up": 1, "thumbs_down": 0}]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let ub = UrbanClient::with_base_url(client(), format!("{}/", server.uri()));
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", false).await.unwrap();
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", true).await.unwrap();
}

#[tokio::test]
async fn llm_cache_hit_avoids_second_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "{\"translations\":[\"机缘\"]}"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let llm = LlmClient::with_endpoint(client(), "k".into(), format!("{}/v1/messages", server.uri()));
    let _ = fetch_with_cache_llm(&llm, cache.clone(), "serendipity", false).await.unwrap();
    let _ = fetch_with_cache_llm(&llm, cache.clone(), "SERENDIPITY", false).await.unwrap();
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test search_integration`
Expected: 4 tests pass.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test`
Expected: all unit + integration tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/search_integration.rs
git commit -m "test: integration tests for source + cache + LLM pipeline

Validates cache hit/bypass for Urban, Wordnik, and LLM clients
using wiremock with strict call-count expectations."
```

---

## Task 14: Extract and commit `info.plist`

**Files:**
- Create: `info.plist`

- [ ] **Step 1: Locate the installed workflow**

Run from any directory:
```bash
find ~/Library/Application\ Support/Alfred/Alfred.alfredpreferences/workflows -name info.plist -exec grep -l "EudicSearch\|alfred-eudic" {} \;
```

This prints the path to the installed `info.plist` for this workflow. Save it as `$PLIST_SRC`.

If you don't have the workflow installed yet, download the latest `.alfredworkflow` from https://github.com/hanleylee/alfred-eudic-workflow/releases and double-click it to install, then re-run the find.

- [ ] **Step 2: Copy into repo as XML**

```bash
cp "$PLIST_SRC" info.plist
plutil -convert xml1 info.plist
plutil -lint info.plist
```

Expected: "info.plist: OK".

- [ ] **Step 3: Note: do NOT modify the plist yet**

Task 15 adds the retry node. This task only brings the existing plist under version control.

- [ ] **Step 4: Update `.gitignore` to not ignore info.plist**

Check `.gitignore`. If it has `info.plist` or `*.plist`, remove that line. Currently it does not.

- [ ] **Step 5: Commit**

```bash
git add info.plist
git commit -m "chore: track workflow info.plist under version control

Extracted from the installed workflow. Future changes to the
Alfred workflow graph are now reviewable in the repo."
```

---

## Task 15: Wire the retry path in `info.plist`

**Files:**
- Modify: `info.plist`

The retry mechanism: error items emit `BYPASS_CACHE=1` via their `variables` map. We add a path in the workflow graph so that when ENTER is pressed on such an item, control loops back through a second copy of the script filter (or through an "Arg and Vars" object back to a Conditional that routes BYPASS_CACHE=1 inputs to a re-run of the search script filter).

- [ ] **Step 1: Inspect the existing workflow graph**

Run:
```bash
plutil -p info.plist | head -100
```

Identify the script filter's UID (it has `"type" => "alfred.workflow.input.scriptfilter"`). Note its UID.

- [ ] **Step 2: Manual workflow edit (Alfred UI)**

Because plist graph edits are tedious to write by hand, do this step in Alfred Preferences for accuracy:

1. Open Alfred Preferences → Workflows → Eudic Search
2. Right-click the canvas, add: **Utilities → Arg and Vars** object
   - Configure: `Argument = {query}`; no additional variables (the upstream item already supplies `BYPASS_CACHE=1`)
3. Connect the Script Filter's output → new Arg-and-Vars object
4. Right-click the canvas, add a **second Script Filter** with identical configuration to the first one (keyword can be left blank — it'll be triggered by upstream connection)
   - Script: same as the original (`./alfred-eudic search ...`)
5. Connect the Arg-and-Vars output → second Script Filter
6. Connect the second Script Filter's output back to wherever the original goes (the `search_eudic.sh` Run Script object)

This creates a loop: error item ENTER → Arg-and-Vars (carries BYPASS_CACHE=1 inherited from item variables) → re-run script filter (env now has BYPASS_CACHE=1) → updated results.

- [ ] **Step 3: Export plist back to repo**

```bash
# Get path of installed workflow (same as Task 14 Step 1).
cp "$PLIST_SRC" info.plist
plutil -convert xml1 info.plist
plutil -lint info.plist
```

- [ ] **Step 4: Verify the retry path manually**

Build a debug binary:
```bash
cargo build --release
cp target/release/alfred-eudic ~/Library/Application\ Support/Alfred/Alfred.alfredpreferences/workflows/<UUID>/
```

Then in Alfred:
1. Type `e xyz` (a word likely to error or produce no Wordnik results)
2. With Wordnik key INTENTIONALLY blanked in workflow config, see `⚙️ Wordnik 未配置 API key`
3. Press ENTER on a `⚠️` retry item (if one appears for an error case)
4. Observe a second invocation; check Alfred debugger that `BYPASS_CACHE` was set in env

- [ ] **Step 5: Commit**

```bash
git add info.plist
git commit -m "feat(workflow): add retry path with BYPASS_CACHE variable

Adds a second script-filter node downstream of the primary one via
an Arg-and-Vars object. Error items emit BYPASS_CACHE=1 which the
loop carries through, causing the re-run to skip cache.get()."
```

---

## Task 16: Update README with new configuration

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Read current README**

Read `README.md` to find a good insertion point — likely between the existing "搜索列表" section and "Feature" section.

- [ ] **Step 2: Add new sections**

Insert before "## Feature":

```markdown
### Urban Dictionary 与 Wordnik 英英查询

输入单词后自动并行查询 Urban Dictionary 与 Wordnik 英英词典；当 Wordnik 返回 <5 条结果或失败时，调用 Claude Haiku 4.5 做中文释义兜底。

启用方式：在 workflow 配置面板 (`Alfred Preferences` → `Workflow` → `Eudic Search` → `[x]` 按钮) 设置以下环境变量：

- `WORDNIK_API_KEY`：在 https://developer.wordnik.com/ 注册免费 key
- `ANTHROPIC_API_KEY`：在 https://console.anthropic.com/ 获取

未设置时该源静默不显示。建议两个都设为 `Don't Export` 以避免分享 workflow 时泄露。

结果按 emoji 区分：

- 📕 ECDICT（本地中文词典）
- 📘 Wordnik（英英）
- 🔥 Urban（俚语）
- 🤖 Claude（LLM 兜底翻译）
- ⚙️ 未配置 API key 的提示
- ⚠️ 该源请求失败；回车重试（绕过缓存）

查询结果在 SQLite 缓存中保存 7 天。
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document Urban/Wordnik/LLM configuration and result icons"
```

---

## Task 17: Final manual verification

This task does not write code. Run through the spec's manual checklist with a real Alfred run.

- [ ] **Step 1: Build release binary**

```bash
make build
```

Place it in the installed workflow directory.

- [ ] **Step 2: Set workflow env vars**

In Alfred Preferences → Workflow → `[x]` icon, set:
- `WORDNIK_API_KEY=<your key>`
- `ANTHROPIC_API_KEY=<your key>`

- [ ] **Step 3: Run the spec's verification cases**

For each, type `e <word>` in Alfred and observe:

1. `serendipity` — Wordnik ≥5, LLM **not** triggered (verify in Alfred debugger logs)
2. `rizz` — Wordnik likely <5, LLM triggered
3. `asdfqwer` — Wordnik 404 / empty, Urban empty, LLM may produce something or refuse
4. Disable WiFi, query `hello` — only ECDICT shows results; 3 error items appear; ENTER on `⚠️ Wordnik: timeout` triggers retry
5. Query `serendipity` twice — second time obviously faster (check log timestamps)
6. Query `'; DROP TABLE stardict;--` — no error, no results from ECDICT, normal behavior
7. From a separate text-editor selection, query `foo" & (do shell script "open -a Calculator") & "bar` — Calculator does **not** launch

- [ ] **Step 4: If everything passes, tag release prep**

```bash
git log --oneline -20
```

Verify all task commits are present.

- [ ] **Step 5: No commit (verification-only task)**

---

## Self-Review Notes (inline)

- **Spec coverage**: every section of the design spec has at least one task. The security fixes (Tasks 2–4) cover the spec's "安全修复" bullets. Tasks 5–13 cover the architecture, components, data flow, and error handling. Tasks 14–15 cover the workflow XML retry mechanism. Task 16 covers configuration documentation.
- **No placeholders**: every code block is complete. URL paths, env var names, table names are concrete.
- **Type consistency**: `DictionarySource::fetch -> Result<Vec<DictEntry>, SourceError>` consistent across Tasks 6, 8, 9, and 13. `Cache::get -> Option<Vec<u8>>` consistent. `LlmResult` has the same fields in `response.rs`, `mod.rs`, and integration tests. `CacheKind` variant names match `CacheKind::Llm` (not `LLM`) across files.
- **Test visibility**: `with_base_url` / `with_endpoint` constructors are marked `#[doc(hidden)]` rather than `#[cfg(test)]` so that integration tests in `tests/` (which compile as a separate crate, not under `cfg(test)` of the lib) can reach them.
- **Workflow XML editing (Task 15)** is done via Alfred UI rather than direct plist editing, because hand-editing UID-laden Alfred plists is error-prone. The commit captures the result.
