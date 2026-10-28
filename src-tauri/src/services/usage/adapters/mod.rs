use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::model::{UsageEvent, UsageSource};

pub mod claude;
pub mod codebuddy;
pub mod codex;
pub mod grok;
pub mod kimi;
pub mod opencode;
pub mod pi;
pub mod qoder;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileCursor {
    pub size: u64,
    pub mtime_ms: i64,
    /// Byte offset just past the last fully consumed line boundary. Append-only
    /// JSONL logs resume from here instead of re-parsing the whole file.
    /// A trailing partial line (writer mid-append) is intentionally not
    /// advanced past, so it is re-read once complete; event-id dedup makes
    /// that safe.
    #[serde(default)]
    pub offset: u64,
    /// Lines consumed so far. Fallback event ids embed the absolute line
    /// number, so this keeps ids stable across resumed reads.
    #[serde(default)]
    pub line: u64,
}

/// Where a resumed read of an append-only log should start.
#[derive(Debug, Clone, Copy, Default)]
pub struct Resume {
    pub byte: u64,
    pub line: u64,
}

pub struct ScanCtx {
    pub cursors: HashMap<String, FileCursor>,
    pub opencode_ts: i64,
    pub events: Vec<UsageEvent>,
    pub dirty: bool,
    min_ts_ms: i64,
    local_ids: HashSet<String>,
}

impl ScanCtx {
    pub fn new(cursors: HashMap<String, FileCursor>, opencode_ts: i64, min_ts_ms: i64) -> Self {
        Self {
            cursors,
            opencode_ts,
            events: Vec::new(),
            dirty: false,
            min_ts_ms,
            local_ids: HashSet::new(),
        }
    }

    /// True when `path` changed since the last scan (or is new). The cursor is
    /// updated even on parse failure, so a corrupt file is not retried on
    /// every tick. For stateful formats (pi/codex carry cross-line context)
    /// that must be fully re-parsed on every change.
    pub fn should_read(&mut self, path: &Path) -> bool {
        let Ok(meta) = fs::metadata(path) else {
            return false;
        };
        let size = meta.len();
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let key = path.to_string_lossy().to_string();
        let changed = self
            .cursors
            .get(&key)
            .map(|p| p.size != size || p.mtime_ms != mtime_ms)
            .unwrap_or(true);
        self.cursors.insert(key, FileCursor { size, mtime_ms, ..Default::default() });
        // Files untouched since the retention cutoff cannot contribute any
        // event we would keep.
        if mtime_ms < self.min_ts_ms {
            return false;
        }
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// Incremental variant of [`ScanCtx::should_read`] for append-only line
    /// logs: instead of a bool, returns where to resume. A file that only grew
    /// resumes at the previous line boundary; a shrunk file (rotated or
    /// truncated) restarts from the top. Returns `None` when unchanged.
    /// The cursor is updated immediately (so a failing open is not retried);
    /// call [`ScanCtx::advance`] after the read with the consumed position.
    pub fn resume_at(&mut self, path: &Path) -> Option<Resume> {
        let Ok(meta) = fs::metadata(path) else {
            return None;
        };
        let size = meta.len();
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let key = path.to_string_lossy().to_string();
        let prev = self.cursors.get(&key).copied();
        let changed = prev
            .map(|p| p.size != size || p.mtime_ms != mtime_ms)
            .unwrap_or(true);
        let resume = match prev {
            // Only pure growth can be resumed; anything else starts over.
            Some(p) if p.offset > 0 && p.offset <= p.size && p.size <= size => {
                Resume { byte: p.offset, line: p.line }
            }
            _ => Resume::default(),
        };
        self.cursors.insert(
            key,
            FileCursor { size, mtime_ms, offset: resume.byte, line: resume.line },
        );
        if mtime_ms < self.min_ts_ms {
            return None;
        }
        if changed {
            self.dirty = true;
        }
        changed.then_some(resume)
    }

    /// Record how far a resumed read consumed. `byte` must be a line boundary
    /// (as reported by [`TailLines::consumed`]).
    pub fn advance(&mut self, path: &Path, byte: u64, line: u64) {
        if let Some(c) = self.cursors.get_mut(&path.to_string_lossy().to_string()) {
            c.offset = byte;
            c.line = line;
        }
    }

    pub fn push(&mut self, event: UsageEvent) {
        if event.tokens.is_zero() {
            return;
        }
        if self.local_ids.insert(event.id.clone()) {
            self.events.push(event);
        }
    }

    pub fn push_simple(
        &mut self,
        id: String,
        agent: crate::services::agents::AgentKind,
        ts_ms: i64,
        model: Option<String>,
        cwd: Option<String>,
        session: Option<String>,
        tokens: super::model::TokenUsage,
        turns: u32,
        coarse: bool,
    ) {
        self.push(UsageEvent {
            id,
            agent,
            ts_ms,
            model: clean_model(model),
            cwd: clean_opt(cwd),
            session: clean_opt(session),
            tokens,
            turns,
            source: UsageSource::Native,
            coarse,
        });
    }
}

pub fn clean_opt(v: Option<String>) -> Option<String> {
    v.map(|s| {
        s.trim()
            .trim_matches(|c| c == '"' || c == '\'' || c == '\\')
            .trim()
            .to_string()
    })
    .filter(|s| !s.is_empty())
}

pub fn clean_model(v: Option<String>) -> Option<String> {
    clean_opt(v).filter(|s| s != "<synthetic>" && s != "synthetic")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

pub fn file_mtime_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn num(v: Option<&serde_json::Value>) -> u64 {
    v.and_then(|x| x.as_u64()).unwrap_or(0)
}

pub fn num_at(v: &serde_json::Value, key: &str) -> u64 {
    num(v.get(key))
}

pub fn walk_files(root: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    fn rec(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
        if depth > max_depth {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                rec(&path, depth + 1, max_depth, out);
            } else if kind.is_file() {
                out.push(path);
            }
        }
    }
    rec(root, 0, max_depth, out);
    out.sort();
}

pub fn event_id(prefix: &str, value: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    format!("{prefix}:{:016x}", h.finish())
}

/// Line reader that yields `(absolute_line_index, line)` starting from a byte
/// offset, and tracks the position just past the last newline-terminated line.
/// A trailing partial line is yielded but not consumed: the next scan re-reads
/// it once the writer has completed it (event-id dedup absorbs the overlap).
///
/// The reader must already be positioned at `start_byte` (see [`open_tail`]);
/// this type only tracks counters.
pub struct TailLines<R: BufRead> {
    reader: R,
    next_line: u64,
    pos: u64,
    consumed: u64,
}

impl<R: BufRead> TailLines<R> {
    pub fn new(reader: R, start_byte: u64, start_line: u64) -> Self {
        Self { reader, next_line: start_line, pos: start_byte, consumed: start_byte }
    }

    /// Byte offset just past the last complete line; write this back via
    /// [`ScanCtx::advance`].
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// Absolute index the next yielded line will have (== lines consumed).
    pub fn next_line(&self) -> u64 {
        self.next_line
    }
}

impl<R: BufRead> Iterator for TailLines<R> {
    type Item = (u64, std::io::Result<String>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = Vec::new();
        let n = match self.reader.read_until(b'\n', &mut buf) {
            Ok(0) => return None,
            Ok(n) => n,
            Err(e) => {
                let idx = self.next_line;
                self.next_line += 1;
                return Some((idx, Err(e)));
            }
        };
        let idx = self.next_line;
        self.next_line += 1;
        self.pos += n as u64;
        let terminated = buf.last() == Some(&b'\n');
        if terminated {
            self.consumed = self.pos;
        }
        let mut end = buf.len();
        if terminated {
            end -= 1;
            if end > 0 && buf[end - 1] == b'\r' {
                end -= 1;
            }
        }
        Some((idx, Ok(String::from_utf8_lossy(&buf[..end]).into_owned())))
    }
}

/// Open `path` positioned at `resume.byte` and wrap it in [`TailLines`].
/// Mirrors the old `BufReader::lines()` loop when `resume` starts at 0, with
/// `\r\n` handling; invalid UTF-8 degrades to lossy (the JSON parse of such a
/// line fails either way).
pub fn open_tail(path: &Path, resume: Resume) -> Option<TailLines<std::io::BufReader<File>>> {
    use std::io::{Seek, SeekFrom};
    let mut file = File::open(path).ok()?;
    if resume.byte > 0 && file.seek(SeekFrom::Start(resume.byte)).is_err() {
        return None;
    }
    Some(TailLines::new(std::io::BufReader::new(file), resume.byte, resume.line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_lines(input: &str, start_byte: u64, start_line: u64) -> (Vec<(u64, String)>, u64, u64) {
        use std::io::{Seek, SeekFrom};
        let mut reader = std::io::Cursor::new(input.as_bytes());
        if start_byte > 0 {
            reader.seek(SeekFrom::Start(start_byte)).unwrap();
        }
        let mut tail = TailLines::new(reader, start_byte, start_line);
        let mut lines = Vec::new();
        for (idx, line) in tail.by_ref() {
            // Readers used by adapters skip unreadable lines, tests only feed
            // valid UTF-8.
            lines.push((idx, line.unwrap()));
        }
        (lines, tail.consumed(), tail.next_line())
    }

    #[test]
    fn tail_reads_all_lines_with_absolute_indexes() {
        let (lines, consumed, next) = cursor_lines("{\"a\":1}\n{\"a\":2}\n", 0, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (0, "{\"a\":1}".to_string()));
        assert_eq!(lines[1], (1, "{\"a\":2}".to_string()));
        assert_eq!(consumed, 16);
        assert_eq!(next, 2);
    }

    #[test]
    fn tail_resumes_and_keeps_line_numbers() {
        let full = "{\"a\":1}\n{\"a\":2}\n";
        // Resume mid-file at byte 8 (start of the second line), line base 1.
        let (lines, consumed, next) = cursor_lines(full, 8, 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (1, "{\"a\":2}".to_string()));
        assert_eq!(consumed, 16);
        assert_eq!(next, 2);
    }

    #[test]
    fn trailing_partial_line_is_yielded_but_not_consumed() {
        let full = "{\"a\":1}\n{\"a\":2}";
        let (lines, consumed, next) = cursor_lines(full, 0, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], (1, "{\"a\":2}".to_string()));
        // Consumed stops after the first newline; the partial second line is
        // re-read next scan.
        assert_eq!(consumed, 8);
        assert_eq!(next, 2);
        // Simulate the next scan after the writer completes the line.
        let full2 = "{\"a\":1}\n{\"a\":2}\n";
        let (lines2, consumed2, _) = cursor_lines(full2, consumed, 2 - 1);
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], (1, "{\"a\":2}".to_string()));
        assert_eq!(consumed2, 16);
    }

    #[test]
    fn resume_at_detects_growth_and_rotation() {
        let dir = std::env::temp_dir().join(format!("muster-cursor-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("t.jsonl");
        fs::write(&path, "{\"a\":1}\n").unwrap();
        let mut ctx = ScanCtx::new(HashMap::new(), 0, 0);
        // First sight: full read.
        let r = ctx.resume_at(&path).unwrap();
        assert_eq!((r.byte, r.line), (0, 0));
        ctx.advance(&path, 8, 1);
        // Append: resume from the boundary.
        fs::write(&path, "{\"a\":1}\n{\"a\":2}\n").unwrap();
        let r2 = ctx.resume_at(&path).unwrap();
        assert_eq!((r2.byte, r2.line), (8, 1));
        // Unchanged: skip.
        assert!(ctx.resume_at(&path).is_none());
        // Rotation (shrink): start over.
        fs::write(&path, "{\"b\":1}\n").unwrap();
        let r3 = ctx.resume_at(&path).unwrap();
        assert_eq!((r3.byte, r3.line), (0, 0));
        let _ = fs::remove_dir_all(&dir);
    }
}

pub fn scan_all(ctx: &mut ScanCtx) {
    // Each adapter is independent: one missing/corrupt source must not stop
    // the others. Errors are handled inside the adapters.
    claude::scan(ctx);
    qoder::scan(ctx);
    kimi::scan(ctx);
    pi::scan(ctx);
    opencode::scan(ctx);
    grok::scan(ctx);
    codebuddy::scan(ctx);
    codex::scan(ctx);
}
