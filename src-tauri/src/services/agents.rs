//! Agent awareness: detects coding-agent CLIs running inside a session's
//! process tree and tracks whether they are actively working or sitting
//! quiet waiting for input. A background loop refreshes the statuses every
//! few seconds, emits `agent-status-changed` events to each window, and
//! raises a system notification when an agent has been waiting for a while.
//!
//! Status is a two-state heuristic: the agent process is alive and has
//! produced PTY output recently (`working`), or it has been silent for
//! `WAITING_AFTER` (`waiting` — usually "it wants input").

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::commands::SharedState;

/// A coding-agent CLI that Muster recognises inside a session's process tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Opencode,
    ClaudeCode,
    Codex,
    KimiCode,
    Aider,
    Gemini,
    Goose,
}

impl AgentKind {
    /// Human-readable display name. Agent identifiers are brand names, so
    /// they are intentionally not translated.
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Opencode => "opencode",
            AgentKind::ClaudeCode => "Claude Code",
            AgentKind::Codex => "Codex",
            AgentKind::KimiCode => "Kimi Code",
            AgentKind::Aider => "aider",
            AgentKind::Gemini => "Gemini CLI",
            AgentKind::Goose => "Goose",
        }
    }
}

/// Two-state heuristic: `Working` (agent process alive, output recently) or
/// `Waiting` (alive but silent for a while — likely needs input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AgentStatus {
    pub agent: AgentKind,
    pub state: AgentState,
}

/// Latest status per session, shared with the `muster agents` CLI command.
#[derive(Default)]
pub struct AgentCache {
    pub statuses: HashMap<Uuid, AgentStatus>,
}

/// A session stops being "working" after this much output silence.
const WAITING_AFTER: Duration = Duration::from_secs(10);
/// Notify at most once per session per minute about a waiting agent.
const NOTIFY_COOLDOWN: Duration = Duration::from_secs(60);

/// Marker → agent kind table, in detection priority order. Compound CLI
/// names (`claude-code`, `kimi-code`) get their own markers — the generic
/// marker must not match `codex-app`-style directory names.
const MARKERS: &[(AgentKind, &[&str])] = &[
    (AgentKind::Opencode, &["opencode"]),
    (AgentKind::ClaudeCode, &["claude", "claude-code"]),
    (AgentKind::Codex, &["codex"]),
    (AgentKind::KimiCode, &["kimi", "kimi-code"]),
    (AgentKind::Aider, &["aider"]),
    (AgentKind::Gemini, &["gemini"]),
    (AgentKind::Goose, &["goose"]),
];

/// Which coding agent the process tree `pids` belongs to, if any. The image
/// name is the strongest signal (`opencode.exe`, `claude.exe`, ...); the
/// command line covers node/python launchers that run under a generic
/// runtime (`node.exe` with the CLI path as an argument).
pub fn detect_agent(pids: &[u32]) -> Option<AgentKind> {
    for &pid in pids {
        if let Some((name, cmd)) = crate::services::procs::process_cmdline(pid) {
            if let Some(kind) = detect_in(&name, &cmd) {
                return Some(kind);
            }
        }
    }
    None
}

fn detect_in(name: &str, cmd: &str) -> Option<AgentKind> {
    match_name(name).or_else(|| match_cmdline(cmd))
}

/// Exact image-name match (case-insensitive, `.exe` suffix ignored).
fn match_name(name: &str) -> Option<AgentKind> {
    let stem = name
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .to_lowercase();
    for (kind, markers) in MARKERS {
        if markers.contains(&stem.as_str()) {
            return Some(*kind);
        }
    }
    None
}

/// Command-line scan: any whitespace token whose path segment or stem equals
/// a marker (or extends it as `marker.xxx`, e.g. `opencode.js`) matches.
/// Path-segment matching keeps `npx claude` and `...\bin\opencode` working
/// while ignoring `...\codex-app\server.js` style false positives.
fn match_cmdline(cmd: &str) -> Option<AgentKind> {
    for (kind, markers) in MARKERS {
        if markers
            .iter()
            .any(|m| cmd.split_whitespace().any(|tok| token_matches(tok, m)))
        {
            return Some(*kind);
        }
    }
    None
}

fn token_matches(token: &str, marker: &str) -> bool {
    let t = token
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches([';', ',', ')', '(', ':', '.'])
        .to_lowercase();
    for seg in t.split(['\\', '/']) {
        if seg == marker {
            return true;
        }
        if let Some(rest) = seg.strip_prefix(marker) {
            if rest.starts_with('.') {
                return true; // marker.js / marker.cjs / marker.py ...
            }
        }
    }
    t == marker || t.starts_with(&format!("{marker}."))
}

/// Spawn the background poll loop. Runs for the app's lifetime; every
/// `POLL_INTERVAL` recomputes every window's sessions, diffs against the
/// cache, emits changes per window, and sends throttled notifications for
/// agents that turned waiting.
pub fn spawn_poll_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(3));
        let shared = app.state::<SharedState>();
        poll_once(&app, &shared);
    });
}

fn poll_once(app: &AppHandle, shared: &SharedState) {
    let prev = shared.agents.lock().statuses.clone();
    let mut current: HashMap<Uuid, AgentStatus> = HashMap::new();
    // (session id, Some(status)) for changed/added rows, None for removed.
    let mut changed: HashMap<Uuid, Option<AgentStatus>> = HashMap::new();
    // Changed rows grouped by window label, for per-window emits.
    let mut per_window: HashMap<String, Vec<(Uuid, Option<AgentStatus>)>> = HashMap::new();
    // Every live session's owning window label (removals need to know where
    // to tell the frontend to drop the dot).
    let mut owner: HashMap<Uuid, String> = HashMap::new();

    for (label, state) in shared.all() {
        let g = state.lock();
        for sid in g.sessions.keys() {
            owner.insert(*sid, label.clone());
        }
        for project in &g.projects {
            for sid in project.session_ids() {
                let Some(session) = g.sessions.get(&sid) else { continue };
                if !session.is_spawned() || session.is_exited() {
                    continue;
                }
                let shell_pid = session.shell_pid().unwrap_or(0);
                let pids = crate::services::procs::session_pids(sid, shell_pid);
                let Some(agent) = detect_agent(&pids) else { continue };
                let agent_state = if session.idle_for() >= WAITING_AFTER {
                    AgentState::Waiting
                } else {
                    AgentState::Working
                };
                let status = AgentStatus { agent, state: agent_state };
                current.insert(sid, status);
                if prev.get(&sid) != Some(&status) {
                    changed.insert(sid, Some(status));
                    per_window.entry(label.clone()).or_default().push((sid, Some(status)));
                }
            }
        }
    }

    // Sessions whose agent disappeared (or whose session closed) lose their
    // status — the frontend must drop the dot.
    for sid in prev.keys() {
        if !current.contains_key(sid) && !changed.contains_key(sid) {
            changed.insert(*sid, None);
            if let Some(label) = owner.get(sid) {
                per_window.entry(label.clone()).or_default().push((*sid, None));
            }
        }
    }

    shared.agents.lock().statuses = current;

    for (label, rows) in per_window {
        let payload = serde_json::json!({
            "sessions": rows.iter().map(|(id, status)| match status {
                Some(s) => serde_json::json!({ "id": id, "agent": s.agent, "state": s.state }),
                None => serde_json::json!({ "id": id, "agent": null, "state": null }),
            }).collect::<Vec<_>>(),
        });
        let _ = app.emit_to(&label, "agent-status-changed", payload);
        notify_waiting(app, &label, shared, &rows);
    }
}

/// Send a system notification when a session turned waiting (from working or
/// from none), throttled per session and skipped while the owning window is
/// focused — the user is already looking.
fn notify_waiting(
    app: &AppHandle,
    label: &str,
    shared: &SharedState,
    rows: &[(Uuid, Option<AgentStatus>)],
) {
    let focused = app
        .get_webview_window(label)
        .map(|w| w.is_focused().unwrap_or(true))
        .unwrap_or(true);
    if focused {
        return;
    }
    let Some(s) = shared.get_label(label) else { return };
    let g = s.lock();
    for (sid, status) in rows {
        let Some(status) = status else { continue };
        if status.state != AgentState::Waiting {
            continue;
        }
        let Some(session) = g.sessions.get(sid) else { continue };
        if !session.try_mark_agent_notify(NOTIFY_COOLDOWN) {
            continue;
        }
        let _ = app
            .notification()
            .builder()
            .title("Muster")
            .body(format!(
                "{} is waiting for input \u{2014} {}",
                status.agent.label(),
                session.title()
            ))
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matching_is_case_insensitive_and_ignores_exe() {
        assert_eq!(match_name("opencode.exe"), Some(AgentKind::Opencode));
        assert_eq!(match_name("OPEncode"), Some(AgentKind::Opencode));
        assert_eq!(match_name("claude"), Some(AgentKind::ClaudeCode));
        assert_eq!(match_name("claude.exe"), Some(AgentKind::ClaudeCode));
        assert_eq!(match_name("codex"), Some(AgentKind::Codex));
        assert_eq!(match_name("kimi"), Some(AgentKind::KimiCode));
        assert_eq!(match_name("aider"), Some(AgentKind::Aider));
        assert_eq!(match_name("gemini"), Some(AgentKind::Gemini));
        assert_eq!(match_name("goose"), Some(AgentKind::Goose));
        assert_eq!(match_name("powershell.exe"), None);
        assert_eq!(match_name("node.exe"), None);
    }

    #[test]
    fn cmdline_matching_finds_launcher_paths() {
        // node-based launchers: the CLI path is a command-line argument.
        let cmd = r#""C:\Program Files\nodejs\node.exe" "C:\Users\x\.local\bin\opencode""#;
        assert_eq!(match_cmdline(cmd), Some(AgentKind::Opencode));
        let cmd = r#"node "C:\Users\x\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js""#;
        assert_eq!(match_cmdline(cmd), Some(AgentKind::ClaudeCode));
        let cmd = "npx claude";
        assert_eq!(match_cmdline(cmd), Some(AgentKind::ClaudeCode));
        let cmd = "bun C:\\tools\\kimi\\kimi.js";
        assert_eq!(match_cmdline(cmd), Some(AgentKind::KimiCode));
    }

    #[test]
    fn cmdline_matching_rejects_lookalike_paths() {
        // A dev server in a directory whose name merely contains a marker
        // must not be mistaken for an agent.
        let cmd = r#"node "D:\codex-app\server.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = r#"node "C:\work\gemini-docs\watch.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = "npm run dev";
        assert_eq!(match_cmdline(cmd), None);
        let cmd = "";
        assert_eq!(match_cmdline(cmd), None);
    }

    #[test]
    fn token_matching_rules() {
        assert!(token_matches("claude", "claude"));
        assert!(token_matches("claude.js", "claude"));
        assert!(token_matches("claude-code", "claude-code"));
        assert!(token_matches(r#"C:\Users\x\bin\opencode"#, "opencode"));
        assert!(!token_matches("codex-app", "codex"));
        assert!(!token_matches("codex-app/server.js", "codex"));
        assert!(token_matches("codex.js", "codex"));
    }

    #[test]
    fn detect_agent_empty_tree_is_none() {
        assert_eq!(detect_agent(&[]), None);
    }
}
