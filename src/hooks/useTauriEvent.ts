import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/// Subscribe to a Tauri event and rerun when payload arrives.
export function useTauriEvent<T>(name: string, initial: T): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(initial);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    // `listen` resolves asynchronously; if the component unmounts first
    // (StrictMode double-mount makes this common), the listener must be
    // dropped as soon as its handle arrives or it leaks forever.
    let cancelled = false;
    listen<T>(name, (e) => setValue(e.payload)).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [name]);
  return [value, setValue];
}