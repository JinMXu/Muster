import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  readClipboardFiles: vi.fn(),
}));

vi.mock("./invoke", () => ({
  api: { readClipboardFiles: mocks.readClipboardFiles },
}));

import { formatFilePaths, getPastePayload } from "./terminalPaste";

function mockClipboardText(text: string): void {
  Object.defineProperty(navigator, "clipboard", {
    value: { readText: vi.fn(async () => text) },
    configurable: true,
  });
}

describe("formatFilePaths", () => {
  it("quotes a single path", () => {
    expect(formatFilePaths(["D:\\a b\\c.txt"])).toBe(`'D:\\a b\\c.txt'`);
  });

  it("space-joins multiple quoted paths", () => {
    expect(formatFilePaths(["D:\\a.txt", "D:\\b.txt"])).toBe(`'D:\\a.txt' 'D:\\b.txt'`);
  });

  it("doubles embedded single quotes", () => {
    expect(formatFilePaths(["D:\\it's.txt"])).toBe(`'D:\\it''s.txt'`);
  });

  it("returns the empty string for no files", () => {
    expect(formatFilePaths([])).toBe("");
  });
});

describe("getPastePayload", () => {
  beforeEach(() => {
    mocks.readClipboardFiles.mockReset();
  });

  it("prefers text over files", async () => {
    mockClipboardText("hello");
    mocks.readClipboardFiles.mockResolvedValue(["D:\\f.txt"]);
    await expect(getPastePayload()).resolves.toEqual({ kind: "text", text: "hello" });
  });

  it("turns an Explorer file copy into quoted paths", async () => {
    mockClipboardText("");
    mocks.readClipboardFiles.mockResolvedValue(["D:\\a b.txt", "D:\\it's.txt"]);
    await expect(getPastePayload()).resolves.toEqual({
      kind: "files",
      text: `'D:\\a b.txt' 'D:\\it''s.txt'`,
    });
  });

  it("degrades to none when the clipboard command fails", async () => {
    mockClipboardText("");
    mocks.readClipboardFiles.mockRejectedValue(new Error("boom"));
    await expect(getPastePayload()).resolves.toEqual({ kind: "none" });
  });

  it("reports none on an empty clipboard", async () => {
    mockClipboardText("");
    mocks.readClipboardFiles.mockResolvedValue([]);
    await expect(getPastePayload()).resolves.toEqual({ kind: "none" });
  });
});
