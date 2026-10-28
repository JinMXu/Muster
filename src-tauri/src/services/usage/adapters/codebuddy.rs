use serde_json::Value;

use super::{file_mtime_ms, home, num, open_tail, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;

pub fn scan(ctx: &mut ScanCtx) {
    let root = home().join(".codebuddy").join("projects");
    if !root.is_dir() {
        return;
    }
    let mut files = Vec::new();
    walk_files(&root, 3, &mut files);
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        // Append-only JSONL; lines are self-contained (the timestamp comes
        // from the file mtime), so tails are parsed incrementally.
        let Some(resume) = ctx.resume_at(&path) else {
            continue;
        };
        // CodeBuddy's JSONL has no per-message timestamp; the file mtime is
        // the only temporal signal, so attribution is marked coarse.
        let ts = file_mtime_ms(&path);
        if ts == 0 {
            continue;
        }
        let session = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
        let Some(mut tail) = open_tail(&path, resume) else {
            continue;
        };
        for (idx, line) in tail.by_ref() {
            let Ok(line) = line else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(usage) = v.get("providerData").and_then(|p| p.get("usage")) else {
                continue;
            };
            let cached = usage
                .get("inputTokensDetails")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .map(|x| num(x.get("cached_tokens")))
                .unwrap_or(0);
            let reasoning = usage
                .get("outputTokensDetails")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .map(|x| num(x.get("reasoning_tokens")))
                .unwrap_or(0);
            let input = num(usage.get("inputTokens"));
            let tokens = TokenUsage {
                input: input.saturating_sub(cached),
                output: num(usage.get("outputTokens")),
                reasoning,
                cache_read: cached,
                cache_write: 0,
            };
            if tokens.is_zero() {
                continue;
            }
            let id = v
                .get("id")
                .and_then(|x| x.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    super::event_id("codebuddy", &format!("{}:{idx}", path.display()))
                });
            ctx.push_simple(
                format!("codebuddy:{id}"),
                AgentKind::CodeBuddy,
                ts,
                None,
                None,
                session.clone(),
                tokens,
                1,
                true,
            );
        }
        ctx.advance(&path, tail.consumed(), tail.next_line());
    }
}
