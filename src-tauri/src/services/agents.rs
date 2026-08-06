//! Agent awareness: detects coding-agent CLIs running inside a session's
//! process tree and tracks whether they are actively working, sitting quiet
//! waiting for input, or have finished without the user looking yet. A
//! background loop refreshes the statuses every few seconds, emits
//! `agent-status-changed` events to each window, and raises a system
//! notification when an agent turns waiting or finishes.
//!
//! Status is a three-state model:
//! - `working` — agent process alive, produced PTY output recently;
//! - `waiting` — alive but silent for `WAITING_AFTER` (likely needs input);
//! - `done` — the agent process exited while the session is still alive and
//!   the user hasn't viewed that tab yet (drops the dot once they do).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

use crate::commands::SharedState;
use crate::models::pane::PaneContent;

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

/// Three-state model: `Working` (alive, output recently), `Waiting` (alive,
/// silent — likely needs input), or `Done` (agent exited, session alive, the
/// user hasn't viewed the tab yet; the dot drops once they do).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Waiting,
    Done,
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
    detect_agent_with_sys(&crate::services::procs::refresh_and_snapshot(), pids)
}

/// Same as `detect_agent` but reads a caller-provided `ProcSnapshot`. Use
/// this inside a batch built on one `refresh_and_snapshot()` call.
pub fn detect_agent_with_sys(sys: &crate::services::procs::ProcSnapshot, pids: &[u32]) -> Option<AgentKind> {
    for &pid in pids {
        if let Some((name, cmd)) = crate::services::procs::process_cmdline_with_sys(sys, pid) {
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
    // Sessions the user is currently viewing (any pane of a window's selected
    // tab). A `done` agent whose session is in here is "seen" — drop the dot
    // instead of keeping it.
    let mut seen: HashSet<Uuid> = HashSet::new();
    // Display info per detected-agent session (title, project name), so the
    // cross-window mini-bar popover can show "what" and "where" without
    // another IPC round-trip.
    let mut meta: HashMap<Uuid, (String, String)> = HashMap::new();

    // One full-system refresh per poll, shared across every session below.
    // Refreshing inside `session_pids` per-session made the poll cost ~N× of
    // this (and serialised under the sysinfo lock); the `with_sys` callers
    // read the same snapshot, so per-session pid detection is now cache-only.
    // `refresh_and_snapshot` copies out the pid/parent/cmdline data and
    // releases the global SYSTEM lock immediately, so the scan below (and any
    // concurrent `session_processes` command) never waits on it.
    let procs_cache = crate::services::procs::refresh_and_snapshot();

    for (label, state) in shared.all() {
        let g = state.lock();
        for sid in g.sessions.keys() {
            owner.insert(*sid, label.clone());
        }
        // Collect the sessions inside each project's selected tab — the
        // user is looking at that tab, so a finished agent there is "seen".
        for project in &g.projects {
            if let Some(tab) = project.selected_tab_view() {
                for col in &tab.columns {
                    for pane in &col.panes {
                        if let PaneContent::Session(id) = &pane.content {
                            seen.insert(*id);
                        }
                    }
                }
            }
        }
        for project in &g.projects {
            for sid in project.session_ids() {
                let Some(session) = g.sessions.get(&sid) else { continue };
                if !session.is_spawned() || session.is_exited() {
                    continue;
                }
                let shell_pid = session.shell_pid().unwrap_or(0);
                let pids = crate::services::procs::session_pids_with_sys(
                    &procs_cache,
                    sid,
                    shell_pid,
                );
                if let Some(agent) = detect_agent_with_sys(&procs_cache, &pids) {
                    // Agent alive this poll → working / waiting.
                    let agent_state = if session.idle_for() >= WAITING_AFTER {
                        AgentState::Waiting
                    } else {
                        AgentState::Working
                    };
                    let status = AgentStatus { agent, state: agent_state };
                    current.insert(sid, status);
                    meta.insert(sid, (session.title(), project.name(&g)));
                    if prev.get(&sid) != Some(&status) {
                        changed.insert(sid, Some(status));
                        per_window.entry(label.clone()).or_default().push((sid, Some(status)));
                    }
                } else if let Some(prev_status) = prev.get(&sid) {
                    // The agent was alive last poll but is gone now, while the
                    // session itself is still alive → it finished. Show a
                    // `done` dot unless the user is already looking at it
                    // (then drop the dot — it's been "seen").
                    if seen.contains(&sid) {
                        changed.insert(sid, None);
                        per_window.entry(label.clone()).or_default().push((sid, None));
                    } else {
                        let status = AgentStatus { agent: prev_status.agent, state: AgentState::Done };
                        current.insert(sid, status);
                        meta.insert(sid, (session.title(), project.name(&g)));
                        if prev.get(&sid) != Some(&status) {
                            changed.insert(sid, Some(status));
                            per_window.entry(label.clone()).or_default().push((sid, Some(status)));
                        }
                    }
                }
                // else: no agent now and never had one → nothing to report.
            }
        }
    }

    // Sessions that closed entirely (in prev, no longer in current and not
    // already handled above) lose their status — the frontend drops the dot.
    for sid in prev.keys() {
        if !current.contains_key(sid) && !changed.contains_key(sid) {
            changed.insert(*sid, None);
            if let Some(label) = owner.get(sid) {
                per_window.entry(label.clone()).or_default().push((*sid, None));
            }
        }
    }

    for (label, rows) in per_window {
        let payload = serde_json::json!({
            "sessions": rows.iter().map(|(id, status)| match status {
                Some(s) => serde_json::json!({ "id": id, "agent": s.agent, "state": s.state }),
                None => serde_json::json!({ "id": id, "agent": null, "state": null }),
            }).collect::<Vec<_>>(),
        });
        let _ = app.emit_to(&label, "agent-status-changed", payload);
        notify_changed(app, &label, shared, &prev, &rows);
    }

    // Push all changed rows (across windows) to any live `muster watch` IPC
    // subscribers. Cheap when nobody is watching — `broadcast` returns after a
    // single empty-subscribers check.
    if !changed.is_empty() {
        let sessions = changed
            .iter()
            .map(|(id, status)| match status {
                Some(s) => serde_json::json!({ "id": id, "agent": s.agent, "state": s.state }),
                None => serde_json::json!({ "id": id, "agent": null, "state": null }),
            })
            .collect::<Vec<_>>();
        crate::services::ipc::broadcast(serde_json::json!({
            "event": "agent-status-changed",
            "sessions": sessions,
        }));

        // Global broadcast of the FULL current snapshot (all windows, all
        // detected agents) so any window's agent mini-bar can render the
        // complete cross-window picture. The `per-window` emit above is
        // scoped to a single window; this `app.emit` reaches every window.
        // Sent only when something changed, so a quiet poll costs nothing.
        let all_sessions = current
            .iter()
            .map(|(id, s)| {
                let (title, project) = meta.get(id).cloned().unwrap_or_default();
                serde_json::json!({ "id": id, "agent": s.agent, "state": s.state, "title": title, "project": project })
            })
            .collect::<Vec<_>>();
        let _ = app.emit("all-agent-status", serde_json::json!({ "sessions": all_sessions }));
    }

    shared.agents.lock().statuses = current;
}

/// Send a system notification when a session turned waiting or finished,
/// throttled per session (waiting) and skipped while the owning window is
/// focused — the user is already looking. A `done` transition always
/// notifies once (it fires exactly once per agent run); the cooldown is
/// still recorded so a later `waiting` can't immediately re-fire.
fn notify_changed(
    app: &AppHandle,
    label: &str,
    shared: &SharedState,
    prev: &HashMap<Uuid, AgentStatus>,
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
        let prev_state = prev.get(sid).map(|p| p.state);
        let Some(session) = g.sessions.get(sid) else { continue };
        let body = match status.state {
            AgentState::Waiting if prev_state != Some(AgentState::Waiting) => {
                if !session.try_mark_agent_notify(NOTIFY_COOLDOWN) {
                    continue;
                }
                format!("{} is waiting for input \u{2014} {}", status.agent.label(), session.title())
            }
            AgentState::Done
                if matches!(prev_state, Some(AgentState::Working) | Some(AgentState::Waiting)) =>
            {
                // Record the send so a flapping agent can't re-notify, but
                // don't gate the `done` notification itself — it fires once.
                session.try_mark_agent_notify(NOTIFY_COOLDOWN);
                format!("{} finished \u{2014} {}", status.agent.label(), session.title())
            }
            _ => continue,
        };
        crate::services::notify::send(app, label, *sid, body);
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
