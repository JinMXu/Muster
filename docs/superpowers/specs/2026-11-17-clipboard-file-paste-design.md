# 终端「复制文件」粘贴与外部文件拖放 — 设计

**日期**: 2026-11-17
**状态**: 已确认（用户批准设计）

## 背景与问题

在资源管理器里复制文件（Ctrl+C）后，终端的粘贴（Ctrl+V、右键智能粘贴/菜单粘贴）
完全无反应。原因：Windows 为复制的文件在剪贴板上放的格式是 **CF_HDROP**
（文件句柄列表），不含 `text/plain`；而 Muster 的粘贴链路只读文本——

- `terminalRegistry.ts` Ctrl+V 分支 → `navigator.clipboard.readText()`
- `TerminalPane.tsx` `doPaste()` → 同样 `readText()`

两者都以 `if (text)` 守卫，空串静默跳过。WebView 的 Clipboard API 拿不到
CF_HDROP（`navigator.clipboard.read()` 也只给无磁盘路径的 File blob），所以
必须从 Rust 侧读。Windows Terminal 支持此场景正是因为它在原生层调
`GetClipboardData(CF_HDROP)` 转成带引号路径再写入 conpty。

同理，从资源管理器往终端窗格拖文件也不支持：现有 onDrop 只认内部协议
`application/x-muster-path`（FileTree 拖出），OS 来源拖放被 Tauri
`dragDropEnabled` 拦截后从未被消费。

## 目标

1. 复制文件后，Ctrl+V / 右键粘贴在终端插入带引号的文件路径（多文件空格分隔）
2. 从资源管理器拖文件到终端窗格，插入同样的路径串，并复用现有 ring 高亮反馈

## 非目标（YAGNI）

- 不给文件路径走 `looksDangerousPaste` 危险粘贴弹窗——路径是不透明数据，
  与现有拖放落点（`TerminalPane.tsx` 直接 `sendText`）行为一致
- 不改 Ctrl+V 现有「文本无警告」行为（与右键粘贴的警告行为差异维持现状）
- 不支持非文件剪贴板格式（CF_UNICODETEXT 之外的图片等已有各自通道）
- 不改现有 FileTree 内部拖拽路径（`application/x-muster-path`）
- cmd.exe 的单引号语义问题不做特殊处理（见「已知限制」，与拖放一致）

## 方案取舍

- **读 CF_HDROP**：不用 `clipboard-win`/`arboard`（加依赖），
  不用 tauri-plugin-clipboard-manager（不支持文件列表）。直接用已有的
  `windows` crate——`Win32_UI_Shell` + `Win32_UI_WindowsAndMessaging`
  feature 均已启用，零新依赖。
- **外部拖放拿真实路径**：HTML5 `dataTransfer.files` 在 WebView2 里无磁盘
  路径且被 `dragDropEnabled` 拦截，只能走 Tauri 原生
  `onDragDropEvent`（payload 自带 `paths`）。该事件仅对 OS 来源拖放触发，
  与 HTML5 内部拖拽互不干扰，不会双写。

## 数据流

### A. Rust：`read_clipboard_files` 命令

新文件 `src-tauri/src/commands/clipboard.rs`：

```
IsClipboardFormatAvailable(CF_HDROP)?
  → 否: 返回空 Vec
  → 是: OpenClipboard(null hwnd)（被占用则重试，5 次 × 2ms，仍败返空）
        GetClipboardData(CF_HDROP) → HDROP
        DragQueryFileW(count) → 循环 DragQueryFileW(i) 取宽字符 → String::from_utf16_lossy
        CloseClipboard（guard 保证释放）
  → Vec<String>
```

- 注册进 `commands/mod.rs::register_all`
- `cargo run --bin export_bindings` 重新生成 `src/lib/bindings.ts`
- `invoke.ts` 的 `api` 增加一行 `readClipboardFiles`（透传 commands）
- 非 Windows 提供 `#[cfg]` stub 返回空 Vec（项目虽只跑 Windows，与现有
  `cfg(windows)` 依赖习惯一致）

### B. 前端：统一粘贴入口

新 `src/lib/terminalPaste.ts`：

- 纯函数 `formatFilePaths(paths: string[]): string`
  = `paths.map(shellQuotePath).join(" ")`（复用 `shellEscape.ts`，
  PowerShell 单引号即字面量；与拖放同一约定）
- `getPastePayload(): Promise<{kind:"text",text} | {kind:"files",text} | {kind:"none"}>`
  = 先 `readText()`；为空再 `readClipboardFiles()`（失败视作空）；
  files 的 `text` 在此处即 `formatFilePaths(paths)` 的结果，调用方拿到直接
  `sendText` 即可；两路皆空 → `none`

两个粘贴点只补 files 分支、行为不变：

- `TerminalPane.doPaste`：text → 现有 PasteWarning 流程原样；files → 直接
  `api.sendText(sessionId, formatFilePaths(...))`
- `terminalRegistry.ts` Ctrl+V：text → 原样直发；files → 同上直发

### C. 外部拖放（Explorer → 终端窗格）

- 新 `src/lib/externalDropStore.ts`：模块级 store（照 `terminalSearch.ts`
  模式），状态 `{ sessionId: string | null }` + subscribe/set
- 新 `src/hooks/useNativeFileDrop.ts`，App 挂载一次：
  - `enter`/`over`：`elementFromPoint(物理坐标 ÷ devicePixelRatio)` →
    `closest("[data-terminal-pane]")` → 写 store；未命中 → 清空
  - `drop`：重做命中测试；命中终端 → `api.sendText(sessionId, formatFilePaths(paths))`；
    未命中（FileTree/编辑器等）→ 不动作；清 store
  - `leave`：清 store
- `TerminalPane` 以 `useSyncExternalStore` 订阅，外部拖放命中本窗格时
  复用现有 `ring-1 ring-inset ring-muster-accent` 高亮（与 `dropActive` 同样式）

## 错误处理

- 剪贴板命令任何失败 → 空数组 → 前端静默（等价今天的行为）
- drop 坐标换算/命中失败 → 不动作，绝不误插到错误窗格
- 剪贴板被其他进程长时间占用 → 重试后放弃，返回空

## 测试

- vitest 新增 `terminalPaste.test.ts`：`formatFilePaths` 单文件、多文件空格
  join、含单引号路径转义（`'` → `''`）、空数组
- Rust 侧读剪贴板依赖真实系统剪贴板状态，不做单测，手测覆盖：
  单文件/多文件复制粘贴、跨进程占用剪贴板、外部拖放单/多文件、
  外部拖放到非终端区域不误插、FileTree 内部拖拽回归

## 已知限制

- 单引号包裹在 cmd.exe 中不构成引用（PowerShell/WSL/bash 下正确）；
  与现有拖放路径行为一致，未来如需按 shell 类型切换引用规则再议
- `elementFromPoint` 命中测试依赖 `[data-terminal-pane]` 属性，
  隐藏 tab 下的窗格不可命中（不可见即不可点，行为正确）

## 实现差异（2026-11-17 当天补记）

C 节的 Tauri `onDragDropEvent` 方案**落地时被推翻**：本仓库
`tauri.conf.json` 刻意配置了 `"dragDropEnabled": false`——该 flag 打开后
wry 会对每个 WebView2 子窗口 `RevokeDragDrop` 并替换 Chromium 的 drop
处理器，直接杀死 FileTree→终端、窗格重排等页内 HTML5 拖拽；flag 关闭时
`onDragDropEvent` 则永远不会触发（首次实现因此无效）。

替代尝试：`services/native_drop.rs` 自建链式 IDropTarget（通过
`OleDropTargetInterface` 属性找到 Chromium 的目标并转发）。

**该方案经实测被否决**（启动即 AV，probe 数据见下）：
- 找到的目标窗口是 `Chrome_WidgetWin_1`，属于 **WebView2 host 进程**
  （pid 不同于主进程）——窗口所有权在别的进程，`RegisterDragDrop`/
  `RevokeDragDrop` 无法在非所有者进程上操作；
- 真实 Windows 上 `OleDropTargetInterface` 属性存的是 ole32 内部包装
  指针，不是裸 IDropTarget 指针（自注册对比：prop ≠ 传入指针），直接
  QI 解引用 = 访问违例。

外部文件拖放在多进程 WebView2 + 保留 HTML5 拖拽的约束下暂无可靠原生路
径；粘贴路径（CF_HDROP → `read_clipboard_files`）不受影响，独立可用。
拖放功能保留前端代码（useNativeFileDrop/externalDropStore）但后端不
安装，后续如 WebView2/tauri 提供受支持 API 再启用。
