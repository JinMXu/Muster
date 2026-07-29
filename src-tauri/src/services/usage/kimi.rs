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

impl Default for KimiProvider {
    fn default() -> Self {
        Self::new()
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
    // work_dir is part of the index schema (sessionId -> workDir) but not yet
    // threaded through to parse(); kept for forward compatibility.
    #[serde(default)]
    #[allow(dead_code)]
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
            let reader = BufReader::new(f);
            for line in reader.lines() {
                let Ok(line) = line else { continue };
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
            let size = meta.as_ref().map(|m| m.len());
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
        // Best-effort: leave empty if unknown (the index has work_dir but we
        // don't thread it through to parse() - could be improved later).
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
            let size = meta.as_ref().map(|m| m.len());
            out.push(DiscoveredSession { key: sid, path: wire, mtime, size });
        }
    }
    out
}

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
