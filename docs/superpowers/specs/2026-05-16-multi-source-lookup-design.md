# Multi-Source Lookup Design

**Date**: 2026-05-16
**Status**: Approved — pending implementation plan
**Author**: brainstormed with Claude

## 目标

在 Alfred 输入完单词后（已有防抖），并行查询 Urban Dictionary、Wordnik 英英词典，以及本地 ECDICT 中文词典；当 Wordnik 返回 <5 条释义或失败时，再调用 LLM（Claude Haiku 4.5）做中文释义兜底。结果统一渲染到 Alfred 下拉列表，分区呈现。

## 范围

**包含**:

- 三个新数据源（Urban / Wordnik / LLM）的客户端实现
- SQLite 持久化缓存，7 天 TTL
- 错误降级与重试机制（含 `info.plist` 改动）
- 修复审查发现的两处安全漏洞（SQL 注入、AppleScript 注入），因为修复成本极低且在变更范围内
- 将 `info.plist` 纳入 repo 版本控制

**不包含**:

- 更换 ECDICT 数据库结构
- 移除现有功能（完全保留 completion 文件搜索、updater、speak）
- CI / GitHub Actions 配置
- 切换更新源到用户自己的 fork（独立小改动，可后续单独处理）

## 架构

模块化为：本地字典层（保留）、远程字典层（新）、LLM 层（独立）、缓存层（新）、共享 HTTP 层（新）、渲染层（新）、编排层（改写）。

```
src/
├── main.rs                    (改：新依赖、配置传递)
├── command/
│   └── search.rs              (改：编排者)
├── dictionary/                (保留：ECDICT + completion)
├── sources/                   (新)
│   ├── mod.rs                 trait DictionarySource + DictEntry + SourceError
│   ├── urban.rs               UrbanClient
│   └── wordnik.rs             WordnikClient
├── llm/                       (新 — 与 sources/ 平级，故意拆分)
│   ├── mod.rs                 LlmClient
│   ├── prompt.rs              system prompt 模板
│   └── response.rs            LlmResult + 解析
├── cache/                     (新)
│   ├── mod.rs                 trait Cache + CacheKind + TTL 常量
│   └── sqlite.rs              SqliteCache
├── http.rs                    (新)  shared reqwest::Client
└── render.rs                  (新)  → Alfred Item

info.plist                     (新 — 纳入 repo)
```

### 为何 LLM 不归入 `sources/`

LLM 与字典 API 在以下维度根本不同：契约（自由文本 vs 固定 schema）、调用时机（条件性 vs 无条件）、错误模式（含 refusal/JSON 失败）、配置面（含 model/prompt）、演化方向（换提供商 vs 加字典）。强行塞进 `trait Source` 会使 `SourceResult` 退化成 `serde_json::Value`。拆分后，"Wordnik <5 才跑 LLM" 这种条件分支自然写在编排层而非源内部。

## 组件接口

### `sources/mod.rs`

```rust
#[async_trait::async_trait]
pub trait DictionarySource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn fetch(&self, spell: &str) -> Result<Vec<DictEntry>, SourceError>;
}

pub enum SourceKind { Urban, Wordnik }

pub struct DictEntry {
    pub headword: String,
    pub definition: String,
    pub extra: Option<String>,    // 词性 / thumbs_up / 来源等源特定信息
}

pub enum SourceError {
    Http(reqwest::Error),
    Timeout,
    BadResponse(String),
    RateLimited,
    NoApiKey,
}
```

### `sources/urban.rs`

- `GET https://api.urbandictionary.com/v0/define?term=<spell>`
- 按 `thumbs_up` 取前 3 条
- `extra = Some(format!("👍 {} 👎 {}", up, down))`
- 无需 API key
- 不过滤敏感内容（用户主动查 Urban 即默认接受其内容性质）

### `sources/wordnik.rs`

- `GET https://api.wordnik.com/v4/word.json/<spell>/definitions?limit=10&includeRelated=false&sourceDictionaries=ahd-5,wiktionary,wordnet&useCanonical=true&api_key=<key>`
- 取返回前 10 条原始结果（决定是否触发 LLM 的判据是 `len() < 5`）
- 渲染时最多展示 5 条
- API key 来自 `WORDNIK_API_KEY` 环境变量

### `llm/mod.rs`

```rust
pub struct LlmClient { http: Arc<reqwest::Client>, api_key: String }

pub struct LlmResult {
    pub translations: Vec<String>,    // 1-3 条中文释义，每条 ≤20 字
    pub example: Option<String>,      // 一句英文例句，≤15 词
}

impl LlmClient {
    pub async fn translate(&self, spell: &str) -> Result<LlmResult, LlmError>;
}

pub enum LlmError {
    Http(reqwest::Error),
    Timeout,
    BadJson(String),
    ApiError { status: u16, body: String },
    Refused,
    NoApiKey,
}
```

调用 Anthropic `POST /v1/messages`，model=`claude-haiku-4-5`，max_tokens=200，强约束返回 JSON。API key 来自 `ANTHROPIC_API_KEY` 环境变量。

**Prompt 模板**（双向支持，英→中或中→英）:

```
System: You are a concise bilingual English-Chinese dictionary. Detect the input language. Output JSON only.
User: Word: "<spell>". Output JSON:
{"translations":["释义1","释义2"],"example":"example sentence"}
Rules: 1-3 translations, each ≤20 Chinese chars. Example in the opposite language, ≤15 words. No extra text.
```

### `cache/mod.rs`

```rust
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, kind: CacheKind, key: &str) -> Option<Vec<u8>>;
    async fn put(&self, kind: CacheKind, key: &str, value: &[u8]);
}

pub enum CacheKind { Urban, Wordnik, Llm }

pub const CACHE_TTL_SECS: i64 = 7 * 24 * 3600;
```

### `cache/sqlite.rs`

每个 `CacheKind` 一张表，schema:

```sql
CREATE TABLE IF NOT EXISTS cache_urban (
  key TEXT PRIMARY KEY,
  value BLOB NOT NULL,
  fetched_at INTEGER NOT NULL
);
-- cache_wordnik, cache_llm 同
```

- Key: `spell.to_lowercase().trim()`
- Value: `serde_json::to_vec(&entries_or_result)` — 人类可读，方便排查
- DB 位置: `$alfred_workflow_cache/lookup_cache.db`，环境变量缺失时回退到 `std::env::temp_dir().join("alfred-eudic-cache")`
- 打开标志: `OPEN_READ_WRITE | OPEN_CREATE`，`busy_timeout(500ms)` 处理进程并发

### `http.rs`

```rust
pub fn dict_client() -> Arc<reqwest::Client>;   // timeout 2s
pub fn llm_client() -> Arc<reqwest::Client>;    // timeout 8s
```

进程内 `OnceCell` 缓存，所有源共享同一 client 复用连接池。User-Agent 含 workflow 版本。

### `render.rs`

```rust
pub fn render_dict(entries: &[DictEntry], source: SourceKind) -> Vec<Item>;
pub fn render_llm(result: &LlmResult, spell: &str) -> Vec<Item>;
pub fn render_ecdict(entries: &[StardictEntry]) -> Vec<Item>;
pub fn render_no_api_key(source_name: &str, config_url: &str) -> Item;
pub fn render_error(source_name: &str, err: &dyn Display, spell: &str) -> Item;
```

各源条目前缀:

| 源 | 前缀 |
|---|---|
| ECDICT | `📕` |
| Wordnik | `📘` |
| Urban | `🔥` |
| LLM | `🤖` |
| 错误 | `⚠️` |
| 未配置 key | `⚙️` |

## 数据流

```
Alfred (script filter, ~200ms debounce)
   │
   ▼
alfred-eudic search --completion-file=... --db-file=... <spell>
   │
   ├─ 读 env: WORDNIK_API_KEY, ANTHROPIC_API_KEY, BYPASS_CACHE
   ├─ 打开/迁移 cache DB
   │
   ├─ ECDICT 查询（同步，本地 SQLite）
   ├─ tokio::join!(urban_with_cache, wordnik_with_cache)
   │
   ├─ 判断 wordnik:
   │    Ok(v) where v.len() < 5  → 触发 LLM
   │    Err(_)                    → 触发 LLM
   │    Ok(v) where v.len() >= 5 → 跳过 LLM
   │
   ├─ (条件) llm_with_cache
   │
   ├─ 渲染顺序：
   │    ① "<spell>" fallback item（保留现有：打开 Eudic）
   │    ② ECDICT items
   │    ③ Wordnik items / 错误条目 / 未配置 key 条目
   │    ④ Urban items / 错误条目
   │    ⑤ LLM items / 错误条目（如调用）
   │    ⑥ updater 提示（如有）
   │
   ▼
AlfredUtils::output(...)
   │
   ▼
后台静默 updater check（保留）
```

### 缓存逻辑

```
async fn fetch_with_cache(source, cache, spell, bypass):
    let key = normalize(spell)
    if not bypass:
        if let Some(bytes) = cache.get(kind, key):
            return Ok(deserialize(bytes))
    let result = source.fetch(spell).await?
    cache.put(kind, key, serialize(&result)).await   // await，非 fire-and-forget
    Ok(result)
```

**只缓存成功结果**。`SourceError` / `LlmError` 不写缓存，下次查询会重试。

### 输入预处理与边界

- `spell.len() <= 1`：所有源短路，仅显示 "Input more than one letter"（现有行为）
- `spell.len() > 50`：跳过 LLM（避免对粘贴长文本浪费 token），其他源照查
- 中文输入：ECDICT 查不到、Wordnik/Urban 通常无结果，LLM 中→英兜底
- 短语（含空格）：照常查 Urban / Wordnik / LLM
- **传给各源的 spell 形态**：
  - ECDICT 查询沿用现有 `split_whitespace().collect()`（去空格，匹配 `sw` 列规范化形式）
  - Urban / Wordnik / LLM 接收 **trim 后的原始 spell**（保留内部空格，因为 "ice cream"、"break up" 等短语在这些源中有独立词条）
  - 缓存 key 统一用 `spell.to_lowercase().trim()`（保留空格、统一大小写）

## 错误处理

| 触发点 | 行为 |
|---|---|
| API key 未设 | 渲染 `⚙️ <Source> 未配置 API key — 回车查看配置说明`；`arg` 指向 README 配置章节 URL |
| `*::fetch` `Timeout` | 渲染 `⚠️ <Source>: 请求超时 (2s)`；`variables: {BYPASS_CACHE: "1"}`；ENTER 触发 re-run |
| `RateLimited` (429) | 渲染 `⚠️ <Source>: API 配额耗尽` |
| `Http` / `BadResponse` | 渲染 `⚠️ <Source>: <err 简述>` |
| LLM `BadJson` | 渲染错误条目；**不缓存**；下次重试 |
| LLM `ApiError 5xx` | 渲染错误条目；不缓存 |
| LLM `Refused` | 渲染错误条目；**不缓存**（用户也许只是一时触雷） |
| ECDICT DB 路径错 | `db_file not exist: <path>`（现有） |
| Cache DB 打不开 | 降级为"无缓存模式"，所有调用直通；log 一次 |
| Cache 写失败 | 静默 swallow + log，不影响渲染 |

**ENTER 重试机制**：错误条目设置 `variables: {BYPASS_CACHE: "1"}` 和 `arg: <spell>`。`info.plist` 的 script filter 输出连接到一个 "Args and Variables" → 第二个 script filter（与第一个等价），后者继承 `BYPASS_CACHE=1`，Rust 端检测到该 env 后跳过 cache.get。具体 plist 节点图在实现期定。

**Panic 防护**：现有 `manager.rs:51` 的 `panic!("Failed to read completion file")` 改为返回空 vec + log。新代码禁止任何 panic 路径。

## 安全修复（顺带）

不在主要范围但与渲染层相邻：

1. **`src/dictionary/database.rs`**: 改用参数化 SQL + `LIKE` 转义，避免 SQL 注入
2. **`script/search_eudic.sh` / `speak_eudic.sh`**: 通过环境变量传 word 给 osascript，避免 shell + AppleScript 双重注入
3. **`src/dictionary/database.rs`**: 数据库以只读模式打开（`OpenFlags::SQLITE_OPEN_READ_ONLY`）

## 配置

新增 Alfred workflow 环境变量（在 workflow Configuration 面板设置）:

| 变量 | 说明 | 必需 |
|---|---|---|
| `WORDNIK_API_KEY` | wordnik.com 注册免费 key | 否（不设则不显示 Wordnik 区） |
| `ANTHROPIC_API_KEY` | console.anthropic.com 获取 | 否（不设则不显示 LLM 区） |
| `ALFRED_EUDIC_COMPLETION_FILE` | 现有 | 否 |
| `ALFRED_EUDIC_DATABASE_FILE` | 现有 ECDICT 路径 | 否 |

`BYPASS_CACHE` 由 retry 路径动态注入，用户不直接设置。

## 新增依赖

```toml
[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
once_cell = "1"

[dev-dependencies]
wiremock = "0.6"
tempfile = "3"
```

## 测试

### 单元

- `sources/urban.rs`、`sources/wordnik.rs`: wiremock 模拟成功/404/429/5xx/超时/坏 JSON
- `llm/mod.rs`: wiremock 模拟 Anthropic 完整 JSON / 非 JSON / 字段缺失 / 400 / refusal
- `cache/sqlite.rs`: `:memory:` DB，put/get 一致性、TTL 过期、kind 隔离、并发不死锁、缺失时自动建表
- `render.rs`: emoji 前缀、definition 截断、错误条目带 retry variable

### 集成（`tests/search_integration.rs`）

跑完整 `run_search` 入口：

- 全部成功 → 4 个分区完整
- Wordnik ≥5 条 → LLM 不被调用（wiremock 调用次数 = 0）
- Wordnik <5 条 → LLM 被调用一次
- Wordnik 超时 → LLM 仍被调用 + 错误条目
- 缓存命中 → 外部 mock 不被调用
- BYPASS_CACHE=1 → 命中也重新调

### 手动验证清单

1. `serendipity` — Wordnik ≥5，LLM 不触发
2. `rizz` — Wordnik 可能 <5，触发 LLM
3. `asdfqwer` — 三个源都空/失败，验证错误条目
4. 断网 — 仅 ECDICT 出结果，三个错误条目正确
5. 同词查两次 — 第二次明显更快（缓存命中）
6. `'; DROP TABLE stardict;--` — SQLi 修复后应无害
7. `foo" & (do shell script "open -a Calculator") & "bar` — Shell 修复后不弹计算器

不在范围：CI、真实 API 端点回归测试。

## 不在范围的事项

- GitHub Actions / CI
- 切换 updater 到用户 fork（小独立改动）
- 更换 ECDICT 数据库
- 移除/重写 Swift 旧代码（已是历史遗留）
- frontend / GUI 配置面板
