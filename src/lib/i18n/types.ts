export type Lang = "en" | "zh";

export type LangOrSystem = Lang | "system";

/** Recursively collect dot-separated paths from a nested translation object. */
export type DeepPath<T, P extends string = ""> = T extends (...args: never[]) => unknown
  ? P
  : T extends object
    ? {
        [K in keyof T & string]: DeepPath<T[K], P extends "" ? K : `${P}.${K}`>;
      }[keyof T & string]
    : P;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Translations = Record<string, any>;
