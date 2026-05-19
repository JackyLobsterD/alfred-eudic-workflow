# Rich Dictionary Quick Look Card — Design

Date: 2026-05-18
Status: Approved (pending written-spec review)

## Problem

The Quick Look card (Shift / ⌘Y on any Alfred result) currently shows only
ECDICT Chinese, Wordnik English definitions, and Urban Dictionary. The user
wants a much richer card: multiple English-English definitions, part of
speech, inflections/tenses, synonyms/antonyms, related words, phrases /
collocations, example sentences, etymology, pronunciation, Wikipedia, and
Chinese gloss — aggregated from several free online sources.

The Alfred dropdown list stays exactly as it is today (one row per source).
**All new content goes only into the Quick Look card.**

## Goals

- Aggregate many sources into one well-organised card.
- English-English definitions are the top priority and appear first.
- Each block renders only when it has data; missing/failed sources are
  silently skipped (never an error, never blocks the list).
- No regression to the lean inline list or to list latency.

## Non-Goals

- Changing the Alfred dropdown list layout/content.
- Inline (non-card) display of the new data.
- Offline support for the new network sources.
- Audio autoplay inside Quick Look (we surface phonetics + an audio link
  only; Quick Look's webview audio support is unreliable).

## Sources

Card-only sources, fetched in parallel, each independently cached and
degraded:

| Source | Key | Notes |
|---|---|---|
| ECDICT | — (local) | Existing. Also surface unused fields: `exchange` (inflections), `tag` (exam tags), `collins`, `pos`, `phonetic`. |
| Wordnik | existing `WORDNIK_API_KEY` | Existing definitions endpoint. |
| Urban Dictionary | — | Existing. |
| Claude (LLM) | existing `ANTHROPIC_API_KEY` | Existing, conditional (Wordnik < 8 or `!` prefix). |
| Youdao `dict.youdao.com/jsonapi` | — | **Unofficial**, accepted as primary rich source. Provides `ee`, `collins`, `syno`, `rel_word`, `phrs`, `blng_sents_part`, `web_trans`, `ec`, `wikipedia_digest`, `etym`. Query text goes to Youdao servers. |
| Wikipedia REST | — | Official `https://en.wikipedia.org/api/rest_v1/page/summary/<word>`. Youdao `wikipedia_digest` is the fallback. |
| Datamuse `api.datamuse.com` | — | Synonyms/antonyms/related/triggers (`rel_syn`, `rel_ant`, `rel_trg`, `ml`). |
| Wiktionary REST | — | Official `https://en.wiktionary.org/api/rest_v1/page/definition/<word>`. |
| Free Dictionary API | — | `https://api.dictionaryapi.dev/api/v2/entries/en/<word>` — definitions, pos, examples, synonyms/antonyms, phonetics/audio, origin. |
| Merriam-Webster Collegiate Dictionary | optional `MW_COLLEGIATE_API_KEY` | Authoritative EE + etymology + date + inflections + audio. Free non-commercial key (1000/day). Skipped if unset. |
| Merriam-Webster Collegiate Thesaurus | optional `MW_THESAURUS_API_KEY` | Synonyms/antonyms/near-(ant)onyms. Skipped if unset. |

## Architecture & Data Flow

```
run_search:
  ECDICT (local, sync)
  parallel: Wordnik, Urban, Youdao, Wikipedia, Datamuse,
            Wiktionary, FreeDict, MW-Collegiate, MW-Thesaurus
  LLM (conditional, existing rules)

  → Alfred list: fallback / 📕ECDICT / 📘Wordnik / 🔥Urban / 🤖LLM
                 (UNCHANGED — one row per source)
  → preview.rs: build card from ALL data, prioritized blocks
```

- One new module per network source under `src/sources/` (e.g.
  `src/sources/youdao.rs`, `wikipedia.rs`, `datamuse.rs`, `wiktionary.rs`,
  `freedict.rs`, `mw.rs`), following the existing client pattern (struct +
  `with_base_url` test seam + `reqwest` via `dict_client()`), each
  returning a typed, all-optional struct. These are **card data
  providers**, not implementations of the `DictionarySource` trait (that
  trait yields a single definition list row for the inline list).
- Orchestrator (`command/search.rs`) fetches all card sources with
  `tokio::join!`/`futures` alongside the existing Wordnik/Urban calls,
  passes the aggregated data to `preview::write_preview`.
- `preview.rs` is rewritten to render the prioritized block layout.
- The inline list code path is untouched apart from passing extra data to
  the preview builder.

## Card Block Layout (priority order)

A block renders only if it has content. Each block: emoji + heading +
divider, reusing the current dark card CSS. HTML-escaped; reuse
`strip_tags` for markup like `<xref>`.

Title bar: word + phonetic.

| # | Block | Data |
|---|---|---|
| 1 | 🔤 English-English | Wordnik + Youdao `ee`/`collins` + MW-Collegiate + Wiktionary + FreeDict (merged, deduped, source-labelled) |
| 2 | 🔄 Synonyms / Antonyms / Related | MW-Thesaurus (primary) + Datamuse + Youdao `syno`/`rel_word` + FreeDict |
| 3 | 🧩 Phrases / Collocations + Examples | Youdao `phrs` + Youdao `blng_sents_part` + FreeDict examples |
| 4 | 📕 Chinese + POS + Phonetic | ECDICT + Youdao `ec`/`web_trans` |
| 5 | 🔀 Inflections + Exam Tags + Collins | ECDICT `exchange` + `tag` + `collins` stars (local) |
| 6 | 📖 Wikipedia | Official Wikipedia REST (Youdao `wikipedia_digest` fallback) |
| 7 | 🌱 Etymology | Youdao `etym` + FreeDict/Wiktionary origin + MW-Collegiate etymology |
| 8 | 🔊 Pronunciation | MW-Collegiate / FreeDict audio link + phonetic text |
| 9 | 🔥 Urban Dictionary | Existing |
| 10 | 🤖 Claude translation | Existing (only when triggered) |

## Configuration

Two new **optional** `userconfigurationconfig` textfields in `info.plist`
(same mechanism as the existing Wordnik/Anthropic key fields), so they
appear in Eudic's "Configure Workflow" panel:

- `MW_COLLEGIATE_API_KEY` → label "Merriam-Webster Collegiate Key"
- `MW_THESAURUS_API_KEY` → label "Merriam-Webster Thesaurus Key"

Read via clap `env=` / `std::env` like the existing keys. Empty key →
source silently skipped (no error item).

## Error Handling

- Every card source returns its own `Result`; failure (network, timeout,
  schema drift, 404, rate-limit) is logged and that block is skipped.
- Card sources never affect the inline list or other sources.
- The list's critical path (ECDICT/Wordnik/Urban) is never blocked waiting
  on card-only sources beyond the shared timeout.

## Performance

- Quick Look uses a `quicklookurl` pointing at a pre-written HTML file, so
  the card must be generated **at query time** (cannot lazily fetch on
  Shift). Every debounced query fans out to ~9 network sources.
- Mitigations: all fetches run in parallel (total ≈ slowest source, not
  the sum); 2s timeout via the existing `dict_client()`; 7-day SQLite
  cache (cache hit < ~50 ms); Alfred's input debounce reduces call volume.
- Worst case (all cold): ~2 s. Cache hit: near-instant.

## Caching

- One `CacheKind` per new source; reuse the existing `SqliteCache`
  (7-day TTL, success-only caching).
- Cache key = normalized `spell.trim().to_lowercase()` (matches existing
  convention).

## Testing

- Each new source: `wiremock` unit tests covering success, empty, 404,
  rate-limit, and missing/extra fields (schema-drift tolerance).
- `preview.rs`: unit tests for block ordering, block-skipped-when-empty,
  HTML escaping, and tag stripping.
- Integration test wiring multiple mocked sources through the orchestrator
  into a rendered card.
- Follow the existing TDD + subagent-driven-development workflow; keep
  35+ unit and integration suites green.

## Risks

- **Youdao jsonapi is unofficial**: may change shape or rate-limit.
  Mitigated by schema-tolerant parsing (all fields optional), caching, and
  silent degradation. Privacy: query words reach Youdao (Chinese service) —
  documented in README.
- **Source sprawl / latency**: bounded by parallelism + 2s timeout +
  cache. If a source proves consistently flaky it can be dropped without
  affecting others.
- **M-W free tier**: 1000 queries/key/day, non-commercial only —
  documented; cache reduces volume.

## Out of Scope / Future

- Per-source enable/disable toggles.
- Audio autoplay in the card.
- Eudic 生词本 auto-add (separate, parked).
- OpenRouter / Eudic AI-translation proxy (separate thread).
