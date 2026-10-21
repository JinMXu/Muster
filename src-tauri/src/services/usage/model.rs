use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::agents::AgentKind;

/// Normalized token counters. `input` is always *fresh* (non-cached) input;
/// cached and cache-write tokens are tracked separately so agents that report
/// a combined input number stay comparable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.reasoning + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.reasoning += other.reasoning;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    pub fn is_zero(&self) -> bool {
        self.total() == 0
    }

    /// Cache reads as a share of all input-ish tokens.
    pub fn cache_hit(&self) -> f64 {
        let denom = self.input + self.cache_read + self.cache_write;
        if denom == 0 {
            0.0
        } else {
            self.cache_read as f64 / denom as f64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    Native,
}

/// One normalized usage observation. `id` is stable across rescans and is the
/// dedup key for the append-only ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub id: String,
    pub agent: AgentKind,
    pub ts_ms: i64,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub session: Option<String>,
    pub tokens: TokenUsage,
    pub turns: u32,
    pub source: UsageSource,
    /// The source only gives a file-level timestamp (e.g. CodeBuddy), so
    /// run attribution is inherently coarse.
    #[serde(default)]
    pub coarse: bool,
}

/// One coding-agent process lifetime inside a Muster session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: Uuid,
    pub agent: AgentKind,
    pub project_id: Uuid,
    pub project: String,
    pub session_id: Uuid,
    pub cwd: String,
    pub branch: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// Updated every poll and persisted with the active-run checkpoint. A
    /// crashed session's run is closed at this time on the next launch.
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UsageRunView {
    pub id: Uuid,
    pub agent: AgentKind,
    pub project_id: Uuid,
    pub project: String,
    pub session_id: Uuid,
    pub cwd: String,
    pub branch: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub tokens: TokenUsage,
    pub turns: u32,
    pub model: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct UsageTotals {
    pub tokens: TokenUsage,
    pub turns: u64,
    pub runs: u64,
    pub events: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UsageOverview {
    pub updated_at: i64,
    pub since_ms: i64,
    pub today: UsageTotals,
    pub active_runs: Vec<UsageRunView>,
    pub coverage: Vec<AgentCoverage>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AgentCoverage {
    pub agent: AgentKind,
    pub has_adapter: bool,
    pub events: u64,
    pub runs: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UsageCompareRow {
    pub agent: AgentKind,
    pub model: Option<String>,
    pub runs: u64,
    pub events: u64,
    pub tokens: TokenUsage,
    pub total_tokens: u64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_per_run: f64,
    #[specta(type = specta_typescript::Number)]
    pub fresh_input_per_run: f64,
    #[specta(type = specta_typescript::Number)]
    pub cache_hit: f64,
    #[specta(type = specta_typescript::Number)]
    pub tokens_per_turn: f64,
    #[specta(type = specta_typescript::Number)]
    pub output_per_turn: f64,
    #[specta(type = specta_typescript::Number)]
    pub avg_duration_ms: f64,
    #[specta(type = specta_typescript::Number)]
    pub attributed_ratio: f64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UsageCompare {
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub project_id: Option<Uuid>,
    pub rows: Vec<UsageCompareRow>,
    pub totals: UsageTotals,
    pub coverage: Vec<AgentCoverage>,
    pub updated_at: i64,
}
