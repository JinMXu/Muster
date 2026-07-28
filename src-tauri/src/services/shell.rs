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
    let dir = crate::services::persist::app_data_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("shell-integration.ps1");
    std::fs::write(&path, INTEGRATION_PS1).ok()?;
    Some(path.to_string_lossy().to_string())
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