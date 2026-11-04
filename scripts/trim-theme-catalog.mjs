#!/usr/bin/env node
// One-off: rewrites src-tauri/src/theme/catalog_ghostty.rs keeping only the
// curated allowlist entries, extracted from the existing generated file —
// no network round-trip to GitHub. Same output as re-running
// generate-theme-catalog.mjs against upstream, for when that isn't handy.

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CURATED_THEMES } from "./theme-allowlist.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT = path.join(ROOT, "src-tauri", "src", "theme", "catalog_ghostty.rs");

const src = readFileSync(OUT, "utf8");

const startMarker = "pub static THEMES: &[StaticTheme] = &[";
const start = src.indexOf(startMarker);
if (start < 0) throw new Error("cannot find THEMES table marker");
const tableStart = start + startMarker.length;
const tailStart = src.lastIndexOf("];");
if (tailStart < tableStart) throw new Error("cannot find table end");

const body = src.slice(tableStart, tailStart);
const entryRe = /    StaticTheme \{[\s\S]*?\n    \},/g;
const entries = [...body.matchAll(entryRe)].map((m) => m[0]);
if (entries.length === 0) throw new Error("no entries parsed");

const kept = [];
const names = new Set();
for (const e of entries) {
  const m = e.match(/        name: "((?:[^"\\]|\\.)*)",/);
  if (!m) throw new Error("entry without name");
  names.add(m[1]);
  if (CURATED_THEMES.has(m[1])) kept.push(e);
}
const missing = [...CURATED_THEMES].filter((n) => !names.has(n));
if (missing.length) {
  console.error(`warning: allowlist names not in catalog: ${missing.join(", ")}`);
  process.exitCode = 1;
}

let prefix = src.slice(0, tableStart);
// Refresh the header count and note the curation.
prefix = prefix.replace(
  /\/\/! \d+ themes, sorted alphabetically by name\./,
  `//! ${kept.length} themes (curated allowlist — see\n//! scripts/generate-theme-catalog.mjs), sorted alphabetically by name.`,
);

writeFileSync(OUT, prefix + "\n" + kept.join("\n") + "\n];\n", "utf8");
console.log(`kept ${kept.length} of ${entries.length} themes`);
