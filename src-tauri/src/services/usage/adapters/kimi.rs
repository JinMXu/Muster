use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{clean_model, home, num, open_tail, walk_files, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;
use crate::services::usage::time::parse_iso_ms;

pub fn scan(ctx: &mut ScanCtx) {
    let root = std::env::var_os("KIMI_CODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".kimi-code"));
    if !root.is_dir() {
        return;
    }
    let cwds = session_cwds(&root);
    let mut files = Vec::new();
    walk_files(&root.join("sessions"), 6, &mut files);
    for path in files {
        if path.file_name().and_then(|n| n.to_str()) != Some("wire.jsonl") {
            continue;
        }
        // wire.jsonl is append-only and self-contained per line (session/cwd
        // come from the index file), so tails are parsed incrementally.
        let Some(resume) = ctx.resume_at(&path) else {
            continue;
        };
        let session = session_id_from_path(&path);
        let cwd = session.as_ref().and_then(|s| cwds.get(s).cloned());
        let Some(mut tail) = open_tail(&path, resume) else {
            continue;
        };
        for (idx, line) in tail.by_ref() {
            let Ok(line) = line else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("type").and_then(|x| x.as_str()) != Some("usage.record") {
                continue;
            }
            let Some(usage) = v.get("usage") else {
                continue;
            };
            let tokens = TokenUsage {
                input: num(usage.get("inputOther")),
                output: num(usage.get("output")),
                reasoning: num(usage.get("reasoning")),
                cache_read: num(usage.get("inputCacheRead")),
                cache_write: num(usage.get("inputCacheCreation")),
            };
            if tokens.is_zero() {
                continue;
            }
            let Some(ts) = v.get("time").and_then(|x| match x {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => parse_iso_ms(s),
                _ => None,
            }) else {
                continue;
            };
            let model = v.get("model").and_then(|x| x.as_str()).map(str::to_owned);
            ctx.push_simple(
                format!("kimi:{}:{idx}", path.display()),
                AgentKind::KimiCode,
                ts,
                clean_model(model),
                cwd.clone(),
                session.clone(),
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
    let Ok(file) = File::open(root.join("session_index.jsonl")) else {
        return out;
    };
    for line in BufReader::new(file).lines().flatten() {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (Some(id), Some(dir)) = (
            v.get("sessionId").and_then(|x| x.as_str()),
            v.get("workDir").and_then(|x| x.as_str()),
        ) else {
            continue;
        };
        out.insert(id.to_string(), dir.to_string());
    }
    out
}

fn session_id_from_path(path: &Path) -> Option<String> {
    path.components().find_map(|c| {
        let s = c.as_os_str().to_str()?;
        s.starts_with("session_").then(|| s.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_session_id_from_wire_path() {
        let p = Path::new(
            r"C:\Users\x\.kimi-code\sessions\wd_pi-gui_6d2e8a2cf282\session_4bbdd759-c43b-433b-a165-4fe8a66c94ea\agents\agent-0\wire.jsonl",
        );
        assert_eq!(
            session_id_from_path(p).as_deref(),
            Some("session_4bbdd759-c43b-433b-a165-4fe8a66c94ea")
        );
    }
}
