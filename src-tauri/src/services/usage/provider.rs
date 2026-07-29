use std::path::PathBuf;
use std::time::SystemTime;

use super::model::{ToolKind, UsageSession};

/// A file or DB record discovered on disk that may yield a UsageSession.
#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    /// Tool-specific key: file path for JSONL tools, session id for OpenCode.
    pub key: String,
    /// File path (for mtime tracking); for OpenCode this is the db path.
    pub path: PathBuf,
    /// mtime at discovery time, for incremental skip.
    pub mtime: Option<SystemTime>,
    /// File size at discovery time (extra change signal).
    pub size: Option<u64>,
}

/// Each tool implements this. Implementations must be cheap to clone (they
/// hold no state beyond config paths).
pub trait UsageProvider: Send + Sync {
    fn tool_kind(&self) -> ToolKind;
    /// Find all candidate session sources on disk. Returns empty vec if the
    /// tool is not installed (not an error).
    fn discover(&self) -> Vec<DiscoveredSession>;
    /// Parse one discovered source into a UsageSession. Returns None on any
    /// parse error (the scanner logs and skips).
    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession>;
}

/// Resolve the user home directory. Returns None if it cannot be determined.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Read an env var as a PathBuf, falling back to `default` if unset/empty.
pub fn env_path_or(var: &str, default: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(default)
}
