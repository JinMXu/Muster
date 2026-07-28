/// Lightweight string lookup: translate(user-facing key, language). No
/// external i18n crate — the set is small (~18 keys), so a match table is
/// clearer than a hash map and doesn't need lazy_static.
pub fn translate<'a>(key: &'a str, lang: &str) -> &'a str {
    let zh = lang == "zh";
    match (key, zh) {
        // ── commands.rs ──
        ("no-selected-project", true) => "未选择项目",
        ("session-not-registered", true) => "会话未注册",
        ("name-already-exists", true) => "此处已存在同名项",
        ("invalid-name", true) => "名称无效",
        ("name-no-slash", true) => "名称不能包含 '/' 或 '\\'",
        ("not-a-directory", true) => "不是目录",
        // ── file.rs ──
        ("cannot-read-file", true) => "无法读取文件",
        ("file-too-large", true) => "文件太大，无法打开",
        ("binary-file", true) => "二进制文件",
        // ── explorer.rs ──
        ("trash-failed", true) => "删除失败",
        // ── bootstrap.rs / tray ──
        ("new-window", true) => "新建窗口",
        ("quit", true) => "退出",
        ("muster-tray", true) => "Muster",
        // ── session.rs / notifications ──
        ("bell-notification", true) => "{} — 终端响铃",
        // ── app.rs / project ──
        ("project-fallback", true) => "项目 {}",
        // ── shell.rs ──
        ("shell-exited", true) => "[会话已退出]",

        // ── English defaults ──
        ("no-selected-project", _) => "no selected project",
        ("session-not-registered", _) => "session not registered",
        ("name-already-exists", _) => "An item named '{name}' already exists here.",
        ("invalid-name", _) => "'{name}' is not a valid name.",
        ("name-no-slash", _) => "A name can't contain '/' or '\\\\'.",
        ("not-a-directory", _) => "Not a directory",
        ("cannot-read-file", _) => "Could not read file",
        ("file-too-large", _) => "File is too large to open",
        ("binary-file", _) => "Binary file",
        ("trash-failed", _) => "Trash failed",
        ("new-window", _) => "New Window",
        ("quit", _) => "Quit",
        ("muster-tray", _) => "Muster",
        ("bell-notification", _) => "{} — Bell",
        ("project-fallback", _) => "Project {}",
        ("shell-exited", _) => "[session exited]",
        // Unknown keys fall through to the original key itself.
        _ => key,
    }
}
