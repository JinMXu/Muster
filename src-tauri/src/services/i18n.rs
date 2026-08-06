/// Lightweight string lookup: translate(user-facing key, language). No
/// external i18n crate — the set is small (used by commands.rs and the tray
/// menu), so a match table is clearer than a hash map and doesn't need
/// lazy_static.
pub fn translate<'a>(key: &'a str, lang: &str) -> &'a str {
    let zh = lang == "zh";
    match (key, zh) {
        ("no-selected-project", true) => "未选择项目",
        ("session-not-registered", true) => "会话未注册",
        ("name-already-exists", true) => "此处已存在名为 '{name}' 的项",
        ("invalid-name", true) => "名称无效",
        ("name-no-slash", true) => "名称不能包含 '/' 或 '\\'",
        ("tray-show-main", true) => "打开主窗口",
        ("tray-open-folder", true) => "打开目录…",
        ("tray-open-settings", true) => "设置",
        ("tray-quit", true) => "退出",

        // English defaults; unknown keys fall through to the key itself.
        ("no-selected-project", _) => "no selected project",
        ("session-not-registered", _) => "session not registered",
        ("name-already-exists", _) => "An item named '{name}' already exists here.",
        ("invalid-name", _) => "'{name}' is not a valid name.",
        ("name-no-slash", _) => "A name can't contain '/' or '\\'.",
        ("tray-show-main", _) => "Open Main Window",
        ("tray-open-folder", _) => "Open Folder…",
        ("tray-open-settings", _) => "Settings",
        ("tray-quit", _) => "Quit",
        _ => key,
    }
}

/// Resolve the effective UI language: explicit "en"/"zh" win; "system"
/// follows the OS display language (any Chinese variant → "zh", else "en").
pub fn effective(setting: &str) -> String {
    match setting {
        "en" | "zh" => setting.to_string(),
        _ => system_language(),
    }
}

#[cfg(windows)]
fn system_language() -> String {
    // GetUserDefaultUILanguage returns a LANGID; primary language id 0x04 is
    // Chinese (any sub-language).
    let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    if langid & 0x3ff == 0x04 {
        "zh".into()
    } else {
        "en".into()
    }
}

#[cfg(not(windows))]
fn system_language() -> String {
    "en".into()
}
