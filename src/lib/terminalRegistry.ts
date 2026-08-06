import { Terminal, type ITheme, type ILinkProvider } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { api } from "./invoke";
import type { ThemeColors } from "./types";

interface PtyDataPayload {
  id: string;
  data: string;
}
interface PtyExitPayload {
  id: string;
}

interface Entry {
  term: Terminal;
  fit: FitAddon;
  /// Scrollback search, installed per terminal (used by the search bar).
  search: SearchAddon;
}

/// Terminal instance registry — the "parking lot" for xterm instances.
///
/// React mounts/unmounts panes freely (tab switches, zoom, project switches),
/// but an xterm instance must live as long as its PTY session: destroying it
/// would lose the visible buffer while the shell keeps running. Instances are
/// therefore owned here, keyed by session id, and panes just attach/detach the
/// terminal's DOM element. `pruneSessions` disposes instances whose session
/// has been closed (i.e. is no longer present in the app state).
const registry = new Map<string, Entry>();

/// Per-session buffer of incoming PTY data that arrived before the xterm
/// instance was registered. Data is base64-encoded strings from the backend,
/// flushed into the terminal when `create()` is called. Capped per session:
/// a background session's output must not grow the JS heap without bound,
/// and keeping more than xterm's scrollback (1000 lines) is pointless anyway.
const MAX_PENDING_BYTES = 256 * 1024; // base64 chars kept per session
interface PendingBuffer {
  chunks: string[];
  size: number;
}
const pendingBuffers = new Map<string, PendingBuffer>();

/// Backend PTY events are listened to once for the whole module (not once per
/// session) and dispatched by session id. Per-session listeners would outlive
/// their terminals — the unlisten handles were dropped, so every disposed
/// terminal stayed subscribed forever, pinning it in memory and making event
/// dispatch cost grow with the number of historical sessions. The unlisten
/// handles returned here are intentionally ignored: the listeners live as
/// long as the app itself.
///
/// Returns a promise that resolves once BOTH listeners are actually
/// registered (the IPC round-trip completes). Callers that start PTY read
/// pumps must await it: output emitted before the listeners are registered
/// is dropped by Tauri, and restored sessions would boot into a blank
/// terminal (a lone blinking cursor) with their initial prompt lost.
let listenersReady: Promise<void> | null = null;
export function ensureListeners(): Promise<void> {
  if (!listenersReady) {
    listenersReady = Promise.all([
      listen<PtyDataPayload>("pty:data", (event) => {
        const entry = registry.get(event.payload.id);
        if (!entry) {
          // Terminal not yet created — buffer output so it renders when the
          // pane mounts. Keep only the tail: drop the oldest chunks once the
          // cap is exceeded.
          let buf = pendingBuffers.get(event.payload.id);
          if (!buf) {
            buf = { chunks: [], size: 0 };
            pendingBuffers.set(event.payload.id, buf);
          }
          buf.chunks.push(event.payload.data);
          buf.size += event.payload.data.length;
          while (buf.size > MAX_PENDING_BYTES && buf.chunks.length > 1) {
            buf.size -= buf.chunks.shift()!.length;
          }
          return;
        }
        entry.term.write(base64ToBytes(event.payload.data));
      }),
      listen<PtyExitPayload>("pty:exit", (event) => {
        const entry = registry.get(event.payload.id);
        if (!entry) {
          pendingBuffers.delete(event.payload.id); // cleanup pending buffer
          return;
        }
        entry.term.write("\r\n\x1b[2m[session exited]\x1b[0m\r\n");
      }),
    ]).then(() => {}).catch(() => {
      // A failed listen must not break startup; pumps just start immediately.
    });
  }
  return listenersReady;
}

/// The theme applied to every parked terminal — and to terminals created
/// after it is set. `null` until the first `applyTerminalTheme` call; new
/// terminals then fall back to the hardcoded GitHub-dark theme below.
let currentTheme: ITheme | null = null;

const FALLBACK_THEME: ITheme = {
  background: "#0d1117",
  foreground: "#e6edf3",
  cursor: "#58a6ff",
  cursorAccent: "#0d1117",
  selectionBackground: "#1f6feb",
};

/// Map resolved theme colors (hex without '#') onto an xterm theme.
function toXtermTheme(colors: ThemeColors): ITheme {
  const h = (hex: string) => `#${hex}`;
  const p = colors.palette;
  return {
    background: h(colors.background),
    foreground: h(colors.foreground),
    cursor: h(colors.cursor),
    cursorAccent: h(colors.background),
    selectionBackground: h(colors.selection_bg),
    black: h(p[0]),
    red: h(p[1]),
    green: h(p[2]),
    yellow: h(p[3]),
    blue: h(p[4]),
    magenta: h(p[5]),
    cyan: h(p[6]),
    white: h(p[7]),
    brightBlack: h(p[8]),
    brightRed: h(p[9]),
    brightGreen: h(p[10]),
    brightYellow: h(p[11]),
    brightBlue: h(p[12]),
    brightMagenta: h(p[13]),
    brightCyan: h(p[14]),
    brightWhite: h(p[15]),
  };
}

/// Store `colors` as the terminal theme and re-skin every parked terminal.
export function applyTerminalTheme(colors: ThemeColors): void {
  currentTheme = toXtermTheme(colors);
  for (const entry of registry.values()) {
    entry.term.options.theme = currentTheme;
  }
}

const DEFAULT_FONT_FAMILY = "'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace";
const DEFAULT_FONT_SIZE = 13;

/// Font settings applied to every parked terminal — and to terminals created
/// after they are set. Defaults reproduce the previous hardcoded values, so
/// nothing changes until the first `applyTerminalFont` call.
let currentFont = { family: "", size: DEFAULT_FONT_SIZE, thicken: false };

function resolveFontFamily(family: string): string {
  return family ? `${family}, ${DEFAULT_FONT_FAMILY}` : DEFAULT_FONT_FAMILY;
}

/// Store the font settings and re-apply them to every parked terminal.
export function applyTerminalFont(font: { family: string; size: number; thicken: boolean }): void {
  currentFont = font;
  for (const entry of registry.values()) {
    entry.term.options.fontFamily = resolveFontFamily(font.family);
    entry.term.options.fontSize = font.size || DEFAULT_FONT_SIZE;
    entry.term.options.fontWeight = font.thicken ? 600 : 400;
  }
}

function create(sessionId: string): Entry {
  ensureListeners();
  const term = new Terminal({
    fontFamily: resolveFontFamily(currentFont.family),
    fontSize: currentFont.size || DEFAULT_FONT_SIZE,
    fontWeight: currentFont.thicken ? 600 : 400,
    theme: currentTheme ?? FALLBACK_THEME,
    cursorBlink: true,
    cursorStyle: "block",
    allowProposedApi: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  const search = new SearchAddon();
  term.loadAddon(search);
  // Clickable http(s) links (dev-server URLs etc.) open in the system browser.
  term.loadAddon(
    new WebLinksAddon((_event, uri) => {
      openUrl(uri).catch(() => {});
    })
  );
  // Clickable `path:line[:col]` links (tsc / cargo / git errors) open the
  // file at that line in the editor.
  installPathLinkProvider(term, sessionId);

  // Input → PTY. Registered once per terminal, not per pane mount.
  term.onData((data) => api.sendText(sessionId, data));

  // Clipboard: Ctrl+C copies only when there is a selection, Ctrl+V pastes.
  // Attached here (in create) rather than TerminalPane's useEffect so the
  // handler isn't stacked on every unmount/remount (tab switch, zoom).
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown" || !e.ctrlKey || e.altKey || e.shiftKey) return true;
    const key = e.key.toLowerCase();
    if (key === "c" && term.hasSelection()) {
      navigator.clipboard.writeText(term.getSelection());
      term.clearSelection();
      e.preventDefault();
      return false;
    }
    if (key === "v") {
      e.preventDefault();
      navigator.clipboard.readText().then((text) => {
        if (text) api.sendText(sessionId, text);
      });
      return false;
    }
    return true;
  });

  // PTY output → terminal is handled by the module-level listeners above,
  // which dispatch to this entry via the registry. Keeps buffers in sync
  // even while the pane that hosts this terminal is unmounted.

  // Restore the OSC 0 title the session had when this terminal is created
  // after the fact (e.g. on app relaunch with a restored layout).
  api.sessionInfo(sessionId).then((info) => {
    if (info && info.title) term.write(`\x1b]0;${info.title}\x1b\\`);
  });

  // Flush any PTY output that arrived before this terminal was registered
  // (race between the read pump starting during setup and React mounting
  // the pane).
  const pending = pendingBuffers.get(sessionId);
  if (pending) {
    for (const data of pending.chunks) {
      term.write(base64ToBytes(data));
    }
    pendingBuffers.delete(sessionId);
  }

  return { term, fit, search };
}

// ---- path:line link provider (click compiler errors to jump to the file) --

/// Any run of non-whitespace ending in `:<digits>(:<digits>)` is a candidate;
/// `parsePathLine` + the existence check in the backend reject the rest.
const PATH_LINE_RE = /[^\s]+:\d+(?::\d+)?/;

/// Split a `path:line` / `path:line:col` candidate into its parts. Rejects
/// URLs and strings that don't look like a filesystem path (`12:34`, words).
function parsePathLine(text: string): { path: string; line: number } | null {
  const trimmed = text.trim();
  const m = /^(.+):(\d+)(?::\d+)?$/.exec(trimmed);
  if (!m) return null;
  const pathPart = m[1];
  if (pathPart.includes("://")) return null; // a URL, handled by the web-links addon
  // Must look like a path: contain a directory separator or a file extension.
  if (!/[\\/.]/.test(pathPart)) return null;
  const line = Number(m[2]);
  if (!Number.isInteger(line) || line < 1 || line > 1_000_000) return null;
  return { path: pathPart, line };
}

function isAbsolutePath(p: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(p) || p.startsWith("\\") || p.startsWith("/");
}

/// Resolve a (possibly relative) path from terminal output against the
/// session's cwd and open it at the given line. The backend refuses to open
/// paths that don't exist, so false positives are silently ignored.
async function handlePathLink(sessionId: string, uri: string): Promise<void> {
  const parsed = parsePathLine(uri);
  if (!parsed) return;
  const info = await api.sessionInfo(sessionId);
  const cwd = info?.working_directory;
  if (!cwd) return;
  const candidate = isAbsolutePath(parsed.path)
    ? parsed.path
    : `${cwd}\\${parsed.path.replace(/\//g, "\\")}`;
  api.openFileAt(candidate, parsed.line);
}

/// Register an xterm link provider that highlights `path:line[:col]` in
/// terminal output. Runs once per terminal (in `create`).
function installPathLinkProvider(term: Terminal, sessionId: string): void {
  const provider: ILinkProvider = {
    provideLinks(lineNumber, callback) {
      const line = term.buffer.active.getLine(lineNumber - 1);
      if (!line) {
        callback(undefined);
        return;
      }
      const text = line.translateToString(true);
      const rex = new RegExp(PATH_LINE_RE.source, "g");
      const links = [];
      let match: RegExpExecArray | null;
      while ((match = rex.exec(text))) {
        const startX = match.index;
        const uri = match[0];
        links.push({
          range: {
            start: { x: startX + 1, y: lineNumber },
            end: { x: startX + uri.length, y: lineNumber },
          },
          text: uri,
          activate: (_event: MouseEvent) => {
            handlePathLink(sessionId, uri);
          },
        });
      }
      callback(links);
    },
  };
  term.registerLinkProvider(provider);
}

/// Get (or lazily create) the terminal for a session.
export function acquire(sessionId: string): Entry {
  let entry = registry.get(sessionId);
  if (!entry) {
    entry = create(sessionId);
    registry.set(sessionId, entry);
  }
  return entry;
}

/// The scrollback-search addon for a session's terminal, or null when the
/// terminal was never created (e.g. a session that never had a pane).
export function getSearchAddon(sessionId: string): SearchAddon | null {
  return registry.get(sessionId)?.search ?? null;
}

/// Dispose the terminal for a session and drop it from the registry.
function release(sessionId: string): void {
  const entry = registry.get(sessionId);
  if (!entry) return;
  entry.term.dispose();
  registry.delete(sessionId);
}

/// Dispose every terminal whose session id is not in `active`.
export function pruneSessions(active: ReadonlySet<string>): void {
  for (const id of [...registry.keys()]) {
    if (!active.has(id)) release(id);
  }
  // Also prune any pending buffers for sessions that no longer exist
  // (session created but terminal never mounted, then session closed).
  for (const id of [...pendingBuffers.keys()]) {
    if (!active.has(id)) pendingBuffers.delete(id);
  }
}

/// Wipe the visible buffer + scrollback of a parked terminal (Ctrl+K).
export function clear(sessionId: string): void {
  registry.get(sessionId)?.term.clear();
}

function base64ToBytes(data: string): Uint8Array {
  const bin = atob(data);
  const len = bin.length;
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
