// Generates src/assets/vscode-icons-subset.json with only the icons the file
// tree references from src/lib/fileIcons.ts. The full vscode-icons collection
// is ~3.9 MB; the tree uses ~60 of its 1600+ icons, so shipping the whole
// collection makes the first Files-panel mount parse a large chunk.
//
// Wired into `npm run dev` / `npm run build`; re-run manually after adding a
// mapping: `npm run icons:subset`.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = readFileSync(resolve(root, "src/lib/fileIcons.ts"), "utf8");

let collection;
try {
  collection = require("@iconify-json/vscode-icons/icons.json");
} catch {
  collection = JSON.parse(
    readFileSync(resolve(root, "node_modules/@iconify-json/vscode-icons/icons.json"), "utf8")
  );
}

const names = new Set(
  [...source.matchAll(/"file-type-[a-z0-9-]+"/g)].map((m) => m[0].slice(1, -1))
);
names.add("default-file");

const icons = {};
const aliases = {};
const missing = [];

function add(name, seen = new Set()) {
  if (icons[name] || aliases[name] || seen.has(name)) return;
  seen.add(name);
  const icon = collection.icons?.[name];
  if (icon) {
    icons[name] = icon;
    return;
  }
  const alias = collection.aliases?.[name];
  if (alias) {
    // An Iconify alias renders through its parent, so include that too.
    aliases[name] = alias;
    add(alias.parent, seen);
    return;
  }
  missing.push(name);
}

for (const name of [...names].sort()) add(name);

if (missing.length > 0) {
  console.warn(
    `icons:subset: not in the collection (UI falls back to the generic icon): ${missing.join(", ")}`
  );
}

const subset = {
  prefix: collection.prefix,
  ...(collection.lastModified ? { lastModified: collection.lastModified } : {}),
  icons,
  ...(Object.keys(aliases).length > 0 ? { aliases } : {}),
  width: collection.width,
  height: collection.height,
};

const outDir = resolve(root, "src/assets");
mkdirSync(outDir, { recursive: true });
const out = resolve(outDir, "vscode-icons-subset.json");
writeFileSync(out, JSON.stringify(subset));
console.log(
  `icons:subset: ${Object.keys(icons).length} icons, ${(JSON.stringify(subset).length / 1024).toFixed(1)} KB -> ${out}`
);
