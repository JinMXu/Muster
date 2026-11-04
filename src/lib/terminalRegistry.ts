import { Terminal, type ITheme, type ILinkProvider } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "./invoke";
import { events } from "./bindings";
import { createPtyAckBatcher } from "./ptyAcks";
import type { ThemeColors } from "./types";

interface Entry {
  term: Terminal;
  fit: FitAddon;
  /// Scrollback search, installed per terminal (used by the search bar).
  search: SearchAddon;
  /// Whether the viewport is pinned to the bottom (following output).
  /// Tracked via xterm's onScroll so the pty:data write path and the
  /// resize handler can re-assert following after the viewport's scroll
  /// metrics are rebuilt (the event that knocks xterm out of "at bottom"
  /// and makes it stop following streaming output).
  atBottom: boolean;
  /// Set after the first real fit. Before it, xterm still sits at its
  /// 80x24 spawn size while the restore-flushed buffer is pending — scroll
  /// events fired by that transient reflow must not clear `atBottom`, or
  /// the viewport stays stuck above the live prompt (the covered-input-box
  /// bug on restored sessions).
  fitted: boolean;
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

/// Flow-control credit is batched (see `ptyAcks.ts`): one IPC call per
/// consumed ~256 KiB, or on a 100ms timer for low-volume streams.
const ptyAcks = createPtyAckBatcher((sessionId, bytes) => {
  api.terminalAck(sessionId, bytes).catch(() => {});
});

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
      events.ptyData.listen((event) => {
        const { id, data } = event.payload;
        const entry = registry.get(id);
        if (!entry) {
          // Terminal not yet created - buffer output so it renders when the
          // pane mounts. Keep only the tail: drop the oldest chunks once the
          // cap is exceeded.
          let buf = pendingBuffers.get(id);
          if (!buf) {
            buf = { chunks: [], size: 0 };
            pendingBuffers.set(id, buf);
          }
          buf.chunks.push(data);
          buf.size += data.length;
          while (buf.size > MAX_PENDING_BYTES && buf.chunks.length > 1) {
            buf.size -= buf.chunks.shift()!.length;
          }
          // Nothing will throttle this buffered/dropped output, so credit it
          // immediately to keep the session (and its agent) running.
          ptyAcks.queue(id, base64ByteLength(data));
          return;
        }
        // Capture the follow state before the async write. term.write is
        // time-sliced across animation frames; by the time the callback
        // fires a resize may have rebuilt the viewport's scroll metrics and
        // knocked ydisp off the bottom. Re-asserting here keeps the
        // terminal pinned to new output while the user hasn't scrolled up.
        const wasAtBottom = entry.atBottom;
        const bytes = base64ToBytes(data);
        if (document.hidden) {
          // Hidden WebViews throttle xterm's write scheduling, so waiting for
          // the callback would stall background sessions (and their agents).
          // Credit on arrival; only the visible path applies real backpressure.
          entry.term.write(bytes);
          ptyAcks.queue(id, bytes.length);
          return;
        }
        entry.term.write(bytes, () => {
          if (wasAtBottom && registry.has(id)) {
            entry.term.scrollToBottom();
          }
          // xterm has parsed this chunk: release its PTY flow-control credit.
          ptyAcks.queue(id, bytes.length);
        });
      }),
      events.ptyExit.listen((event) => {
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
  const metricsChanged =
    currentFont.family !== font.family ||
    currentFont.size !== font.size ||
    currentFont.thicken !== font.thicken;
  currentFont = font;
  for (const entry of registry.values()) {
    entry.term.options.fontFamily = resolveFontFamily(font.family);
    entry.term.options.fontSize = font.size || DEFAULT_FONT_SIZE;
    entry.term.options.fontWeight = font.thicken ? 600 : 400;
  }
  // A font change re-measures the cell size: the same cols/rows then occupy
  // more pixels than the pane, and the pane's `overflow-hidden` clips the
  // surplus — the bottom rows, where full-screen TUIs (claude code) pin
  // their input box. The container itself didn't change, so no
  // ResizeObserver fires and nothing else corrects it; refit explicitly.
  // (The settings load lands here AFTER the startup-restored panes mounted
  // with module-default metrics — 13px regular vs the configured 14px
  // thicken — which is the restored-session covered-input-box bug.)
  if (metricsChanged) {
    for (const id of registry.keys()) refitEntry(id);
  }
}

/// Re-sync a parked terminal's scroll viewport after its DOM element was
/// detached and re-inserted (tab switch, zoom, project switch), or after a
/// resize rebuilt the scroll metrics.
///
/// While the element is detached, the browser destroys the
/// `.xterm-viewport` scrolling box, resetting its scrollTop to 0 — but
/// xterm's internal ydisp keeps pointing at the previous position (usually
/// the bottom). Re-inserting the element does not restore scrollTop, and
/// xterm only re-applies it lazily; any scroll event read in the meantime
/// (a wheel tick, or one queued by the detach itself) is interpreted by
/// xterm's Viewport as a giant upward scroll and slams the viewport to the
/// top of the scrollback, after which the terminal no longer follows
/// output. Force the viewport's cached metrics to be recomputed so it
/// re-applies scrollTop from ydisp right now — wherever ydisp is, so a
/// user who scrolled up stays put.
export function resyncViewport(term: Terminal): void {
  const viewport = (term as unknown as {
    viewport?: { reset(): void; syncScrollArea(immediate?: boolean): void };
  }).viewport;
  if (!viewport) return;
  viewport.reset(); // clear cached sizes/scrollTop so the sync isn't skipped
  viewport.syncScrollArea(true); // immediate: re-apply scrollTop = ydisp * rowHeight
}

/// Re-fit one parked terminal after a non-container-driven metric change
/// (font settings applied to already-mounted panes) and push the negotiated
/// size to its PTY. Mirrors TerminalPane's ResizeObserver path: fit,
/// resize_terminal, re-assert the viewport bottom so streaming output stays
/// in view after the scroll metrics are rebuilt.
function refitEntry(sessionId: string): void {
  const entry = registry.get(sessionId);
  if (!entry || !entry.term.element) return;
  const wasAtBottom = entry.atBottom;
  try {
    entry.fit.fit();
  } catch (_) {
    return; // zero-sized container — nothing to compute against
  }
  entry.fitted = true;
  const { cols, rows } = entry.term;
  if (cols && rows) api.resizeTerminal(sessionId, cols, rows).catch(() => {});
  if (wasAtBottom) {
    resyncViewport(entry.term);
    entry.term.scrollToBottom();
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

  const entry: Entry = { term, fit, search, atBottom: true, fitted: false };

  // Track whether the viewport is at the bottom so the pty:data write path
  // and the resize handler (in TerminalPane) know when to re-assert
  // following. onScroll fires with the new viewportY; comparing it to
  // baseY tells us if the user is pinned to the bottom or has scrolled up.
  // Before the first fit, scroll events come from the spawn-size → real-size
  // reflow of the restore-flushed buffer, not from the user — ignore them.
  term.onScroll((viewportY: number) => {
    if (!entry.fitted) return;
    entry.atBottom = viewportY === term.buffer.active.baseY;
  });

  // PTY output -> terminal is handled by the module-level listeners above,
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

  return entry;
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

/// Session ids that currently own a live xterm instance — the iteration
/// set for the screen-text push (see `screenPush`).
export function screenSessionIds(): string[] {
  return [...registry.keys()];
}

/// The visible viewport's text as logical lines (wrapped rows merged back
/// together), the input for screen-level agent state detection. Mirrors
/// kero's `term.Text()`: what the user sees, not the raw byte stream.
export function screenText(sessionId: string): string | null {
  const entry = registry.get(sessionId);
  if (!entry) return null;
  const buf = entry.term.buffer.active;
  const parts: string[] = [];
  for (let i = 0; i < entry.term.rows; i++) {
    const line = buf.getLine(buf.viewportY + i);
    if (!line) continue;
    const text = line.translateToString(true);
    if (line.isWrapped && parts.length > 0) {
      parts[parts.length - 1] += text;
    } else {
      parts.push(text);
    }
  }
  return parts.join("\n") + "\n";
}

/// One-line viewport snapshot for the screen-push log — the state the user's
/// eyes see alongside the text they see. Temporary bug-hunt instrumentation
/// for the restored-session covered-input-box report: `vp` far from `base`
/// while `len` grows means the viewport stopped following output; `buf=alt`
/// means a full-screen TUI's alternate screen is active. The px metrics
/// (canvas/screen/element/parent) split a stale renderer from a stale
/// buffer: with the buffer text complete but pixels missing below row N,
/// a canvas height far below `rows * cellHeight` pins the renderer as the
/// layer that skipped the resize.
export function screenDiag(sessionId: string): string | null {
  const entry = registry.get(sessionId);
  if (!entry) return null;
  const buf = entry.term.buffer.active;
  const el = entry.term.element;
  const screen = el?.querySelector(".xterm-screen");
  const canvas = el?.querySelector("canvas");
  const viewport = el?.querySelector(".xterm-viewport");
  // The renderer's own idea of the canvas size (xterm internal); compared
  // against the DOM canvas it reveals whether the dimension change was
  // dropped at the RenderService level or at the DOM level.
  const dims = (
    entry.term as unknown as {
      _core?: {
        _renderService?: {
          dimensions?: {
            css?: {
              canvas?: { width?: number; height?: number };
              cell?: { height?: number };
            };
          };
        };
      };
    }
  )._core?._renderService?.dimensions?.css;
  // The renderer pause state (RenderService._isPaused): while paused,
  // refreshRows drops requests and canvas resizes defer to a task that only
  // flushes when the IntersectionObserver reports the terminal visible
  // again. A terminal stuck paused with a stale canvas is exactly the
  // "buffer text complete, pixels missing" shape.
  const rs = (
    entry.term as unknown as {
      _core?: {
        _renderService?: {
          _isPaused?: boolean;
          _needsFullRefresh?: boolean;
        };
        _charSizeService?: { width?: number; height?: number; hasValidSize?: boolean };
      };
    }
  )._core;
  const rect = (n?: Element | null) => {
    const r = n?.getBoundingClientRect();
    return r ? `${Math.round(r.width)}x${Math.round(r.height)}` : "?";
  };
  return [
    `vp=${buf.viewportY}`,
    `base=${buf.baseY}`,
    `len=${buf.length}`,
    `cursor=${buf.cursorY},${buf.cursorX}`,
    `rows=${entry.term.rows}`,
    `cols=${entry.term.cols}`,
    `atBottom=${entry.atBottom}`,
    `fitted=${entry.fitted}`,
    `buf=${buf.type}`,
    `cv=${canvas ? `${(canvas as HTMLCanvasElement).width}x${(canvas as HTMLCanvasElement).height}` : "?"}`,
    `cvCss=${rect(canvas)}`,
    `screen=${rect(screen)}`,
    `el=${rect(el)}`,
    `parent=${rect(el?.parentElement)}`,
    `vpEl=${viewport ? `${(viewport as HTMLElement).clientWidth}x${(viewport as HTMLElement).clientHeight} scroll=${Math.round((viewport as HTMLElement).scrollTop)}/${(viewport as HTMLElement).scrollHeight}` : "?"}`,
    `rndr=${dims?.canvas?.width != null && dims?.canvas?.height != null ? `${Math.round(dims.canvas.width)}x${Math.round(dims.canvas.height)} cell=${dims.cell?.height?.toFixed(1)}` : "?"}`,
    `paused=${rs?._renderService?._isPaused}`,
    `needsFull=${rs?._renderService?._needsFullRefresh}`,
    `char=${rs?._charSizeService?.width}x${rs?._charSizeService?.height}`,
  ].join(" ");
}

/// Dispose the terminal for a session and drop it from the registry.
function release(sessionId: string): void {
  ptyAcks.forget(sessionId);
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

/// Decoded byte length of a base64 string without decoding it — used to ACK
/// output that is only buffered (no terminal attached yet).
function base64ByteLength(data: string): number {
  if (!data) return 0;
  let padding = 0;
  if (data.charCodeAt(data.length - 1) === 61) padding += 1; // '='
  if (data.length > 1 && data.charCodeAt(data.length - 2) === 61) padding += 1;
  return (data.length / 4) * 3 - padding;
}

function base64ToBytes(data: string): Uint8Array {
  const bin = atob(data);
  const len = bin.length;
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
