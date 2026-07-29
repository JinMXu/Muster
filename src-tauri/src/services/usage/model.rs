use serde::{Deserialize, Serialize};

/// Which CLI tool a usage record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Opencode,
    ClaudeCode,
    Codex,
    KimiCode,
}

/// Normalized token breakdown shared across all four tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Non-cached input tokens.
    pub input: u64,
    /// Completion / output tokens.
    pub output: u64,
    /// Reasoning tokens (some models only).
    pub reasoning: u64,
    /// Cache-hit reads.
    pub cache_read: u64,
    /// Tokens written to cache.
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
}

/// One session's usage, the unit of the sessions list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSession {
    pub tool: ToolKind,
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub model: String,
    /// epoch ms
    pub started_at: i64,
    /// epoch ms
    pub updated_at: i64,
    pub tokens: TokenUsage,
    /// USD cost; only OpenCode stores this.
    pub cost_usd: Option<f64>,
}

/// Per-tool aggregate for the summary cards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    pub tool: ToolKind,
    pub total_tokens: u64,
    pub tokens: TokenUsage,
    pub session_count: usize,
    /// USD cost; only OpenCode.
    pub cost_usd: Option<f64>,
    /// epoch ms of the most recent session for this tool.
    pub last_updated: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    pub tools: Vec<ToolSummary>,
}

impl UsageSummary {
    pub fn from_sessions(sessions: &[UsageSession]) -> Self {
        use std::collections::HashMap;
        let mut buckets: HashMap<ToolKind, ToolSummary> = HashMap::new();
        for s in sessions {
            let entry = buckets.entry(s.tool).or_insert_with(|| ToolSummary {
                tool: s.tool,
                total_tokens: 0,
                tokens: TokenUsage::default(),
                session_count: 0,
                cost_usd: None,
                last_updated: 0,
            });
            entry.total_tokens = entry.total_tokens.saturating_add(s.tokens.total());
            entry.tokens.add(&s.tokens);
            entry.session_count += 1;
            if s.cost_usd.is_some() {
                entry.cost_usd = Some(entry.cost_usd.unwrap_or(0.0) + s.cost_usd.unwrap_or(0.0));
            }
            if s.updated_at > entry.last_updated {
                entry.last_updated = s.updated_at;
            }
        }
        // Stable order: OpenCode, ClaudeCode, Codex, KimiCode
        let order = [
            ToolKind::Opencode,
            ToolKind::ClaudeCode,
            ToolKind::Codex,
            ToolKind::KimiCode,
        ];
        let tools = order
            .into_iter()
            .filter_map(|t| buckets.remove(&t))
            .collect();
        Self { tools }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(tool: ToolKind, tokens: TokenUsage, cost: Option<f64>, updated: i64) -> UsageSession {
        UsageSession {
            tool,
            session_id: format!("s-{}", updated),
            title: "t".into(),
            cwd: "/".into(),
            model: "m".into(),
            started_at: updated - 1000,
            updated_at: updated,
            tokens,
            cost_usd: cost,
        }
    }

    #[test]
    fn total_sums_all_fields() {
        let t = TokenUsage { input: 10, output: 20, reasoning: 5, cache_read: 100, cache_write: 50 };
        assert_eq!(t.total(), 185);
    }

    #[test]
    fn summary_aggregates_per_tool() {
        let sessions = vec![
            sess(ToolKind::Opencode, TokenUsage { input: 100, output: 50, ..Default::default() }, Some(0.5), 1000),
            sess(ToolKind::Opencode, TokenUsage { input: 200, output: 50, ..Default::default() }, Some(1.5), 2000),
            sess(ToolKind::ClaudeCode, TokenUsage { input: 300, cache_read: 50, ..Default::default() }, None, 1500),
        ];
        let s = UsageSummary::from_sessions(&sessions);
        assert_eq!(s.tools.len(), 2);
        let oc = s.tools.iter().find(|t| t.tool == ToolKind::Opencode).unwrap();
        assert_eq!(oc.session_count, 2);
        assert_eq!(oc.total_tokens, 400); // 100+50 + 200+50
        assert_eq!(oc.cost_usd, Some(2.0));
        assert_eq!(oc.last_updated, 2000);
        let cc = s.tools.iter().find(|t| t.tool == ToolKind::ClaudeCode).unwrap();
        assert_eq!(cc.total_tokens, 350); // 300 + 50
        assert!(cc.cost_usd.is_none());
    }

    #[test]
    fn summary_stable_order() {
        let sessions = vec![
            sess(ToolKind::KimiCode, TokenUsage::default(), None, 1),
            sess(ToolKind::Codex, TokenUsage::default(), None, 2),
            sess(ToolKind::ClaudeCode, TokenUsage::default(), None, 3),
            sess(ToolKind::Opencode, TokenUsage::default(), None, 4),
        ];
        let s = UsageSummary::from_sessions(&sessions);
        assert_eq!(s.tools[0].tool, ToolKind::Opencode);
        assert_eq!(s.tools[1].tool, ToolKind::ClaudeCode);
        assert_eq!(s.tools[2].tool, ToolKind::Codex);
        assert_eq!(s.tools[3].tool, ToolKind::KimiCode);
    }
}
