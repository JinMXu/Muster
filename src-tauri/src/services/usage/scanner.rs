use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use tauri::Emitter;

use super::model::{ToolKind, UsageSession, UsageSummary};
use super::provider::UsageProvider;

/// Cache of parsed sessions, plus mtime/size fingerprints for incremental skip.
#[derive(Default)]
pub struct UsageCache {
    sessions: Vec<UsageSession>,
    /// fingerprints keyed by DiscoveredSession.key + suffix
    fingerprints: HashMap<String, (SystemTime, u64)>,
    /// cached per-source parse results, keyed by DiscoveredSession.key + suffix.
    /// The key is the same fingerprint key so reuse is a direct lookup.
    parsed: HashMap<String, UsageSession>,
}

impl UsageCache {
    /// Rebuild `sessions` from `parsed` map. Called after a scan pass.
    fn rebuild(&mut self) {
        self.sessions = self.parsed.values().cloned().collect();
        // Sort by updated_at desc for a sensible default order.
        self.sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    }

    pub fn summary(&self) -> UsageSummary {
        UsageSummary::from_sessions(&self.sessions)
    }

    pub fn sessions_filtered(
        &self,
        tool: Option<ToolKind>,
        since: Option<i64>,
        limit: Option<usize>,
    ) -> Vec<UsageSession> {
        let mut out: Vec<UsageSession> = self
            .sessions
            .iter()
            .filter(|s| tool.map_or(true, |t| s.tool == t))
            .filter(|s| since.map_or(true, |since| s.updated_at >= since))
            .cloned()
            .collect();
        if let Some(limit) = limit {
            out.truncate(limit);
        }
        out
    }
}

/// All four providers. OpenCode needs special handling (parse_all).
fn build_providers() -> (
    super::opencode::OpenCodeProvider,
    super::claude::ClaudeProvider,
    super::codex::CodexProvider,
    super::kimi::KimiProvider,
) {
    (
        super::opencode::OpenCodeProvider::new(),
        super::claude::ClaudeProvider::new(),
        super::codex::CodexProvider::new(),
        super::kimi::KimiProvider::new(),
    )
}

/// Serialize full scan passes: the background loop and the `usage_refresh`
/// command can otherwise interleave and clobber each other's commit.
static SCAN_SERIAL: Mutex<()> = Mutex::new(());

/// Run one full scan pass, updating the cache in place.
pub fn scan_once(cache: &Mutex<UsageCache>) {
    let _serial = SCAN_SERIAL.lock();
    let (oc, cc, cx, kc) = build_providers();

    // Snapshot the reuse state under the lock, then RELEASE it: discovery and
    // JSONL parsing are slow filesystem work that must not block the usage
    // commands (summary/sessions readers) for the whole pass.
    let (snap_fingerprints, snap_parsed) = {
        let g = cache.lock();
        (g.fingerprints.clone(), g.parsed.clone())
    };

    let mut new_parsed: HashMap<String, UsageSession> = HashMap::new();
    let mut new_fingerprints: HashMap<String, (SystemTime, u64)> = HashMap::new();

    // --- OpenCode (special: one DB -> many sessions via parse_all) ---
    {
        let discovered = oc.discover();
        if let Some(src) = discovered.first() {
            let fp_key = src.key.clone() + "|oc";
            let unchanged = snap_fingerprints.get(&fp_key)
                .is_some_and(|(m, s)| src.mtime == Some(*m) && src.size == Some(*s));
            if unchanged {
                // Reuse cached OpenCode sessions (all keys ending in "|oc").
                for (k, s) in snap_parsed.iter() {
                    if k.ends_with("|oc") {
                        new_parsed.insert(k.clone(), s.clone());
                    }
                }
                new_fingerprints.insert(fp_key, (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
            } else {
                if let Some(sessions) = oc.parse_all() {
                    new_fingerprints.insert(fp_key, (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
                    for s in sessions {
                        new_parsed.insert(s.session_id.clone() + "|oc", s);
                    }
                }
            }
        }
    }

    // --- JSONL providers: Claude, Codex, Kimi (all unlocked, against the
    // snapshot taken above) ---
    scan_jsonl_provider(&cc, &mut new_parsed, &mut new_fingerprints, &snap_fingerprints, &snap_parsed, "cc");
    scan_jsonl_provider(&cx, &mut new_parsed, &mut new_fingerprints, &snap_fingerprints, &snap_parsed, "cx");
    scan_jsonl_provider(&kc, &mut new_parsed, &mut new_fingerprints, &snap_fingerprints, &snap_parsed, "kc");

    // Commit. The cache is only written by scan passes (serialized above), so
    // a full swap cannot clobber a concurrent writer.
    let mut g = cache.lock();
    g.parsed = new_parsed;
    g.fingerprints = new_fingerprints;
    g.rebuild();
}

fn scan_jsonl_provider<P: UsageProvider>(
    provider: &P,
    new_parsed: &mut HashMap<String, UsageSession>,
    new_fingerprints: &mut HashMap<String, (SystemTime, u64)>,
    fingerprints: &HashMap<String, (SystemTime, u64)>,
    parsed: &HashMap<String, UsageSession>,
    suffix: &str,
) {
    for src in provider.discover() {
        let fp_key = format!("{}|{}", src.key, suffix);
        let unchanged = fingerprints.get(&fp_key)
            .is_some_and(|(m, s)| src.mtime == Some(*m) && src.size == Some(*s));
        new_fingerprints.insert(fp_key.clone(), (src.mtime.unwrap_or(SystemTime::UNIX_EPOCH), src.size.unwrap_or(0)));
        if unchanged {
            // Reuse the cached parse for this exact source key.
            if let Some(cached) = parsed.get(&fp_key) {
                new_parsed.insert(fp_key, cached.clone());
                continue;
            }
        }
        if let Some(session) = provider.parse(&src) {
            new_parsed.insert(fp_key, session);
        }
    }
}

/// Spawn the background scan loop. Returns immediately; the thread runs for
/// the lifetime of the app. Emits `usage-updated` to all windows after each
/// successful scan.
pub fn spawn_scan_loop(app: tauri::AppHandle, cache: Arc<Mutex<UsageCache>>) {
    std::thread::spawn(move || {
        // Initial scan immediately on startup.
        scan_once(&cache);
        let _ = app.emit("usage-updated", ());
        loop {
            std::thread::sleep(Duration::from_secs(60));
            scan_once(&cache);
            let _ = app.emit("usage-updated", ());
        }
    });
}
