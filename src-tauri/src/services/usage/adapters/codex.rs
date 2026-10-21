use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use super::{clean_model, home, num, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = home().join(".codex");
    for sub in ["sessions", "archived_sessions"] {
        let dir = root.join(sub);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk_files(&dir, 7, &mut files);
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".jsonl.zst") && !name.ends_with(".jsonl") {
                continue;
            }
            if !ctx.should_read(&path) {
                continue;
            }
            scan_file(ctx, &path);
        }
    }
}

fn scan_file(ctx: &mut ScanCtx, path: &Path) {
    let Ok(file) = File::open(path) else {
        return;
    };
    let reader: Box<dyn BufRead> = if path.extension().and_then(|e| e.to_str()) == Some("zst") {
        match zstd::stream::read::Decoder::new(file) {
            Ok(dec) => Box::new(BufReader::new(dec)),
            Err(_) => return,
        }
    } else {
        Box::new(BufReader::new(file))
    };

    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut session: Option<String> = None;
    // (input, cached, cache_write, output, reasoning) from total_token_usage.
    let mut prev: Option<(u64, u64, u64, u64, u64)> = None;

    for (idx, line) in reader.lines().enumerate() {
        let Ok(line) = line else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = v.get("payload");
        match v.get("type").and_then(|x| x.as_str()) {
            Some("session_meta") => {
                cwd = payload
                    .and_then(|p| p.get("cwd"))
                    .and_then(|x| x.as_str())
                    .map(str::to_owned);
                session = payload
                    .and_then(|p| p.get("id"))
                    .and_then(|x| x.as_str())
                    .map(str::to_owned);
            }
            Some("turn_context") => {
                model = payload
                    .and_then(|p| p.get("model"))
                    .or_else(|| {
                        payload
                            .and_then(|p| p.get("collaboration_mode"))
                            .and_then(|m| m.get("settings"))
                            .and_then(|s| s.get("model"))
                    })
                    .and_then(|x| x.as_str())
                    .map(str::to_owned);
            }
            Some("event_msg") => {
                if payload.and_then(|p| p.get("type")).and_then(|x| x.as_str())
                    != Some("token_count")
                {
                    continue;
                }
                let Some(info) = payload.and_then(|p| p.get("info")) else {
                    continue;
                };
                let Some(total) = info.get("total_token_usage") else {
                    continue;
                };
                let now = (
                    num(total.get("input_tokens")),
                    num(total.get("cached_input_tokens")),
                    num(total.get("cache_write_input_tokens")),
                    num(total.get("output_tokens")),
                    num(total.get("reasoning_output_tokens")),
                );
                let delta = match prev {
                    None => now,
                    Some(p) if now.0 >= p.0 && now.1 >= p.1 && now.3 >= p.3 && now.4 >= p.4 => (
                        now.0 - p.0,
                        now.1 - p.1,
                        now.2.saturating_sub(p.2),
                        now.3 - p.3,
                        now.4 - p.4,
                    ),
                    Some(_) => now, // totals reset (new session in the same file)
                };
                prev = Some(now);
                let tokens = TokenUsage {
                    input: delta.0.saturating_sub(delta.1),
                    output: delta.3,
                    reasoning: delta.4,
                    cache_read: delta.1,
                    cache_write: delta.2,
                };
                if tokens.is_zero() {
                    continue;
                }
                let Some(ts) = v
                    .get("timestamp")
                    .and_then(|x| x.as_str())
                    .and_then(parse_iso_ms)
                else {
                    continue;
                };
                let ordinal = v
                    .get("ordinal")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(idx as u64);
                ctx.push_simple(
                    format!("codex:{}:{ordinal}", path.display()),
                    AgentKind::Codex,
                    ts,
                    clean_model(model.clone()),
                    cwd.clone(),
                    session.clone(),
                    tokens,
                    1,
                    false,
                );
            }
            _ => {}
        }
    }
}
