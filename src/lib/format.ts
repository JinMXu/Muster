/// Compact token-count formatting shared by the usage views (1.2K / 3.4M).
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return (n / 1000).toFixed(1).replace(/\.0$/, "") + "K";
  return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
}
