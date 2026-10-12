import React from "react";
import { createRoot } from "react-dom/client";
import "../src/styles/globals.css";
import Settings from "../src/components/Settings";
import { LanguageProvider, makeT } from "../src/lib/i18n/context";
import zh from "../src/lib/i18n/zh";

const t = makeT("zh");
// Expose for scripted checks from Playwright.
(window as unknown as Record<string, unknown>).__t = t;

function Root() {
  const [open, setOpen] = React.useState(true);
  return (
    <LanguageProvider lang="zh">
      {open ? <Settings onClose={() => setOpen(false)} initialTab="general" /> : null}
    </LanguageProvider>
  );
}

createRoot(document.getElementById("root")!).render(<Root />);
