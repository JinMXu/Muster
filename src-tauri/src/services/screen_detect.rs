//! Screen-level agent state detection, kero-style: a rule manifest per
//! coding agent tells what the agent is doing from what its screen shows —
//! working (spinner/turn banner), blocked on the user (permission dialog),
//! or idle at its prompt. A redraw shows the same screen, so it is no
//! event, whatever made it.
//!
//! The engine and the manifests are kero's (github.com/egoist/kero), which
//! ports herdr's engine (github.com/ogulcancelik/herdr, Apache-2.0 — see
//! `detect-manifests/LICENSE-herdr-manifests`): of the rules that match,
//! the one of the highest priority wins; with none, the agent is idle.
//! Manifests are the TOMLs verbatim, embedded at compile time.
//!
//! Input is the rendered screen text (the frontend pushes the visible
//! xterm viewport), the session's title, and its OSC 9;4 progress.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

/// What an agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenState {
    Idle,
    Working,
    /// Waiting for the user: a permission, a question.
    Blocked,
    /// The screen tells nothing (a transcript viewer, a picker menu) —
    /// rules with this state carry `skip_state_update` and the caller
    /// keeps the previous status.
    Unknown,
}

/// The inputs a manifest's rules may read.
pub struct DetectInput<'a> {
    pub screen: &'a str,
    pub title: &'a str,
    /// ConEmu progress after `9;4`, as `state;progress` — e.g. "4;3".
    pub progress: &'a str,
}

/// The outcome: the winning state, whether the screen showed it outright
/// (a prompt box, a permission dialog, a spinner — as opposed to idle by
/// elimination), and whether the screen tells nothing (keep the old state).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectResult {
    pub state: ScreenState,
    pub visible: bool,
    pub skip: bool,
}

#[derive(Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    id: String,
    rules: Vec<RawRule>,
}

#[derive(Deserialize)]
struct RawRule {
    id: String,
    state: ScreenState,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    region: String,
    #[serde(default)]
    visible_idle: bool,
    #[serde(default)]
    visible_blocker: bool,
    #[serde(default)]
    visible_working: bool,
    #[serde(default, rename = "skip_state_update")]
    skip_state_update: bool,
    #[serde(flatten)]
    gate: RawGate,
}

/// A compiled rule: the raw manifest data plus its ready regexes.
struct Rule {
    id: String,
    state: ScreenState,
    priority: i32,
    region: String,
    visible_idle: bool,
    visible_blocker: bool,
    visible_working: bool,
    skip_state_update: bool,
    gate: Gate,
}

/// A rule's conditions, all of which hold: every needle contained (ignoring
/// case), every regex matching, every line regex matching a line, every
/// gate of `all`, one of `any` (when there are any), none of `not`.
struct Gate {
    contains: Vec<String>,
    regex: Vec<Regex>,
    line_regex: Vec<Regex>,
    all: Vec<Gate>,
    any: Vec<Gate>,
    not: Vec<Gate>,
}

#[derive(Deserialize)]
struct RawGate {
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
    #[serde(default)]
    all: Vec<RawGate>,
    #[serde(default)]
    any: Vec<RawGate>,
    #[serde(default)]
    not: Vec<RawGate>,
}

impl RawGate {
    fn compile(self) -> Result<Gate, String> {
        Ok(Gate {
            contains: self.contains.iter().map(|s| s.to_lowercase()).collect(),
            regex: self
                .regex
                .iter()
                .map(|s| Regex::new(s).map_err(|e| format!("regex {s:?}: {e}")))
                .collect::<Result<_, _>>()?,
            line_regex: self
                .line_regex
                .iter()
                .map(|s| Regex::new(s).map_err(|e| format!("line_regex {s:?}: {e}")))
                .collect::<Result<_, _>>()?,
            all: self
                .all
                .into_iter()
                .map(|g| g.compile())
                .collect::<Result<_, _>>()?,
            any: self
                .any
                .into_iter()
                .map(|g| g.compile())
                .collect::<Result<_, _>>()?,
            not: self
                .not
                .into_iter()
                .map(|g| g.compile())
                .collect::<Result<_, _>>()?,
        })
    }
}

impl Rule {
    fn parse(raw: RawRule) -> Result<Rule, String> {
        Ok(Rule {
            region: if raw.region.is_empty() {
                "whole_recent".to_string()
            } else {
                raw.region
            },
            id: raw.id,
            state: raw.state,
            priority: raw.priority,
            visible_idle: raw.visible_idle,
            visible_blocker: raw.visible_blocker,
            visible_working: raw.visible_working,
            skip_state_update: raw.skip_state_update,
            gate: raw.gate.compile()?,
        })
    }

    fn matches(&self, text: &str, lower: &str) -> bool {
        self.gate.matches(text, lower)
    }
}

impl Gate {
    fn matches(&self, text: &str, lower: &str) -> bool {
        if self.contains.iter().any(|s| !lower.contains(s)) {
            return false;
        }
        if self.regex.iter().any(|re| !re.is_match(text)) {
            return false;
        }
        if self.line_regex.iter().any(|re| !any_line(text, re)) {
            return false;
        }
        if self.all.iter().any(|g| !g.matches(text, lower)) {
            return false;
        }
        if !self.any.is_empty() && !self.any.iter().any(|g| g.matches(text, lower)) {
            return false;
        }
        if self.not.iter().any(|g| g.matches(text, lower)) {
            return false;
        }
        true
    }
}

fn any_line(text: &str, re: &Regex) -> bool {
    text.lines().any(|l| re.is_match(l.trim_end_matches(['\r', '\n'])))
}

// --- manifest loading --------------------------------------------------------

/// Agent kind (as `AgentKind::as_str`) → manifest name, where they differ.
/// Kinds without a manifest map to themselves and simply miss the table.
fn manifest_name(kind: &str) -> &'static str {
    match kind {
        "claude_code" => "claude",
        "kimi_code" => "kimi",
        "copilot" => "github-copilot",
        "amp" => "amp",
        "cline" => "cline",
        "codex" => "codex",
        "cursor" => "cursor",
        "droid" => "droid",
        "gemini" => "gemini",
        "kiro" => "kiro",
        "opencode" => "opencode",
        "pi" => "pi",
        "qwen" => "qwen",
        _ => "none",
    }
}

macro_rules! embed_manifest {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../detect-manifests/", $name, ".toml")),
        )
    };
}

/// The manifests verbatim from kero (see the module comment for licensing).
static MANIFEST_SOURCES: &[(&str, &str)] = &[
    embed_manifest!("amp"),
    embed_manifest!("claude"),
    embed_manifest!("cline"),
    embed_manifest!("codex"),
    embed_manifest!("cursor"),
    embed_manifest!("droid"),
    embed_manifest!("gemini"),
    embed_manifest!("github-copilot"),
    embed_manifest!("kimi"),
    embed_manifest!("kiro"),
    embed_manifest!("opencode"),
    embed_manifest!("pi"),
    embed_manifest!("qwen"),
];

fn manifests() -> &'static HashMap<&'static str, Vec<Rule>> {
    static MANIFESTS: OnceLock<HashMap<&'static str, Vec<Rule>>> = OnceLock::new();
    MANIFESTS.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, source) in MANIFEST_SOURCES {
            let raw: Manifest = match toml::from_str(source) {
                Ok(m) => m,
                // A manifest that does not compile is a packaging bug; skip
                // it (the agent falls back to idle-timeout detection) and
                // log — never panic the poller over a rule.
                Err(e) => {
                    log::warn!("screen_detect: manifest {name}: {e}");
                    continue;
                }
            };
            let rules = raw
                .rules
                .into_iter()
                .map(Rule::parse)
                .collect::<Result<Vec<_>, _>>();
            match rules {
                Ok(rules) => {
                    map.insert(*name, rules);
                }
                Err(e) => log::warn!("screen_detect: manifest {name}: {e}"),
            }
        }
        map
    })
}

/// Whether there are rules for the agent kind.
pub fn known(kind: &str) -> bool {
    manifests().contains_key(manifest_name(kind))
}

/// Run the manifest's rules: the highest-priority match wins; none → idle
/// (by elimination — `visible` false).
pub fn detect(kind: &str, input: &DetectInput) -> DetectResult {
    let name = manifest_name(kind);
    let Some(rules) = manifests().get(name) else {
        return idle_result();
    };
    let mut won: Option<&Rule> = None;
    for rule in rules {
        if won.is_some_and(|w| w.priority >= rule.priority) {
            continue;
        }
        let text = region(input, &rule.region);
        if rule.matches(&text, &text.to_lowercase()) {
            won = Some(rule);
        }
    }
    let Some(rule) = won else {
        log::debug!("screen_detect[{kind}]: no rule matched -> idle by elimination");
        return idle_result();
    };
    let state = rule.state;
    let visible = match state {
        ScreenState::Idle => rule.visible_idle,
        ScreenState::Blocked => rule.visible_blocker,
        ScreenState::Working => rule.visible_working,
        // An "unknown" verdict exists only to skip the update.
        ScreenState::Unknown => false,
    };
    log::debug!(
        "screen_detect[{kind}]: rule {} -> {:?} visible={visible} skip={}",
        rule.id,
        rule.state,
        rule.skip_state_update,
    );
    DetectResult {
        state,
        visible,
        skip: rule.skip_state_update || state == ScreenState::Unknown,
    }
}

fn idle_result() -> DetectResult {
    DetectResult {
        state: ScreenState::Idle,
        visible: false,
        skip: false,
    }
}

// --- regions: the parts of the screen a rule reads ---------------------------

fn region(input: &DetectInput, spec: &str) -> String {
    let spec = spec.trim();
    let s = input.screen;
    match spec {
        "osc_title" => return input.title.to_string(),
        "osc_progress" => return input.progress.to_string(),
        "whole_recent" => return s.to_string(),
        "after_last_prompt_marker" => {
            let ls = lines(s);
            for i in (0..ls.len()).rev() {
                if prompt_line(ls[i]) {
                    return join(&ls[i + 1..]);
                }
            }
            return s.to_string();
        }
        "before_current_prompt_marker" => {
            let ls = lines(s);
            if let Some(i) = current_prompt(&ls) {
                return join(&ls[..i]);
            }
            return s.to_string();
        }
        "whole_recent_without_current_prompt_marker" => {
            if current_prompt(&lines(s)).is_some() {
                return String::new();
            }
            return s.to_string();
        }
        "current_prompt_block_marker" => {
            let ls = lines(s);
            if let Some(i) = current_prompt(&ls) {
                for j in (0..i).rev() {
                    if block_line(ls[j]) {
                        return ls[j].to_string();
                    }
                }
            }
            return String::new();
        }
        "after_current_prompt_block_marker" => {
            let ls = lines(s);
            if let Some(i) = current_prompt(&ls) {
                for j in (0..i).rev() {
                    if block_line(ls[j]) {
                        return join(&ls[j..]);
                    }
                }
            }
            return String::new();
        }
        "prompt_box_body" => {
            let ls = lines(s);
            let Some(top) = prompt_box_top(&ls) else {
                return String::new();
            };
            let mut end = ls.len();
            for j in top + 1..ls.len() {
                if horizontal_rule(ls[j]) {
                    end = j;
                    break;
                }
            }
            return join(&ls[top + 1..end]);
        }
        "above_prompt_box" => return above_prompt_box(s),
        "last_non_empty_above_prompt_box" => {
            let above = above_prompt_box(s);
            let ls = lines(&above);
            for l in ls.iter().rev() {
                if !l.trim().is_empty() {
                    return l.to_string();
                }
            }
            return String::new();
        }
        "after_last_horizontal_rule" => {
            let ls = lines(s);
            for i in (0..ls.len()).rev() {
                if horizontal_rule(ls[i]) {
                    return join(&ls[i + 1..]);
                }
            }
            return s.to_string();
        }
        _ => {}
    }
    if let Some(n) = counted(spec, "bottom_lines") {
        let ls = lines(s);
        return join(&ls[ls.len().saturating_sub(n)..]);
    }
    if let Some(n) = counted(spec, "bottom_non_empty_lines") {
        let ls = lines(s);
        let mut left = n;
        for i in (0..ls.len()).rev() {
            if !ls[i].trim().is_empty() {
                left -= 1;
                if left == 0 {
                    return join(&ls[i..]);
                }
            }
        }
        return s.to_string();
    }
    if let Some(n) = counted(spec, "top_non_empty_lines") {
        let ls = lines(s);
        let mut end = 0;
        let mut left = n;
        for (i, l) in ls.iter().enumerate() {
            if !l.trim().is_empty() {
                end = i + 1;
                left -= 1;
                if left == 0 {
                    break;
                }
            }
        }
        return join(&ls[..end]);
    }
    String::new()
}

fn counted(spec: &str, name: &str) -> Option<usize> {
    let rest = spec.strip_prefix(&format!("{name}("))?;
    let rest = rest.strip_suffix(')')?;
    rest.parse::<usize>().ok().filter(|&n| n > 0)
}

fn lines(s: &str) -> Vec<&str> {
    let s = s.strip_suffix('\n').unwrap_or(s);
    if s.is_empty() {
        Vec::new()
    } else {
        s.split('\n').collect()
    }
}

fn join(ls: &[&str]) -> String {
    if ls.is_empty() {
        String::new()
    } else {
        format!("{}\n", ls.join("\n"))
    }
}

/// Codex's prompt: "›" at the start of a line.
fn prompt_line(l: &str) -> bool {
    l == "›" || l.starts_with("› ")
}

/// One of Codex's transcript blocks.
fn block_line(l: &str) -> bool {
    l.starts_with('•') || l.starts_with('■') || l.starts_with('✗') || l.starts_with('✓')
}

/// The index of Codex's prompt line, unless a block comes after it; none
/// otherwise.
fn current_prompt(ls: &[&str]) -> Option<usize> {
    for i in (0..ls.len()).rev() {
        if prompt_line(ls[i]) {
            for l in &ls[i + 1..] {
                if block_line(l) {
                    return None;
                }
            }
            return Some(i);
        }
    }
    None
}

/// The index of a prompt box's top border: the second rule from the bottom.
fn prompt_box_top(ls: &[&str]) -> Option<usize> {
    let mut n = 0;
    for i in (0..ls.len()).rev() {
        if horizontal_rule(ls[i]) {
            n += 1;
            if n == 2 {
                return Some(i);
            }
        }
    }
    None
}

fn above_prompt_box(s: &str) -> String {
    let ls = lines(s);
    if let Some(top) = prompt_box_top(&ls) {
        return join(&ls[..top]);
    }
    s.to_string()
}

/// A line of "─", or one starting with three — a box's border with a label.
/// The leading run of `─` is counted as Unicode chars, but the rest must be
/// sliced at a *byte* boundary, so the byte offset where the run ends is
/// tracked as chars are consumed (a `─` spans 3 bytes).
fn horizontal_rule(l: &str) -> bool {
    let t = l.trim();
    let mut n = 0usize;
    let mut byte_end = 0usize;
    for (i, c) in t.char_indices() {
        if c != '─' {
            byte_end = i;
            break;
        }
        n += 1;
        byte_end = i + c.len_utf8();
    }
    if n == 0 {
        return false;
    }
    let rest = t[byte_end..].trim();
    rest.is_empty() || n >= 3
}

// --- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_compile_and_cover_known_agents() {
        for kind in [
            "claude_code", "codex", "gemini", "opencode", "amp", "cursor", "droid", "qwen",
            "copilot", "kiro", "cline", "kimi_code", "pi",
        ] {
            assert!(known(kind), "no rules for {kind}");
        }
        assert!(!known("bash"));
    }

    fn detect_screen(kind: &str, screen: &str) -> DetectResult {
        detect(
            kind,
            &DetectInput {
                screen,
                title: "",
                progress: "",
            },
        )
    }

    /// Claude Code's screens, as kero's fixtures printed them (no title —
    /// the screen rules alone must classify).
    #[test]
    fn claude_screens() {
        let idle = include_str!("../../detect-manifests/testdata/claude-idle.txt");
        let working = include_str!("../../detect-manifests/testdata/claude-working.txt");
        let blocked = include_str!("../../detect-manifests/testdata/claude-blocked.txt");
        let r = detect_screen("claude_code", idle);
        assert_eq!(r.state, ScreenState::Idle, "at its prompt");
        assert!(r.visible, "the prompt box shows");
        let r = detect_screen("claude_code", working);
        assert_eq!(r.state, ScreenState::Working, "thinking");
        let r = detect_screen("claude_code", blocked);
        assert_eq!(r.state, ScreenState::Blocked, "asking to run a command");
        // A redraw at another width shows the same: still idle.
        let rewrapped = idle.replace("────", "──");
        assert_eq!(
            detect_screen("claude_code", &rewrapped).state,
            ScreenState::Idle
        );
    }

    /// Pi's spinner lines, from its manifest.
    #[test]
    fn pi_working_spinner() {
        let screen = "── ⠋ Working ────────\nfoo\n";
        assert_eq!(detect_screen("pi", screen).state, ScreenState::Working);
        let screen = "⠙ Working\nfoo\n";
        assert_eq!(detect_screen("pi", screen).state, ScreenState::Working);
        assert_eq!(detect_screen("pi", "just a prompt\n").state, ScreenState::Idle);
    }

    #[test]
    fn regions() {
        let input = DetectInput {
            screen: "a\n\nb\n───\nbox\n───\nfoot\n",
            title: "T",
            progress: "4;3",
        };
        let cases: &[(&str, &str)] = &[
            ("bottom_non_empty_lines(2)", "───\nfoot\n"),
            ("bottom_lines(1)", "foot\n"),
            ("top_non_empty_lines(2)", "a\n\nb\n"),
            ("after_last_horizontal_rule", "foot\n"),
            ("prompt_box_body", "box\n"),
            ("above_prompt_box", "a\n\nb\n"),
            ("osc_title", "T"),
            ("osc_progress", "4;3"),
        ];
        for (spec, want) in cases {
            assert_eq!(&region(&input, spec), want, "region {spec}");
        }
    }

    /// Regression: a rule one character wide (2 `─`, not a multiple of the
    /// 3-byte `─`) used to panic on `t[n..]` — byte index off a char
    /// boundary. This killed the whole agent poll thread whenever a real
    /// screen had a horizontal rule of non-multiple-of-3 width.
    #[test]
    fn horizontal_rule_non_multiple_of_three() {
        // kero's rule: all `─` (or `n >= 3` with a label after); a 2-rule
        // line leaving trailing text is not a rule. The key assertion here
        // is that none of these PANIC on the byte index.
        assert!(horizontal_rule("───"));
        assert!(horizontal_rule("─── x"));
        assert!(horizontal_rule("─────"));
        assert!(!horizontal_rule("── x")); // n=2 with label -> not a rule
        assert!(!horizontal_rule("──┐"));
        // A 2-rule prompt box still resolves (no panic, correct body).
        let s = "head\n──\nbox\n──\nfoot\n";
        let _ = detect(
            "claude_code",
            &DetectInput {
                screen: s,
                title: "",
                progress: "",
            },
        );
    }
}