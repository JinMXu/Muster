//! The app's two fallback themes (GitHub-palette defaults). The curated
//! Ghostty catalog lives in `catalog_ghostty` (generated); built-ins are
//! listed first by `available` and win name ties in `by_name`.

use super::ThemeColors;

fn default_dark() -> ThemeColors {
    ThemeColors {
        name: "Default Dark".into(),
        background: "0d1117".into(),
        foreground: "e6edf3".into(),
        cursor: "58a6ff".into(),
        accent: "58a6ff".into(),
        selection_bg: "1f6feb".into(),
        selection_fg: "ffffff".into(),
        sidebar: "010409".into(),
        divider: "30363d".into(),
        palette: [
            "484f58".into(), "ff7b72".into(), "3fb950".into(), "d29922".into(),
            "58a6ff".into(), "bc8cff".into(), "39c5cf".into(), "b1bac4".into(),
            "6e7681".into(), "ffa198".into(), "56d364".into(), "e3b341".into(),
            "79c0ff".into(), "d2a8ff".into(), "56d4dd".into(), "f0f6fc".into(),
        ],
    }
}

fn default_light() -> ThemeColors {
    ThemeColors {
        name: "Default Light".into(),
        background: "ffffff".into(),
        foreground: "1f2328".into(),
        cursor: "0969da".into(),
        accent: "0969da".into(),
        selection_bg: "ddf4ff".into(),
        selection_fg: "1f2328".into(),
        sidebar: "f6f8fa".into(),
        divider: "d0d7de".into(),
        palette: [
            "24292f".into(), "cf222e".into(), "116329".into(), "4d2d00".into(),
            "0969da".into(), "8250df".into(), "1b7c83".into(), "57606a".into(),
            "6e7781".into(), "a40e26".into(), "1a7f37".into(), "633c01".into(),
            "218bff".into(), "a475f4".into(), "3192aa".into(), "8c959f".into(),
        ],
    }
}

pub fn by_name(name: &str) -> Option<ThemeColors> {
    let built_in = [
        ("Default Dark", default_dark as fn() -> ThemeColors),
        ("Default Light", default_light),
    ];
    for (n, f) in built_in {
        if n.eq_ignore_ascii_case(name) {
            return Some(f());
        }
    }
    super::catalog_ghostty::THEMES
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .map(|t| t.to_colors())
}

pub fn default_for(dark: bool) -> ThemeColors {
    if dark { default_dark() } else { default_light() }
}

const BUILT_IN_NAMES: [&str; 2] = ["Default Dark", "Default Light"];

fn built_in_themes() -> Vec<ThemeColors> {
    vec![default_dark(), default_light()]
}

pub fn available() -> Vec<String> {
    let mut names: Vec<String> = BUILT_IN_NAMES.iter().map(|s| s.to_string()).collect();
    names.extend(
        super::catalog_ghostty::THEMES
            .iter()
            .map(|t| t.name)
            .filter(|n| !BUILT_IN_NAMES.iter().any(|b| b.eq_ignore_ascii_case(n)))
            .map(str::to_owned),
    );
    names
}

/// All themes with dark/light metadata + swatch colors, built-ins first.
pub fn available_with_info() -> Vec<super::ThemeInfo> {
    let built_ins = built_in_themes();
    let mut result: Vec<super::ThemeInfo> = built_ins.iter().map(|t| t.to_info()).collect();
    result.extend(
        super::catalog_ghostty::THEMES
            .iter()
            .filter(|t| !BUILT_IN_NAMES.iter().any(|b| b.eq_ignore_ascii_case(t.name)))
            .map(|t| t.to_colors().to_info()),
    );
    result
}