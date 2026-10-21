import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Testing Library only auto-registers cleanup when Vitest globals are on;
// tests import `describe`/`it` explicitly, so unmount manually.
afterEach(() => {
  cleanup();
});

// jsdom has no matchMedia; stores that follow the OS theme read it on import.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}
