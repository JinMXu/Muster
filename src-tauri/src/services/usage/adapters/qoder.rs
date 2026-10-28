use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use super::{clean_model, home, open_tail, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = home().join(".qoder");
    if !root.is_dir() {
        return;
    }
    let cwds = session_cwds(&root);
    let mut files = Vec::new();
    walk_files(&root.join("logs").join("runs"), 3, &mut files);
    for path in files {
        if path.file_name().and_then(|n| n.to_str()) != Some("qodercli.log") {
            continue;
        }
        // Append-only run log; each completion line is self-contained (cwd
        // comes from the project index), so tails are parsed incrementally.
        let Some(resume) = ctx.resume_at(&path) else {
            continue;
        };
        let Some(mut tail) = open_tail(&path, resume) else {
            continue;
        };
        for (idx, line) in tail.by_ref() {
            let Ok(line) = line else { continue };
            if !line.contains("model.response.completed") {
                continue;
            }
            let Some(ts) = line.split_whitespace().next().and_then(parse_iso_ms) else {
                continue;
            };
            let tokens = TokenUsage {
                input: field(&line, "input_tokens")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                output: field(&line, "output_tokens")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                reasoning: field(&line, "reasoning_tokens")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                cache_read: field(&line, "cache_read_input_tokens")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                cache_write: field(&line, "cache_creation_input_tokens")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            };
            if tokens.is_zero() {
                continue;
            }
            let session = field(&line, "session");
            let cwd = session.as_ref().and_then(|s| cwds.get(s).cloned());
            let id = field(&line, "request")
                .unwrap_or_else(|| super::event_id("qoder", &format!("{}:{idx}", path.display())));
            ctx.push_simple(
                format!("qoder:{id}"),
                AgentKind::Qoder,
                ts,
                clean_model(field(&line, "model")),
                cwd,
                session,
                tokens,
                1,
                false,
            );
        }
        ctx.advance(&path, tail.consumed(), tail.next_line());
    }
}

/// The run log carries session ids but no cwd; the project JSONL headers map
/// them. Reading only the first few lines keeps this cheap.
fn session_cwds(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut files = Vec::new();
    walk_files(&root.join("projects"), 3, &mut files);
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(file) = File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().take(5).flatten() {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("type").and_then(|x| x.as_str()) != Some("workspace-directories") {
                continue;
            }
            let Some(session) = v.get("sessionId").and_then(|x| x.as_str()) else {
                continue;
            };
            let Some(dir) = v
                .get("directories")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
            else {
                continue;
            };
            out.insert(session.to_string(), dir.to_string());
            break;
        }
    }
    out
}

fn field(line: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=");
    let start = line.find(&pat)? + pat.len();
    let rest = &line[start..];
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ']')
        .unwrap_or(rest.len());
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_completion_fields() {
        let line = r#"2026-10-21T15:01:37.840+08:00 INFO  [session=abc turn=t loop=t:1 request=req-1] model.response.completed request_index=1 request_id="req-1" provider="qoder" model="dfmodel" stop_reason="tool_use" input_tokens=10 output_tokens=20 cache_read_input_tokens=30 cache_creation_input_tokens=4"#;
        assert_eq!(field(line, "session").as_deref(), Some("abc"));
        assert_eq!(field(line, "request").as_deref(), Some("req-1"));
        assert_eq!(field(line, "model").as_deref(), Some("dfmodel"));
        assert_eq!(field(line, "input_tokens").as_deref(), Some("10"));
        assert_eq!(
            parse_iso_ms(line.split_whitespace().next().unwrap()),
            Some(1792566097840)
        );
    }
}
