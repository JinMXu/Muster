use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir, parse_iso_ms};

pub struct ClaudeProvider {
    root: Option<PathBuf>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self { root: resolve_root() }
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
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
    let s = folder.to_string();
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
                let size = meta.as_ref().map(|m| m.len());
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

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.is_empty() { continue; }
            let Ok(parsed): std::result::Result<ClaudeLine, _> = serde_json::from_str(&line) else { continue };
            if let Some(ts) = parsed.timestamp.as_deref().and_then(parse_iso_ms) {
                if first_ts.is_none() { first_ts = Some(ts); }
                last_ts = ts;
            }
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
