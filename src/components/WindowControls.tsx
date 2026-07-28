import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconMinus, IconRestore, IconSquare, IconX } from "./icons";

/// Windows-style caption buttons for the frameless window (decorations:
/// false in tauri.conf.json). Rendered flush against the right edge of the
/// Header; min/max use the shared button hover token, close uses the
/// Windows convention (#e81123 with white glyph). Close goes through the
/// backend's CloseRequested flow (last window hides to tray) — no special
/// handling needed here.
export default function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    win.isMaximized().then(setMaximized).catch(() => {});
    const unlisten = win.onResized(() => {
      win.isMaximized().then(setMaximized).catch(() => {});
    });
    return () => {
      unlisten.then((f) => f()).catch(() => {});
    };
  }, []);

  return (
    // No data-tauri-drag-region here: Tauri starts a drag only when the
    // mousedown target itself carries the attribute, so these buttons stay
    // clickable while the Header's empty space around them drags.
    <div className="flex items-stretch self-stretch flex-shrink-0">
      <button
        title="Minimize"
        onClick={() => getCurrentWindow().minimize()}
        className="w-[46px] flex items-center justify-center text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn"
      >
        <IconMinus size={14} />
      </button>
      <button
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => getCurrentWindow().toggleMaximize()}
        className="w-[46px] flex items-center justify-center text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn"
      >
        {maximized ? <IconRestore size={12} /> : <IconSquare size={12} />}
      </button>
      <button
        title="Close"
        onClick={() => getCurrentWindow().close()}
        className="w-[46px] flex items-center justify-center text-muster-muted hover:bg-[#e81123] hover:text-white"
      >
        <IconX size={14} />
      </button>
    </div>
  );
}
