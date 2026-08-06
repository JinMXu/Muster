/// Resolves the user's preferred interactive shell, with sensible fallbacks.
/// Priority (Windows): pwsh.exe → powershell.exe → wsl.exe → cmd.exe.
#[derive(Debug, Clone)]
pub struct ShellSpec {
    pub path: String,
    pub args: Vec<String>,
    pub name: String,
}

#[cfg(windows)]
pub fn detect_default_shell() -> ShellSpec {
    if let Ok(p) = which::which("pwsh.exe") {
        return powershell_spec(p.to_string_lossy().to_string(), "pwsh");
    }
    if let Ok(p) = which::which("powershell.exe") {
        return powershell_spec(p.to_string_lossy().to_string(), "PowerShell");
    }
    if let Ok(p) = which::which("wsl.exe") {
        return ShellSpec {
            path: p.to_string_lossy().to_string(),
            args: vec![],
            name: "wsl".into(),
        };
    }
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into());
    ShellSpec { path: comspec, args: vec![], name: "cmd".into() }
}

/// PowerShell spec with shell integration: spawn via a small script that
/// dot-sources the user profile and wraps `prompt` to report the cwd through
/// OSC 9;9, which the PTY read loop picks up to keep the session's working
/// directory live. Falls back to plain args if the script can't be written.
#[cfg(windows)]
fn powershell_spec(path: String, name: &str) -> ShellSpec {
    match write_integration_script() {
        Some(script) => ShellSpec {
            path,
            args: vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                script,
            ],
            name: name.into(),
        },
        None => ShellSpec { path, args: vec!["-NoLogo".into(), "-NoExit".into()], name: name.into() },
    }
}

#[cfg(windows)]
fn write_integration_script() -> Option<String> {
    // Resolved once per process: the script content is a compile-time
    // constant, so after the first write (or a matching existing file)
    // later spawns skip the disk I/O entirely.
    static SCRIPT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SCRIPT
        .get_or_init(|| {
            let dir = crate::services::persist::app_data_dir();
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join("shell-integration.ps1");
            let stale = match std::fs::read(&path) {
                Ok(existing) => existing != INTEGRATION_PS1.as_bytes(),
                Err(_) => true,
            };
            if stale {
                std::fs::write(&path, INTEGRATION_PS1).ok()?;
            }
            Some(path.to_string_lossy().to_string())
        })
        .clone()
}

#[cfg(windows)]
const INTEGRATION_PS1: &str = r#"# muster shell integration — reports the current directory to the terminal
# via OSC 9;9 after every command, so the app can track the session's cwd.
if (Test-Path $PROFILE) { . $PROFILE }
$global:__muster_inner_prompt = $function:prompt
function global:prompt {
    $esc = [char]27
    $cwd = $executionContext.SessionState.Path.CurrentLocation.ProviderPath
    [Console]::Write("$esc]9;9;`"$cwd`"$esc\")
    if ($global:__muster_inner_prompt) { & $global:__muster_inner_prompt } else { "PS $cwd$('>' * ($nestedPromptLevel + 1)) " }
}
"#;

/// PATH for spawned shells, read fresh from the registry (machine + user)
/// instead of inherited from Muster's own process. When the app is launched
/// from Explorer / the Start Menu, its environment can predate a newly
/// installed CLI (nvm, npm globals, …), making those commands "not found" in
/// the terminal even though a freshly opened PowerShell sees them.
#[cfg(windows)]
pub fn fresh_path_from_registry() -> Option<String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
        .and_then(|k| k.get_value::<String, _>("Path"));
    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .and_then(|k| k.get_value::<String, _>("Path"));

    let mut parts: Vec<String> = Vec::new();
    for raw in [machine, user].into_iter().flatten() {
        let expanded = expand_env_vars(&raw);
        if !expanded.is_empty() {
            parts.push(expanded);
        }
    }
    if parts.is_empty() { None } else { Some(parts.join(";")) }
}

/// Expand `%VAR%` references using this process's environment; unresolved
/// names are kept verbatim. The registry stores PATH as REG_EXPAND_SZ.
#[cfg(windows)]
fn expand_env_vars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        match tail.find('%') {
            Some(end) => {
                let name = &tail[..end];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push('%');
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(all(test, windows))]
mod tests {
    use super::expand_env_vars;

    #[test]
    fn expands_known_keeps_unknown_and_plain() {
        std::env::set_var("MUSTER_TEST_EXPAND", "xyz");
        assert_eq!(
            expand_env_vars(r"%MUSTER_TEST_EXPAND%\bin;%NO_SUCH_ENV_VAR%\x"),
            r"xyz\bin;%NO_SUCH_ENV_VAR%\x"
        );
        assert_eq!(expand_env_vars("plain"), "plain");
        assert_eq!(expand_env_vars("100%"), "100%");
    }
}

#[cfg(not(windows))]
pub fn detect_default_shell() -> ShellSpec {
    use std::env;
    let sh = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let name = std::path::Path::new(&sh)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("shell")
        .to_string();
    ShellSpec { path: sh, args: vec!["-l".into()], name }
}