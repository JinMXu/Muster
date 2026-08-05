use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::models::project::AppTheme;

/// Path of the config file: `config_dir()/muster/config.toml`
/// (`muster-dev` in debug builds).
pub fn config_path() -> PathBuf {
    if cfg!(debug_assertions) {
        dirs::config_dir().unwrap_or_default().join("muster-dev").join("config.toml")
    } else {
        dirs::config_dir().unwrap_or_default().join("muster").join("config.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "AppTheme::system_default")]
    pub theme: AppTheme,
    #[serde(default = "default_dark")]
    pub theme_dark: String,
    #[serde(default = "default_light")]
    pub theme_light: String,
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: f64,
    #[serde(default)]
    pub font_thicken: bool,
    #[serde(default)]
    pub editor_wrap_lines: bool,
    /// Diff viewer layout: side-by-side (true) or unified/inline (false).
    #[serde(default = "default_diff_side_by_side")]
    pub diff_side_by_side: bool,
    /// Info panel PORTS: also include listeners from processes that merely
    /// work inside the project directory (dev servers started outside the
    /// session), not just the session's own process tree.
    #[serde(default)]
    pub project_ports: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: AppTheme::System,
            theme_dark: "Default Dark".into(),
            theme_light: "Default Light".into(),
            font_family: String::new(),
            font_size: 13.0,
            ui_font_size: 12.0,
            font_thicken: false,
            editor_wrap_lines: false,
            diff_side_by_side: true,
            project_ports: false,
            language: "system".into(),
        }
    }
}

impl Settings {
    pub const FONT_SIZE_RANGE: (f64, f64) = (8.0, 32.0);
    const DEFAULT_FONT_SIZE: f64 = 13.0;
    const DEFAULT_UI_FONT_SIZE: f64 = 12.0;

    pub fn load() -> Self {
        Self::load_from(&config_path())
    }

    fn load_from(path: &std::path::Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            let s = Self::default();
            let _ = s.save_to(path);
            return s;
        };
        match toml::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                // Corrupt config: fall back to defaults, but keep the
                // original file as config.toml.bak so the user can recover
                // whatever was in it. Backup failure is logged, never fatal.
                log::warn!("failed to parse {}: {e}; resetting to defaults", path.display());
                let bak = path.with_extension("toml.bak");
                if let Err(be) = fs::copy(path, &bak) {
                    log::warn!("failed to back up corrupt config to {}: {be}", bak.display());
                }
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_path())
    }

    fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        // Propagate serialization failures: writing an empty string would
        // clobber the existing config with an unusable file.
        let toml = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml)?;
        Ok(())
    }
}

impl AppTheme {
    pub fn system_default() -> Self { AppTheme::System }
}
fn default_dark() -> String { "Default Dark".into() }
fn default_light() -> String { "Default Light".into() }
fn default_font_size() -> f64 { Settings::DEFAULT_FONT_SIZE }
fn default_ui_font_size() -> f64 { Settings::DEFAULT_UI_FONT_SIZE }
fn default_language() -> String { "system".into() }
fn default_diff_side_by_side() -> bool { true }

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp dir per test so parallel runs don't collide.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muster-config-test-{}-{}", tag, uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_returns_defaults_and_creates_it() {
        let dir = temp_dir("missing");
        let path = dir.join("config.toml");

        let s = Settings::load_from(&path);

        assert_eq!(s.font_size, Settings::DEFAULT_FONT_SIZE);
        assert_eq!(s.ui_font_size, Settings::DEFAULT_UI_FONT_SIZE);
        assert_eq!(s.theme, AppTheme::System);
        assert!(path.exists(), "defaults are written back on first load");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_toml_falls_back_to_defaults() {
        let dir = temp_dir("malformed");
        let path = dir.join("config.toml");
        fs::write(&path, "this is = [not valid toml").unwrap();

        let s = Settings::load_from(&path);

        assert_eq!(s.font_size, Settings::DEFAULT_FONT_SIZE);
        assert_eq!(s.theme_dark, "Default Dark");
        assert!(!s.font_thicken);
        // The corrupt original is preserved next to the config.
        let bak = path.with_extension("toml.bak");
        assert_eq!(fs::read_to_string(&bak).unwrap(), "this is = [not valid toml");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn partial_toml_uses_defaults_for_missing_fields() {
        let dir = temp_dir("partial");
        let path = dir.join("config.toml");
        fs::write(&path, "theme = \"dark\"\nfont_size = 20.0\n").unwrap();

        let s = Settings::load_from(&path);

        assert_eq!(s.theme, AppTheme::Dark);
        assert_eq!(s.font_size, 20.0);
        // Field added after this config was written: old files lack it and
        // must fall back to the serde default.
        assert_eq!(s.ui_font_size, Settings::DEFAULT_UI_FONT_SIZE);
        assert_eq!(s.theme_dark, "Default Dark");
        assert_eq!(s.theme_light, "Default Light");
        assert!(s.font_family.is_empty());
        assert!(!s.editor_wrap_lines);
        assert!(s.diff_side_by_side);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("config.toml");

        let s = Settings {
            font_size: 17.5,
            ui_font_size: 14.0,
            font_family: "Cascadia Code".into(),
            theme: AppTheme::Light,
            ..Default::default()
        };
        s.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.font_size, 17.5);
        assert_eq!(loaded.ui_font_size, 14.0);
        assert_eq!(loaded.font_family, "Cascadia Code");
        assert_eq!(loaded.theme, AppTheme::Light);
        fs::remove_dir_all(&dir).ok();
    }
}
