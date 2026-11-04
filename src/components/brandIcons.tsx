import type { AgentKind } from "../lib/types";
import { cn } from "./ui/cn";
import amp from "../assets/brands/amp.svg?raw";
import claude from "../assets/brands/claude.svg?raw";
import cline from "../assets/brands/cline.svg?raw";
import cursor from "../assets/brands/cursor.svg?raw";
import githubcopilot from "../assets/brands/githubcopilot.svg?raw";
import googlegemini from "../assets/brands/googlegemini.svg?raw";
import kimi from "../assets/brands/kimi.svg?raw";
import openai from "../assets/brands/openai.svg?raw";
import opencode from "../assets/brands/opencode.svg?raw";
import qwen from "../assets/brands/qwen.svg?raw";

// Coding agents' brand logos, from Simple Icons (CC0); the brands are their
// owners' trademarks. Same set kero maps pane titles with. Agents without an
// entry here (aider, goose, pi, ...) fall back to the terminal icon.
const BRANDS: Partial<Record<AgentKind, string>> = {
  claude_code: claude,
  codex: openai,
  gemini: googlegemini,
  opencode: opencode,
  amp: amp,
  cursor: cursor,
  copilot: githubcopilot,
  qwen: qwen,
  kimi_code: kimi,
  cline: cline,
};

export function hasBrandIcon(kind: AgentKind | null | undefined): boolean {
  return !!kind && !!BRANDS[kind];
}

/** A coding agent's brand logo. The SVGs draw at 1em in currentColor, so
 * the surrounding text color tints them; `size` sets that font size in px. */
export function BrandIcon({
  kind,
  size = 13,
  className,
}: {
  kind: AgentKind | null | undefined;
  size?: number;
  className?: string;
}) {
  const svg = kind ? BRANDS[kind] : undefined;
  if (!svg) return null;
  return (
    <span
      aria-hidden
      className={cn("inline-flex items-center justify-center flex-shrink-0 leading-none", className)}
      style={{ fontSize: size }}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
