use std::fs::File;
use std::io::{BufRead, BufReader};

use serde_json::Value;

use super::{clean_model, home, num, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = home().join(".pi").join("agent").join("sessions");
    if !root.is_dir() {
        return;
    }
    let mut files = Vec::new();
    walk_files(&root, 3, &mut files);
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
        let mut cwd: Option<String> = None;
        let mut model: Option<String> = None;
        let mut session: Option<String> = None;
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let Ok(line) = line else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match v.get("type").and_then(|x| x.as_str()) {
                Some("session") => {
                    cwd = v.get("cwd").and_then(|x| x.as_str()).map(str::to_owned);
                    session = v.get("id").and_then(|x| x.as_str()).map(str::to_owned);
                }
                Some("model_change") => {
                    model = v
                        .get("modelId")
                        .or_else(|| v.get("model"))
                        .and_then(|x| x.as_str())
                        .map(str::to_owned);
                }
                Some("message") => {
                    let Some(msg) = v.get("message") else {
                        continue;
                    };
                    if msg.get("role").and_then(|x| x.as_str()) != Some("assistant") {
                        continue;
                    }
                    let Some(usage) = msg.get("usage") else {
                        continue;
                    };
                    let tokens = TokenUsage {
                        input: num(usage.get("input")),
                        output: num(usage.get("output")),
                        reasoning: num(usage.get("reasoning")),
                        cache_read: num(usage.get("cacheRead")),
                        cache_write: num(usage.get("cacheWrite")),
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
                    let id = msg
                        .get("id")
                        .and_then(|x| x.as_str())
                        .map(str::to_owned)
                        .or_else(|| v.get("id").and_then(|x| x.as_str()).map(str::to_owned))
                        .unwrap_or_else(|| {
                            super::event_id("pi", &format!("{}:{idx}", path.display()))
                        });
                    ctx.push_simple(
                        format!("pi:{}:{id}", path.display()),
                        AgentKind::Pi,
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
}
