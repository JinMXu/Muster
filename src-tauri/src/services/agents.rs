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

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

use crate::commands::SharedState;
use crate::models::pane::PaneContent;

/// A coding-agent CLI that Muster recognises inside a session's process tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Opencode,
    ClaudeCode,
    Codex,
    KimiCode,
    Aider,
    Gemini,
    Goose,
    Atomcode,
    Copilot,
    Cursor,
    Kiro,
    Amp,
    Droid,
    Crush,
    Cline,
    ContinueCli,
    Kilo,
    Grok,
    OpenHands,
    Pi,
    Vibe,
    Qwen,
    Qoder,
    Trae,
    CodeBuddy,
    Comate,
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
            AgentKind::Atomcode => "AtomCode",
            AgentKind::Copilot => "GitHub Copilot",
            AgentKind::Cursor => "Cursor",
            AgentKind::Kiro => "Kiro",
            AgentKind::Amp => "Amp",
            AgentKind::Droid => "Factory Droid",
            AgentKind::Crush => "Crush",
            AgentKind::Cline => "Cline",
            AgentKind::ContinueCli => "Continue",
            AgentKind::Kilo => "Kilo Code",
            AgentKind::Grok => "Grok Build",
            AgentKind::OpenHands => "OpenHands",
            AgentKind::Pi => "Pi",
            AgentKind::Vibe => "Mistral Vibe",
            AgentKind::Qwen => "Qwen Code",
            AgentKind::Qoder => "Qoder",
            AgentKind::Trae => "Trae CLI",
            AgentKind::CodeBuddy => "CodeBuddy",
            AgentKind::Comate => "Comate",
        }
    }
}

/// Three-state model: `Working` (alive, output recently), `Waiting` (alive,
/// silent — likely needs input), or `Done` (agent exited, session alive, the
/// user hasn't viewed the tab yet; the dot drops once they do).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Waiting,
    Done,
}

/// Extra signal layered on top of `Waiting`: the agent is blocked on a
/// permission/approval prompt rather than an ordinary input prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentAttention {
    Approval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AgentStatus {
    pub agent: AgentKind,
    pub state: AgentState,
    pub attention: Option<AgentAttention>,
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
/// No system notifications during the launch window. Restored sessions spawn
/// fresh shells and their first status transitions carry no signal — this
/// keeps a restart from ever opening with a burst of agent pings (mirrors
/// herdr's "don't mis-report on session restore" fixes).
const STARTUP_NOTIFY_GRACE: Duration = Duration::from_secs(20);

/// Marker → agent kind table, in detection priority order. Compound CLI
/// names (`claude-code`, `kimi-code`) get their own markers — the generic
/// marker must not match `codex-app`-style directory names.
///
/// Scope markers (`@continuedev`, `@qwen-code`, ...) cover agents whose npm
/// package name differs from the command they expose.
const MARKERS: &[(AgentKind, &[&str])] = &[
    (AgentKind::Opencode, &["opencode"]),
    (
        AgentKind::ClaudeCode,
        &["claude", "claude-code", "@anthropic-ai"],
    ),
    (AgentKind::Codex, &["codex"]),
    (AgentKind::KimiCode, &["kimi", "kimi-code", "kimi-cli"]),
    (AgentKind::Aider, &["aider"]),
    (AgentKind::Gemini, &["gemini", "gemini-cli"]),
    (AgentKind::Goose, &["goose"]),
    (AgentKind::Atomcode, &["atomcode", "atom-code"]),
    (AgentKind::Copilot, &["copilot"]),
    (AgentKind::Cursor, &["cursor-agent"]),
    (AgentKind::Kiro, &["kiro-cli"]),
    (AgentKind::Amp, &["amp"]),
    (AgentKind::Droid, &["droid"]),
    (AgentKind::Crush, &["crush"]),
    (AgentKind::Cline, &["cline"]),
    (AgentKind::ContinueCli, &["cn", "@continuedev"]),
    (AgentKind::Kilo, &["kilo", "kilocode", "@kilocode"]),
    (AgentKind::Grok, &["grok", "grok-cli"]),
    (AgentKind::OpenHands, &["openhands", "openhands-cli"]),
    (AgentKind::Pi, &["pi"]),
    (AgentKind::Vibe, &["vibe", "mistral-vibe"]),
    (AgentKind::Qwen, &["qwen", "qwen-code", "@qwen-code"]),
    (AgentKind::Qoder, &["qoder"]),
    (AgentKind::Trae, &["trae-cli", "traecli", "trae-agent"]),
    (
        AgentKind::CodeBuddy,
        &["codebuddy", "codebuddy-code", "cbc", "@tencent-ai"],
    ),
    (AgentKind::Comate, &["comate"]),
];

/// A detected agent plus the minimal command that launched it, suitable for
/// persisting and re-running after a restart. Only the executable and (for
/// interpreters/package runners) one script or package argument are kept, so
/// flags that may carry secrets never reach the snapshot file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedAgent {
    pub kind: AgentKind,
    pub command: Option<String>,
}

/// Which coding agent the process tree `pids` belongs to, if any. The image
/// name is the strongest signal (`opencode.exe`, `claude.exe`, ...); the
/// command line covers node/python launchers that run under a generic
/// runtime (`node.exe` with the CLI path as an argument).
pub fn detect_agent(pids: &[u32]) -> Option<DetectedAgent> {
    detect_agent_with_sys(&crate::services::procs::refresh_and_snapshot(), pids)
}

/// Same as `detect_agent` but reads a caller-provided `ProcSnapshot`. Use
/// this inside a batch built on one `refresh_and_snapshot()` call.
pub fn detect_agent_with_sys(sys: &crate::services::procs::ProcSnapshot, pids: &[u32]) -> Option<DetectedAgent> {
    for &pid in pids {
        if let Some((name, cmd)) = crate::services::procs::process_cmdline_with_sys(sys, pid) {
            if let Some(kind) = detect_in(&name, &cmd) {
                return Some(DetectedAgent {
                    kind,
                    command: launch_command(&cmd, &name),
                });
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

/// Command-line scan: the executable/script name is the strongest signal, and
/// global install paths and `@scope/pkg` specifiers may name the agent in a
/// parent segment. A bare argument is only considered when it follows a
/// package-runner (`npx`, `bunx`, `uvx`, ...), so `npm run pi` or a script
/// argument that happens to read `pi` can't fake a detection.
fn match_cmdline(cmd: &str) -> Option<AgentKind> {
    let tokens = cmd_tokens(cmd);
    if tokens.is_empty() {
        return None;
    }
    let runner_args = is_package_runner(&tokens);
    for (kind, markers) in MARKERS {
        for (i, tok) in tokens.iter().enumerate() {
            let eligible = i == 0 || runner_args || is_path_like(tok);
            if eligible && markers.iter().any(|m| token_matches(tok, m)) {
                return Some(*kind);
            }
        }
    }
    None
}

/// Split a command line into arguments, keeping quoted paths (which contain
/// spaces, e.g. `C:\Program Files\nodejs\node.exe`) as one token.
fn cmd_tokens(cmd: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    for (i, ch) in cmd.char_indices() {
        match ch {
            '"' => {
                if quoted {
                    if let Some(s) = start.take() {
                        out.push(&cmd[s..i]);
                    }
                } else {
                    start = Some(i + 1);
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if let Some(s) = start.take() {
                    out.push(&cmd[s..i]);
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(s) = start {
        out.push(&cmd[s..]);
    }
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

fn is_path_like(token: &str) -> bool {
    token.contains(['\\', '/'])
}

/// First token names a tool that launches a package by name (`npx claude`),
/// so its bare arguments are command names rather than data.
fn is_package_runner(tokens: &[&str]) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };
    match executable_stem(first).as_str() {
        "npx" | "pnpx" | "bunx" | "uvx" | "pipx" | "dlx" => true,
        "npm" | "pnpm" | "yarn" => matches!(tokens.get(1).copied(), Some("exec" | "x" | "dlx")),
        _ => false,
    }
}

fn executable_stem(tok: &str) -> String {
    let t = tok.trim_matches('"').to_lowercase();
    let last = t.rsplit(['\\', '/']).next().unwrap_or("");
    last.trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .to_string()
}

fn token_matches(token: &str, marker: &str) -> bool {
    let t = token
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches([';', ',', ')', '(', ':', '.'])
        .to_lowercase();
    if t.is_empty() {
        return false;
    }
    if !is_path_like(&t) {
        return stem_matches(&t, marker);
    }
    let last = t.rsplit(['\\', '/']).next().unwrap_or("");
    if stem_matches(last, marker) {
        return true;
    }
    // Parent segments are only trustworthy inside an install location or an
    // npm scope. `C:\projects\pi\server.js` must not look like the Pi CLI.
    let scan_parents = t.starts_with('@') || is_install_path(&t);
    scan_parents && t.split(['\\', '/']).any(|seg| stem_matches(seg, marker))
}

/// Exact segment match, or a script/executable extension (`opencode.js`,
/// `codex.exe`). `codex-app` intentionally does not match `codex`.
fn stem_matches(seg: &str, marker: &str) -> bool {
    seg == marker
        || seg
            .strip_prefix(marker)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn is_install_path(t: &str) -> bool {
    const HINTS: &[&str] = &[
        "node_modules",
        "\\.local\\",
        "/.local/",
        "\\npm\\",
        "/npm/",
        "\\pnpm\\",
        "/pnpm/",
        "\\yarn\\",
        "/yarn/",
        "\\scoop\\",
        "/scoop/",
        "\\.bun\\",
        "/.bun/",
        "\\pipx\\",
        "/pipx/",
        "site-packages",
        "\\.cargo\\bin",
        "/.cargo/bin",
        "\\go\\bin",
        "/go/bin",
        "\\programs\\",
        "/programs/",
    ];
    HINTS.iter().any(|h| t.contains(h))
}

/// Minimal re-runnable invocation for `cmd` (argv[0] plus one script/package
/// argument for interpreters and package runners). Never persists flags.
fn launch_command(cmd: &str, name: &str) -> Option<String> {
    let tokens = cmd_tokens(cmd);
    let Some(&first) = tokens.first() else {
        return (!name.is_empty()).then(|| name.to_string());
    };
    let exe = executable_stem(first);
    let second = match exe.as_str() {
        "python" | "python3" | "py" => tokens
            .iter()
            .position(|t| *t == "-m")
            .and_then(|i| tokens.get(i + 1).copied())
            .or_else(|| tokens.iter().copied().skip(1).find(|t| is_script_token(t))),
        "node" | "bun" | "deno" => tokens.iter().copied().skip(1).find(|t| is_script_token(t)),
        "npx" | "pnpx" | "bunx" | "uvx" | "pipx" | "dlx" => {
            tokens.iter().copied().skip(1).find(|t| !t.starts_with('-'))
        }
        "npm" | "pnpm" | "yarn" => tokens.iter().copied().skip(1).find(|t| {
            !t.starts_with('-')
                && !matches!(
                    t.to_lowercase().as_str(),
                    "exec" | "x" | "dlx" | "run" | "run-script" | "tool"
                )
        }),
        _ => None,
    };
    let mut out = quote_arg(first);
    if let Some(extra) = second {
        out.push(' ');
        out.push_str(&quote_arg(extra));
    }
    let out: String = out.chars().take(512).collect();
    (!out.trim().is_empty()).then_some(out)
}

fn is_script_token(t: &str) -> bool {
    !t.starts_with('-')
        && (is_path_like(t)
            || ["js", "mjs", "cjs", "ts", "py"]
                .iter()
                .any(|e| t.to_lowercase().ends_with(&format!(".{e}"))))
}

fn quote_arg(arg: &str) -> String {
    let clean: String = arg.chars().filter(|c| *c != '"' && *c != '\r' && *c != '\n').collect();
    if clean.is_empty() {
        String::new()
    } else if clean.contains([' ', '\t']) {
        format!("\"{clean}\"")
    } else {
        clean
    }
}

/// Approval/permission prompts that block the agent until the user answers.
/// Only the last few rendered lines are inspected, so an old prompt that has
/// scrolled away — or the same words merely echoed in output — cannot keep
/// the attention flag stuck.
fn approval_needed(lines: &[String]) -> bool {
    const MARKERS: &[&str] = &[
        "do you want to proceed",
        "do you want to allow",
        "allow this command",
        "allow command",
        "would you like to run",
        "run the following command",
        "permission to",
        "approve this",
        "press enter to confirm",
        "[y/n",
        "(y/n",
        "(y)es/(n)o",
        "yes (y)",
        "是否允许",
        "是否继续",
        "确认执行",
        "按回车确认",
        "需要确认",
    ];
    let tail: Vec<String> = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .map(|l| l.to_lowercase())
        .collect();
    if tail.iter().any(|l| MARKERS.iter().any(|m| l.contains(m))) {
        return true;
    }
    // Numbered permission menus: "1. Yes / 2. No".
    let has_yes = tail.iter().any(|l| l.contains("yes") || l.contains("allow"));
    let has_no = tail.iter().any(|l| l.contains("no") || l.contains("deny"));
    let numbered = tail.iter().any(|l| l.contains("1.") || l.contains("1)"))
        && tail.iter().any(|l| l.contains("2.") || l.contains("2)"));
    has_yes && has_no && numbered
}

/// Spawn the background poll loop. Runs for the app's lifetime; every
/// `POLL_INTERVAL` recomputes every window's sessions, diffs against the
/// cache, emits changes per window, and sends throttled notifications for
/// agents that turned waiting.
pub fn spawn_poll_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let shared = app.state::<SharedState>();
            poll_once(&app, &shared, started);
        }
    });
}

fn poll_once(app: &AppHandle, shared: &SharedState, started: std::time::Instant) {
    // Fast path: nothing live to track. Skip the full-system process
    // enumeration (the most expensive step of the poll) entirely when no
    // window has a live spawned session - e.g. the app is sitting in the
    // tray with only file tabs open, or there are no windows at all. We
    // still must run the cleanup tail when there are leftover statuses
    // (sessions that just exited), otherwise their agent dots would never
    // be cleared; in that case `current` stays empty and the tail emits a
    // removal for every previously-known session.
    let has_live_session = shared.all().iter().any(|(_, s)| {
        let g = s.lock();
        g.sessions
            .values()
            .any(|sess| sess.is_spawned() && !sess.is_exited())
    });

    let prev = shared.agents.lock().statuses.clone();
    if !has_live_session {
        // Nothing to detect, and nothing left to clear -> fully idle.
        if prev.is_empty() {
            return;
        }
        // Leftover statuses from sessions that have since exited: drop them
        // so the frontend dots disappear. `current` is empty by design, so
        // the cleanup tail below treats every `prev` entry as a removal.
        let mut per_window: HashMap<String, Vec<(Uuid, Option<AgentStatus>)>> = HashMap::new();
        for (label, state) in shared.all() {
            let g = state.lock();
            for sid in g.sessions.keys() {
                if prev.contains_key(sid) {
                    per_window.entry(label.clone()).or_default().push((*sid, None));
                }
            }
        }
        for (label, rows) in per_window {
            let payload = serde_json::json!({
                "sessions": rows.iter().map(|(id, _)| serde_json::json!({
                    "id": id, "agent": null, "state": null, "attention": null, "title": null, "project": null
                })).collect::<Vec<_>>(),
            });
            let _ = app.emit_to(&label, "agent-status-changed", payload);
        }
        let sessions: Vec<_> = prev.keys().map(|id| serde_json::json!({
            "id": id, "agent": null, "state": null, "attention": null, "title": null, "project": null
        })).collect();
        crate::services::ipc::broadcast(serde_json::json!({
            "event": "agent-status-changed",
            "sessions": sessions,
        }));
        let _ = app.emit("all-agent-status", serde_json::json!({ "sessions": [] }));
        shared.agents.lock().statuses = HashMap::new();
        return;
    }

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
    // The poller only reads pid/parent/cmdline, so use the lightweight
    // snapshot instead of `refresh_and_snapshot` (which also collects CPU%,
    // memory and exe paths for every process on the machine).
    let procs_cache = crate::services::procs::refresh_and_snapshot_light();

    for (label, state) in shared.all() {
        let mut g = state.lock();
        let mut persisted_changed = false;
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
                if let Some(detected) = detect_agent_with_sys(&procs_cache, &pids) {
                    if session.record_agent(detected.kind, detected.command.clone()) {
                        persisted_changed = true;
                    }
                    // Reading the tail activates the capture ring for this
                    // session (agent sessions only). That is the price of
                    // prompt detection: ANSI-stripped lines are accumulated
                    // from here on, unlike the capture-on-demand default.
                    // Capture must start as soon as the agent is detected,
                    // otherwise a prompt printed before the first idle poll
                    // would never enter the ring.
                    let tail = session.scrollback_lines(8);
                    // Only inspect once output has settled: an agent that is
                    // streaming is not blocked on an approval prompt, and the
                    // gate also clears a stale prompt from the flag as soon
                    // as the approved command starts producing output.
                    let idle = session.idle_for();
                    let attention = (idle >= Duration::from_millis(700)
                        && !session.awaiting_output()
                        && approval_needed(&tail))
                    .then_some(AgentAttention::Approval);
                    let agent_state = if attention.is_some() || idle >= WAITING_AFTER {
                        AgentState::Waiting
                    } else {
                        AgentState::Working
                    };
                    let status = AgentStatus {
                        agent: detected.kind,
                        state: agent_state,
                        attention,
                    };
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
                        let status = AgentStatus {
                            agent: prev_status.agent,
                            state: AgentState::Done,
                            attention: None,
                        };
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
        // A newly detected agent changes what the autosave snapshot should
        // contain (its launch command for restart-after-relaunch).
        if persisted_changed {
            g.bump();
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
                Some(s) => serde_json::json!({
                    "id": id, "agent": s.agent, "state": s.state, "attention": s.attention, "title": null, "project": null
                }),
                None => serde_json::json!({
                    "id": id, "agent": null, "state": null, "attention": null, "title": null, "project": null
                }),
            }).collect::<Vec<_>>(),
        });
        let _ = app.emit_to(&label, "agent-status-changed", payload);
        if started.elapsed() >= STARTUP_NOTIFY_GRACE {
            notify_changed(app, &label, shared, &prev, &rows);
        }
    }

    // Push all changed rows (across windows) to any live `muster watch` IPC
    // subscribers. Cheap when nobody is watching — `broadcast` returns after a
    // single empty-subscribers check.
    if !changed.is_empty() {
        let sessions = changed
            .iter()
            .map(|(id, status)| match status {
                Some(s) => serde_json::json!({
                    "id": id, "agent": s.agent, "state": s.state, "attention": s.attention, "title": null, "project": null
                }),
                None => serde_json::json!({
                    "id": id, "agent": null, "state": null, "attention": null, "title": null, "project": null
                }),
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
                serde_json::json!({ "id": id, "agent": s.agent, "state": s.state, "attention": s.attention, "title": title, "project": project })
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
    let lang = crate::services::i18n::effective(&shared.settings().lock().language);
    for (sid, status) in rows {
        let Some(status) = status else { continue };
        let prev_state = prev.get(sid).map(|p| p.state);
        let prev_attention = prev.get(sid).and_then(|p| p.attention);
        let Some(session) = g.sessions.get(sid) else { continue };
        let body = match status.state {
            // An approval prompt is more urgent than an ordinary idle prompt,
            // and may arrive while the agent was already `waiting` — key off
            // the attention transition instead of the state transition.
            AgentState::Waiting
                if status.attention == Some(AgentAttention::Approval)
                    && prev_attention != Some(AgentAttention::Approval) =>
            {
                // Record the send for later cooldown, but don't let a recent
                // waiting ping suppress an approval request.
                session.try_mark_agent_notify(NOTIFY_COOLDOWN);
                crate::services::i18n::translate("notify-agent-approval", &lang)
                    .replace("{agent}", status.agent.label())
                    .replace("{title}", &session.title())
            }
            AgentState::Waiting
                if prev_state != Some(AgentState::Waiting) && status.attention.is_none() =>
            {
                if !session.try_mark_agent_notify(NOTIFY_COOLDOWN) {
                    continue;
                }
                crate::services::i18n::translate("notify-agent-waiting", &lang)
                    .replace("{agent}", status.agent.label())
                    .replace("{title}", &session.title())
            }
            AgentState::Done
                if matches!(prev_state, Some(AgentState::Working) | Some(AgentState::Waiting)) =>
            {
                // Record the send so a flapping agent can't re-notify, but
                // don't gate the `done` notification itself — it fires once.
                session.try_mark_agent_notify(NOTIFY_COOLDOWN);
                crate::services::i18n::translate("notify-agent-done", &lang)
                    .replace("{agent}", status.agent.label())
                    .replace("{title}", &session.title())
            }
            _ => continue,
        };
        crate::services::notify::send(app, label, *sid, body);
    }
}

/// One row of the `agent_status_snapshot` command: the live detection for a
/// session, plus its display title so UI lists (close confirmation, command
/// palette) don't need a second round-trip.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SessionAgentStatus {
    pub session_id: Uuid,
    pub agent: AgentKind,
    pub state: AgentState,
    pub attention: Option<AgentAttention>,
    pub title: String,
}

/// Flatten the live agent cache into rows enriched with session titles.
/// Each session lives in exactly one window, so ids can't collide across the
/// per-window scans. Entries whose session has since closed are skipped.
pub fn snapshot_rows(shared: &SharedState) -> Vec<SessionAgentStatus> {
    let cache = shared.agents.lock();
    let mut rows = Vec::with_capacity(cache.statuses.len());
    for (_, s) in shared.all() {
        let g = s.lock();
        for (sid, status) in &cache.statuses {
            if let Some(session) = g.sessions.get(sid) {
                rows.push(SessionAgentStatus {
                    session_id: *sid,
                    agent: status.agent,
                    state: status.state,
                    attention: status.attention,
                    title: session.title(),
                });
            }
        }
    }
    rows
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
        assert_eq!(match_name("atomcode"), Some(AgentKind::Atomcode));
        assert_eq!(match_name("atomcode.exe"), Some(AgentKind::Atomcode));
        assert_eq!(match_name("copilot.exe"), Some(AgentKind::Copilot));
        assert_eq!(match_name("cursor-agent.exe"), Some(AgentKind::Cursor));
        assert_eq!(match_name("kiro-cli.exe"), Some(AgentKind::Kiro));
        assert_eq!(match_name("amp.exe"), Some(AgentKind::Amp));
        assert_eq!(match_name("droid.exe"), Some(AgentKind::Droid));
        assert_eq!(match_name("crush.exe"), Some(AgentKind::Crush));
        assert_eq!(match_name("cline.exe"), Some(AgentKind::Cline));
        assert_eq!(match_name("cn.exe"), Some(AgentKind::ContinueCli));
        assert_eq!(match_name("kilocode.exe"), Some(AgentKind::Kilo));
        assert_eq!(match_name("grok.exe"), Some(AgentKind::Grok));
        assert_eq!(match_name("openhands.exe"), Some(AgentKind::OpenHands));
        assert_eq!(match_name("pi.exe"), Some(AgentKind::Pi));
        assert_eq!(match_name("vibe.exe"), Some(AgentKind::Vibe));
        assert_eq!(match_name("qwen.exe"), Some(AgentKind::Qwen));
        assert_eq!(match_name("qoder.exe"), Some(AgentKind::Qoder));
        assert_eq!(match_name("traecli.exe"), Some(AgentKind::Trae));
        assert_eq!(match_name("codebuddy.exe"), Some(AgentKind::CodeBuddy));
        assert_eq!(match_name("comate.exe"), Some(AgentKind::Comate));
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
    fn cmdline_matching_covers_new_agents() {
        for (cmd, kind) in [
            ("npx @github/copilot", AgentKind::Copilot),
            (
                r#""C:\Users\x\AppData\Local\Programs\cursor-agent\cursor-agent.exe""#,
                AgentKind::Cursor,
            ),
            ("kiro-cli chat", AgentKind::Kiro),
            ("bunx @sourcegraph/amp", AgentKind::Amp),
            ("droid exec --help", AgentKind::Droid),
            (
                r#"C:\Users\x\scoop\apps\crush\current\crush.exe"#,
                AgentKind::Crush,
            ),
            ("cline task", AgentKind::Cline),
            ("npx -y @continuedev/cli", AgentKind::ContinueCli),
            ("npm exec --yes @kilocode/cli", AgentKind::Kilo),
            ("grok", AgentKind::Grok),
            ("uvx openhands", AgentKind::OpenHands),
            ("npx @mariozechner/pi", AgentKind::Pi),
            ("vibe", AgentKind::Vibe),
            ("npx @qwen-code/qwen-code", AgentKind::Qwen),
            (
                r#""C:\Users\x\AppData\Local\Programs\qoder\qoder.exe""#,
                AgentKind::Qoder,
            ),
            ("trae-cli run", AgentKind::Trae),
            ("npx @tencent-ai/codebuddy-code", AgentKind::CodeBuddy),
            ("comate", AgentKind::Comate),
        ] {
            assert_eq!(match_cmdline(cmd), Some(kind), "cmdline: {cmd}");
        }
    }

    #[test]
    fn cmdline_matching_rejects_lookalike_paths() {
        // A dev server in a directory whose name merely contains a marker
        // must not be mistaken for an agent.
        let cmd = r#"node "D:\codex-app\server.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = r#"node "C:\work\gemini-docs\watch.js""#;
        assert_eq!(match_cmdline(cmd), None);
        // Short markers are the risky ones: project directories named pi or
        // amp, npm scripts, and plain script arguments must stay untouched.
        let cmd = r#"node "C:\projects\pi\server.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = r#"node "C:\projects\amp\server.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = r#"node "C:\projects\kimi\src\main.js""#;
        assert_eq!(match_cmdline(cmd), None);
        let cmd = "npm run kilo";
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
        assert!(token_matches("@continuedev/cli", "@continuedev"));
        assert!(token_matches(r#"C:\projects\pi\server.js"#, "pi") == false);
        assert!(token_matches(
            r#"C:\Users\x\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js"#,
            "claude-code"
        ));
    }

    #[test]
    fn cmd_tokens_keep_quoted_paths_together() {
        let cmd = r#""C:\Program Files\nodejs\node.exe" "C:\tools\my agent\agent.js" --flag"#;
        assert_eq!(
            cmd_tokens(cmd),
            vec![
                r#"C:\Program Files\nodejs\node.exe"#,
                r#"C:\tools\my agent\agent.js"#,
                "--flag"
            ]
        );
    }

    #[test]
    fn launch_command_keeps_invocation_but_not_flags() {
        assert_eq!(
            launch_command(
                r#""C:\Program Files\nodejs\node.exe" "C:\Users\x\.local\bin\opencode" --api-key sk-secret"#,
                "node.exe"
            )
            .as_deref(),
            Some(r#""C:\Program Files\nodejs\node.exe" C:\Users\x\.local\bin\opencode"#)
        );
        assert_eq!(
            launch_command("claude --api-key sk-secret", "claude.exe").as_deref(),
            Some("claude")
        );
        assert_eq!(
            launch_command("npx -y @github/copilot", "node.exe").as_deref(),
            Some("npx @github/copilot")
        );
        assert_eq!(
            launch_command("python -m aider --model x", "python.exe").as_deref(),
            Some("python aider")
        );
        assert_eq!(
            launch_command("", "claude.exe").as_deref(),
            Some("claude.exe")
        );
    }

    #[test]
    fn approval_detection_reads_only_the_tail() {
        let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(approval_needed(&lines(&[
            "Do you want to proceed?",
            "❯ 1. Yes",
            "  2. No",
        ])));
        assert!(approval_needed(&lines(&["Allow command?", "> Run `npm test`"])));
        assert!(approval_needed(&lines(&["是否允许执行该命令？", "[y/N]"])));
        assert!(approval_needed(&lines(&["Choose an action", "1. Yes", "2. No"])));
        assert!(!approval_needed(&lines(&["Reading files...", "All done."])));
        assert!(!approval_needed(&lines(&[
            "Do you want to proceed?",
            "line",
            "line",
            "line",
            "line",
            "line",
        ])));
        assert!(!approval_needed(&[]));
    }

    #[test]
    fn detect_agent_empty_tree_is_none() {
        assert_eq!(detect_agent(&[]), None);
    }
}
