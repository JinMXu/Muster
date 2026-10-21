use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::{clean_model, home, num, ScanCtx};
use crate::services::agents::AgentKind;
use crate::services::usage::model::TokenUsage;

pub fn scan(ctx: &mut ScanCtx) {
    let Some(path) = db_path() else {
        return;
    };
    if !path.is_file() {
        return;
    }
    let Ok(conn) = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return;
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(2_000));
    let since = ctx.opencode_ts.saturating_sub(60_000);
    let Ok(mut stmt) = conn.prepare(
        "SELECT m.id, m.time_created, m.data, s.directory, s.id
         FROM message m JOIN session s ON s.id = m.session_id
         WHERE m.time_created > ?1
         ORDER BY m.time_created ASC",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([since], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    }) else {
        return;
    };
    let mut max_ts = ctx.opencode_ts;
    for row in rows.flatten() {
        let (id, row_ts, data, directory, session) = row;
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if v.get("role").and_then(|x| x.as_str()) != Some("assistant") {
            continue;
        }
        let Some(tokens) = v.get("tokens") else {
            continue;
        };
        let cache = tokens.get("cache");
        let usage = TokenUsage {
            input: num(tokens.get("input")),
            output: num(tokens.get("output")),
            reasoning: num(tokens.get("reasoning")),
            cache_read: cache.map(|c| num(c.get("read"))).unwrap_or(0),
            cache_write: cache.map(|c| num(c.get("write"))).unwrap_or(0),
        };
        if usage.is_zero() {
            continue;
        }
        let ts = v
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(|x| x.as_i64())
            .unwrap_or(row_ts);
        max_ts = max_ts.max(ts);
        let model = v
            .get("modelID")
            .or_else(|| v.get("model"))
            .and_then(|x| x.as_str())
            .map(str::to_owned);
        ctx.push_simple(
            format!("opencode:{id}"),
            AgentKind::Opencode,
            ts,
            clean_model(model),
            directory,
            Some(session),
            usage,
            1,
            false,
        );
    }
    if max_ts > ctx.opencode_ts {
        ctx.opencode_ts = max_ts;
        ctx.dirty = true;
    }
}

fn db_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENCODE_DB") {
        return Some(PathBuf::from(p));
    }
    if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(base).join("opencode").join("opencode.db"));
    }
    let p = home()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    p.is_file().then_some(p)
}
