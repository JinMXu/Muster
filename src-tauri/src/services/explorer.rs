//! Explorer integration helpers (registry writes for "Open in Muster"),
//! Recycle-Bin trash, and the file-tree filesystem operations (listing,
//! creation, rename) behind the `commands::fs` handlers.
//! The registry writes only run when invoked explicitly by the user. Entries
//! go under HKCU (per-user classes), so no elevation is required — same
//! approach as VS Code's user-setup installer.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::i18n::translate;

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
    let output = super::procs::quiet_command("reg")
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
    let output = super::procs::quiet_command("reg")
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

// --- File tree operations ----------------------------------------------------

/// One entry of a directory listing for the file tree.
#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

/// Directory listing for the file tree: skips `.git` / `.` / `..`, sorted
/// directories-first then by case-insensitive name. An unreadable directory
/// yields an empty list (the tree shows it as empty).
pub fn list_directory(path: &str) -> Vec<DirEntry> {
    let Ok(entries) = std::fs::read_dir(path) else { return Vec::new() };
    let mut items: Vec<DirEntry> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name == ".git" || name == "." || name == ".." {
                return None;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(DirEntry { name: name.clone(), path: e.path().to_string_lossy().to_string(), is_directory: is_dir })
        })
        .collect();
    items.sort_by(|a, b| {
        if a.is_directory != b.is_directory {
            b.is_directory.cmp(&a.is_directory)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });
    items
}

/// Create an empty file or directory `parent_dir/name`, returning the full
/// path. Refuses invalid names and existing targets.
pub fn create_entry(parent_dir: &str, name: &str, is_directory: bool, lang: &str) -> Result<String, String> {
    validate_rename_name(name, lang)?;
    let path = Path::new(parent_dir).join(name);
    if path.exists() { return Err(translate("name-already-exists", lang).replace("{name}", name)); }
    if is_directory {
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    } else {
        std::fs::write(&path, "").map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().to_string())
}

/// Rename `from` to the sibling name `to`, returning the new full path.
/// Refuses invalid names and name clashes with a different on-disk entry.
pub fn rename(from: &str, to: &str, lang: &str) -> Result<String, String> {
    let from_path = Path::new(from);
    let to_path = rename_target(from_path, to, lang)?;
    if to_path.exists() && !same_file(from_path, &to_path) {
        return Err(translate("name-already-exists", lang).replace("{name}", to));
    }
    std::fs::rename(from_path, &to_path).map_err(|e| e.to_string())?;
    Ok(to_path.to_string_lossy().to_string())
}

/// Reject names that can't be renamed to: empty, "." / "..", or containing
/// a path separator. The frontend mirrors these rules before invoking.
fn validate_rename_name(name: &str, lang: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(translate("invalid-name", lang).replace("{name}", name));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(translate("name-no-slash", lang).to_string());
    }
    Ok(())
}

/// True when both paths resolve to the same on-disk entry. Case-only
/// renames hit this on Windows: the target "exists" because the filesystem
/// is case-insensitive, and `fs::rename` handles the rename fine.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Rewrite `path` after `from` was renamed to `to`: an exact match, or a
/// path inside the renamed directory (separator-boundary prefix match).
pub fn remap_renamed_path(path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        return Some(to.to_string());
    }
    let rest = path.strip_prefix(from)?;
    if rest.starts_with('\\') || rest.starts_with('/') {
        return Some(format!("{to}{rest}"));
    }
    None
}

/// Destination for renaming `from` to `to`: the sibling path
/// `from.parent()/to`. Validates `to` itself (not just the result's file
/// name) and refuses anything that escapes the parent directory —
/// `Path::join` replaces the base wholesale when `to` is absolute
/// (`C:\x`, `\\server\share`) or drive-prefixed (Windows `C:x`).
fn rename_target(from: &Path, to: &str, lang: &str) -> Result<PathBuf, String> {
    validate_rename_name(to, lang)?;
    let target = from.parent().unwrap_or(Path::new("")).join(to);
    if target.parent() != from.parent() {
        return Err(translate("invalid-name", lang).replace("{name}", to));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rename_name_accepts_plain_names() {
        assert!(validate_rename_name("foo.txt", "en").is_ok());
        assert!(validate_rename_name("a b.rs", "en").is_ok());
        assert!(validate_rename_name(".gitignore", "en").is_ok());
    }

    #[test]
    fn validate_rename_name_rejects_bad_names() {
        assert!(validate_rename_name("", "en").is_err());
        assert!(validate_rename_name(".", "en").is_err());
        assert!(validate_rename_name("..", "en").is_err());
        assert!(validate_rename_name("a/b", "en").is_err());
        assert!(validate_rename_name("a\\b", "en").is_err());
    }

    #[test]
    fn validate_rename_name_rejects_absolute_paths() {
        assert!(validate_rename_name("C:\\Windows\\evil", "en").is_err());
        assert!(validate_rename_name("C:/abs/evil", "en").is_err());
        assert!(validate_rename_name("\\\\server\\share", "en").is_err());
        assert!(validate_rename_name("/etc/passwd", "en").is_err());
    }

    #[test]
    fn rename_target_accepts_plain_sibling_name() {
        let from = Path::new("C:\\proj\\a.txt");
        let target = rename_target(from, "b.txt", "en").unwrap();
        assert_eq!(target, Path::new("C:\\proj").join("b.txt"));
        assert_eq!(target.parent(), from.parent());
    }

    #[test]
    fn rename_target_rejects_escapes() {
        let from = Path::new("C:\\proj\\a.txt");
        // Separators and dot-names are caught by validate_rename_name.
        assert!(rename_target(from, "..", "en").is_err());
        assert!(rename_target(from, "sub\\dir", "en").is_err());
        assert!(rename_target(from, "sub/dir", "en").is_err());
        assert!(rename_target(from, "C:\\Windows\\evil", "en").is_err());
        assert!(rename_target(from, "\\\\server\\share", "en").is_err());
        // Drive-relative "C:x" has no separator but Path::join replaces the
        // whole base on Windows — caught by the parent-equality assert.
        assert!(rename_target(from, "C:evil", "en").is_err());
    }

    #[test]
    fn remap_renamed_path_exact_match() {
        assert_eq!(
            remap_renamed_path("C:\\proj\\a.txt", "C:\\proj\\a.txt", "C:\\proj\\b.txt"),
            Some("C:\\proj\\b.txt".to_string())
        );
    }

    #[test]
    fn remap_renamed_path_inside_renamed_dir() {
        assert_eq!(
            remap_renamed_path("C:\\proj\\dir\\sub\\f.txt", "C:\\proj\\dir", "C:\\proj\\renamed"),
            Some("C:\\proj\\renamed\\sub\\f.txt".to_string())
        );
    }

    #[test]
    fn remap_renamed_path_rejects_sibling_prefix() {
        // "dir2" starts with "dir" but not on a separator boundary.
        assert_eq!(remap_renamed_path("C:\\proj\\dir2\\f.txt", "C:\\proj\\dir", "C:\\proj\\x"), None);
        assert_eq!(remap_renamed_path("C:\\other\\f.txt", "C:\\proj\\dir", "C:\\proj\\x"), None);
    }

    #[test]
    fn remap_renamed_path_accepts_forward_slash_boundary() {
        assert_eq!(
            remap_renamed_path("C:/proj/dir/f.txt", "C:/proj/dir", "C:/proj/x"),
            Some("C:/proj/x/f.txt".to_string())
        );
    }
}
