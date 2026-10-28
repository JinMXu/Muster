// Side-effect-only imports (see lib/monaco.ts and lib/monaco-editor-contribs.ts):
// monaco-editor 0.5x ships these contribution modules without adjacent .d.ts
// files, and the package exports map exposes no types for them. They only
// register editor contributions / worker-backed language modes, so `any` is
// fine.
declare module "monaco-editor/editor/*";
declare module "monaco-editor/features/*";
declare module "monaco-editor/base/*";
declare module "monaco-editor/language/typescript/monaco.contribution";
declare module "monaco-editor/language/css/monaco.contribution";
declare module "monaco-editor/language/html/monaco.contribution";
declare module "monaco-editor/language/json/monaco.contribution";
