//! The app's six built-in themes (GitHub-palette defaults plus a few popular
//! ones). The full Ghostty catalog lives in `catalog_ghostty` (generated);
//! built-ins are listed first by `available` and win name ties in `by_name`.

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

fn dracula() -> ThemeColors {
    ThemeColors {
        name: "Dracula".into(),
        background: "282a36".into(),
        foreground: "f8f8f2".into(),
        cursor: "bd93f9".into(),
        accent: "bd93f9".into(),
        selection_bg: "44475a".into(),
        selection_fg: "f8f8f2".into(),
        sidebar: "21222c".into(),
        divider: "44475a".into(),
        palette: [
            "21222c".into(), "ff5555".into(), "50fa7b".into(), "f1fa8c".into(),
            "bd93f9".into(), "ff79c6".into(), "8be9fd".into(), "f8f8f2".into(),
            "6272a4".into(), "ff6e6e".into(), "69ff94".into(), "ffffa5".into(),
            "d6acff".into(), "ff92df".into(), "a4eaff".into(), "ffffff".into(),
        ],
    }
}

fn tokyo_night() -> ThemeColors {
    ThemeColors {
        name: "Tokyo Night".into(),
        background: "1a1b26".into(),
        foreground: "c0caf5".into(),
        cursor: "c0caf5".into(),
        accent: "7aa2f7".into(),
        selection_bg: "33467c".into(),
        selection_fg: "c0caf5".into(),
        sidebar: "16161e".into(),
        divider: "3b4261".into(),
        palette: [
            "15161e".into(), "f7768e".into(), "9ece6a".into(), "e0af68".into(),
            "7aa2f7".into(), "bb9af7".into(), "7dcfff".into(), "a9b1d6".into(),
            "414868".into(), "f7768e".into(), "9ece6a".into(), "e0af68".into(),
            "7aa2f7".into(), "bb9af7".into(), "7dcfff".into(), "c0caf5".into(),
        ],
    }
}

fn gruvbox_dark() -> ThemeColors {
    ThemeColors {
        name: "Gruvbox Dark".into(),
        background: "282828".into(),
        foreground: "ebdbb2".into(),
        cursor: "ebdbb2".into(),
        accent: "458588".into(),
        selection_bg: "504945".into(),
        selection_fg: "ebdbb2".into(),
        sidebar: "1d2021".into(),
        divider: "504945".into(),
        palette: [
            "282828".into(), "cc241d".into(), "98971a".into(), "d79921".into(),
            "458588".into(), "b16286".into(), "689d6a".into(), "a89984".into(),
            "928374".into(), "fb4934".into(), "b8bb26".into(), "fabd2f".into(),
            "83a598".into(), "d3869b".into(), "8ec07c".into(), "ebdbb2".into(),
        ],
    }
}

fn monokai_pro() -> ThemeColors {
    ThemeColors {
        name: "Monokai Pro".into(),
        background: "2d2a2e".into(),
        foreground: "fcfcfa".into(),
        cursor: "fcfcfa".into(),
        accent: "78dce8".into(),
        selection_bg: "403e41".into(),
        selection_fg: "fcfcfa".into(),
        sidebar: "221f22".into(),
        divider: "403e41".into(),
        palette: [
            "2d2a2e".into(), "ff6188".into(), "a9dc76".into(), "ffd866".into(),
            "78dce8".into(), "ab9df2".into(), "78dce8".into(), "fcfcfa".into(),
            "727072".into(), "ff6188".into(), "a9dc76".into(), "ffd866".into(),
            "78dce8".into(), "ab9df2".into(), "78dce8".into(), "fcfcfa".into(),
        ],
    }
}

pub fn by_name(name: &str) -> Option<ThemeColors> {
    let built_in = [
        ("Default Dark", default_dark as fn() -> ThemeColors),
        ("Default Light", default_light),
        ("Dracula", dracula),
        ("Tokyo Night", tokyo_night),
        ("Gruvbox Dark", gruvbox_dark),
        ("Monokai Pro", monokai_pro),
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

pub fn available() -> Vec<String> {
    let built_in = ["Default Dark", "Default Light", "Dracula", "Tokyo Night", "Gruvbox Dark", "Monokai Pro"];
    // Built-ins first (the app's defaults at top), then the full Ghostty catalog,
    // skipping any names that duplicate a built-in (matched case-insensitively
    // so the built-in entry always wins in `by_name`).
    let mut names: Vec<String> = built_in.iter().map(|s| s.to_string()).collect();
    names.extend(
        super::catalog_ghostty::THEMES
            .iter()
            .map(|t| t.name)
            .filter(|n| !built_in.iter().any(|b| b.eq_ignore_ascii_case(n)))
            .map(str::to_owned),
    );
    names
}