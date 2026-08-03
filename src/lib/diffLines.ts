/// Line-diff between two texts, producing 1-based line markers in the NEW
/// text for the editor gutter: lines that were added, and anchors for removed
/// lines (the line that follows a deletion run). Used by the FilePane inline
/// diff, computed on the live buffer so unsaved edits stay accurate.

export interface LineDiff {
  added: number[];
  removed: number[];
}

/// Products above this skip the exact DP and fall back to a coarse scan
/// (a huge minified file is still marked, just less precisely).
const MAX_DP_CELLS = 4_000_000;

export function diffLines(oldText: string, newText: string): LineDiff {
  const oldLines = oldText.split("\n");
  const newLines = newText.split("\n");
  // split() yields a trailing "" for a final newline; drop it so counts match
  // the editor's visible line numbers.
  if (oldLines[oldLines.length - 1] === "") oldLines.pop();
  if (newLines[newLines.length - 1] === "") newLines.pop();

  const added: number[] = [];
  const removed: number[] = [];
  const n = oldLines.length;
  const m = newLines.length;

  if (n === 0 && m === 0) return { added, removed };
  if (n === 0) {
    for (let j = 1; j <= m; j++) added.push(j);
    return { added, removed };
  }
  if (m === 0) {
    for (let i = 1; i <= n; i++) removed.push(1);
    return { added, removed };
  }
  if (n * m > MAX_DP_CELLS) {
    const len = Math.min(n, m);
    for (let i = 0; i < len; i++) {
      if (oldLines[i] !== newLines[i]) {
        added.push(i + 1);
        removed.push(i + 1);
      }
    }
    for (let j = len; j < m; j++) added.push(j + 1);
    for (let i = len; i < n; i++) removed.push(len + 1);
    return finalize(added, removed, m);
  }

  // Classic LCS DP (lengths), then backtrack for the edit script.
  const width = m + 1;
  const dp = new Int32Array((n + 1) * width);
  for (let i = 1; i <= n; i++) {
    const row = dp.subarray(i * width);
    const prev = dp.subarray((i - 1) * width);
    const oi = oldLines[i - 1];
    for (let j = 1; j <= m; j++) {
      row[j] = oi === newLines[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], row[j - 1]);
    }
  }

  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (oldLines[i - 1] === newLines[j - 1]) {
      i--;
      j--;
    } else if (dp[(i - 1) * width + j] >= dp[i * width + j - 1]) {
      // Deletion of old[i-1]: anchor at the current new-line position.
      removed.push(j || 1);
      i--;
    } else {
      added.push(j);
      j--;
    }
  }
  while (i > 0) {
    removed.push(j || 1);
    i--;
  }
  while (j > 0) {
    added.push(j);
    j--;
  }
  return finalize(added, removed, m);
}

function finalize(added: number[], removed: number[], newLineCount: number): LineDiff {
  // Dedupe and clamp removed anchors into valid line range.
  const seen = new Set<number>();
  const uniq: number[] = [];
  for (const r of removed) {
    const clamped = Math.max(1, Math.min(r, Math.max(newLineCount, 1)));
    if (!seen.has(clamped)) {
      seen.add(clamped);
      uniq.push(clamped);
    }
  }
  return { added, removed: uniq };
}
