pub mod catalog;
mod catalog_ghostty;
pub mod fonts;

use serde::{Deserialize, Serialize};

/// Resolved color palette for window chrome and terminal alike.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub accent: String,
    pub selection_bg: String,
    pub selection_fg: String,
    pub sidebar: String,
    pub divider: String,
    pub palette: [String; 16],
}

/// Lightweight theme metadata for the settings picker: name, dark/light
/// classification, and two swatch colors for a visual preview.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeInfo {
    pub name: String,
    pub is_dark: bool,
    pub background: String,
    pub accent: String,
}

impl ThemeColors {
    /// Resolve by name; falls back to the built-in default for the appearance.
    pub fn resolve(name: &str, dark: bool) -> Self {
        catalog::by_name(name).unwrap_or_else(|| catalog::default_for(dark))
    }

    /// Classify as dark or light from the background's relative luminance.
    pub fn is_dark(&self) -> bool {
        let bg = self.background.trim_start_matches('#');
        if bg.len() < 6 {
            return true;
        }
        let r = u8::from_str_radix(&bg[0..2], 16).unwrap_or(0) as f32;
        let g = u8::from_str_radix(&bg[2..4], 16).unwrap_or(0) as f32;
        let b = u8::from_str_radix(&bg[4..6], 16).unwrap_or(0) as f32;
        (0.299 * r + 0.587 * g + 0.114 * b) < 128.0
    }

    pub fn to_info(&self) -> ThemeInfo {
        ThemeInfo {
            name: self.name.clone(),
            is_dark: self.is_dark(),
            background: self.background.clone(),
            accent: self.accent.clone(),
        }
    }
}