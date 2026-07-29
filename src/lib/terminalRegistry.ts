import { Terminal, type ITheme } from "@xterm/xterm";
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

/// Backend PTY events are listened to once for the whole module (not once per
/// session) and dispatched by session id. Per-session listeners would outlive
/// their terminals — the unlisten handles were dropped, so every disposed
/// terminal stayed subscribed forever, pinning it in memory and making event
/// dispatch cost grow with the number of historical sessions. The unlisten
/// handles returned here are intentionally ignored: the listeners live as
/// long as the app itself.
let listenersReady = false;
function ensureListeners(): void {
  if (listenersReady) return;
  listenersReady = true;
  listen<PtyDataPayload>("pty:data", (event) => {
    const entry = registry.get(event.payload.id);
    if (!entry) return;
    entry.term.write(base64ToBytes(event.payload.data));
  });
  listen<PtyExitPayload>("pty:exit", (event) => {
    const entry = registry.get(event.payload.id);
    if (!entry) return;
    entry.term.write("\r\n\x1b[2m[session exited]\x1b[0m\r\n");
  });
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
  term.loadAddon(new SearchAddon());
  // Clickable http(s) links (dev-server URLs etc.) open in the system browser.
  term.loadAddon(
    new WebLinksAddon((_event, uri) => {
      openUrl(uri).catch(() => {});
    })
  );

  // Input → PTY. Registered once per terminal, not per pane mount.
  term.onData((data) => api.sendText(sessionId, data));

  // PTY output → terminal is handled by the module-level listeners above,
  // which dispatch to this entry via the registry. Keeps buffers in sync
  // even while the pane that hosts this terminal is unmounted.

  // Restore the OSC 0 title the session had when this terminal is created
  // after the fact (e.g. on app relaunch with a restored layout).
  api.sessionInfo(sessionId).then((info) => {
    if (info && info.title) term.write(`\x1b]0;${info.title}\x1b\\`);
  });

  return { term, fit };
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
