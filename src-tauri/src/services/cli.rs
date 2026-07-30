//! CLI integration: PATH management and command-line argument parsing.
//! Adds/removes the Muster install directory from the user's PATH so the
//! `muster` / `Muster` commands work from any terminal.

#[cfg(windows)]
pub fn add_to_path(exe_path: &str) -> Result<(), String> {
    let exe_dir = std::path::Path::new(exe_path)
        .parent()
        .ok_or_else(|| "invalid exe path".to_string())?
        .to_string_lossy()
        .to_string();

    // Read current user PATH from HKCU\Environment
    let current = read_user_path()?;
    if path_list_contains(&current, &exe_dir) {
        return Ok(()); // already present
    }
    let new_path = if current.is_empty() {
        exe_dir
    } else {
        format!("{current};{exe_dir}")
    };
    write_user_path(&new_path)
}

#[cfg(windows)]
pub fn remove_from_path(exe_path: &str) -> Result<(), String> {
    let exe_dir = std::path::Path::new(exe_path)
        .parent()
        .ok_or_else(|| "invalid exe path".to_string())?
        .to_string_lossy()
        .to_string();

    let current = read_user_path()?;
    let filtered: Vec<&str> = current
        .split(';')
        .filter(|p| !p.eq_ignore_ascii_case(&exe_dir))
        .collect();
    write_user_path(&filtered.join(";"))
}

#[cfg(windows)]
pub fn is_on_path(exe_path: &str) -> Result<bool, String> {
    let exe_dir = std::path::Path::new(exe_path)
        .parent()
        .ok_or_else(|| "invalid exe path".to_string())?
        .to_string_lossy()
        .to_string();
    let current = read_user_path()?;
    Ok(path_list_contains(&current, &exe_dir))
}

#[cfg(windows)]
fn read_user_path() -> Result<String, String> {
    let output = crate::services::procs::quiet_command("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        // If the value doesn't exist (reg returns error code 1), PATH is empty.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("unable to find") || stderr.contains("ERROR") {
            return Ok(String::new());
        }
        return Err(stderr.to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // reg output format: "    Path    REG_EXPAND_SZ    value"
    let path = stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if let Some(idx) = trimmed.find("REG_") {
                let after = trimmed[idx..].trim();
                if let Some(val_idx) = after.find("    ") {
                    Some(after[val_idx..].trim().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or_default();
    Ok(path)
}

#[cfg(windows)]
fn write_user_path(path: &str) -> Result<(), String> {
    let output = crate::services::procs::quiet_command("reg")
        .args(["add", "HKCU\\Environment", "/v", "Path", "/t", "REG_EXPAND_SZ", "/d", path, "/f"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    // Broadcast WM_SETTINGCHANGE so new terminals see the change immediately.
    // Existing terminals must be reopened; the registry change is persistent.
    broadcast_environment_change();
    Ok(())
}

#[cfg(windows)]
fn broadcast_environment_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG,
    };
    let wide: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            0x001A, // WM_SETTINGCHANGE
            WPARAM(0),
            LPARAM(wide.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

fn path_list_contains(path_list: &str, entry: &str) -> bool {
    path_list.split(';').any(|p| p.eq_ignore_ascii_case(entry))
}

#[cfg(not(windows))]
pub fn add_to_path(_exe_path: &str) -> Result<(), String> {
    Err("Not supported on this platform".into())
}

#[cfg(not(windows))]
pub fn remove_from_path(_exe_path: &str) -> Result<(), String> {
    Err("Not supported on this platform".into())
}

#[cfg(not(windows))]
pub fn is_on_path(_exe_path: &str) -> Result<bool, String> {
    Err("Not supported on this platform".into())
}
