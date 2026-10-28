import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;
const here = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: [
      // monaco-editor's exports map rewrites every subpath to `esm/vs/*.js`,
      // which breaks its own CSS side-effect imports; point them at the
      // real files instead.
      {
        find: /^monaco-editor\/(.+\.css)$/,
        replacement: path.resolve(here, "node_modules/monaco-editor/esm/vs/$1"),
      },
    ],
  },
  build: {
    // The lazily-loaded monaco chunk stays over 1 MB even with a curated
    // language set; the entry itself should stay well under this.
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      output: {
        // Split the two big startup vendors out of the entry chunk: better
        // caching across app updates and a smaller entry to parse.
        manualChunks: {
          react: ["react", "react-dom"],
          xterm: [
            "@xterm/xterm",
            "@xterm/addon-fit",
            "@xterm/addon-search",
            "@xterm/addon-web-links",
          ],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
}));