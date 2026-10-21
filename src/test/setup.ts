import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Testing Library only auto-registers cleanup when Vitest globals are on;
// tests import `describe`/`it` explicitly, so unmount manually.
afterEach(() => {
  cleanup();
});
