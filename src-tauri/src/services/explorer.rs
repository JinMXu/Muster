//! Explorer integration helpers (registry writes for "Open in Muster").
//! These are commands exposed as Tauri commands and only run when invoked
//! explicitly by the user. Entries go under HKCU (per-user classes), so no
//! elevation is required — same approach as VS Code's user-setup installer.

#[cfg(windows)]
pub fn install_context_menu(exe_path: &str) -> Result<(), String> {
    // Defer to a reg.exe child process for portability.
    let args_key_dir = "HKCU\\SOFTWARE\\Classes\\Directory\\shell\\OpenInMuster";
    let args_key_bg = "HKCU\\SOFTWARE\\Classes\\Directory\\Background\\shell\\OpenInMuster";
    run_reg_add(args_key_dir, "Open in Muster")?;
    run_reg_add_cmd(args_key_dir, exe_path)?;
    run_reg_add(args_key_bg, "Open in Muster")?;
    run_reg_add_cmd(args_key_bg, exe_path)?;
    Ok(())
}

#[cfg(windows)]
fn run_reg_add(key: &str, value: &str) -> Result<(), String> {
    let output = std::process::Command::new("reg")
        .args(["add", key, "/ve", "/d", value, "/f"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn run_reg_add_cmd(key: &str, exe_path: &str) -> Result<(), String> {
    let cmd_key = format!("{key}\\command");
    let cmd_value = format!("\"{exe_path}\" \"%V\"");
    let output = std::process::Command::new("reg")
        .args(["add", &cmd_key, "/ve", "/d", &cmd_value, "/f"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn install_context_menu(_exe_path: &str) -> Result<(), String> {
    Err("Not supported on this platform".into())
}

/// Move a file or directory to the Recycle Bin (SHFileOperationW with undo).
#[cfg(windows)]
pub fn trash(path: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHFileOperationW, SHFILEOPSTRUCTW, FO_DELETE};
    // FOF_ALLOWUNDO=0x40, FOF_NOCONFIRMATION=0x10, FOF_SILENT=0x04
    const FLAGS: u16 = 0x0040 | 0x0010 | 0x0004;
    let mut wide: Vec<u16> = path
        .encode_utf16()
        .chain(std::iter::once(0))
        .chain(std::iter::once(0))
        .collect();
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_mut_ptr()),
        fFlags: FLAGS,
        ..Default::default()
    };
    let result = unsafe { SHFileOperationW(&mut op) };
    if result == 0 { Ok(()) } else { Err(format!("SHFileOperationW failed: {result}")) }
}

#[cfg(not(windows))]
pub fn trash(_path: &str) -> Result<(), String> {
    Err("Not supported on this platform".into())
}