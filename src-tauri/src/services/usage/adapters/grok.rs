use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::{home, num, open_tail, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = home().join(".grok");
    if !root.is_dir() {
        return;
    }
    let cwds = session_cwds(&root);
    let mut files = Vec::new();
    walk_files(&root.join("logs"), 2, &mut files);
    for path in files {
        if path.file_name().and_then(|n| n.to_str()) != Some("unified.jsonl") {
            continue;
        }
        // unified.jsonl is append-only with self-contained lines; cwd comes
        // from the sqlite index, so tails are parsed incrementally.
        let Some(resume) = ctx.resume_at(&path) else {
            continue;
        };
        let Some(mut tail) = open_tail(&path, resume) else {
            continue;
        };
        for (idx, line) in tail.by_ref() {
            let Ok(line) = line else { continue };
            if !line.contains("shell.turn.inference_done") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("msg").and_then(|x| x.as_str()) != Some("shell.turn.inference_done") {
                continue;
            }
            let Some(c) = v.get("ctx") else { continue };
            let prompt = num(c.get("prompt_tokens"));
            let cached = num(c.get("cached_prompt_tokens"));
            let tokens = TokenUsage {
                input: prompt.saturating_sub(cached),
                output: num(c.get("completion_tokens")),
                reasoning: num(c.get("reasoning_tokens")),
                cache_read: cached,
                cache_write: 0,
            };
            if tokens.is_zero() {
                continue;
            }
            let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(parse_iso_ms) else {
                continue;
            };
            let sid = v.get("sid").and_then(|x| x.as_str());
            let loop_index = c.get("loop_index").and_then(|x| x.as_u64()).unwrap_or(0);
            let id = sid
                .map(|s| format!("grok:{s}:{ts}:{loop_index}"))
                .unwrap_or_else(|| super::event_id("grok", &format!("{}:{idx}", path.display())));
            ctx.push_simple(
                id,
                AgentKind::Grok,
                ts,
                None,
                sid.and_then(|s| cwds.get(s).cloned()),
                sid.map(str::to_owned),
                tokens,
                1,
                false,
            );
        }
        ctx.advance(&path, tail.consumed(), tail.next_line());
    }
}

fn session_cwds(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let path = root.join("sessions").join("session_search.sqlite");
    if !path.is_file() {
        return out;
    }
    let Ok(conn) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return out;
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(1_000));
    let Ok(mut stmt) = conn.prepare("SELECT session_id, cwd FROM session_docs") else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return out;
    };
    for row in rows.flatten() {
        out.insert(row.0, row.1);
    }
    out
}
