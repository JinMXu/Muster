# 应用内快捷键帮助 — 设计

日期：2026-07-28
状态：已确认，实现中

## 背景

Muster 的快捷键已有 20+ 条（见 `App.tsx` 的 NAV_MAP），但应用内没有任何地方展示它们，用户只能读 README。需要在应用内提供随时可查的快捷键说明。

## 方案

独立快捷键弹窗，两个打开入口：

1. `Ctrl+/`（加入 NAV_MAP；与 `Ctrl+P` 一样，焦点在终端时按键留给 shell，不生效）
2. 命令面板新增「键盘快捷键 / Keyboard Shortcuts」命令（任意焦点可用）

## 组件

新增 `src/components/ShortcutsHelp.tsx`：

- 复用 `Settings.tsx` 的弹层模式：半透明遮罩（点击关闭）+ 居中卡片 + `muster-pop` 动画 + `Esc` 关闭
- 内容为静态分组表，三组：「窗口与项目」「标签与分屏」「面板与工具」（与 README 快捷键章节一致）
- 每行：左侧 `kbd` 风格按键胶囊（`font-mono`、`bg-white/[0.06]`），右侧功能说明
- 快捷键数据为组件内静态数组，所有文案走 i18n
- 列表过长时可滚动（`max-h` + `overflow-y-auto`）

## i18n

`zh.ts` / `en.ts` 各新增一组 `shortcuts.*` key：标题、3 个分组名、24 条动作说明，外加命令面板条目标签。

## 改动文件

- 新增：`src/components/ShortcutsHelp.tsx`
- 修改：`src/App.tsx`（`showShortcuts` 状态、NAV_MAP 加 `ctrl+/`、渲染弹窗）、`src/components/CommandPalette.tsx`（一条命令）、`src/lib/i18n/zh.ts`、`src/lib/i18n/en.ts`
- 后端无改动

## 验证

- `npx tsc --noEmit` 通过
- 手动：`Ctrl+/` 与命令面板均能打开；遮罩点击 / `Esc` 关闭；分组与样式正确
