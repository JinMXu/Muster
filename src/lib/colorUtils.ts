/// Mix two 6-digit hex colors (no '#'); `amount` = how far toward `target`.
export function mixHex(hex: string, target: string, amount: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  const mix = (a: number, b: number) =>
    Math.round(a + (b - a) * amount)
      .toString(16)
      .padStart(2, "0");
  return [0, 2, 4].map((i) => mix(ch(hex, i), ch(target, i))).join("");
}
