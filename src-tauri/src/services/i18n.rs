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
        ("notify-agent-waiting", true) => "{agent} 正在等待输入 — {title}",
        ("notify-agent-done", true) => "{agent} 已完成 — {title}",
        ("notify-bell", true) => "{title} — 响铃",
        ("git-open-index", true) => "打开索引",
        ("git-stage-file", true) => "暂存文件",
        ("git-write-index", true) => "写入索引",
        ("git-stage-all", true) => "暂存全部",
        ("git-head-tree", true) => "读取 HEAD 树",
        ("git-remove-path", true) => "移除路径",
        ("git-reset-to-head", true) => "重置到 HEAD",
        ("git-head-object", true) => "读取 HEAD 对象",
        ("git-reset-index", true) => "重置索引",
        ("git-clear-index", true) => "清空索引",
        ("git-write-tree", true) => "写入树",
        ("git-find-tree", true) => "查找树",
        ("git-head-commit", true) => "读取 HEAD 提交",
        ("git-commit", true) => "提交",
        ("git-set-head", true) => "设置 HEAD",
        ("git-head", true) => "读取 HEAD",
        ("git-checkout", true) => "检出",
        ("git-create-branch", true) => "创建分支",
        ("git-list-remotes", true) => "列出远程",
        ("git-find-remote", true) => "查找远程",
        ("git-push", true) => "推送",
        ("git-stash", true) => "保存暂存",
        ("git-pop-stash", true) => "弹出暂存",
        ("git-fetch", true) => "获取 {name}",
        ("git-binary-file", true) => "二进制文件",
        ("git-files-changed", true) => "确认期间文件已发生变化",
        ("git-moved-to-recycle", true) => "已将 {path} 移至回收站",
        ("git-discarded", true) => "已丢弃 {path} 中的更改",
        ("git-detached-head", true) => "分离的 HEAD",
        ("time-ago", true) => "{n}{unit}前",
        ("time-second", true) => "秒",
        ("time-minute", true) => "分钟",
        ("time-hour", true) => "小时",
        ("time-day", true) => "天",
        ("time-week", true) => "周",
        ("time-month", true) => "个月",
        ("time-year", true) => "年",
        ("diff-vs-head", true) => "{name}（对比 HEAD）",
        ("diff-vs-rev", true) => "{name}（对比 {rev}）",
        ("diff-staged", true) => "{name}（已暂存）",
        ("diff-revs", true) => "{name}（{old}..{new}）",
        ("project-default-name", true) => "项目 {n}",
        ("kill-pid-not-in-session", true) => "PID 不属于此会话",
        ("no-process-with-pid", true) => "不存在进程 {pid}",
        ("failed-to-kill-pid", true) => "无法结束进程 {pid}",
        ("invalid-exe-path", true) => "无效的可执行文件路径",
        ("context-menu-open", true) => "用 Muster 打开",
        ("trash-failed", true) => "移动到回收站失败（错误码 {code}）",

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
        ("notify-agent-waiting", _) => "{agent} is waiting for input — {title}",
        ("notify-agent-done", _) => "{agent} finished — {title}",
        ("notify-bell", _) => "{title} — Bell",
        ("git-open-index", _) => "open index",
        ("git-stage-file", _) => "stage file",
        ("git-write-index", _) => "write index",
        ("git-stage-all", _) => "stage all",
        ("git-head-tree", _) => "head tree",
        ("git-remove-path", _) => "remove path",
        ("git-reset-to-head", _) => "reset to head",
        ("git-head-object", _) => "head object",
        ("git-reset-index", _) => "reset index",
        ("git-clear-index", _) => "clear index",
        ("git-write-tree", _) => "write tree",
        ("git-find-tree", _) => "find tree",
        ("git-head-commit", _) => "head commit",
        ("git-commit", _) => "commit",
        ("git-set-head", _) => "set head",
        ("git-head", _) => "head",
        ("git-checkout", _) => "checkout",
        ("git-create-branch", _) => "create branch",
        ("git-list-remotes", _) => "list remotes",
        ("git-find-remote", _) => "find remote",
        ("git-push", _) => "push",
        ("git-stash", _) => "stash",
        ("git-pop-stash", _) => "pop stash",
        ("git-fetch", _) => "fetch {name}",
        ("git-binary-file", _) => "Binary file",
        ("git-files-changed", _) => "Files changed while the confirmation was open",
        ("git-moved-to-recycle", _) => "Moved {path} to Recycle Bin",
        ("git-discarded", _) => "Discarded changes in {path}",
        ("git-detached-head", _) => "detached HEAD",
        ("time-ago", _) => "{n} {unit} ago",
        ("time-second", _) => "second",
        ("time-minute", _) => "minute",
        ("time-hour", _) => "hour",
        ("time-day", _) => "day",
        ("time-week", _) => "week",
        ("time-month", _) => "month",
        ("time-year", _) => "year",
        ("diff-vs-head", _) => "{name} (vs HEAD)",
        ("diff-vs-rev", _) => "{name} (vs {rev})",
        ("diff-staged", _) => "{name} (Staged)",
        ("diff-revs", _) => "{name} ({old}..{new})",
        ("project-default-name", _) => "Project {n}",
        ("kill-pid-not-in-session", _) => "PID does not belong to this session",
        ("no-process-with-pid", _) => "no process with pid {pid}",
        ("failed-to-kill-pid", _) => "failed to kill pid {pid}",
        ("invalid-exe-path", _) => "invalid exe path",
        ("context-menu-open", _) => "Open in Muster",
        ("trash-failed", _) => "Move to Recycle Bin failed (error {code})",
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
