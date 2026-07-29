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

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
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
                if let Some(c) = parsed.payload.get("cwd").and_then(|v| v.as_str()) {
                    cwd = c.to_string();
                }
            }
            // turn_context: model
            if parsed.r#type == "turn_context" {
                if let Some(m) = parsed.payload.get("model").and_then(|v| v.as_str()) {
                    if !m.is_empty() { model = m.to_string(); }
                }
            }
            // event_msg with token_count: take the LAST one (cumulative).
            if parsed.r#type == "event_msg"
                && parsed.payload.get("type").and_then(|v| v.as_str()) == Some("token_count")
            {
                if let Some(info) = parsed.payload.get("info") {
                    if let Some(t) = info.get("total_token_usage") {
                        if let Ok(parsed_total) = serde_json::from_value::<TotalUsage>(t.clone()) {
                            total = Some(parsed_total);
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
        } else if ft.is_file()
            && path.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")).unwrap_or(false)
        {
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
}

/// Parse an ISO-8601 timestamp like "2026-05-20T10:26:21.000Z" to epoch ms.
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
        // Realistic JSON: "D:\\repo" (two backslashes in the file) JSON-parses
        // to the string "D:\repo" (one backslash). In a raw string, backslashes
        // are literal (no escape processing), so we write two backslashes here.
        writeln!(f, r#"{{"timestamp":"2026-05-20T10:26:21.000Z","type":"session_meta","payload":{{"cwd":"D:\\repo"}}}}"#).unwrap();
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
