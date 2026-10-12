import { defineConfig } from "vite";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = (p: string) => resolve(dirname(fileURLToPath(import.meta.url)), p);
function dirname(p: string) {
  return p.slice(0, Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")));
}

// Standalone preview of the Settings dialog with mocked Tauri APIs.
// Used only for visual verification during development — never built into the app.
export default defineConfig({
  root: here("preview"),
  publicDir: here("public"),
  resolve: {
    alias: [
      { find: /^@tauri-apps\/api\/core$/, replacement: here("preview/mocks/tauri.ts") },
      { find: /^@tauri-apps\/api\/app$/, replacement: here("preview/mocks/app.ts") },
      { find: /^@tauri-apps\/plugin-opener$/, replacement: here("preview/mocks/opener.ts") },
      { find: /^@tauri-apps\/plugin-updater$/, replacement: here("preview/mocks/updater.ts") },
      { find: /^@tauri-apps\/plugin-process$/, replacement: here("preview/mocks/process.ts") },
    ],
  },
  build: { outDir: here("dist-preview"), emptyOutDir: true },
  clearScreen: false,
  server: { port: 7431, strictPort: true },
});
