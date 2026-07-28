import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  type ReactNode,
} from "react";
import type { Lang, DeepPath } from "./types";
import en from "./en";
import zh from "./zh";
import type enObj from "./en";

export type TKey = DeepPath<typeof enObj>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type TFunc = (path: string, args?: Record<string, any>) => string;

interface LangCtx {
  lang: Lang;
  t: TFunc;
}

export const LangContext = createContext<LangCtx>({
  lang: "en",
  t: (k) => k,
});

const translations = { en, zh };

function resolve(
  obj: Record<string, unknown>,
  path: string[],
): unknown {
  let cur: unknown = obj;
  for (const seg of path) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[seg];
  }
  if (typeof cur === "string" || typeof cur === "function")
    return cur as string | ((...args: never[]) => string);
  return undefined;
}

function makeT(lang: Lang): TFunc {
  const dict = translations[lang] as Record<string, unknown>;
  return (path, args) => {
    const segments = path.split(".");
    const val = resolve(dict, segments);
    if (typeof val === "function") return (val as (a?: Record<string, unknown>) => string)(args ?? {});
    if (typeof val === "string") return val;
    return path;
  };
}

/** Resolve initial language: OS detection only (no settings awareness). */
export function detectInitialLang(): Lang {
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

export function LanguageProvider({
  children,
  lang,
}: {
  children: ReactNode;
  lang: Lang;
}) {
  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  }, [lang]);

  const t = useMemo(() => makeT(lang), [lang]);

  return (
    <LangContext.Provider value={{ lang, t }}>
      {children}
    </LangContext.Provider>
  );
}

export function useT() {
  return useContext(LangContext);
}
