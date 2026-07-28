import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/// Subscribe to a Tauri event and rerun when payload arrives.
export function useTauriEvent<T>(name: string, initial: T): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(initial);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<T>(name, (e) => setValue(e.payload)).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [name]);
  return [value, setValue];
}