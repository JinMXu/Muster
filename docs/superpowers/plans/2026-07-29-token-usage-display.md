# Token Usage Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Usage panel to Muster that aggregates token consumption from four local AI coding CLIs (OpenCode, Claude Code, Codex, Kimi Code) by reading their local data stores, with a background scanner keeping data fresh.

**Architecture:** Rust backend `services/usage/` module with one parser per tool implementing a `UsageProvider` trait. A `std::thread::spawn` background loop (matching the codebase's existing concurrency idiom) scans every 60s with mtime-based incremental skipping, caching results in `SharedState` behind a `parking_lot::Mutex`. Three Tauri commands expose the cache to the frontend. A new `UsagePanel.tsx` modal renders summary cards + a session table, opened via `Ctrl+Shift+U` or command palette.

**Tech Stack:** Rust (rusqlite with `bundled` feature, serde_json, parking_lot), React/TypeScript, Tailwind CSS, Tauri 2.

**Key spec deviations from the design doc (resolved to match codebase reality):**
- Spec said `RwLock` + `tokio::spawn`; codebase uses `parking_lot::Mutex` + `std::thread::spawn` exclusively. Plan uses the codebase idiom.
- Spec said usage cache lives in `AppState`; but usage is global (not per-window), so it lives in `SharedState` alongside `settings`.

---

## File Structure

**Backend (Rust) - all new files under `src-tauri/src/services/usage/`:**
- `model.rs` — `ToolKind`, `TokenUsage`, `UsageSession`, `ToolSummary`, `UsageSummary`, `DiscoveredSession` structs/enums
- `provider.rs` — `UsageProvider` trait + path resolution helpers (`home_dir`, env-var overrides)
- `opencode.rs` — SQLite read-only parser implementing `UsageProvider`
- `claude.rs` — JSONL streaming parser for Claude Code
- `codex.rs` — JSONL streaming parser for Codex
- `kimi.rs` — JSONL streaming parser for Kimi Code
- `scanner.rs` — background scan loop + `UsageCache` type + mtime tracking
- `mod.rs` — module declarations + `collect_all()` entry point

**Backend modified files:**
- `src-tauri/src/services/mod.rs` — add `pub mod usage;`
- `src-tauri/src/commands.rs` — add 3 commands + `SharedState` gets a `usage` field
- `src-tauri/src/bootstrap.rs` — spawn the scanner thread, register commands
- `src-tauri/Cargo.toml` — add `rusqlite` with `bundled` feature

**Frontend new files:**
- `src/components/UsagePanel.tsx` — the modal panel (cards + table inline)

**Frontend modified files:**
- `src/lib/types.ts` — add `ToolKind`, `TokenUsage`, `UsageSummary`, `ToolSummary`, `UsageSession`
- `src/lib/invoke.ts` — add `usage` namespace to `api`
- `src/lib/i18n/en.ts` — add `usage` namespace
- `src/lib/i18n/zh.ts` — add `usage` namespace
- `src/components/CommandPalette.tsx` — add "Open Usage" command + `onOpenUsage` prop
- `src/App.tsx` — `showUsage` state, `NAV_MAP` entry, render `<UsagePanel/>`

---

## Task 1: Add rusqlite dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add rusqlite to Cargo.toml**

In the `[dependencies]` section, after the `serde_json = "1"` line, add:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

The `bundled` feature compiles SQLite from source so no system SQLite is needed on Windows.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles (may take a while first time to build bundled SQLite). No errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: add rusqlite with bundled feature for usage tracking"
```

---

## Task 2: Data model (`model.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/model.rs`

- [ ] **Step 1: Write the model file**

```rust
use serde::{Deserialize, Serialize};

/// Which CLI tool a usage record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Opencode,
    ClaudeCode,
    Codex,
    KimiCode,
}

/// Normalized token breakdown shared across all four tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Non-cached input tokens.
    pub input: u64,
    /// Completion / output tokens.
    pub output: u64,
    /// Reasoning tokens (some models only).
    pub reasoning: u64,
    /// Cache-hit reads.
    pub cache_read: u64,
    /// Tokens written to cache.
    pub cache_write: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.reasoning + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.reasoning += other.reasoning;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

/// One session's usage, the unit of the sessions list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSession {
    pub tool: ToolKind,
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub model: String,
    /// epoch ms
    pub started_at: i64,
    /// epoch ms
    pub updated_at: i64,
    pub tokens: TokenUsage,
    /// USD cost; only OpenCode stores this.
    pub cost_usd: Option<f64>,
}

/// Per-tool aggregate for the summary cards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    pub tool: ToolKind,
    pub total_tokens: u64,
    pub tokens: TokenUsage,
    pub session_count: usize,
    /// USD cost; only OpenCode.
    pub cost_usd: Option<f64>,
    /// epoch ms of the most recent session for this tool.
    pub last_updated: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    pub tools: Vec<ToolSummary>,
}

impl UsageSummary {
    pub fn from_sessions(sessions: &[UsageSession]) -> Self {
        use std::collections::HashMap;
        let mut buckets: HashMap<ToolKind, ToolSummary> = HashMap::new();
        for s in sessions {
            let entry = buckets.entry(s.tool).or_insert_with(|| ToolSummary {
                tool: s.tool,
                total_tokens: 0,
                tokens: TokenUsage::default(),
                session_count: 0,
                cost_usd: None,
                last_updated: 0,
            });
            entry.total_tokens = entry.total_tokens.saturating_add(s.tokens.total());
            entry.tokens.add(&s.tokens);
            entry.session_count += 1;
            if s.cost_usd.is_some() {
                entry.cost_usd = Some(entry.cost_usd.unwrap_or(0.0) + s.cost_usd.unwrap_or(0.0));
            }
            if s.updated_at > entry.last_updated {
                entry.last_updated = s.updated_at;
            }
        }
        // Stable order: OpenCode, ClaudeCode, Codex, KimiCode
        let order = [
            ToolKind::Opencode,
            ToolKind::ClaudeCode,
            ToolKind::Codex,
            ToolKind::KimiCode,
        ];
        let tools = order
            .into_iter()
            .filter_map(|t| buckets.remove(&t))
            .collect();
        Self { tools }
    }
}
```

- [ ] **Step 2: Write tests for the aggregation logic**

Append to the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sess(tool: ToolKind, tokens: TokenUsage, cost: Option<f64>, updated: i64) -> UsageSession {
        UsageSession {
            tool,
            session_id: format!("s-{}", updated),
            title: "t".into(),
            cwd: "/".into(),
            model: "m".into(),
            started_at: updated - 1000,
            updated_at: updated,
            tokens,
            cost_usd: cost,
        }
    }

    #[test]
    fn total_sums_all_fields() {
        let t = TokenUsage { input: 10, output: 20, reasoning: 5, cache_read: 100, cache_write: 50 };
        assert_eq!(t.total(), 185);
    }

    #[test]
    fn summary_aggregates_per_tool() {
        let sessions = vec![
            sess(ToolKind::Opencode, TokenUsage { input: 100, output: 50, ..Default::default() }, Some(0.5), 1000),
            sess(ToolKind::Opencode, TokenUsage { input: 200, output: 50, ..Default::default() }, Some(1.5), 2000),
            sess(ToolKind::ClaudeCode, TokenUsage { input: 300, cache_read: 50, ..Default::default() }, None, 1500),
        ];
        let s = UsageSummary::from_sessions(&sessions);
        assert_eq!(s.tools.len(), 2);
        let oc = s.tools.iter().find(|t| t.tool == ToolKind::Opencode).unwrap();
        assert_eq!(oc.session_count, 2);
        assert_eq!(oc.total_tokens, 400); // 100+50 + 200+50
        assert_eq!(oc.cost_usd, Some(2.0));
        assert_eq!(oc.last_updated, 2000);
        let cc = s.tools.iter().find(|t| t.tool == ToolKind::ClaudeCode).unwrap();
        assert_eq!(cc.total_tokens, 350); // 300 + 50
        assert!(cc.cost_usd.is_none());
    }

    #[test]
    fn summary_stable_order() {
        let sessions = vec![
            sess(ToolKind::KimiCode, TokenUsage::default(), None, 1),
            sess(ToolKind::Codex, TokenUsage::default(), None, 2),
            sess(ToolKind::ClaudeCode, TokenUsage::default(), None, 3),
            sess(ToolKind::Opencode, TokenUsage::default(), None, 4),
        ];
        let s = UsageSummary::from_sessions(&sessions);
        assert_eq!(s.tools[0].tool, ToolKind::Opencode);
        assert_eq!(s.tools[1].tool, ToolKind::ClaudeCode);
        assert_eq!(s.tools[2].tool, ToolKind::Codex);
        assert_eq!(s.tools[3].tool, ToolKind::KimiCode);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage::model`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/usage/model.rs
git commit -m "feat(usage): data model and aggregation logic"
```

---

## Task 3: Provider trait + path helpers (`provider.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/provider.rs`

- [ ] **Step 1: Write the provider trait and path helpers**

```rust
use std::path::PathBuf;
use std::time::SystemTime;

use super::model::{ToolKind, UsageSession};

/// A file or DB record discovered on disk that may yield a UsageSession.
#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    /// Tool-specific key: file path for JSONL tools, session id for OpenCode.
    pub key: String,
    /// File path (for mtime tracking); for OpenCode this is the db path.
    pub path: PathBuf,
    /// mtime at discovery time, for incremental skip.
    pub mtime: Option<SystemTime>,
    /// File size at discovery time (extra change signal).
    pub size: Option<u64>,
}

/// Each tool implements this. Implementations must be cheap to clone (they
/// hold no state beyond config paths).
pub trait UsageProvider: Send + Sync {
    fn tool_kind(&self) -> ToolKind;
    /// Find all candidate session sources on disk. Returns empty vec if the
    /// tool is not installed (not an error).
    fn discover(&self) -> Vec<DiscoveredSession>;
    /// Parse one discovered source into a UsageSession. Returns None on any
    /// parse error (the scanner logs and skips).
    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession>;
}

/// Resolve the user home directory. Returns None if it cannot be determined.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Read an env var as a PathBuf, falling back to `default` if unset/empty.
pub fn env_path_or(var: &str, default: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(default)
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/services/usage/provider.rs
git commit -m "feat(usage): UsageProvider trait and path helpers"
```

---

## Task 4: OpenCode parser (`opencode.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/opencode.rs`

- [ ] **Step 1: Write the OpenCode parser**

OpenCode stores everything in a single SQLite DB. We open it read-only (the live opencode process holds a WAL write lock) and run one query. The `model` column is JSON like `{"id":"...","providerID":"..."}`.

```rust
use std::path::PathBuf;
use std::time::SystemTime;

use rusqlite::{OpenFlags, Connection};

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir};

pub struct OpenCodeProvider {
    db_path: Option<PathBuf>,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self { db_path: resolve_db_path() }
    }
}

/// Resolve the opencode.db path, honoring XDG_DATA_HOME and OPENCODE_DB.
fn resolve_db_path() -> Option<PathBuf> {
    // OPENCODE_DB can be an absolute path to the db file directly.
    if let Some(p) = env_path_or("OPENCODE_DB", || None) {
        if p.is_file() {
            return Some(p);
        }
    }
    // XDG_DATA_HOME overrides the data root; opencode appends "opencode/opencode.db".
    let data_dir = env_path_or("XDG_DATA_HOME", || {
        home_dir().map(|h| h.join(".local").join("share"))
    })?;
    let db = data_dir.join("opencode").join("opencode.db");
    if db.is_file() { Some(db) } else { None }
}

impl UsageProvider for OpenCodeProvider {
    fn tool_kind(&self) -> ToolKind { ToolKind::Opencode }

    fn discover(&self) -> Vec<DiscoveredSession> {
        let Some(path) = &self.db_path else { return vec![] };
        let meta = std::fs::metadata(path).ok();
        let mtime = meta.as_ref().and_then(|m| m.modified().ok());
        let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
        // One "source" representing the whole DB; key is the path string.
        vec![DiscoveredSession {
            key: path.to_string_lossy().into_owned(),
            path: path.clone(),
            mtime,
            size,
        }]
    }

    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession> {
        // We return a sentinel UsageSession per row via a different method;
        // the scanner calls parse_all for OpenCode. To satisfy the trait,
        // parse() returns None (OpenCode is handled specially via parse_all).
        // See OpenCodeProvider::parse_all below — the scanner checks for this.
        None
    }
}

impl OpenCodeProvider {
    /// OpenCode is special: one DB yields many sessions. Parse them all at once.
    /// Returns None if the DB can't be opened or queried.
    pub fn parse_all(&self) -> Option<Vec<UsageSession>> {
        let path = match &self.db_path {
            Some(p) => p,
            None => return vec![],
        };
        // Open read-only to avoid clashing with the live opencode WAL lock.
        let uri = format!("file:{}?mode=ro&immutable=1", path.to_string_lossy());
        let conn = Connection::open_with_flags_and_path(&uri, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        // Belt and suspenders: never write.
        let _ = conn.pragma_update(None, "query_only", true);

        let mut stmt = conn
            .prepare(
                "SELECT id, title, directory, cost,
                        tokens_input, tokens_output, tokens_reasoning,
                        tokens_cache_read, tokens_cache_write,
                        json_extract(model,'$.id'),
                        json_extract(model,'$.providerID'),
                        time_created, time_updated
                 FROM session
                 WHERE time_archived IS NULL
                 ORDER BY time_updated DESC",
            )
            .ok()?;

        let rows = stmt
            .query_map([], |row| {
                let model_id: Option<String> = row.get(9)?;
                let provider_id: Option<String> = row.get(10)?;
                let model = match (model_id, provider_id) {
                    (Some(m), Some(p)) if !m.is_empty() => format!("{}/{}", p, m),
                    (Some(m), _) if !m.is_empty() => m,
                    _ => "unknown".to_string(),
                };
                Ok(UsageSession {
                    tool: ToolKind::Opencode,
                    session_id: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    cwd: row.get::<_, String>(2)?,
                    cost_usd: {
                        let c: f64 = row.get(3)?;
                        if c > 0.0 { Some(c) } else { None }
                    },
                    tokens: TokenUsage {
                        input: row.get::<_, i64>(4)? as u64,
                        output: row.get::<_, i64>(5)? as u64,
                        reasoning: row.get::<_, i64>(6)? as u64,
                        cache_read: row.get::<_, i64>(7)? as u64,
                        cache_write: row.get::<_, i64>(8)? as u64,
                    },
                    model,
                    started_at: row.get::<_, i64>(11)?,
                    updated_at: row.get::<_, i64>(12)?,
                })
            })
            .ok()?;

        let mut out = Vec::new();
        for r in rows {
            if let Ok(s) = r { out.push(s); }
        }
        Some(out)
    }
}
```

Note: `open_with_flags_and_path` doesn't exist in rusqlite 0.31; use the URI form. Replace the connection-opening lines with:

```rust
        // Open read-only via URI to avoid clashing with the live opencode WAL lock.
        let conn = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ).ok()?;
```

(`Connection::open_with_flags` accepts a URI when `SQLITE_OPEN_URI` is set.)

- [ ] **Step 2: Write a test using an in-memory SQLite fixture**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_fixture() -> (tempfile::NamedTempFile, Connection) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (
                id text PRIMARY KEY, project_id text NOT NULL DEFAULT 'g',
                parent_id text, workspace_id text, slug text NOT NULL DEFAULT '',
                directory text NOT NULL DEFAULT '', path text,
                title text NOT NULL DEFAULT '', version text NOT NULL DEFAULT '',
                share_url text, agent text, model text,
                cost real NOT NULL DEFAULT 0,
                tokens_input integer NOT NULL DEFAULT 0,
                tokens_output integer NOT NULL DEFAULT 0,
                tokens_reasoning integer NOT NULL DEFAULT 0,
                tokens_cache_read integer NOT NULL DEFAULT 0,
                tokens_cache_write integer NOT NULL DEFAULT 0,
                summary_additions integer, summary_deletions integer,
                summary_files integer, summary_diffs text, revert text,
                permission text, metadata text,
                time_created integer NOT NULL DEFAULT 0,
                time_updated integer NOT NULL DEFAULT 0,
                time_compacting integer, time_archived integer
            );",
        ).unwrap();
        conn.execute(
            "INSERT INTO session (id, title, directory, cost, model,
                tokens_input, tokens_output, tokens_reasoning,
                tokens_cache_read, tokens_cache_write, time_created, time_updated)
             VALUES ('ses_1', 'Fix bug', '/repo', 0.42,
                '{\"id\":\"deepseek-v4\",\"providerID\":\"opencode\"}',
                1000, 200, 50, 500, 0, 100000, 200000)",
            [],
        ).unwrap();
        (f, conn)
    }

    #[test]
    fn parses_sessions_from_db() {
        let (f, _conn) = make_fixture();
        let provider = OpenCodeProvider { db_path: Some(f.path().to_path_buf()) };
        let sessions = provider.parse_all().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, "ses_1");
        assert_eq!(s.title, "Fix bug");
        assert_eq!(s.cwd, "/repo");
        assert_eq!(s.cost_usd, Some(0.42));
        assert_eq!(s.tokens.input, 1000);
        assert_eq!(s.tokens.output, 200);
        assert_eq!(s.tokens.reasoning, 50);
        assert_eq!(s.tokens.cache_read, 500);
        assert_eq!(s.model, "opencode/deepseek-v4");
        assert_eq!(s.started_at, 100000);
        assert_eq!(s.updated_at, 200000);
    }
}
```

Add `tempfile` to `[dev-dependencies]` in `src-tauri/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage::opencode`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/usage/opencode.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(usage): OpenCode SQLite parser"
```

---

## Task 5: Claude Code parser (`claude.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/claude.rs`

- [ ] **Step 1: Write the Claude Code JSONL parser**

Claude Code writes one JSONL file per session at `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`. Each line is a JSON object; assistant lines carry `message.usage`. We sum usage across all `type == "assistant"` lines. The cwd is decoded from the folder name (`D--agents-Foo` -> `D:\agents\Foo`).

```rust
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir};

pub struct ClaudeProvider {
    root: Option<PathBuf>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self { root: resolve_root() }
    }
}

fn resolve_root() -> Option<PathBuf> {
    // CLAUDE_CONFIG_DIR moves the whole .claude root.
    let base = env_path_or("CLAUDE_CONFIG_DIR", || home_dir().map(|h| h.join(".claude")))?;
    let projects = base.join("projects");
    if projects.is_dir() { Some(projects) } else { None }
}

/// Decode Claude Code's encoded cwd folder name back to a path.
/// "D--agents-Foo" -> "D:\\agents\\Foo" (or "D:/agents/Foo").
fn decode_cwd(folder: &str) -> String {
    // The encoding replaces non-alphanumeric chars (including : \ /) with '-'.
    // We can't perfectly reverse it, but the common Windows case is
    // "D--path-..." meaning "D:\path\...". We reconstruct heuristically:
    // a drive letter followed by "--" -> "X:\".
    let mut s = folder.to_string();
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[1] == b'-' && bytes[2] == b'-' && bytes[0].is_ascii_alphabetic() {
            // "D--rest" -> "D:" + rest with '-' -> '\\'
            let drive = bytes[0] as char;
            let rest = &s[3..];
            let rest = rest.replace('-', "\\");
            return format!("{}:\\{}", drive, rest);
        }
    }
    // Fallback: just swap '-' for '/'.
    s.replace('-', "/")
}

#[derive(Deserialize)]
struct ClaudeLine {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    message: Option<ClaudeMessage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize, Default)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

impl UsageProvider for ClaudeProvider {
    fn tool_kind(&self) -> ToolKind { ToolKind::ClaudeCode }

    fn discover(&self) -> Vec<DiscoveredSession> {
        let Some(root) = &self.root else { return vec![] };
        let mut out = Vec::new();
        let Ok(project_dirs) = std::fs::read_dir(root) else { return vec![] };
        for p in project_dirs.flatten() {
            if !p.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
            let Ok(files) = std::fs::read_dir(p.path()) else { continue };
            for f in files.flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
                let meta = std::fs::metadata(&path).ok();
                let mtime = meta.as_ref().and_then(|m| m.modified().ok());
                let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
                out.push(DiscoveredSession {
                    key: path.to_string_lossy().into_owned(),
                    path,
                    mtime,
                    size,
                });
            }
        }
        out
    }

    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession> {
        let file = std::fs::File::open(&source.path).ok()?;
        let reader = BufReader::new(file);
        let mut tokens = TokenUsage::default();
        let mut model = String::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts: i64 = 0;
        let mut first_user_text: Option<String> = None;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.is_empty() { continue; }
            let Ok(parsed): std::result::Result<ClaudeLine, _> = serde_json::from_str(&line) else { continue };
            if parsed.r#type == "assistant" {
                if let Some(msg) = &parsed.message {
                    if let Some(u) = &msg.usage {
                        tokens.input += u.input_tokens;
                        tokens.cache_write += u.cache_creation_input_tokens;
                        tokens.cache_read += u.cache_read_input_tokens;
                        tokens.output += u.output_tokens;
                    }
                    if let Some(m) = &msg.model {
                        if !m.is_empty() { model = m.clone(); }
                    }
                }
                if let Some(ts) = parse_iso_ms(&parsed.timestamp) {
                    if first_ts.is_none() { first_ts = Some(ts); }
                    last_ts = ts;
                }
            } else if parsed.r#type == "user" && first_user_text.is_none() {
                // Try to grab the first user message text as a title hint.
                // (We don't deeply parse content here to keep it cheap.)
            }
        }

        // Derive cwd + session_id from path.
        let session_id = source
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let cwd = source
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(decode_cwd)
            .unwrap_or_default();
        let title = if session_id.len() >= 8 {
            session_id[..8].to_string()
        } else {
            session_id.clone()
        };

        Some(UsageSession {
            tool: ToolKind::ClaudeCode,
            session_id,
            title,
            cwd,
            model,
            started_at: first_ts.unwrap_or(last_ts),
            updated_at: last_ts,
            tokens,
            cost_usd: None,
        })
    }
}

/// Parse an ISO-8601 timestamp like "2025-08-20T19:42:30.851Z" to epoch ms.
fn parse_iso_ms(s: &str) -> Option<i64> {
    // Cheap parse: "YYYY-MM-DDTHH:MM:SS.mmmZ"
    let s = s.trim();
    if s.len() < 20 { return None; }
    let b = s.as_bytes();
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let h: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let se: i64 = s.get(17..19)?.parse().ok()?;
    let ms: i64 = s.get(20..23).unwrap_or("0").parse().ok()?;
    // Approximate epoch ms via days-since-epoch (not perfect, no leap-second
    // handling, but sufficient for ordering/filtering).
    let days = days_from_civil(y, mo, d);
    Some(((days * 86400 + h * 3600 + mi * 60 + se) * 1000) + ms)
}

/// Howard Hinnant's days_from_civil (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe as i64 - 719468
}
```

- [ ] **Step 2: Write a test with a fixture JSONL**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_assistant_usage() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("D--agents-Foo");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("abc12345-uuid.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type":"user","timestamp":"2025-08-20T19:40:00.000Z"}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","timestamp":"2025-08-20T19:42:30.851Z","message":{{"model":"claude-sonnet-4-20250514","usage":{{"input_tokens":45871,"cache_creation_input_tokens":0,"cache_read_input_tokens":4480,"output_tokens":878}}}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","timestamp":"2025-08-20T19:43:00.000Z","message":{{"model":"claude-sonnet-4-20250514","usage":{{"input_tokens":1000,"cache_creation_input_tokens":50,"cache_read_input_tokens":0,"output_tokens":200}}}}}}"#).unwrap();
        drop(f);

        let provider = ClaudeProvider { root: Some(dir.path().join("projects")) };
        // Rewrite: we created the dir directly, so point root at dir.path()
        let provider = ClaudeProvider { root: Some(dir.path().to_path_buf()) };
        let discovered = provider.discover();
        assert_eq!(discovered.len(), 1);
        let s = provider.parse(&discovered[0]).unwrap();
        assert_eq!(s.session_id, "abc12345-uuid");
        assert_eq!(s.cwd, "D:\\agents\\Foo");
        assert_eq!(s.model, "claude-sonnet-4-20250514");
        assert_eq!(s.tokens.input, 46871);       // 45871 + 1000
        assert_eq!(s.tokens.output, 1078);        // 878 + 200
        assert_eq!(s.tokens.cache_write, 50);
        assert_eq!(s.tokens.cache_read, 4480);
        assert!(s.cost_usd.is_none());
        assert_eq!(s.started_at, parse_iso_ms("2025-08-20T19:40:00.000Z").unwrap());
        assert_eq!(s.updated_at, parse_iso_ms("2025-08-20T19:43:00.000Z").unwrap());
    }

    #[test]
    fn decode_cwd_windows_drive() {
        assert_eq!(decode_cwd("D--agents-Foo"), "D:\\agents\\Foo");
        assert_eq!(decode_cwd("C--Users-xujin"), "C:\\Users\\xujin");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage::claude`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/usage/claude.rs
git commit -m "feat(usage): Claude Code JSONL parser"
```

---

## Task 6: Codex parser (`codex.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/codex.rs`

- [ ] **Step 1: Write the Codex JSONL parser**

Codex writes rollout JSONL files at `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. Token usage lives in `event_msg` events with `payload.type == "token_count"`; the `total_token_usage` field is **cumulative** across the whole session, so we take the LAST such event. `input_tokens` INCLUDES `cached_input_tokens`, so non-cached input = `input_tokens - cached_input_tokens`.

```rust
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir};

pub struct CodexProvider {
    root: Option<PathBuf>,
}

impl CodexProvider {
    pub fn new() -> Self {
        Self { root: resolve_root() }
    }
}

fn resolve_root() -> Option<PathBuf> {
    let base = env_path_or("CODEX_HOME", || home_dir().map(|h| h.join(".codex")))?;
    let sessions = base.join("sessions");
    if sessions.is_dir() { Some(sessions) } else { None }
}

#[derive(Deserialize)]
struct CodexLine {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Deserialize, Default)]
struct TotalUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
}

impl UsageProvider for CodexProvider {
    fn tool_kind(&self) -> ToolKind { ToolKind::Codex }

    fn discover(&self) -> Vec<DiscoveredSession> {
        let Some(root) = &self.root else { return vec![] };
        let mut out = Vec::new();
        walk_jsonl(root, &mut out);
        // Also check archived_sessions sibling.
        if let Some(archive) = root.parent().map(|p| p.join("archived_sessions")) {
            if archive.is_dir() { walk_jsonl(&archive, &mut out); }
        }
        out
    }

    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession> {
        let file = std::fs::File::open(&source.path).ok()?;
        let reader = BufReader::new(file);
        let mut total: Option<TotalUsage> = None;
        let mut model = String::new();
        let mut cwd = String::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts: i64 = 0;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.is_empty() { continue; }
            let Ok(parsed): std::result::Result<CodexLine, _> = serde_json::from_str(&line) else { continue };

            // session_meta (first line): cwd
            if parsed.r#type == "session_meta" {
                if let Some(obj) = parsed.payload.as_object() {
                    if let Some(c) = obj.get("cwd").and_then(|v| v.as_str()) {
                        cwd = c.to_string();
                    }
                }
            }
            // turn_context: model
            if parsed.r#type == "turn_context" {
                if let Some(m) = parsed.payload.get("model").and_then(|v| v.as_str()) {
                    if !m.is_empty() { model = m.to_string(); }
                }
            }
            // event_msg with token_count: take the LAST one (cumulative).
            if parsed.r#type == "event_msg" {
                if parsed.payload.get("type").and_then(|v| v.as_str()) == Some("token_count") {
                    if let Some(info) = parsed.payload.get("info") {
                        if let Some(t) = info.get("total_token_usage") {
                            if let Ok(parsed_total) = serde_json::from_value::<TotalUsage>(t.clone()) {
                                total = Some(parsed_total);
                            }
                        }
                    }
                }
            }
            if let Some(ts) = parsed.timestamp.as_deref().and_then(parse_iso_ms) {
                if first_ts.is_none() { first_ts = Some(ts); }
                last_ts = ts;
            }
        }

        let t = total?;
        // Codex: input_tokens INCLUDES cached_input_tokens.
        let non_cached_input = t.input_tokens.saturating_sub(t.cached_input_tokens);
        let session_id = source
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let title = session_id.get(..20).unwrap_or(&session_id).to_string();

        Some(UsageSession {
            tool: ToolKind::Codex,
            session_id,
            title,
            cwd,
            model,
            started_at: first_ts.unwrap_or(last_ts),
            updated_at: last_ts,
            tokens: TokenUsage {
                input: non_cached_input,
                output: t.output_tokens,
                reasoning: t.reasoning_output_tokens,
                cache_read: t.cached_input_tokens,
                cache_write: 0, // Codex has no cache_write concept.
            },
            cost_usd: None,
        })
    }
}

fn walk_jsonl(dir: &std::path::Path, out: &mut Vec<DiscoveredSession>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let ft = match e.file_type() { Ok(t) => t, Err(_) => continue };
        if ft.is_dir() {
            walk_jsonl(&path, out);
        } else if ft.is_file() {
            if path.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")).unwrap_or(false) {
                let meta = std::fs::metadata(&path).ok();
                let mtime = meta.as_ref().and_then(|m| m.modified().ok());
                let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
                out.push(DiscoveredSession {
                    key: path.to_string_lossy().into_owned(),
                    path,
                    mtime,
                    size,
                });
            }
        }
    }
}

/// Reuse the same ISO parser as claude.rs (duplicated to keep modules independent).
fn parse_iso_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 20 { return None; }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let h: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let se: i64 = s.get(17..19)?.parse().ok()?;
    let ms: i64 = s.get(20..23).unwrap_or("0").parse().ok()?;
    let days = days_from_civil(y, mo, d);
    Some(((days * 86400 + h * 3600 + mi * 60 + se) * 1000) + ms)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}
```

- [ ] **Step 2: Write a test with a fixture rollout JSONL**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_last_cumulative_token_count() {
        let dir = tempfile::tempdir().unwrap();
        let sess_dir = dir.path().join("2026").join("05").join("20");
        std::fs::create_dir_all(&sess_dir).unwrap();
        let file = sess_dir.join("rollout-2026-05-20T10-26-21-abc.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-05-20T10:26:21.000Z","type":"session_meta","payload":{{"cwd":"D:\\\\repo"}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-05-20T10:26:22.000Z","type":"turn_context","payload":{{"model":"gpt-5"}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-05-20T10:27:00.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":50,"reasoning_output_tokens":10,"total_tokens":1050}}}}}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-05-20T10:28:00.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":5000,"cached_input_tokens":1000,"output_tokens":200,"reasoning_output_tokens":40,"total_tokens":5040}}}}}}}}"#).unwrap();
        drop(f);

        let provider = CodexProvider { root: Some(dir.path().to_path_buf()) };
        let discovered = provider.discover();
        assert_eq!(discovered.len(), 1);
        let s = provider.parse(&discovered[0]).unwrap();
        // LAST cumulative event: input=5000, cached=1000 -> non_cached=4000
        assert_eq!(s.tokens.input, 4000);
        assert_eq!(s.tokens.cache_read, 1000);
        assert_eq!(s.tokens.output, 200);
        assert_eq!(s.tokens.reasoning, 40);
        assert_eq!(s.tokens.cache_write, 0);
        assert_eq!(s.cwd, "D:\\repo");
        assert_eq!(s.model, "gpt-5");
        assert!(s.cost_usd.is_none());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage::codex`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/usage/codex.rs
git commit -m "feat(usage): Codex JSONL parser"
```

---

## Task 7: Kimi Code parser (`kimi.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/kimi.rs`

- [ ] **Step 1: Write the Kimi Code JSONL parser**

Kimi Code stores sessions under `~/.kimi-code/sessions/<workDirKey>/<sessionId>/agents/main/wire.jsonl`. Token usage is in `usage.record` events with fields `inputOther`, `output`, `inputCacheRead`, `inputCacheCreation`. We sum all such events per session. The session index at `~/.kimi-code/session_index.jsonl` maps sessionId -> workDir.

```rust
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir};

pub struct KimiProvider {
    root: Option<PathBuf>,
}

impl KimiProvider {
    pub fn new() -> Self {
        Self { root: resolve_root() }
    }
}

fn resolve_root() -> Option<PathBuf> {
    let base = env_path_or("KIMI_CODE_HOME", || home_dir().map(|h| h.join(".kimi-code")))?;
    if base.is_dir() { Some(base) } else { None }
}

#[derive(Deserialize)]
struct IndexEntry {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    session_dir: Option<String>,
    #[serde(default)]
    work_dir: Option<String>,
}

#[derive(Deserialize)]
struct WireLine {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<UsageRecord>,
    #[serde(default)]
    time: Option<i64>,
}

#[derive(Deserialize, Default)]
struct UsageRecord {
    #[serde(default, rename = "inputOther")]
    input_other: u64,
    #[serde(default)]
    output: u64,
    #[serde(default, rename = "inputCacheRead")]
    input_cache_read: u64,
    #[serde(default, rename = "inputCacheCreation")]
    input_cache_creation: u64,
}

impl UsageProvider for KimiProvider {
    fn tool_kind(&self) -> ToolKind { ToolKind::KimiCode }

    fn discover(&self) -> Vec<DiscoveredSession> {
        let Some(root) = &self.root else { return vec![] };
        // Read session_index.jsonl for the session -> workDir mapping.
        let index_path = root.join("session_index.jsonl");
        let mut entries: Vec<IndexEntry> = Vec::new();
        if let Ok(f) = std::fs::File::open(&index_path) {
            for line in BufReader::new(f).lines().flatten() {
                if line.is_empty() { continue; }
                if let Ok(e) = serde_json::from_str::<IndexEntry>(&line) {
                    entries.push(e);
                }
            }
        }
        // Fallback: if no index, walk sessions/<workDirKey>/<sessionId>/agents/main/wire.jsonl.
        if entries.is_empty() {
            return discover_by_walking(root);
        }

        let mut out = Vec::new();
        for e in entries {
            let sid = match e.session_id { Some(s) if !s.is_empty() => s, _ => continue };
            // sessionDir is relative to root or absolute.
            let session_dir = match &e.session_dir {
                Some(s) if !s.is_empty() => {
                    let p = PathBuf::from(s);
                    if p.is_absolute() { p } else { root.join(s) }
                }
                _ => root.join("sessions").join(&sid),
            };
            let wire = session_dir.join("agents").join("main").join("wire.jsonl");
            if !wire.is_file() { continue; }
            let meta = std::fs::metadata(&wire).ok();
            let mtime = meta.as_ref().and_then(|m| m.modified().ok());
            let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
            out.push(DiscoveredSession {
                key: sid,
                path: wire,
                mtime,
                size,
            });
        }
        out
    }

    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession> {
        let file = std::fs::File::open(&source.path).ok()?;
        let reader = BufReader::new(file);
        let mut tokens = TokenUsage::default();
        let mut model = String::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts: i64 = 0;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.is_empty() { continue; }
            let Ok(parsed): std::result::Result<WireLine, _> = serde_json::from_str(&line) else { continue };
            if parsed.r#type == "usage.record" {
                if let Some(u) = &parsed.usage {
                    tokens.input += u.input_other;
                    tokens.output += u.output;
                    tokens.cache_read += u.input_cache_read;
                    tokens.cache_write += u.input_cache_creation;
                }
                if let Some(m) = &parsed.model {
                    if !m.is_empty() { model = m.clone(); }
                }
            }
            if let Some(t) = parsed.time {
                if first_ts.is_none() { first_ts = Some(t); }
                last_ts = t;
            }
        }

        // cwd: we stored sessionId in key; cwd is not directly in wire.jsonl.
        // Try to recover from the index (best-effort: leave empty if unknown).
        let cwd = String::new();
        let title = if source.key.len() >= 12 {
            source.key[..12].to_string()
        } else {
            source.key.clone()
        };

        Some(UsageSession {
            tool: ToolKind::KimiCode,
            session_id: source.key.clone(),
            title,
            cwd,
            model,
            started_at: first_ts.unwrap_or(last_ts),
            updated_at: last_ts,
            tokens,
            cost_usd: None,
        })
    }
}

fn discover_by_walking(root: &std::path::Path) -> Vec<DiscoveredSession> {
    let mut out = Vec::new();
    let sessions = root.join("sessions");
    let Ok(work_dirs) = std::fs::read_dir(&sessions) else { return out };
    for wd in work_dirs.flatten() {
        if !wd.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
        let Ok(sessions) = std::fs::read_dir(wd.path()) else { continue };
        for s in sessions.flatten() {
            if !s.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
            let sid = s.file_name().to_string_lossy().into_owned();
            let wire = s.path().join("agents").join("main").join("wire.jsonl");
            if !wire.is_file() { continue; }
            let meta = std::fs::metadata(&wire).ok();
            let mtime = meta.as_ref().and_then(|m| m.modified().ok());
            let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
            out.push(DiscoveredSession { key: sid, path: wire, mtime, size });
        }
    }
    out
}
```

- [ ] **Step 2: Write a test with a fixture wire.jsonl + index**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sums_usage_record_events() {
        let dir = tempfile::tempdir().unwrap();
        let session_dir = dir.path().join("sessions").join("key1").join("sid_abc123");
        std::fs::create_dir_all(session_dir.join("agents").join("main")).unwrap();
        let wire = session_dir.join("agents").join("main").join("wire.jsonl");
        let mut f = std::fs::File::create(&wire).unwrap();
        writeln!(f, r#"{{"type":"llm.request","model":"kimi-code/kimi-for-coding","time":1784165547949}}"#).unwrap();
        writeln!(f, r#"{{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{{"inputOther":3269,"output":219,"inputCacheRead":18176,"inputCacheCreation":0}},"usageScope":"turn","time":1784165547949}}"#).unwrap();
        writeln!(f, r#"{{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{{"inputOther":1000,"output":500,"inputCacheRead":2000,"inputCacheCreation":100}},"usageScope":"turn","time":1784165600000}}"#).unwrap();
        drop(f);

        let provider = KimiProvider { root: Some(dir.path().to_path_buf()) };
        let discovered = provider.discover();
        assert_eq!(discovered.len(), 1);
        let s = provider.parse(&discovered[0]).unwrap();
        assert_eq!(s.tokens.input, 4269);        // 3269 + 1000
        assert_eq!(s.tokens.output, 719);         // 219 + 500
        assert_eq!(s.tokens.cache_read, 20176);   // 18176 + 2000
        assert_eq!(s.tokens.cache_write, 100);    // 0 + 100
        assert_eq!(s.model, "kimi-code/kimi-for-coding");
        assert!(s.cost_usd.is_none());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage::kimi`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/usage/kimi.rs
git commit -m "feat(usage): Kimi Code JSONL parser"
```

---

## Task 8: Scanner + cache (`scanner.rs`, `mod.rs`)

**Files:**
- Create: `src-tauri/src/services/usage/scanner.rs`
- Create: `src-tauri/src/services/usage/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Write the scanner with mtime-based incremental skipping**

`scanner.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;

use super::model::{ToolKind, UsageSession, UsageSummary};
use super::provider::{DiscoveredSession, UsageProvider};

/// Cache of parsed sessions, plus mtime/size fingerprints for incremental skip.
pub struct UsageCache {
    sessions: Vec<UsageSession>,
    /// fingerprints keyed by DiscoveredSession.key + suffix
    fingerprints: HashMap<String, (SystemTime, u64)>,
    /// cached per-source parse results, keyed by DiscoveredSession.key + suffix.
    /// The key is the same fingerprint key so reuse is a direct lookup.
    parsed: HashMap<String, UsageSession>,
}

impl Default for UsageCache {
    fn default() -> Self {
        Self { sessions: Vec::new(), fingerprints: HashMap::new(), parsed: HashMap::new() }
    }
}

impl UsageCache {
    /// Rebuild `sessions` from `parsed` map. Called after a scan pass.
    fn rebuild(&mut self) {
        self.sessions = self.parsed.values().cloned().collect();
        // Sort by updated_at desc for a sensible default order.
        self.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }

    pub fn summary(&self) -> UsageSummary {
        UsageSummary::from_sessions(&self.sessions)
    }

    pub fn sessions_filtered(
        &self,
        tool: Option<ToolKind>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Vec<UsageSession> {
        let mut out: Vec<UsageSession> = self
            .sessions
            .iter()
            .filter(|s| tool.map_or(true, |t| s.tool == t))
            .filter(|s| since.map_or(true, |since| s.updated_at >= since))
            .cloned()
            .collect();
        if let Some(limit) = limit {
            out.truncate(limit);
        }
        out
    }
}

/// All four providers. OpenCode needs special handling (parse_all).
fn build_providers() -> (
    super::opencode::OpenCodeProvider,
    super::claude::ClaudeProvider,
    super::codex::CodexProvider,
    super::kimi::KimiProvider,
) {
    (
        super::opencode::OpenCodeProvider::new(),
        super::claude::ClaudeProvider::new(),
        super::codex::CodexProvider::new(),
        super::kimi::KimiProvider::new(),
    )
}

/// Run one full scan pass, updating the cache in place.
pub fn scan_once(cache: &Mutex<UsageCache>) {
    let (oc, cc, cx, kc) = build_providers();

    // OpenCode: special parse_all (one DB -> many sessions).
    // We track the DB file's mtime/size to skip re-querying when unchanged.
    let mut new_parsed: HashMap<String, UsageSession> = HashMap::new();
    let mut new_fingerprints: HashMap<String, (SystemTime, u64)> = HashMap::new();

    // --- OpenCode ---
    {
        let discovered = oc.discover();
        if let Some(src) = discovered.first() {
            let fp_key = src.key.clone() + "|oc";
            let unchanged = cache.lock().fingerprints.get(&fp_key)
                .map_or(false, |(m, s)| src.mtime.map_or(false, |m2| m2 == *m) && src.size == Some(*s));
            if unchanged {
                // Reuse cached OpenCode sessions (all keys ending in "|oc").
                let g = cache.lock();
                for (k, s) in g.parsed.iter() {
                    if k.ends_with("|oc") {
                        new_parsed.insert(k.clone(), s.clone());
                    }
                }
                new_fingerprints.insert(fp_key, (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
            } else {
                if let Some(sessions) = oc.parse_all() {
                    new_fingerprints.insert(fp_key, (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
                    for s in sessions {
                        new_parsed.insert(s.session_id.clone() + "|oc", s);
                    }
                }
            }
        }
    }

    // --- JSONL providers: Claude, Codex, Kimi ---
    scan_jsonl_provider(&cc, &mut new_parsed, &mut new_fingerprints, &cache.lock(), "cc");
    scan_jsonl_provider(&cx, &mut new_parsed, &mut new_fingerprints, &cache.lock(), "cx");
    scan_jsonl_provider(&kc, &mut new_parsed, &mut new_fingerprints, &cache.lock(), "kc");

    // Commit.
    let mut g = cache.lock();
    g.parsed = new_parsed;
    g.fingerprints = new_fingerprints;
    g.rebuild();
}

fn scan_jsonl_provider<P: UsageProvider>(
    provider: &P,
    new_parsed: &mut HashMap<String, UsageSession>,
    new_fingerprints: &mut HashMap<String, (SystemTime, u64)>,
    cache: &UsageCache,
    suffix: &str,
) {
    for src in provider.discover() {
        let fp_key = format!("{}|{}", src.key, suffix);
        let unchanged = cache.fingerprints.get(&fp_key)
            .map_or(false, |(m, s)| src.mtime.map_or(false, |m2| m2 == *m) && src.size == Some(*s));
        new_fingerprints.insert(fp_key.clone(), (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
        if unchanged {
            // Reuse the cached parse for this exact source key.
            if let Some(cached) = cache.parsed.get(&fp_key) {
                new_parsed.insert(fp_key, cached.clone());
                continue;
            }
        }
        if let Some(session) = provider.parse(&src) {
            new_parsed.insert(fp_key, session);
        }
    }
}

/// Spawn the background scan loop. Returns immediately; the thread runs for
/// the lifetime of the app. Emits `usage-updated` to all windows after each
/// successful scan.
pub fn spawn_scan_loop(app: tauri::AppHandle, cache: Arc<Mutex<UsageCache>>) {
    std::thread::spawn(move || {
        // Initial scan immediately on startup.
        scan_once(&cache);
        let _ = app.emit("usage-updated", ());
        loop {
            std::thread::sleep(Duration::from_secs(60));
            scan_once(&cache);
            let _ = app.emit("usage-updated", ());
        }
    });
}
```

- [ ] **Step 2: Write `mod.rs` with the public entry point**

`mod.rs`:

```rust
pub mod model;
pub mod provider;
pub mod opencode;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod scanner;

pub use model::{ToolKind, TokenUsage, UsageSession, ToolSummary, UsageSummary};
pub use scanner::{UsageCache, scan_once, spawn_scan_loop};
```

- [ ] **Step 3: Register the module in `services/mod.rs`**

Add `pub mod usage;` to `src-tauri/src/services/mod.rs` (in alphabetical position, after `pub mod procs;`):

```rust
pub mod procs;
pub mod usage;
pub mod watch;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles. (You will likely need `tauri::Emitter` in scope in scanner.rs — add `use tauri::Emitter;` at the top of `scanner.rs` if the compiler complains about `app.emit`.)

- [ ] **Step 5: Run all usage tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/usage/scanner.rs src-tauri/src/services/usage/mod.rs src-tauri/src/services/mod.rs
git commit -m "feat(usage): background scanner with mtime incremental skipping"
```

---

## Task 9: Wire usage cache into SharedState + add Tauri commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/bootstrap.rs`

- [ ] **Step 1: Add usage cache to SharedState**

In `src-tauri/src/commands.rs`, add the import and field. Find the `SharedState` struct definition and add a `usage` field:

```rust
use crate::services::usage::{self, UsageCache};
use std::sync::Arc;
```

Modify the `SharedState` struct:

```rust
pub struct SharedState {
    states: Mutex<HashMap<String, Arc<Mutex<AppState>>>>,
    settings: Arc<Mutex<Settings>>,
    pub usage: Arc<Mutex<UsageCache>>,
}
```

Update `SharedState::new`:

```rust
impl SharedState {
    pub fn new(settings: Settings) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            settings: Arc::new(Mutex::new(settings)),
            usage: Arc::new(Mutex::new(UsageCache::default())),
        }
    }
```

- [ ] **Step 2: Add the three Tauri commands**

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn usage_summary(state: State<SharedState>) -> usage::UsageSummary {
    state.usage.lock().summary()
}

#[tauri::command]
pub fn usage_sessions(
    state: State<SharedState>,
    tool: Option<usage::ToolKind>,
    since: Option<i64>,
    limit: Option<usize>,
) -> Vec<usage::UsageSession> {
    state.usage.lock().sessions_filtered(tool, since, limit)
}

#[tauri::command]
pub async fn usage_refresh(state: State<'_, SharedState>) -> Result<(), String> {
    // Run the scan on a blocking thread to avoid blocking the async runtime.
    let cache = state.usage.clone();
    tokio::task::spawn_blocking(move || {
        usage::scan_once(&cache);
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 3: Register the commands in `register_all`**

In `src-tauri/src/commands.rs`, find the `register_all` function's `generate_handler!` list and add the three new commands:

```rust
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_settings,
            // ... existing ...
            usage_summary,
            usage_sessions,
            usage_refresh,
        ])
```

- [ ] **Step 4: Spawn the scanner loop in bootstrap**

In `src-tauri/src/bootstrap.rs`, inside the `.setup(|app| { ... })` closure (after the existing autosave thread spawn), add:

```rust
        // Usage tracking: background scan loop.
        {
            let handle = app.handle().clone();
            let usage_cache = handle.state::<crate::commands::SharedState>().usage.clone();
            crate::services::usage::spawn_scan_loop(handle, usage_cache);
        }
```

Make sure `tauri::Emitter` is imported at the top of `scanner.rs` (the `app.emit` call needs it). Add to the top of `src-tauri/src/services/usage/scanner.rs`:

```rust
use tauri::Emitter;
```

- [ ] **Step 5: Verify compilation + clippy**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: both clean.

- [ ] **Step 6: Run full test suite to ensure no regressions**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/bootstrap.rs src-tauri/src/services/usage/scanner.rs
git commit -m "feat(usage): wire cache into SharedState + Tauri commands + scanner loop"
```

---

## Task 10: Frontend types + API layer

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/invoke.ts`

- [ ] **Step 1: Add TypeScript types**

In `src/lib/types.ts`, append at the end of the file:

```ts
/// Which CLI tool a usage record came from (mirrors Rust ToolKind).
export type ToolKind = "opencode" | "claude_code" | "codex" | "kimi_code";

/// Normalized token breakdown shared across all four tools.
export interface TokenUsage {
  input: number;
  output: number;
  reasoning: number;
  cache_read: number;
  cache_write: number;
}

/// One session's usage (mirrors Rust UsageSession).
export interface UsageSession {
  tool: ToolKind;
  session_id: string;
  title: string;
  cwd: string;
  model: string;
  started_at: number;
  updated_at: number;
  tokens: TokenUsage;
  cost_usd: number | null;
}

/// Per-tool aggregate for the summary cards (mirrors Rust ToolSummary).
export interface ToolSummary {
  tool: ToolKind;
  total_tokens: number;
  tokens: TokenUsage;
  session_count: number;
  cost_usd: number | null;
  last_updated: number;
}

/// Top-level summary payload (mirrors Rust UsageSummary).
export interface UsageSummary {
  tools: ToolSummary[];
}
```

- [ ] **Step 2: Add the `usage` namespace to `api`**

In `src/lib/invoke.ts`, add the `ToolKind`, `UsageSummary`, `UsageSession` to the type import, and add a `usage` namespace to the `api` object (next to the existing `git` namespace):

```ts
import type { AppStateView, Settings, ..., ToolKind, UsageSummary, UsageSession } from "./types";
```

Then add to the `api` object (after the `git: { ... }` block):

```ts
  usage: {
    summary: () => c<UsageSummary>("usage_summary"),
    sessions: (opts?: { tool?: ToolKind; since?: number; limit?: number }) =>
      c<UsageSession[]>("usage_sessions", opts ?? {}),
    refresh: () => c<void>("usage_refresh"),
  },
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts src/lib/invoke.ts
git commit -m "feat(usage): frontend types and api layer"
```

---

## Task 11: i18n keys

**Files:**
- Modify: `src/lib/i18n/en.ts`
- Modify: `src/lib/i18n/zh.ts`

- [ ] **Step 1: Add `usage` namespace to `en.ts`**

In `src/lib/i18n/en.ts`, add a new top-level `usage` key to the `en` object (e.g. after the `shortcuts` namespace):

```ts
  usage: {
    title: "Usage",
    refresh: "Refresh",
    today: "Today",
    week: "7 Days",
    month: "30 Days",
    all: "All",
    tokens: "tokens",
    sessions: "sessions",
    session: "session",
    cost: "Cost",
    model: "Model",
    tool: "Tool",
    time: "Time",
    notFound: "Not found",
    empty: "No usage data yet",
    noSessions: "No sessions",
    sortByTime: "By Time",
    sortByTokens: "By Tokens",
    allTools: "All Tools",
  },
```

- [ ] **Step 2: Add matching `usage` namespace to `zh.ts`**

In `src/lib/i18n/zh.ts`, add the same structure with Chinese values:

```ts
  usage: {
    title: "用量",
    refresh: "刷新",
    today: "今天",
    week: "近 7 天",
    month: "近 30 天",
    all: "全部",
    tokens: "tokens",
    sessions: "会话",
    session: "会话",
    cost: "成本",
    model: "模型",
    tool: "工具",
    time: "时间",
    notFound: "未找到",
    empty: "暂无用量数据",
    noSessions: "无会话",
    sortByTime: "按时间",
    sortByTokens: "按 Token",
    allTools: "全部工具",
  },
```

- [ ] **Step 3: Add command-palette label to both files**

In `en.ts`, inside the `commandPalette` namespace, add:

```ts
    openUsage: "Usage",
```

In `zh.ts`, inside the `commandPalette` namespace, add:

```ts
    openUsage: "用量",
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/i18n/en.ts src/lib/i18n/zh.ts
git commit -m "feat(usage): i18n keys for en and zh"
```

---

## Task 12: UsagePanel component

**Files:**
- Create: `src/components/UsagePanel.tsx`

- [ ] **Step 1: Write the UsagePanel component**

This is the main UI. It loads summary + sessions, shows 4 tool cards + a session table, with time-range and tool filters. It listens to the `usage-updated` event to auto-refresh.

```tsx
import { useEffect, useState, useCallback } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../lib/invoke";
import type { ToolKind, ToolSummary, UsageSession, UsageSummary } from "../lib/types";
import { useT } from "../lib/i18n/context";

type TimeRange = "today" | "week" | "month" | "all";

const TOOL_COLORS: Record<ToolKind, string> = {
  opencode: "#a855f7",
  claude_code: "#d97757",
  codex: "#22c55e",
  kimi_code: "#3b82f6",
};

const TOOL_LABELS: Record<ToolKind, string> = {
  opencode: "OpenCode",
  claude_code: "Claude Code",
  codex: "Codex",
  kimi_code: "Kimi Code",
};

const ALL_TOOLS: ToolKind[] = ["opencode", "claude_code", "codex", "kimi_code"];

function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return (n / 1000).toFixed(1).replace(/\.0$/, "") + "K";
  return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
}

function formatCost(c: number | null): string {
  if (c === null) return "-";
  return "$" + c.toFixed(2);
}

function sinceForRange(range: TimeRange): number | undefined {
  if (range === "all") return undefined;
  const now = Date.now();
  if (range === "today") return now - 24 * 60 * 60 * 1000;
  if (range === "week") return now - 7 * 24 * 60 * 60 * 1000;
  if (range === "month") return now - 30 * 24 * 60 * 60 * 1000;
  return undefined;
}

function formatTime(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (sameDay) return `${hh}:${mm}`;
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${mo}/${dd} ${hh}:${mm}`;
}

export default function UsagePanel({ onClose }: { onClose: () => void }) {
  const { t } = useT();
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [sessions, setSessions] = useState<UsageSession[]>([]);
  const [range, setRange] = useState<TimeRange>("week");
  const [toolFilter, setToolFilter] = useState<ToolKind | "all">("all");
  const [sortBy, setSortBy] = useState<"time" | "tokens">("time");

  const load = useCallback(async () => {
    const since = sinceForRange(range);
    const [sum, sess] = await Promise.all([
      api.usage.summary(),
      api.usage.sessions({ since, limit: 500 }),
    ]);
    setSummary(sum);
    setSessions(sess);
  }, [range]);

  // Initial load + reload when range changes.
  useEffect(() => {
    load();
  }, [load]);

  // Listen for background-scan completion.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen("usage-updated", () => { load(); }).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [load]);

  // Esc to close.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleRefresh = useCallback(async () => {
    await api.usage.refresh();
    await load();
  }, [load]);

  const filteredSessions = sessions
    .filter((s) => toolFilter === "all" || s.tool === toolFilter)
    .sort((a, b) => {
      if (sortBy === "tokens") return b.tokens.input + b.tokens.output + b.tokens.cache_read - a.tokens.input - a.tokens.output - a.tokens.cache_read;
      return b.updated_at - a.updated_at;
    });

  // Build a map for quick card lookup.
  const summaryMap = new Map<ToolKind, ToolSummary>();
  summary?.tools.forEach((ts) => summaryMap.set(ts.tool, ts));

  if (!summary) return null;

  const ranges: TimeRange[] = ["today", "week", "month", "all"];

  return (
    <div className="absolute inset-0 z-40 bg-black/35" onClick={onClose}>
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[760px] max-h-[80vh]">
        <div
          className="bg-muster-bg border border-white/[0.08] rounded-[10px] shadow-[0_12px_32px_rgba(0,0,0,0.5)] px-5 py-4 muster-pop flex flex-col max-h-[80vh]"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header: title + range + refresh */}
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold">{t("usage.title")}</h2>
            <div className="flex items-center gap-3">
              <div className="flex gap-1">
                {ranges.map((r) => (
                  <button
                    key={r}
                    onClick={() => setRange(r)}
                    className={`px-2 py-1 rounded text-[11px] transition-colors ${
                      range === r
                        ? "bg-muster-accent text-white"
                        : "bg-white/[0.05] text-muster-muted hover:bg-muster-hover-btn"
                    }`}
                  >
                    {t(`usage.${r}`)}
                  </button>
                ))}
              </div>
              <button
                onClick={handleRefresh}
                className="px-2 py-1 rounded bg-white/[0.05] text-[11px] text-muster-muted hover:bg-muster-hover-btn transition-colors"
                title={t("usage.refresh")}
              >
                ↻ {t("usage.refresh")}
              </button>
            </div>
          </div>

          {/* Summary cards */}
          <div className="grid grid-cols-4 gap-3 mb-4">
            {ALL_TOOLS.map((tk) => {
              const ts = summaryMap.get(tk);
              const color = TOOL_COLORS[tk];
              const found = ts && ts.session_count > 0;
              return (
                <div
                  key={tk}
                  className="bg-white/[0.03] border border-white/[0.06] rounded-lg px-3 py-2.5"
                >
                  <div className="flex items-center gap-1.5 mb-1.5">
                    <span
                      className="inline-block w-2 h-2 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="text-[11px] font-medium text-muster-muted truncate">
                      {TOOL_LABELS[tk]}
                    </span>
                  </div>
                  {found ? (
                    <>
                      <div className="text-lg font-semibold tabular-nums">
                        {formatTokens(ts!.total_tokens)}
                      </div>
                      <div className="text-[10px] text-muster-muted mb-1">
                        {t("usage.tokens")}
                      </div>
                      <div className="text-[11px] tabular-nums text-muster-muted">
                        {formatCost(ts!.cost_usd)} · {ts!.session_count}{" "}
                        {ts!.session_count === 1 ? t("usage.session") : t("usage.sessions")}
                      </div>
                    </>
                  ) : (
                    <div className="text-[11px] text-muster-muted/60 py-2">
                      {t("usage.notFound")}
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* Sessions table */}
          <div className="flex items-center justify-between mb-2">
            <span className="text-[11px] font-medium text-muster-muted uppercase tracking-wide">
              {t("usage.sessions")}
            </span>
            <div className="flex items-center gap-2">
              <select
                value={toolFilter}
                onChange={(e) => setToolFilter(e.target.value as ToolKind | "all")}
                className="bg-white/[0.05] border border-white/[0.06] rounded text-[11px] px-2 py-1 text-muster-muted outline-none"
              >
                <option value="all">{t("usage.allTools")}</option>
                {ALL_TOOLS.map((tk) => (
                  <option key={tk} value={tk}>{TOOL_LABELS[tk]}</option>
                ))}
              </select>
              <select
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as "time" | "tokens")}
                className="bg-white/[0.05] border border-white/[0.06] rounded text-[11px] px-2 py-1 text-muster-muted outline-none"
              >
                <option value="time">{t("usage.sortByTime")}</option>
                <option value="tokens">{t("usage.sortByTokens")}</option>
              </select>
            </div>
          </div>

          <div className="overflow-y-auto flex-1 -mx-1 px-1">
            {filteredSessions.length === 0 ? (
              <div className="text-center text-[11px] text-muster-muted/60 py-8">
                {t("usage.noSessions")}
              </div>
            ) : (
              <table className="w-full text-[11px]">
                <thead>
                  <tr className="text-left text-muster-muted/70 border-b border-white/[0.06]">
                    <th className="py-1.5 pr-3 font-normal">{t("usage.time")}</th>
                    <th className="py-1.5 pr-3 font-normal">{t("usage.tool")}</th>
                    <th className="py-1.5 pr-3 font-normal">{t("usage.model")}</th>
                    <th className="py-1.5 pr-3 font-normal text-right">{t("usage.tokens")}</th>
                    <th className="py-1.5 font-normal text-right">{t("usage.cost")}</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredSessions.map((s) => {
                    const total = s.tokens.input + s.tokens.output + s.tokens.reasoning + s.tokens.cache_read + s.tokens.cache_write;
                    return (
                      <tr key={`${s.tool}-${s.session_id}`} className="border-b border-white/[0.03] hover:bg-white/[0.02]">
                        <td className="py-1.5 pr-3 tabular-nums text-muster-muted">
                          {formatTime(s.updated_at)}
                        </td>
                        <td className="py-1.5 pr-3">
                          <span className="inline-flex items-center gap-1">
                            <span
                              className="inline-block w-1.5 h-1.5 rounded-full"
                              style={{ backgroundColor: TOOL_COLORS[s.tool] }}
                            />
                            {TOOL_LABELS[s.tool]}
                          </span>
                        </td>
                        <td className="py-1.5 pr-3 text-muster-muted truncate max-w-[160px]">
                          {s.model || "—"}
                        </td>
                        <td className="py-1.5 pr-3 text-right tabular-nums">
                          {formatTokens(total)}
                        </td>
                        <td className="py-1.5 text-right tabular-nums text-muster-muted">
                          {formatCost(s.cost_usd)}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/UsagePanel.tsx
git commit -m "feat(usage): UsagePanel component with cards and session table"
```

---

## Task 13: Wire UsagePanel into App + CommandPalette

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/CommandPalette.tsx`

- [ ] **Step 1: Add state + import + NAV_MAP entry in App.tsx**

In `src/App.tsx`:

Add to imports (near the `ShortcutsHelp` import):

```tsx
import UsagePanel from "./components/UsagePanel";
```

Add state (near `const [showShortcuts, setShowShortcuts] = useState(false);`):

```tsx
const [showUsage, setShowUsage] = useState(false);
```

Add to the `NAV_MAP` inside the `useEffect` (near `"ctrl+shift+i"`):

```tsx
    "ctrl+shift+u": () => setShowUsage(true),
```

Add the render (near `{showShortcuts && <ShortcutsHelp ... />}`):

```tsx
{showUsage && <UsagePanel onClose={() => setShowUsage(false)} />}
```

- [ ] **Step 2: Add the command to CommandPalette.tsx**

In `src/components/CommandPalette.tsx`:

Add `onOpenUsage` to the component props interface:

```ts
  onOpenUsage: () => void;
```

Add the command entry in the `commands` useMemo array (after the `keyboard-shortcuts` entry):

```ts
    { id: "open-usage", title: t("commandPalette.openUsage"), icon: "▤", shortcut: "Ctrl+Shift+U", action: onOpenUsage },
```

Add `onOpenUsage` to the `useMemo` deps array.

Destructure `onOpenUsage` from props in the function signature.

- [ ] **Step 3: Pass the prop from App.tsx**

In `src/App.tsx`, find the `<CommandPalette ... />` render and add the prop:

```tsx
{showPalette && (
  <CommandPalette
    onClose={() => setShowPalette(false)}
    onAskNewProject={newProjectWithDialog}
    onClearTerminal={clearTerminal}
    onCloseProject={closeSelectedProject}
    onOpenSettings={() => setShowSettings(true)}
    onOpenShortcuts={() => setShowShortcuts(true)}
    onOpenUsage={() => setShowUsage(true)}
  />
)}
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/components/CommandPalette.tsx
git commit -m "feat(usage): wire UsagePanel into App and command palette"
```

---

## Task 14: Manual verification + README

**Files:**
- Modify: `README.md` (optional, if it lists features/shortcuts)

- [ ] **Step 1: Run the dev server and verify end-to-end**

Run: `npm run tauri dev`

Verify:
1. App starts without errors (check terminal + devtools console).
2. `Ctrl+Shift+U` opens the Usage panel.
3. Command palette (`Ctrl+P`) shows the "Usage" command and opens the panel.
4. Cards for installed tools show token counts; uninstalled tools show "Not found".
5. Session table populates with rows.
6. Time range buttons (Today/7 Days/30 Days/All) filter the sessions.
7. Tool filter dropdown filters by tool.
8. Sort dropdown changes ordering.
9. Refresh button triggers a rescan.
10. Closing via overlay click or `Esc` works.
11. Switch language (Settings) to zh — all Usage panel labels translate.

- [ ] **Step 2: Run linters**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Run: `npx tsc --noEmit`
Expected: both clean.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass.

- [ ] **Step 4: Update README shortcuts section (if present)**

If `README.md` has a keyboard shortcuts section, add:

```
- `Ctrl+Shift+U` — Open Usage panel
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: add Usage panel shortcut to README"
```

(If README wasn't changed, skip this commit.)

---

## Notes for the implementer

1. **rusqlite connection opening**: The plan uses `Connection::open_with_flags` with a `file:...?mode=ro&immutable=1` URI plus `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_OPEN_URI` flags. If the compiler rejects the exact flag combination, the minimal requirement is `OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI` — do NOT include `SQLITE_OPEN_CREATE` or write flags, since the live opencode process holds the WAL write lock.

2. **OpenCode is special-cased**: Unlike the JSONL tools (one file = one session), OpenCode has one DB = many sessions. The scanner calls `OpenCodeProvider::parse_all()` instead of `parse()`. The trait's `parse()` returns `None` for OpenCode; this is by design.

3. **mtime incremental skipping**: The scanner stores `(mtime, size)` per source. On the next scan, if both are unchanged, it reuses the cached parse instead of re-reading. This is critical for the 200MB+ OpenCode DB and large accumulated JSONL files.

4. **Concurrency**: The codebase uses `parking_lot::Mutex` and `std::thread::spawn` (not `tokio::spawn` / `RwLock`). The plan follows this. The scan loop is a bare OS thread sleeping 60s between passes, matching the autosave thread pattern in `bootstrap.rs`.

5. **Frontend `useTauriEvent` hook exists** but returns a `[value, setter]` tuple for state. The UsagePanel instead uses `listen()` directly (like the existing pattern in some components) because it needs to trigger a reload function, not just store a value.

6. **The `usage_refresh` command uses `tokio::task::spawn_blocking`** because `scan_once` does synchronous file/DB IO and must not block the async Tauri command runtime.
