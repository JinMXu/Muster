use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::model::{UsageEvent, UsageSource};

pub mod claude;
pub mod codebuddy;
pub mod codex;
pub mod grok;
pub mod kimi;
pub mod opencode;
pub mod pi;
pub mod qoder;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileCursor {
    pub size: u64,
    pub mtime_ms: i64,
}

pub struct ScanCtx {
    pub cursors: HashMap<String, FileCursor>,
    pub opencode_ts: i64,
    pub events: Vec<UsageEvent>,
    pub dirty: bool,
    min_ts_ms: i64,
    local_ids: HashSet<String>,
}

impl ScanCtx {
    pub fn new(cursors: HashMap<String, FileCursor>, opencode_ts: i64, min_ts_ms: i64) -> Self {
        Self {
            cursors,
            opencode_ts,
            events: Vec::new(),
            dirty: false,
            min_ts_ms,
            local_ids: HashSet::new(),
        }
    }

    /// True when `path` changed since the last scan (or is new). The cursor is
    /// updated even on parse failure, so a corrupt file is not retried on
    /// every tick.
    pub fn should_read(&mut self, path: &Path) -> bool {
        let Ok(meta) = fs::metadata(path) else {
            return false;
        };
        let size = meta.len();
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let key = path.to_string_lossy().to_string();
        let changed = self
            .cursors
            .get(&key)
            .map(|p| p.size != size || p.mtime_ms != mtime_ms)
            .unwrap_or(true);
        self.cursors.insert(key, FileCursor { size, mtime_ms });
        // Files untouched since the retention cutoff cannot contribute any
        // event we would keep.
        if mtime_ms < self.min_ts_ms {
            return false;
        }
        if changed {
            self.dirty = true;
        }
        changed
    }

    pub fn push(&mut self, event: UsageEvent) {
        if event.tokens.is_zero() {
            return;
        }
        if self.local_ids.insert(event.id.clone()) {
            self.events.push(event);
        }
    }

    pub fn push_simple(
        &mut self,
        id: String,
        agent: crate::services::agents::AgentKind,
        ts_ms: i64,
        model: Option<String>,
        cwd: Option<String>,
        session: Option<String>,
        tokens: super::model::TokenUsage,
        turns: u32,
        coarse: bool,
    ) {
        self.push(UsageEvent {
            id,
            agent,
            ts_ms,
            model: clean_model(model),
            cwd: clean_opt(cwd),
            session: clean_opt(session),
            tokens,
            turns,
            source: UsageSource::Native,
            coarse,
        });
    }
}

pub fn clean_opt(v: Option<String>) -> Option<String> {
    v.map(|s| {
        s.trim()
            .trim_matches(|c| c == '"' || c == '\'' || c == '\\')
            .trim()
            .to_string()
    })
    .filter(|s| !s.is_empty())
}

pub fn clean_model(v: Option<String>) -> Option<String> {
    clean_opt(v).filter(|s| s != "<synthetic>" && s != "synthetic")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

pub fn file_mtime_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn num(v: Option<&serde_json::Value>) -> u64 {
    v.and_then(|x| x.as_u64()).unwrap_or(0)
}

pub fn num_at(v: &serde_json::Value, key: &str) -> u64 {
    num(v.get(key))
}

pub fn walk_files(root: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    fn rec(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
        if depth > max_depth {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                rec(&path, depth + 1, max_depth, out);
            } else if kind.is_file() {
                out.push(path);
            }
        }
    }
    rec(root, 0, max_depth, out);
    out.sort();
}

pub fn event_id(prefix: &str, value: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    format!("{prefix}:{:016x}", h.finish())
}

pub fn scan_all(ctx: &mut ScanCtx) {
    // Each adapter is independent: one missing/corrupt source must not stop
    // the others. Errors are handled inside the adapters.
    claude::scan(ctx);
    qoder::scan(ctx);
    kimi::scan(ctx);
    pi::scan(ctx);
    opencode::scan(ctx);
    grok::scan(ctx);
    codebuddy::scan(ctx);
    codex::scan(ctx);
}
