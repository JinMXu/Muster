//! Full-text search over a project directory for the Search panel.
//!
//! Walks the tree with the `ignore` crate (respects `.gitignore` / `.ignore`
//! so `node_modules` and other huge ignored dirs are skipped even outside a
//! git repo), reads text files up to a size cap, and reports every line that
//! contains the query. Matching is case-insensitive by default, char-based
//! (so byte vs. char offsets never drift for CJK), and limited in results so
//! pathological projects can't hang the panel.

use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cap on individual file size: minified bundles and lock files larger than
/// this are skipped entirely.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Hard cap on total matches returned for one query.
const MAX_RESULTS: usize = 500;
/// Lines are clipped to this many chars around the first match for display.
const MAX_LINE_CHARS: usize = 300;
/// Directories skipped regardless of gitignore (worst offenders when a
/// project has no ignore file). `.git` is always skipped by the walker.
const SKIP_DIRS: [&str; 3] = ["node_modules", "target", "__pycache__"];
/// Hard cap on the number of file paths returned for the quick-open list.
const MAX_LISTED_FILES: usize = 2000;

/// One matching line in one file (offsets are char-based into `line_text`).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SearchMatch {
    pub path: String,
    pub rel_path: String,
    pub line: u32,
    pub column: u32,
    pub line_text: String,
    pub match_start: usize,
    pub match_len: usize,
}

/// List every file under `root` as a repo-relative path (with `/`
/// separators), respecting `.gitignore`/`.ignore` and skipping the usual
/// heavy directories. Used by the Ctrl+P quick-open list; capped so
/// pathological projects can't hang the palette.
pub fn list_project_files(root: &str) -> Vec<String> {
    let root = Path::new(root);
    if !root.is_dir() {
        return Vec::new();
    }
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .ignore(true)
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .build();
    let mut files = Vec::new();
    for entry in walker {
        if files.len() >= MAX_LISTED_FILES {
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| entry.path().to_string_lossy().replace('\\', "/"));
        files.push(rel);
    }
    files.sort();
    files
}

/// Returned (as an error) when a newer search for the same window cancelled
/// this one. The frontend ignores stale responses by request sequence, so the
/// message is only for logs.
pub const SEARCH_CANCELLED: &str = "search cancelled";

/// Search `query` under `root`, returning matches capped at `MAX_RESULTS`.
pub fn search_in_project(root: &str, query: &str, case_sensitive: bool) -> Result<Vec<SearchMatch>, String> {
    search_in_project_cancellable(root, query, case_sensitive, &AtomicBool::new(false))
}

/// Same as `search_in_project`, but stops early when `cancel` is set. Each
/// keystroke (after the frontend debounce) supersedes the previous scan; the
/// command layer flips the old flag so stale scans don't keep burning a
/// blocking-pool thread over a large repository.
pub fn search_in_project_cancellable(
    root: &str,
    query: &str,
    case_sensitive: bool,
    cancel: &AtomicBool,
) -> Result<Vec<SearchMatch>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let root = Path::new(root);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(SEARCH_CANCELLED.to_string());
    }

    let walker = ignore::WalkBuilder::new(root)
        // Include hidden files (.env, .github, ...) but keep `.git` excluded.
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .ignore(true)
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .build();

    let mut results = Vec::new();
    for entry in walker {
        if results.len() >= MAX_RESULTS {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SEARCH_CANCELLED.to_string());
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.len() as u64 > MAX_FILE_BYTES || looks_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));
        for (i, line) in text.split('\n').enumerate() {
            // Poll per line block instead of per line: cheap enough to abort
            // a huge file promptly, without an atomic load per line.
            if i & 63 == 0 && cancel.load(Ordering::Relaxed) {
                return Err(SEARCH_CANCELLED.to_string());
            }
            for (start, len) in find_occurrences(line, query, case_sensitive) {
                if results.len() >= MAX_RESULTS {
                    break;
                }
                let (line_text, match_start) = clip_line(line, start, len);
                results.push(SearchMatch {
                    path: path.to_string_lossy().to_string(),
                    rel_path: rel.clone(),
                    line: (i + 1) as u32,
                    column: (start + 1) as u32,
                    line_text,
                    match_start,
                    match_len: len,
                });
            }
            if results.len() >= MAX_RESULTS {
                break;
            }
        }
    }
    Ok(results)
}

/// Heuristic: a file with a NUL byte in its first 8 KiB is binary.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

/// Find all case-appropriate occurrences of `needle` in `line`. Returns
/// (char_start, char_len) pairs. Char-based throughout so the offsets never
/// diverge from the char indices the frontend highlights with.
///
/// This runs for every line of every file on each (debounced) keystroke, so
/// the common ASCII case-insensitive path avoids allocating a lowercased
/// copy of the line; only non-ASCII needles pay for the Unicode fallback.
fn find_occurrences(line: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        return find_substring(line, needle);
    }
    if needle.is_ascii() {
        return find_ascii_case_insensitive(line, needle);
    }
    // Unicode case-insensitive fallback: lowercase both sides once.
    find_substring(&line.to_lowercase(), &needle.to_lowercase())
}

/// Substring scan returning char offsets, advancing one char at a time so
/// overlapping occurrences are still reported.
fn find_substring(hay: &str, pat: &str) -> Vec<(usize, usize)> {
    let pat_chars = pat.chars().count();
    let mut out = Vec::new();
    let mut from = 0usize;
    // Char offset of `from`, advanced incrementally so each match only counts
    // the chars between it and the previous match.
    let mut base_chars = 0usize;
    let mut base_byte = 0usize;
    while from <= hay.len() {
        let Some(rel) = hay[from..].find(pat) else { break };
        let byte = from + rel;
        let char_start = base_chars + hay[base_byte..byte].chars().count();
        out.push((char_start, pat_chars));
        if out.len() >= MAX_RESULTS {
            break;
        }
        base_chars = char_start;
        base_byte = byte;
        let step = hay[byte..].chars().next().map(char::len_utf8).unwrap_or(1);
        from = byte + step;
    }
    out
}

/// ASCII-only case-insensitive scan on the original line. ASCII bytes can't
/// appear inside a UTF-8 multi-byte sequence, so a byte offset found here is
/// always a char boundary; char offsets for the prefix are counted only for
/// actual matches (there are at most `MAX_RESULTS`).
fn find_ascii_case_insensitive(line: &str, needle: &str) -> Vec<(usize, usize)> {
    let hay = line.as_bytes();
    let pat = needle.as_bytes();
    if pat.is_empty() || pat.len() > hay.len() {
        return Vec::new();
    }
    let lower = pat[0].to_ascii_lowercase();
    let upper = pat[0].to_ascii_uppercase();
    let mut out = Vec::new();
    let mut base_chars = 0usize;
    let mut base_byte = 0usize;
    let mut byte = 0usize;
    while byte + pat.len() <= hay.len() {
        let first = hay[byte];
        if first == lower || first == upper {
            if hay[byte..byte + pat.len()].eq_ignore_ascii_case(pat) {
                let char_start = base_chars + line[base_byte..byte].chars().count();
                out.push((char_start, pat.len()));
                if out.len() >= MAX_RESULTS {
                    break;
                }
                base_chars = char_start;
                base_byte = byte;
            }
        }
        byte += 1;
    }
    out
}

/// Clip `line` to `MAX_LINE_CHARS` chars around the match at `start`, so a
/// single minified line doesn't blow up the results list. Returns the clipped
/// string (with `…` markers where text was cut) and the match's char offset
/// inside it.
fn clip_line(line: &str, start: usize, len: usize) -> (String, usize) {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    if n <= MAX_LINE_CHARS {
        return (line.to_string(), start);
    }
    // The window must always cover the whole match; oversized matches just
    // show their head.
    let need = len.min(MAX_LINE_CHARS);
    let half = (MAX_LINE_CHARS - need) / 2;
    let mut lo = start.saturating_sub(half);
    let max_lo = n.saturating_sub(MAX_LINE_CHARS);
    if lo > max_lo {
        lo = max_lo;
    }
    let hi = (lo + MAX_LINE_CHARS).min(n);
    let mut out = String::new();
    if lo > 0 {
        out.push('…');
    }
    out.extend(&chars[lo..hi]);
    if hi < n {
        out.push('…');
    }
    let new_start = start - lo + if lo > 0 { 1 } else { 0 };
    (out, new_start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut f = std::fs::File::create(p).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn list_files_returns_relative_paths_and_respects_ignore() {
        let dir = write_project(&[
            (".gitignore", "node_modules/\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("src/lib/mod.rs", "pub fn x() {}\n"),
            ("node_modules/pkg/index.js", "x"),
        ]);
        let root = dir.path().to_string_lossy().to_string();
        let files = list_project_files(&root);
        assert_eq!(
            files,
            vec![".gitignore".to_string(), "src/lib/mod.rs".to_string(), "src/main.rs".to_string()],
            "gitignore'd and binary dirs skipped, sorted"
        );
    }

    #[test]
    fn list_files_nonexistent_root_is_empty() {
        assert!(list_project_files("Z:\\definitely\\missing").is_empty());
    }

    #[test]
    fn finds_basic_matches_with_line_and_column() {
        let dir = write_project(&[(
            "src/main.rs",
            "fn main() {\n    let answer = 42;\n    println!(\"answer\");\n}\n",
        )]);
        let root = dir.path().to_string_lossy().to_string();

        let res = search_in_project(&root, "answer", false).unwrap();
        assert_eq!(res.len(), 2, "both occurrences reported");
        assert_eq!(res[0].rel_path, "src/main.rs");
        assert_eq!(res[0].line, 2);
        assert_eq!(res[0].column, 9);
        assert!(res[0].line_text.contains("let answer = 42;"));
        assert_eq!(char_slice(&res[0].line_text, res[0].match_start, res[0].match_len), "answer");
    }

    #[test]
    fn case_insensitive_by_default_case_sensitive_opt_in() {
        let dir = write_project(&[("a.txt", "Hello\nhello\n")]);
        let root = dir.path().to_string_lossy().to_string();

        assert_eq!(search_in_project(&root, "hello", false).unwrap().len(), 2);
        assert_eq!(search_in_project(&root, "HELLO", false).unwrap().len(), 2);
        assert_eq!(search_in_project(&root, "hello", true).unwrap().len(), 1);
        assert_eq!(search_in_project(&root, "HELLO", true).unwrap().len(), 0);
    }

    #[test]
    fn ascii_case_insensitive_keeps_char_columns_after_cjk() {
        let dir = write_project(&[("a.txt", "中文 Alpha beta\n")]);
        let root = dir.path().to_string_lossy().to_string();
        let res = search_in_project(&root, "alpha", false).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].column, 4, "three chars precede the match");
        assert_eq!(char_slice(&res[0].line_text, res[0].match_start, res[0].match_len), "Alpha");
    }

    #[test]
    fn cancelled_search_returns_before_scanning() {
        let dir = write_project(&[("a.txt", "needle")]);
        let root = dir.path().to_string_lossy().to_string();
        let cancel = AtomicBool::new(true);
        let err = search_in_project_cancellable(&root, "needle", false, &cancel).unwrap_err();
        assert_eq!(err, SEARCH_CANCELLED);
    }

    #[test]
    fn reports_overlapping_matches_with_char_columns() {
        let dir = write_project(&[("a.txt", "aaaa\n")]);
        let root = dir.path().to_string_lossy().to_string();
        let res = search_in_project(&root, "aa", false).unwrap();
        let columns: Vec<u32> = res.iter().map(|m| m.column).collect();
        assert_eq!(columns, vec![1, 2, 3]);
    }

    #[test]
    fn respects_gitignore() {
        let dir = write_project(&[(".gitignore", "node_modules/\n"), ("keep.txt", "needle"), ("node_modules/pkg/index.js", "needle")]);
        let root = dir.path().to_string_lossy().to_string();

        let res = search_in_project(&root, "needle", false).unwrap();
        assert_eq!(res.len(), 1, "ignored dir is skipped");
        assert_eq!(res[0].rel_path, "keep.txt");
    }

    #[test]
    fn skips_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bin.dat");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&[0x00, 0x01, 0x02, 0x00, b'n', b'e']).unwrap();
        let root = dir.path().to_string_lossy().to_string();

        assert!(search_in_project(&root, "ne", false).unwrap().is_empty());
    }

    #[test]
    fn empty_query_returns_nothing() {
        let dir = write_project(&[("a.txt", "needle")]);
        let root = dir.path().to_string_lossy().to_string();
        assert!(search_in_project(&root, "   ", false).unwrap().is_empty());
        assert!(search_in_project(&root, "", true).unwrap().is_empty());
    }

    #[test]
    fn nonexistent_root_is_empty() {
        assert!(search_in_project("Z:\\definitely\\missing", "x", false).unwrap().is_empty());
    }

    #[test]
    fn clip_keeps_match_visible() {
        let long = format!("{}needle{}", "a".repeat(400), "b".repeat(400));
        let (clipped, start) = clip_line(&long, 400, 6);
        assert!(clipped.contains('…'));
        assert!(clipped.chars().count() <= MAX_LINE_CHARS + 2, "only ellipsis overhead");
        assert_eq!(char_slice(&clipped, start, 6), "needle");
    }

    #[test]
    fn cjk_offsets_are_char_based() {
        let dir = write_project(&[("中文.txt", "你好世界\n这是测试\n")]);
        let root = dir.path().to_string_lossy().to_string();

        let res = search_in_project(&root, "世界", false).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].line, 1);
        assert_eq!(res[0].column, 3, "third CJK char");
        assert_eq!(char_slice(&res[0].line_text, res[0].match_start, res[0].match_len), "世界");
    }

    /// Char-index slice of a string (offsets from SearchMatch are char-based).
    fn char_slice(s: &str, start: usize, len: usize) -> String {
        s.chars().skip(start).take(len).collect()
    }
}
