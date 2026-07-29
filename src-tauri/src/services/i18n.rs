/// Lightweight string lookup: translate(user-facing key, language). No
/// external i18n crate — the set is small (5 keys, all used by commands.rs),
/// so a match table is clearer than a hash map and doesn't need lazy_static.
pub fn translate<'a>(key: &'a str, lang: &str) -> &'a str {
    let zh = lang == "zh";
    match (key, zh) {
        ("no-selected-project", true) => "未选择项目",
        ("session-not-registered", true) => "会话未注册",
        ("name-already-exists", true) => "此处已存在名为 '{name}' 的项",
        ("invalid-name", true) => "名称无效",
        ("name-no-slash", true) => "名称不能包含 '/' 或 '\\'",

        // English defaults; unknown keys fall through to the key itself.
        ("no-selected-project", _) => "no selected project",
        ("session-not-registered", _) => "session not registered",
        ("name-already-exists", _) => "An item named '{name}' already exists here.",
        ("invalid-name", _) => "'{name}' is not a valid name.",
        ("name-no-slash", _) => "A name can't contain '/' or '\\\\'.",
        _ => key,
    }
}
