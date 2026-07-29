# 界面字号（UI Font Size）设置 — 设计

**日期**: 2026-07-29
**状态**: 已确认（用户批准设计）

## 背景与问题

右侧面板（Info/Git/Files）及 UI 外壳文字使用 Tailwind 静态小字号
（`text-xs`=12px、`text-[11px]`、`text-[10px]`、`text-[9px]`），全屏/远距离观看时
（尤其中文）无法清晰阅读，影响工作效率。现有 `font_size` 设置只作用于终端与
Monaco 编辑器，UI 外壳字号全部硬编码，不可调。

## 目标

设置中新增独立于终端字号的「界面字号」，右侧面板、侧栏、标签栏、菜单、弹窗等
全部 UI 文字跟随缩放，即时生效并持久化。

## 非目标（YAGNI）

- 不缩放布局尺寸（padding、图标、面板宽度 w-64 等），只放大文字
- 不改终端与 Monaco 编辑器字号（已有独立 `font_size`）
- 不为字号变大后的截断改布局（现有 truncate/title 兜底）

## 方案

### 数据流（复用现有设置链路）

`config.rs Settings` 新增 `ui_font_size: f64`（默认 12.0 = 现状）
→ `types.ts` 镜像 → Settings「外观」区滑块（10–16，步进 0.5）
→ `settingsStore.applyAll` 换算为 CSS 变量 `--ui-font-scale = ui_font_size / 12`
写到 `document.documentElement`
→ UI 文本通过新工具类随变量缩放。

### CSS 工具类（globals.css）

Tailwind 的 `text-xs`/`text-[Npx]` 是静态值无法跟随变量，新增：

```css
:root { --ui-font-scale: 1; }
.ui-fs-base { font-size: calc(12px * var(--ui-font-scale)); line-height: 1.4; }
.ui-fs-sm   { font-size: calc(11px * var(--ui-font-scale)); line-height: 1.4; }
.ui-fs-xs   { font-size: calc(10px * var(--ui-font-scale)); line-height: 1.35; }
.ui-fs-2xs  { font-size: calc(9px  * var(--ui-font-scale)); line-height: 1.3; }
```

### 类名替换映射（机械替换）

| 原类 | 新类 |
|---|---|
| `text-xs` | `ui-fs-base` |
| `text-[12px]` / `text-[13px]` | `ui-fs-base`（层级靠 font-weight 保持） |
| `text-[11px]` | `ui-fs-sm` |
| `text-[10px]` | `ui-fs-xs` |
| `text-[9px]` | `ui-fs-2xs` |

涉及组件：Sidebar、Header、RightSidebar、InfoPanel、GitPanel、FileTree、
ContextMenu、CommandPalette、TabSwitcher、Settings、ShortcutsHelp、UsagePanel、
App.tsx 弹窗、FilePane/DiffPane 的工具条按钮（Monaco 内容区不动）。

`text-sm`（14px）如出现于 UI 外壳，归入 `ui-fs-base` 并保留其 font-weight。

### i18n

新增 `settings.uiFontSize`：zh「界面字号」/ en「UI font size」。

## 错误处理与边界

- 配置容错：serde 默认值 12.0，旧配置文件无该字段时自动取默认（与现有字段同款模式）
- 滑块范围 10–16，越界不可能（输入受控）；Rust 侧不额外校验
- `--ui-font-scale` 未设置时 `:root` 默认 1，类在变量缺失时仍为现状字号

## 测试

- Rust：config 默认值/旧配置容错已有测试模式，跟进补 `ui_font_size` 断言
- 验证：`cargo test --lib`、`cargo clippy --all-targets`、`npm run build` 全绿
- 实机验收：拖滑块，全 UI 文字实时缩放，终端/编辑器字号不受影响
