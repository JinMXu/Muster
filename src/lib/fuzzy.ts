/// Case-insensitive subsequence fuzzy match, mirroring the app's own scorer:
/// word-boundary hits and consecutive matches score higher.
export function fuzzyScore(candidate: string, pattern: string): number | null {
  const chars = Array.from(candidate.toLowerCase());
  const pat = Array.from(pattern.toLowerCase());
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

export function fuzzyFilter<T>(items: T[], pattern: string, getText: (item: T) => string): T[] {
  const trimmed = pattern.trim();
  if (!trimmed) return items;
  return items
    .map((item) => ({ item, score: fuzzyScore(getText(item), trimmed) }))
    .filter((rec): rec is { item: T; score: number } => rec.score !== null)
    .sort((a, b) => b.score - a.score)
    .map((r) => r.item);
}