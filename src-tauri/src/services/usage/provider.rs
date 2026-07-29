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

/// Parse an ISO-8601 timestamp like "2025-08-20T19:42:30.851Z" to epoch ms.
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    // Cheap parse: "YYYY-MM-DDTHH:MM:SS.mmmZ"
    let s = s.trim();
    if s.len() < 20 { return None; }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let h: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let se: i64 = s.get(17..19)?.parse().ok()?;
    let ms: i64 = s.get(20..23).unwrap_or("0").parse().ok()?;
    let days = days_from_civil(y, mo, d);
    Some(((days * 86400 + h * 3600 + mi * 60 + se) * 1000) + ms)
}

/// Howard Hinnant's days_from_civil (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}
