use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

use super::model::{ToolKind, TokenUsage, UsageSession};
use super::provider::{DiscoveredSession, UsageProvider, env_path_or, home_dir};

pub struct OpenCodeProvider {
    db_path: Option<PathBuf>,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self { db_path: resolve_db_path() }
    }
}

/// Resolve the opencode.db path, honoring XDG_DATA_HOME and OPENCODE_DB.
fn resolve_db_path() -> Option<PathBuf> {
    // OPENCODE_DB can be an absolute path to the db file directly.
    if let Some(p) = env_path_or("OPENCODE_DB", || None) {
        if p.is_file() {
            return Some(p);
        }
    }
    // XDG_DATA_HOME overrides the data root; opencode appends "opencode/opencode.db".
    let data_dir = env_path_or("XDG_DATA_HOME", || {
        home_dir().map(|h| h.join(".local").join("share"))
    })?;
    let db = data_dir.join("opencode").join("opencode.db");
    if db.is_file() { Some(db) } else { None }
}

impl UsageProvider for OpenCodeProvider {
    fn tool_kind(&self) -> ToolKind { ToolKind::Opencode }

    fn discover(&self) -> Vec<DiscoveredSession> {
        let Some(path) = &self.db_path else { return vec![] };
        let meta = std::fs::metadata(path).ok();
        let mtime = meta.as_ref().and_then(|m| m.modified().ok());
        let size = meta.as_ref().and_then(|m| m.len().try_into().ok());
        // One "source" representing the whole DB; key is the path string.
        vec![DiscoveredSession {
            key: path.to_string_lossy().into_owned(),
            path: path.clone(),
            mtime,
            size,
        }]
    }

    fn parse(&self, _source: &DiscoveredSession) -> Option<UsageSession> {
        // OpenCode is handled specially via parse_all (one DB -> many sessions).
        // parse() is never called for OpenCode; returns None to satisfy the trait.
        None
    }
}

impl OpenCodeProvider {
    /// OpenCode is special: one DB yields many sessions. Parse them all at once.
    /// Returns None if the DB can't be opened or queried.
    pub fn parse_all(&self) -> Option<Vec<UsageSession>> {
        let path = match &self.db_path {
            Some(p) => p,
            None => return None,
        };
        // Open read-only via URI to avoid clashing with the live opencode WAL lock.
        let uri = format!("file:{}?mode=ro&immutable=1", path.to_string_lossy());
        let conn = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ).ok()?;
        // Belt and suspenders: never write.
        let _ = conn.pragma_update(None, "query_only", true);

        let mut stmt = conn
            .prepare(
                "SELECT id, title, directory, cost,
                        tokens_input, tokens_output, tokens_reasoning,
                        tokens_cache_read, tokens_cache_write,
                        json_extract(model,'$.id'),
                        json_extract(model,'$.providerID'),
                        time_created, time_updated
                 FROM session
                 WHERE time_archived IS NULL
                 ORDER BY time_updated DESC",
            )
            .ok()?;

        let rows = stmt
            .query_map([], |row| {
                let model_id: Option<String> = row.get(9)?;
                let provider_id: Option<String> = row.get(10)?;
                let model = match (model_id, provider_id) {
                    (Some(m), Some(p)) if !m.is_empty() => format!("{}/{}", p, m),
                    (Some(m), _) if !m.is_empty() => m,
                    _ => "unknown".to_string(),
                };
                Ok(UsageSession {
                    tool: ToolKind::Opencode,
                    session_id: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    cwd: row.get::<_, String>(2)?,
                    cost_usd: {
                        let c: f64 = row.get(3)?;
                        if c > 0.0 { Some(c) } else { None }
                    },
                    tokens: TokenUsage {
                        input: row.get::<_, i64>(4)? as u64,
                        output: row.get::<_, i64>(5)? as u64,
                        reasoning: row.get::<_, i64>(6)? as u64,
                        cache_read: row.get::<_, i64>(7)? as u64,
                        cache_write: row.get::<_, i64>(8)? as u64,
                    },
                    model,
                    started_at: row.get::<_, i64>(11)?,
                    updated_at: row.get::<_, i64>(12)?,
                })
            })
            .ok()?;

        let mut out = Vec::new();
        for r in rows {
            if let Ok(s) = r { out.push(s); }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_fixture() -> (tempfile::NamedTempFile, Connection) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (
                id text PRIMARY KEY, project_id text NOT NULL DEFAULT 'g',
                parent_id text, workspace_id text, slug text NOT NULL DEFAULT '',
                directory text NOT NULL DEFAULT '', path text,
                title text NOT NULL DEFAULT '', version text NOT NULL DEFAULT '',
                share_url text, agent text, model text,
                cost real NOT NULL DEFAULT 0,
                tokens_input integer NOT NULL DEFAULT 0,
                tokens_output integer NOT NULL DEFAULT 0,
                tokens_reasoning integer NOT NULL DEFAULT 0,
                tokens_cache_read integer NOT NULL DEFAULT 0,
                tokens_cache_write integer NOT NULL DEFAULT 0,
                summary_additions integer, summary_deletions integer,
                summary_files integer, summary_diffs text, revert text,
                permission text, metadata text,
                time_created integer NOT NULL DEFAULT 0,
                time_updated integer NOT NULL DEFAULT 0,
                time_compacting integer, time_archived integer
            );",
        ).unwrap();
        conn.execute(
            "INSERT INTO session (id, title, directory, cost, model,
                tokens_input, tokens_output, tokens_reasoning,
                tokens_cache_read, tokens_cache_write, time_created, time_updated)
             VALUES ('ses_1', 'Fix bug', '/repo', 0.42,
                '{\"id\":\"deepseek-v4\",\"providerID\":\"opencode\"}',
                1000, 200, 50, 500, 0, 100000, 200000)",
            [],
        ).unwrap();
        (f, conn)
    }

    #[test]
    fn parses_sessions_from_db() {
        let (f, _conn) = make_fixture();
        // _conn stays alive (holds the write connection open, data is committed via autocommit)
        let provider = OpenCodeProvider { db_path: Some(f.path().to_path_buf()) };
        let sessions = provider.parse_all().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, "ses_1");
        assert_eq!(s.title, "Fix bug");
        assert_eq!(s.cwd, "/repo");
        assert_eq!(s.cost_usd, Some(0.42));
        assert_eq!(s.tokens.input, 1000);
        assert_eq!(s.tokens.output, 200);
        assert_eq!(s.tokens.reasoning, 50);
        assert_eq!(s.tokens.cache_read, 500);
        assert_eq!(s.model, "opencode/deepseek-v4");
        assert_eq!(s.started_at, 100000);
        assert_eq!(s.updated_at, 200000);
    }
}
