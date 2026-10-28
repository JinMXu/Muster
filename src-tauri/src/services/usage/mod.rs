//! Coding-agent token usage: run ledger, native log adapters, attribution and
//! the comparison metrics shown by the Usage panel.
//!
//! Data flow:
//! - `agents.rs` observes agent start/stop and records an `AgentRun`.
//! - The scanner periodically parses each agent's local logs into normalized
//!   `UsageEvent`s, deduped by a stable id and appended to a local ledger.
//! - Events are attributed to runs by agent + cwd + time window; unattributed
//!   events still count toward agent/model totals.

mod adapters;
mod model;
mod time;

pub use model::*;
pub use time::now_ms;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::commands::SharedState;
use crate::services::agents::AgentKind;

pub use adapters::FileCursor;

/// Keep three months of history; enough for trends without an unbounded
/// ledger. The ledger is compacted at startup.
const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1_000;
const SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// Events may land slightly before/after the process window because clocks
/// and log writes are not synchronized.
const ATTRIBUTION_GRACE_MS: i64 = 120_000;

pub const ADAPTER_AGENTS: &[AgentKind] = &[
    AgentKind::ClaudeCode,
    AgentKind::Qoder,
    AgentKind::KimiCode,
    AgentKind::Pi,
    AgentKind::Opencode,
    AgentKind::Grok,
    AgentKind::CodeBuddy,
    AgentKind::Codex,
];

#[derive(Default)]
struct UsageInner {
    events: Vec<UsageEvent>,
    event_ids: HashSet<String>,
    runs: Vec<AgentRun>,
    run_ids: HashSet<Uuid>,
    active: HashMap<Uuid, AgentRun>,
    cursors: HashMap<String, FileCursor>,
    opencode_ts: i64,
    updated_at: i64,
    revision: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct CursorsFile {
    #[serde(default)]
    files: HashMap<String, FileCursor>,
    #[serde(default)]
    opencode_ts: i64,
}

pub struct UsageStore {
    inner: Mutex<UsageInner>,
    dir: PathBuf,
}

pub struct RunObservation {
    pub session_id: Uuid,
    pub agent: AgentKind,
    pub project_id: Uuid,
    pub project: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub now_ms: i64,
}

struct RunAcc {
    tokens: TokenUsage,
    turns: u32,
    models: HashMap<String, u64>,
    confidence: u8,
}

impl Default for RunAcc {
    fn default() -> Self {
        Self {
            tokens: TokenUsage::default(),
            turns: 0,
            models: HashMap::new(),
            confidence: 0,
        }
    }
}

impl UsageStore {
    pub fn new() -> Self {
        Self::new_at(data_dir())
    }

    fn new_at(dir: PathBuf) -> Self {
        let cutoff = now_ms() - RETENTION_MS;
        let mut inner = UsageInner::default();
        for ev in load_json_lines::<UsageEvent>(&events_path(&dir)) {
            if ev.ts_ms >= cutoff && inner.event_ids.insert(ev.id.clone()) {
                inner.events.push(ev);
            }
        }
        let mut runs: Vec<AgentRun> = load_json_lines::<AgentRun>(&runs_path(&dir))
            .into_iter()
            .filter(|r| r.started_at >= cutoff)
            .collect();
        for run in &runs {
            inner.run_ids.insert(run.id);
        }
        if let Some(active) = load_json_file::<Vec<AgentRun>>(&active_path(&dir)) {
            for mut run in active {
                if run.started_at < cutoff {
                    continue;
                }
                run.ended_at = Some(run.last_seen_at.max(run.started_at));
                if inner.run_ids.insert(run.id) {
                    runs.push(run);
                }
            }
        }
        inner.runs = runs;
        let cursors = load_json_file::<CursorsFile>(&cursors_path(&dir)).unwrap_or_default();
        inner.cursors = cursors.files;
        inner.opencode_ts = cursors.opencode_ts;
        inner.updated_at = now_ms();

        let store = Self {
            inner: Mutex::new(inner),
            dir,
        };
        store.compact_at_startup();
        store
    }

    fn compact_at_startup(&self) {
        let (events, runs) = {
            let inner = self.inner.lock();
            (inner.events.clone(), inner.runs.clone())
        };
        let _ = write_json_lines(&events_path(&self.dir), &events);
        let _ = write_json_lines(&runs_path(&self.dir), &runs);
        let _ = write_json_file(&active_path(&self.dir), &Vec::<AgentRun>::new());
    }

    pub fn revision(&self) -> (u64, i64) {
        let inner = self.inner.lock();
        (inner.revision, inner.updated_at)
    }

    /// Incremental scan. Returns the revision so the scanner can emit one
    /// `usage-updated` per change without comparing full snapshots.
    pub fn scan(&self) -> (u64, i64) {
        let now = now_ms();
        let cutoff = now - RETENTION_MS;
        let (cursors, opencode_ts) = {
            let inner = self.inner.lock();
            (inner.cursors.clone(), inner.opencode_ts)
        };
        let mut ctx = adapters::ScanCtx::new(cursors, opencode_ts, cutoff);
        adapters::scan_all(&mut ctx);

        let mut inner = self.inner.lock();
        let cursors_dirty = ctx.dirty;
        let mut new_events = Vec::new();
        for ev in ctx.events {
            if ev.ts_ms < cutoff {
                continue;
            }
            if inner.event_ids.insert(ev.id.clone()) {
                inner.events.push(ev.clone());
                new_events.push(ev);
            }
        }
        inner.cursors = ctx.cursors;
        inner.opencode_ts = ctx.opencode_ts;
        let had_new = !new_events.is_empty();
        if had_new {
            inner.revision += 1;
            inner.updated_at = now;
        }

        if cursors_dirty || had_new {
            let _ = write_json_file(
                &cursors_path(&self.dir),
                &CursorsFile {
                    files: inner.cursors.clone(),
                    opencode_ts: inner.opencode_ts,
                },
            );
        }
        let active: Vec<AgentRun> = inner.active.values().cloned().collect();
        let _ = write_json_file(&active_path(&self.dir), &active);
        if had_new {
            let _ = append_json_lines(&events_path(&self.dir), &new_events);
        }
        (inner.revision, inner.updated_at)
    }

    pub fn needs_run(&self, session_id: Uuid, agent: AgentKind, cwd: &str) -> bool {
        let inner = self.inner.lock();
        match inner.active.get(&session_id) {
            Some(run) => run.agent != agent || !same_cwd(&run.cwd, cwd),
            None => true,
        }
    }

    pub fn observe_run(&self, obs: RunObservation) {
        let mut closed = Vec::new();
        {
            let mut inner = self.inner.lock();
            if let Some(run) = inner.active.get(&obs.session_id).cloned() {
                if run.agent == obs.agent && same_cwd(&run.cwd, &obs.cwd) {
                    if let Some(r) = inner.active.get_mut(&obs.session_id) {
                        r.last_seen_at = obs.now_ms;
                    }
                    return;
                }
                if let Some(mut run) = inner.active.remove(&obs.session_id) {
                    run.ended_at = Some(obs.now_ms);
                    run.last_seen_at = obs.now_ms;
                    inner.run_ids.insert(run.id);
                    inner.runs.push(run.clone());
                    closed.push(run);
                }
            }
            let run = AgentRun {
                id: Uuid::new_v4(),
                agent: obs.agent,
                project_id: obs.project_id,
                project: obs.project,
                session_id: obs.session_id,
                cwd: obs.cwd,
                branch: obs.branch,
                started_at: obs.now_ms,
                ended_at: None,
                last_seen_at: obs.now_ms,
            };
            inner.active.insert(obs.session_id, run);
            inner.revision += 1;
            inner.updated_at = obs.now_ms;
        }
        if !closed.is_empty() {
            let _ = append_json_lines(&runs_path(&self.dir), &closed);
        }
    }

    pub fn end_missing_runs(&self, live_sessions: &HashSet<Uuid>, now: i64) {
        let mut closed = Vec::new();
        {
            let mut inner = self.inner.lock();
            let gone: Vec<Uuid> = inner
                .active
                .keys()
                .copied()
                .filter(|sid| !live_sessions.contains(sid))
                .collect();
            for sid in gone {
                if let Some(mut run) = inner.active.remove(&sid) {
                    run.ended_at = Some(now);
                    run.last_seen_at = now;
                    inner.run_ids.insert(run.id);
                    inner.runs.push(run.clone());
                    closed.push(run);
                }
            }
            if !closed.is_empty() {
                inner.revision += 1;
                inner.updated_at = now;
            }
        }
        if !closed.is_empty() {
            let _ = append_json_lines(&runs_path(&self.dir), &closed);
        }
    }

    pub fn end_all_runs(&self, now: i64) {
        self.end_missing_runs(&HashSet::new(), now);
    }

    pub fn overview(&self, since_ms: i64) -> UsageOverview {
        let now = now_ms();
        let inner = self.inner.lock();
        let runs = all_runs(&inner);
        let attrs = attribute(&inner.events, &runs, now);
        let mut today = UsageTotals::default();
        let mut today_runs = HashSet::new();
        for (ev, run, _) in &attrs {
            if ev.ts_ms < since_ms {
                continue;
            }
            today.tokens.add(&ev.tokens);
            today.turns += ev.turns as u64;
            today.events += 1;
            if let Some(run) = run {
                today_runs.insert(run.id);
            }
        }
        today.runs = today_runs.len() as u64;

        // One pass over the attributions, bucketed per run id (the old code
        // rescanned the full attrs vector once per run).
        let mut accs: HashMap<Uuid, RunAcc> = HashMap::new();
        for (ev, hit, conf) in &attrs {
            let Some(run) = hit else { continue };
            let acc = accs.entry(run.id).or_default();
            acc.tokens.add(&ev.tokens);
            acc.turns += ev.turns;
            acc.confidence = acc.confidence.max(conf_rank(*conf));
            if let Some(model) = &ev.model {
                *acc.models.entry(model.clone()).or_default() += ev.tokens.total();
            }
        }
        let mut active: Vec<UsageRunView> = inner
            .active
            .values()
            .map(|run| {
                let empty = RunAcc::default();
                run_view(run, accs.get(&run.id).unwrap_or(&empty))
            })
            .collect();
        active.sort_by_key(|r| std::cmp::Reverse(r.started_at));

        UsageOverview {
            updated_at: inner.updated_at,
            since_ms,
            today,
            active_runs: active,
            coverage: coverage_locked(&inner),
        }
    }

    pub fn compare(
        &self,
        since_ms: Option<i64>,
        until_ms: Option<i64>,
        project_id: Option<Uuid>,
    ) -> UsageCompare {
        let now = now_ms();
        let inner = self.inner.lock();
        let runs = all_runs(&inner);
        let attrs = attribute(&inner.events, &runs, now);
        let run_by_id: HashMap<Uuid, &AgentRun> = runs.iter().map(|r| (r.id, *r)).collect();

        #[derive(Default)]
        struct RowAcc {
            tokens: TokenUsage,
            total_tokens: u64,
            attributed_tokens: u64,
            turns: u64,
            events: u64,
            run_ids: HashSet<Uuid>,
        }

        let mut rows: HashMap<(AgentKind, Option<String>), RowAcc> = HashMap::new();
        let mut totals = UsageTotals::default();
        for (ev, run, _) in &attrs {
            if !in_range(ev.ts_ms, since_ms, until_ms) {
                continue;
            }
            if let Some(pid) = project_id {
                if run.map(|r| r.project_id) != Some(pid) {
                    continue;
                }
            }
            totals.tokens.add(&ev.tokens);
            totals.turns += ev.turns as u64;
            totals.events += 1;
            let row = rows.entry((ev.agent, ev.model.clone())).or_default();
            row.tokens.add(&ev.tokens);
            row.total_tokens += ev.tokens.total();
            row.turns += ev.turns as u64;
            row.events += 1;
            if let Some(run) = run {
                row.run_ids.insert(run.id);
                row.attributed_tokens += ev.tokens.total();
            }
        }
        let mut all_runs = HashSet::new();
        for row in rows.values() {
            all_runs.extend(row.run_ids.iter().copied());
        }
        totals.runs = all_runs.len() as u64;

        let mut out: Vec<UsageCompareRow> = rows
            .into_iter()
            .map(|((agent, model), row)| {
                let runs_n = row.run_ids.len() as u64;
                let durations: Vec<i64> = row
                    .run_ids
                    .iter()
                    .filter_map(|id| run_by_id.get(id))
                    .map(|r| duration_ms(r, now))
                    .collect();
                let avg_duration = if durations.is_empty() {
                    0.0
                } else {
                    durations.iter().sum::<i64>() as f64 / durations.len() as f64
                };
                UsageCompareRow {
                    agent,
                    model,
                    runs: runs_n,
                    events: row.events,
                    tokens: row.tokens,
                    total_tokens: row.total_tokens,
                    tokens_per_run: if runs_n == 0 {
                        0.0
                    } else {
                        row.total_tokens as f64 / runs_n as f64
                    },
                    fresh_input_per_run: if runs_n == 0 {
                        0.0
                    } else {
                        row.tokens.input as f64 / runs_n as f64
                    },
                    cache_hit: row.tokens.cache_hit(),
                    tokens_per_turn: if row.turns == 0 {
                        0.0
                    } else {
                        row.total_tokens as f64 / row.turns as f64
                    },
                    output_per_turn: if row.turns == 0 {
                        0.0
                    } else {
                        row.tokens.output as f64 / row.turns as f64
                    },
                    avg_duration_ms: avg_duration,
                    attributed_ratio: if row.total_tokens == 0 {
                        0.0
                    } else {
                        row.attributed_tokens as f64 / row.total_tokens as f64
                    },
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.total_tokens
                .cmp(&a.total_tokens)
                .then_with(|| a.agent.label().cmp(b.agent.label()))
                .then_with(|| a.model.cmp(&b.model))
        });

        UsageCompare {
            since_ms,
            until_ms,
            project_id,
            rows: out,
            totals,
            coverage: coverage_locked(&inner),
            updated_at: inner.updated_at,
        }
    }

    pub fn runs(
        &self,
        since_ms: Option<i64>,
        until_ms: Option<i64>,
        project_id: Option<Uuid>,
        limit: usize,
    ) -> Vec<UsageRunView> {
        let now = now_ms();
        let inner = self.inner.lock();
        let runs = all_runs(&inner);
        let attrs = attribute(&inner.events, &runs, now);
        // Single pass over attributions, bucketed per run id.
        let mut accs: HashMap<Uuid, RunAcc> = HashMap::new();
        for (ev, hit, conf) in &attrs {
            let Some(run) = hit else { continue };
            let acc = accs.entry(run.id).or_default();
            acc.tokens.add(&ev.tokens);
            acc.turns += ev.turns;
            acc.confidence = acc.confidence.max(conf_rank(*conf));
            if let Some(model) = &ev.model {
                *acc.models.entry(model.clone()).or_default() += ev.tokens.total();
            }
        }
        let mut views = Vec::new();
        for run in runs {
            let end = run.ended_at.unwrap_or(now);
            if since_ms.is_some_and(|s| end < s) || until_ms.is_some_and(|u| run.started_at > u) {
                continue;
            }
            if project_id.is_some_and(|pid| run.project_id != pid) {
                continue;
            }
            let empty = RunAcc::default();
            views.push(run_view(run, accs.get(&run.id).unwrap_or(&empty)));
        }
        views.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        views.truncate(limit.clamp(1, 1_000));
        views
    }

    pub fn coverage(&self) -> Vec<AgentCoverage> {
        let inner = self.inner.lock();
        coverage_locked(&inner)
    }

    #[cfg(test)]
    fn push_test_event(&self, ev: UsageEvent) {
        let mut inner = self.inner.lock();
        if inner.event_ids.insert(ev.id.clone()) {
            inner.events.push(ev.clone());
            inner.revision += 1;
            let _ = append_json_lines(&events_path(&self.dir), &[ev]);
        }
    }
}

pub fn spawn_scanner(app: AppHandle) {
    let usage = app.state::<SharedState>().usage.clone();
    std::thread::spawn(move || {
        let mut last = u64::MAX;
        loop {
            let (revision, updated_at) = usage.scan();
            if revision != last {
                let _ = app.emit(
                    "usage-updated",
                    crate::events::UsageUpdated {
                        revision,
                        updated_at,
                    },
                );
                last = revision;
            }
            std::thread::sleep(SCAN_INTERVAL);
        }
    });
}

/// Branch resolution cache TTL: a run started within this window of a previous
/// lookup for the same cwd reuses the cached name. Short enough that a branch
/// switch is picked up by the next run in practice, long enough that a burst
/// of run starts in one repo costs a single `discover`.
const BRANCH_CACHE_TTL_MS: i64 = 30_000;

pub fn branch_for(cwd: &str) -> Option<String> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Mutex<HashMap<String, (i64, Option<String>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let now = now_ms();
    let key = normalize_path(cwd);
    if let Some((ts, v)) = cache.lock().get(&key) {
        if now.saturating_sub(*ts) < BRANCH_CACHE_TTL_MS {
            return v.clone();
        }
    }
    let resolved = branch_for_uncached(cwd);
    cache.lock().insert(key, (now, resolved.clone()));
    resolved
}

fn branch_for_uncached(cwd: &str) -> Option<String> {
    let repo = git2::Repository::discover(cwd).ok()?;
    let head = repo.head().ok()?;
    head.shorthand().map(str::to_owned)
}

fn all_runs<'a>(inner: &'a UsageInner) -> Vec<&'a AgentRun> {
    inner.runs.iter().chain(inner.active.values()).collect()
}

fn coverage_locked(inner: &UsageInner) -> Vec<AgentCoverage> {
    let mut events: HashMap<AgentKind, (u64, u64)> = HashMap::new();
    for ev in &inner.events {
        let entry = events.entry(ev.agent).or_default();
        entry.0 += 1;
        entry.1 += ev.tokens.total();
    }
    let mut runs: HashMap<AgentKind, u64> = HashMap::new();
    for run in inner.runs.iter().chain(inner.active.values()) {
        *runs.entry(run.agent).or_default() += 1;
    }
    AgentKind::ALL
        .iter()
        .map(|agent| {
            let (events_n, tokens) = events.get(agent).copied().unwrap_or((0, 0));
            AgentCoverage {
                agent: *agent,
                has_adapter: ADAPTER_AGENTS.contains(agent),
                events: events_n,
                runs: runs.get(agent).copied().unwrap_or(0),
                total_tokens: tokens,
            }
        })
        .collect()
}

fn attribute<'a>(
    events: &'a [UsageEvent],
    runs: &[&'a AgentRun],
    now: i64,
) -> Vec<(&'a UsageEvent, Option<&'a AgentRun>, Confidence)> {
    let mut by_cwd: HashMap<(AgentKind, String), Vec<&AgentRun>> = HashMap::new();
    for run in runs {
        by_cwd
            .entry((run.agent, normalize_path(&run.cwd)))
            .or_default()
            .push(run);
    }
    // Fallback lookup (events without a cwd match): per-agent runs sorted by
    // started_at plus that agent's longest duration. A run can only overlap
    // timestamp ts when started_at >= ts - grace - max_duration, so the
    // "exactly one candidate" check binary-searches that bounded window
    // instead of scanning every run for every event (O(events x runs) with
    // months of ledger).
    let mut by_agent: HashMap<AgentKind, (Vec<&AgentRun>, i64)> = HashMap::new();
    for run in runs {
        by_agent.entry(run.agent).or_default().0.push(run);
    }
    for (_, (list, max_dur)) in by_agent.iter_mut() {
        *max_dur = list.iter().map(|r| duration_ms(r, now)).max().unwrap_or(0);
        list.sort_by_key(|r| r.started_at);
    }

    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        let mut hit = None;
        if let Some(cwd) = &ev.cwd {
            if let Some(cands) = by_cwd.get(&(ev.agent, normalize_path(cwd))) {
                for run in cands {
                    let end = run.ended_at.unwrap_or(now);
                    if ev.ts_ms + ATTRIBUTION_GRACE_MS >= run.started_at
                        && ev.ts_ms <= end + ATTRIBUTION_GRACE_MS
                        && hit
                            .map(|best: &AgentRun| run.started_at > best.started_at)
                            .unwrap_or(true)
                    {
                        hit = Some(*run);
                    }
                }
            }
        }
        let mut confidence = if ev.coarse {
            Confidence::Low
        } else {
            Confidence::High
        };
        if hit.is_none() {
            let mut cands: Vec<&AgentRun> = Vec::new();
            if let Some((list, max_dur)) = by_agent.get(&ev.agent) {
                let lo = ev.ts_ms - ATTRIBUTION_GRACE_MS - max_dur;
                let hi = ev.ts_ms + ATTRIBUTION_GRACE_MS;
                let start = list.partition_point(|r| r.started_at < lo);
                for &run in &list[start..] {
                    if run.started_at > hi {
                        break;
                    }
                    let end = run.ended_at.unwrap_or(now);
                    if end + ATTRIBUTION_GRACE_MS >= ev.ts_ms {
                        cands.push(run);
                    }
                }
            }
            if cands.len() == 1 {
                hit = Some(cands[0]);
                confidence = if ev.coarse {
                    Confidence::Low
                } else {
                    Confidence::Medium
                };
            } else {
                confidence = Confidence::Low;
            }
        }
        out.push((ev, hit, confidence));
    }
    out
}

fn run_view(run: &AgentRun, acc: &RunAcc) -> UsageRunView {
    let model = acc
        .models
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(m, _)| m.clone());
    UsageRunView {
        id: run.id,
        agent: run.agent,
        project_id: run.project_id,
        project: run.project.clone(),
        session_id: run.session_id,
        cwd: run.cwd.clone(),
        branch: run.branch.clone(),
        started_at: run.started_at,
        ended_at: run.ended_at,
        tokens: acc.tokens,
        turns: acc.turns,
        model,
        confidence: rank_confidence(acc.confidence),
    }
}

fn rank_confidence(rank: u8) -> Confidence {
    match rank {
        2 => Confidence::High,
        1 => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn conf_rank(c: Confidence) -> u8 {
    match c {
        Confidence::High => 2,
        Confidence::Medium => 1,
        Confidence::Low => 0,
    }
}

fn duration_ms(run: &AgentRun, now: i64) -> i64 {
    run.ended_at
        .unwrap_or(now)
        .saturating_sub(run.started_at)
        .max(0)
}

fn in_range(ts: i64, since: Option<i64>, until: Option<i64>) -> bool {
    since.is_none_or(|s| ts >= s) && until.is_none_or(|u| ts <= u)
}

pub fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn same_cwd(a: &str, b: &str) -> bool {
    normalize_path(a) == normalize_path(b)
}

fn data_dir() -> PathBuf {
    crate::services::persist::app_data_dir()
}

fn events_path(dir: &Path) -> PathBuf {
    dir.join("usage-events.jsonl")
}

fn runs_path(dir: &Path) -> PathBuf {
    dir.join("usage-runs.jsonl")
}

fn active_path(dir: &Path) -> PathBuf {
    dir.join("usage-active.json")
}

fn cursors_path(dir: &Path) -> PathBuf {
    dir.join("usage-cursors.json")
}

fn load_json_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

fn load_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn append_json_lines<T: Serialize>(path: &Path, items: &[T]) -> std::io::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for item in items {
        let line = serde_json::to_string(item).map_err(std::io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn write_json_lines<T: Serialize>(path: &Path, items: &[T]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    for item in items {
        let line = serde_json::to_string(item).map_err(std::io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(value).map_err(std::io::Error::other)?;
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_paths() {
        assert_eq!(normalize_path(r"D:\Agents\Kero\"), "d:/agents/kero");
        assert_eq!(normalize_path("D:/agents/kero"), "d:/agents/kero");
        assert!(same_cwd(r"D:\agents\kero", "d:/AGENTS/kero/"));
    }

    #[test]
    fn attributes_by_cwd_and_time() {
        let run = AgentRun {
            id: Uuid::new_v4(),
            agent: AgentKind::ClaudeCode,
            project_id: Uuid::new_v4(),
            project: "p".into(),
            session_id: Uuid::new_v4(),
            cwd: r"D:\work".into(),
            branch: None,
            started_at: 1_000,
            ended_at: Some(2_000),
            last_seen_at: 2_000,
        };
        let mut ev = UsageEvent {
            id: "e".into(),
            agent: AgentKind::ClaudeCode,
            ts_ms: 1_500,
            model: Some("m".into()),
            cwd: Some("d:/WORK".into()),
            session: None,
            tokens: TokenUsage {
                input: 10,
                output: 2,
                ..Default::default()
            },
            turns: 1,
            source: UsageSource::Native,
            coarse: false,
        };
        let runs = vec![&run];
        let attrs = attribute(std::slice::from_ref(&ev), &runs, 3_000);
        assert_eq!(attrs[0].1.map(|r| r.id), Some(run.id));
        assert_eq!(attrs[0].2, Confidence::High);
        ev.ts_ms = 200_000;
        let later = [ev];
        let attrs = attribute(&later, &runs, 201_000);
        assert!(attrs[0].1.is_none());
    }

    #[test]
    fn token_metrics_are_normalized() {
        let t = TokenUsage {
            input: 100,
            output: 20,
            reasoning: 5,
            cache_read: 300,
            cache_write: 0,
        };
        assert_eq!(t.total(), 425);
        assert!((t.cache_hit() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn tracks_runs_and_attributes_events() {
        let dir = std::env::temp_dir().join(format!("muster-usage-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = UsageStore::new_at(dir.clone());
        let session = Uuid::new_v4();
        let project = Uuid::new_v4();
        let start = now_ms() - 10_000;
        store.observe_run(RunObservation {
            session_id: session,
            agent: AgentKind::ClaudeCode,
            project_id: project,
            project: "p".into(),
            cwd: r"D:\work".into(),
            branch: Some("main".into()),
            now_ms: start,
        });
        store.push_test_event(UsageEvent {
            id: "e1".into(),
            agent: AgentKind::ClaudeCode,
            ts_ms: start + 500,
            model: Some("m".into()),
            cwd: Some("d:/WORK".into()),
            session: None,
            tokens: TokenUsage {
                input: 100,
                output: 10,
                ..Default::default()
            },
            turns: 1,
            source: UsageSource::Native,
            coarse: false,
        });

        let cmp = store.compare(None, None, None);
        assert_eq!(cmp.rows.len(), 1);
        assert_eq!(cmp.rows[0].runs, 1);
        assert!((cmp.rows[0].tokens_per_run - 110.0).abs() < 1e-9);
        assert!(cmp.rows[0].avg_duration_ms > 0.0);

        store.end_all_runs(start + 1_000);
        let runs = store.runs(None, None, None, 10);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].tokens.input, 100);
        assert_eq!(runs[0].ended_at, Some(start + 1_000));
        assert_eq!(runs[0].branch.as_deref(), Some("main"));

        // Reopening loads both the ledger and the completed run.
        drop(store);
        let reopened = UsageStore::new_at(dir.clone());
        let cmp = reopened.compare(None, None, None);
        assert_eq!(cmp.rows.len(), 1);
        assert_eq!(cmp.rows[0].runs, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reads the real agent logs on this machine. Ignored by default because
    /// it depends on the user's installed agents and writes to the dev app
    /// data ledger. Run manually: `cargo test --lib -- --ignored usage`.
    #[test]
    #[ignore = "reads real agent logs"]
    fn scans_real_sources() {
        let store = UsageStore::new();
        store.scan();
        let compare = store.compare(None, None, None);
        println!(
            "rows={} totals={:?} coverage={}",
            compare.rows.len(),
            compare.totals,
            compare.coverage.len()
        );
        for row in compare.rows.iter().take(20) {
            println!(
                "{} {:?} runs={} events={} tokens={} cache={:.2} per_run={:.0}",
                row.agent.label(),
                row.model,
                row.runs,
                row.events,
                row.total_tokens,
                row.cache_hit,
                row.tokens_per_run
            );
        }
        assert_eq!(compare.coverage.len(), AgentKind::ALL.len());
    }
}
