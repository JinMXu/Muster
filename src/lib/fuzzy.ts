/// Case-insensitive subsequence fuzzy match, mirroring the app's own scorer:
/// word-boundary hits and consecutive matches score higher.
///
/// Candidates are prepared once (`prepareFuzzyText`) so scoring a large list
/// on every keystroke doesn't re-lowercase and re-split each title.
export function prepareFuzzyText(text: string): string[] {
  return Array.from(text.toLowerCase());
}

export function fuzzyScore(chars: readonly string[], pat: readonly string[]): number | null {
  let score = 0;
  let index = 0;
  let lastMatch = -1;
  for (const ch of pat) {
    let found = false;
    while (index < chars.length) {
      if (chars[index] === ch) {
        if (index === 0 || chars[index - 1] === " ") {
          score += 10;
        } else if (index === lastMatch + 1) {
          score += 5;
        } else {
          score += 1;
        }
        lastMatch = index;
        index += 1;
        found = true;
        break;
      }
      index += 1;
    }
    if (!found) return null;
  }
  return score;
}
