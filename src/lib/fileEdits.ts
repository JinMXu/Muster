/// Global store of the latest unsaved text for each open file tab.
/// FilePane writes here on every edit; App.tsx reads here before saving
/// dirty files on tab-close, so large files (> 100KB) that skip periodic
/// `file_text_changed` sync still have their edits preserved.

const pendingTexts = new Map<string, string>();

export function setLatestText(fileId: string, text: string) {
  pendingTexts.set(fileId, text);
}

export function getLatestText(fileId: string): string | undefined {
  return pendingTexts.get(fileId);
}

export function clearLatestText(fileId: string) {
  pendingTexts.delete(fileId);
}
