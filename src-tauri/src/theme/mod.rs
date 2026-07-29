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

impl ThemeColors {
    /// Resolve by name; falls back to the built-in default for the appearance.
    pub fn resolve(name: &str, dark: bool) -> Self {
        catalog::by_name(name).unwrap_or_else(|| catalog::default_for(dark))
    }
}