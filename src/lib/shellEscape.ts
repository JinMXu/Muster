/// Quote a file path for safe injection into a shell command line.
///
/// PowerShell single-quoted strings are fully literal (no interpolation),
/// so we wrap the path in single quotes and double any embedded single
/// quotes. This prevents injection via `$`, backticks, `"`, `!`, etc.
///
/// For cmd.exe (the fallback shell), single quotes are not recognized, but
/// cmd's `cd` handles spaces natively and the risk surface is minimal.
/// The primary shell (pwsh / powershell) is fully protected.
export function shellQuotePath(path: string): string {
  return `'${path.replace(/'/g, "''")}'`;
}
