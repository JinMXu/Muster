use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde_json::Value;

use super::{clean_model, home, num_at, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"));
    let mut files = Vec::new();
    walk_files(&root.join("projects"), 3, &mut files);
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if !ctx.should_read(&path) {
            continue;
        }
        let Ok(file) = File::open(&path) else {
            continue;
        };
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let Ok(line) = line else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
                continue;
            }
            let Some(usage) = v.get("message").and_then(|m| m.get("usage")) else {
                continue;
            };
            let tokens = TokenUsage {
                input: num_at(usage, "input_tokens"),
                output: num_at(usage, "output_tokens"),
                reasoning: 0,
                cache_read: num_at(usage, "cache_read_input_tokens"),
                cache_write: num_at(usage, "cache_creation_input_tokens"),
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
            let model = v
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|x| x.as_str())
                .map(str::to_owned);
            let cwd = v.get("cwd").and_then(|x| x.as_str()).map(str::to_owned);
            let session = v
                .get("sessionId")
                .and_then(|x| x.as_str())
                .map(str::to_owned);
            let id = v
                .get("uuid")
                .and_then(|x| x.as_str())
                .map(str::to_owned)
                .or_else(|| {
                    v.get("message")
                        .and_then(|m| m.get("id"))
                        .and_then(|x| x.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| super::event_id("claude", &format!("{}:{idx}", path.display())));
            // The same message can appear in a parent session and a subagent
            // file; the message uuid is the dedup key across both.
            ctx.push_simple(
                format!("claude:{id}"),
                AgentKind::ClaudeCode,
                ts,
                clean_model(model),
                cwd,
                session,
                tokens,
                1,
                false,
            );
        }
    }
}
