# Kero Windows — 开发进度

**最后更新**: 2026-07-29（第十二轮）

---

## 整体状态：Rust 后端 ✅ | React 前端 ✅ | 单测 77 ✅ | clippy 0 警告 ✅ | i18n 中英文 ✅ | 界面字号设置 ✅（待实机验收）

### 构建状态
| 目标 | 状态 | 备注 |
|------|------|------|
| `cargo check` / `cargo build` | ✅ 通过 | 无需 vcvars |
| `cargo test --lib` | ✅ 77 个测试全过 | 含 procs 路径匹配 + 2 个实进程 ignored 测试手动通过 |
| `cargo clippy --all-targets` | ✅ 零警告 | |
| `npm run build` | ✅ 通过 | `tsc && vite build` 零 TS 错误 |
| `npm audit` | ✅ 0 漏洞 | dompurify 经 overrides 升到 3.4.12 |
| 实机验证 | ⚠️ 部分 | 第 0-1 批 + UI 重设计已目检；界面字号滑块、CSP 启用后实机验收待做 |

---

## 本轮已修复（2026-07-29 第十二轮：界面字号设置）

问题：右侧面板（Info/Git/Files）及 UI 外壳文字硬编码 9-12px，全屏/远距离看不清。
设计 spec：`docs/superpowers/specs/2026-07-29-ui-font-size-design.md`（用户已批准）。

- `Settings` 新增 `ui_font_size`（默认 12，10–16 滑块，Settings 外观区，i18n 中英）
- `settingsStore.applyAll` 换算 `--ui-font-scale = size/12` 写 `:root`，加载/保存即时生效
- `globals.css` 新增 `ui-fs-base/sm/xs/2xs` 四个缩放工具类（calc(px × scale) + 相对行高）
- 14 个组件 112 处硬编码字号类机械替换（`text-xs`→base、`text-[11px]`→sm、
  `text-[10px]`→xs、`text-[9px]`→2xs），替换后 src 下静态小字号类 0 残留
- 终端与 Monaco 编辑器字号不受影响（独立 `font_size`）；布局尺寸不缩放

验证：`cargo test` 77 ✅、`clippy` 0 警告 ✅、`npm run build` ✅。实机拖滑块验收待做
（顺带验收第十一轮的 CSP：Monaco worker / xterm / 图片预览 / HMR）。

---

## 本轮已修复（2026-07-29 第十一轮：全面审计 + 四批修复）

四路并行审计（Rust 安全 / Rust 质量 / 前端 / 依赖仓库卫生）后按批修复：

**第一批（正确性）**
- `rename_path`/`create_file` 路径逃逸：新增 `rename_target()` 助手，直接校验入参名字本身
  （含 Windows `C:x` drive-relative 逃逸），join 后断言父目录不变；补 3 个单测
- 快照/配置序列化失败不再写空串覆盖现有文件（persist.rs/config.rs 改为 `?` 传播）
- git stash 失败不再假成功（`try_git!` 传播）；split 不再吞 PTY spawn 失败（死终端有报错）
- 新增原子命令 `close_tab(tab_id)`（`AppState::close_tab`），消除 select+close 两步竞态
- 图片预览 `file://` 改 `convertFileSrc` + 启用 assetProtocol（scope `**`）
- FilePane 卸载 flush 防抖回写（不再丢最后一笔编辑）+ fileInfo/diffInfo/轮询 alive 守卫
- npm：`overrides` 强制 `dompurify@^3.4.12`，audit 归零

**第二批（资源泄漏）**
- terminalRegistry 改模块级单例 listen 按 `payload.id` 分发（原每会话 2 个 listen 永不 unlisten，
  prune 形同虚设）；useTauriEvent + FileTree/UsagePanel 的异步 listen 加 cancelled 守卫
- 副窗口快照：启动时 `prune_secondary_snapshots()` 清孤儿 `sessions-win-*.json`，
  Destroyed 时 `delete_snapshot_for(label)`；Job Object 句柄在读循环退出路径也 `untrack_session`
- Header 进度条 timeout/条目清理、App subtitleCache 按存活 tab prune、InfoPanel 轮询守卫

**第三批（清理）**
- 删除死模块 `history.rs`（连带 `history_dir()`、`terminal_restore_history` 设置及前端假开关）；
  删除 ~10 处无调用方 pub 方法/字段；i18n.rs 裁到 5 个实际使用的 key（修复中文缺 `{name}` 占位符）
- 依赖裁剪：Rust 删 `chrono` + `tauri-plugin-updater`（含 bootstrap 注册/conf/capabilities/npm 包）；
  tokio `full` → `rt`；usage 重复时间解析器上移到 provider.rs
- 启用 CSP（`default-src 'self'` + asset:/blob:/ws: 白名单，**需实机验证**）；
  子进程统一 `quiet_command()`（CREATE_NO_WINDOW）；git.rs 乱码注释修复
- 前端散件：死 import/死按钮/死包装器清理、快捷键事实源对齐、i18n `TKey` 接线
  （顺带修了 `git.detached` 真实拼写错误）

**第四批（架构）**
- `commands.rs`（987 行 84 命令）拆分为 `commands/` 子模块（state/window/project/terminal/tabs/
  panes/editor/fs/git/usage）；fs 业务逻辑下沉 `services/explorer.rs`（含全部 10 个测试搬家）；
  命令名/参数/签名零变化
- usage 扫描器不再持锁做文件 I/O（锁内快照 → 释放 → 解析 → 重新取锁提交，`SCAN_SERIAL` 串行化）
- watch 按窗口 label 分桶：监视集互不覆盖、`fs-changed` 改 `emit_to(label)`、窗口销毁释放 watcher
- 杂项：`save_settings` 单 guard；app.rs 两处双 find+unwrap 合并；config 解析失败备份
  `config.toml.bak` + warn；`SharedState::get_label`/`for_label` 拆分（迟到 invoke 不再复活空状态）

**未做（记录在案）**：Monaco 主题不随全局主题（固定 muster-dark）；`useProjectCwd` 多路重复轮询
未合并；session.rs model/service 拆分、App.tsx hook 抽取（更大重构，另议）；
`.cargo/config.toml` 移出 git 跟踪需手动 `git rm --cached`；`sidebar.sendFeedback` 死翻译 key 保留。

---

## 本轮已修复（2026-07-28 第十轮：中英文 i18n 切换）

全部 ~140 条 UI 字符串从硬编码迁移到 `useT()` hook，支持中文/英文/跟随系统三种模式。

1. **前端 i18n 基础设施** — `src/lib/i18n/` 新建 4 文件：手写 `LanguageProvider` +
   `useT` hook（零依赖，~80 行），`en.ts`/`zh.ts` 翻译表按组件嵌套（类型安全
   key 路径，`t("info.noListeningPorts")` 自动补全），支持 `{param}` 插值和
   `(n) => string` 函数式复数；`LanguageProvider` 挂载时设置
   `document.documentElement.lang`
2. **App.tsx 语言状态** — `lang` 状态提升到 App，挂载 Provider；跟随系统
   （`navigator.language` 检测），Settings 有具体语言时覆盖
3. **Settings 语言选择器** — `types.ts` 加 `language: "system" | "en" | "zh"`
   字段，Settings UI 新增下拉框（系统/English/中文），切换即时生效，Save 持久化
4. **全组件迁移** — 18 个 React 组件（App / InfoPanel / GitPanel / CommandPalette /
   FileTree / Header / Sidebar / RightSidebar / TerminalPane / DiffPane /
   FilePane / PaneLayout / Settings 等）全部硬编码替换为 `t()` 调用，涵盖标签、
   按钮、提示、菜单、弹窗、空状态、tooltip、右键菜单等
5. **Rust 后端翻译** — 新建 `services/i18n.rs`（translate 函数，18 个 key 中英
   对照表）；`config.rs` + `types.ts` 同步加 language 字段；`commands.rs` 5 处
   用户可见错误消息接入 translate()（会话未注册、文件名无效、重名等）
6. **测试** — `cargo test --lib` 58 全过；`cargo clippy` 零警告；`npm run build`
   零 TS 错误
7. **修复（实机发现）** — 带参数翻译全部显示 `[object Object]`：`t()` 把参数对象
   整体作为单个实参传入，而翻译函数声明的是位置参数。已将 en/zh 全部 33 个
   函数式翻译改为解构参数 `({ n }: { n: number }) => ...`；顺带把 Stash 中文
   从「暂存工作区」改为「储藏」（避免与 stage「暂存」混淆且修复按钮折行）

### 不翻译的内容
CSS class、Monaco 编辑器内部 UI、xterm.js 终端、shell 名称标识符（"PowerShell"
等）、窗口控制按钮 aria-label（图标视觉足够）、Git diff/文件内容

---

## 本轮已修复（2026-07-27 第八轮：三面板对齐原版 kero）

对照原版源码（`.analysis/kero-original`）逐项补齐 Info/Files/Git 面板差距：

1. **根锚定语义** — 新命令 `resolve_project_root`（git2 discover 向上找最近 repo）；
  `useProjectCwd` 返回 `{root, cwd}`：pinned 目录优先，否则锚定最近 git repo（带缓存），
  不再跟随 `cd` 重定根；InfoPanel 拆出 Project Directory（AUTO）/ Current Directory（≠root 才显示）两行
2. **Git Discard 指纹复核 + 操作横幅** — `GitGuard`（HEAD oid + branch + 文件 exists/size/mtime
  快照），确认时复核，变了整个操作作废（"Files changed while the confirmation was open"）；
  全部 git 操作接入 ops banner（running/ok/failed + 可展开 transcript：等效 git 命令行 + 输出）
3. **Info 进程/端口监控** — 新模块 `services/procs.rs`（sysinfo 0.30）：shell 后代进程
  BFS（CPU%/内存/hover 杀进程/右键菜单）；netstat 解析监听端口（点击开浏览器、右键 Kill）；
  手动刷新按钮；stale pid 防护
4. **Git 批量操作 + 行右键 + 提交菜单 + 过滤条** — `git_unstage_all`（有/无 HEAD 两路）；
  区头 Unstage All / Stage All / Discard All（走指纹复核流程）；三个区行右键菜单
  （Open Changes/Open File/Stage/Discard/Reveal/Copy Path/Insert Path in Terminal 等）；
  提交拆分按钮（Commit Staged / Stage All & Commit / Amend / Stage All & Amend）+
  禁用规则（空消息/有冲突/无 staged）；变更文件过滤条（激活时隐藏批量操作和 recent commits）
5. **Files 内联草稿行 + 重命名完善** — 新建文件/文件夹改内联草稿行（VS Code 式
  Enter/Esc/失焦语义）；重命名前后端双重校验（`/` `\` `.` `..`、重名提示、仅大小写允许）；
  目录改名 remap 整棵子树展开态；已打开 tab 路径跟随（`rename_path` 更新 open FileTabs）；
  聚焦 pane 的文件在树中高亮联动
6. **小修** — recent commits 真实相对时间（`relative_date` 助手 + 18 断言）；
  提交/stash 签名读 git config（回退 Kero <kero@local>）；fetch 改为全 remote + prune
  （单个 remote 失败不阻塞）；staged rename 填充 `orig_path`（行内显示 old → new）

原版仓库克隆在 `.analysis/kero-original/` 供后续对照。**原版没有、本地更优的点**：
fs 监听自动刷新、文件树 hover 按钮、Git 信息进 Info 面板、双击重命名。

已知残留差异（低优先）：porcelain v2 `-z` rename 完整语义（undo rename）、
intent-to-add discard 特例、操作横幅持久 transcript 历史、进程树 ppid 断裂
（中间进程退出后孙进程丢失）、Windows 只有 force-kill 语义。

---

## 本轮已修复（2026-07-27 第二轮）

PROGRESS 记录的 3 个 TS 错误实际已部分修过，但 `App.tsx`、`CommandPalette.tsx`、`GitPanel.tsx`
三个文件存在**编码损坏**：emoji/符号字符曾被按 GBK 错误转码（`▶` → `鈻?` 等），
吃掉了字符串的结束引号和 JSX 的 `<`，造成 140+ 条 TS 语法错误。已全部修复：

- `App.tsx` — 注释中的 `—` 复原；EmptyState 的 logo 字符改为 `◆`
- `CommandPalette.tsx` — 全部命令图标复原；`focusPane("bottom")` → `focusPane("down")`
  （`FocusDirection` 的 serde 值是 `up/down/left/right/next/previous`，无 `bottom`）
- `GitPanel.tsx` — 图标/省略号/`↑${info.ahead}` 模板字符串复原；
  Merge Changes 区的 `onOpenDiff={() => ... e.path ...}` 修复为 `(e) => ...`（`e` 原本未定义）；
  `stageLabel="−"` 复原

---

## 本轮已修复（2026-07-27 第三轮：右侧面板补全）

后端命令原本就齐全，缺的主要是前端接线。本轮补全：

- **FileTree.tsx（重写）** — 真正的懒加载展开（按目录缓存 children，展开时才 `list_directory`）；
  双击或悬停按钮 inline 重命名（接 `rename_path`）；根级和目录级的「新建文件/文件夹」按钮
  （接 `create_file`，之前传给 `Row` 的 `onNewFile` 是没渲染的死代码）；新建/重命名/删除后自动刷新
- **InfoPanel.tsx** — 新增 Shell PID（后端 `SessionInfo` 加了 `pid` 字段，取自 portable-pty
  `Child::process_id()`）；新增 Git 区（branch / remote / ahead/behind，4s 轮询 `git_status`）；
  自动目录显示 `(AUTO)` 标记
- **GitPanel.tsx** — 分支下拉切换（接 `git_switch_branch`）+ 「+」新建分支按钮
  （接 `git_create_branch`，后端创建后自动切换）；Changes 区每行加 discard 按钮
  （接 `git_discard`，带确认）

验证：`npm run build` ✅、`cargo check` ✅

---

## 本轮已修复（2026-07-27 第四轮：路线图第 1 批）

对照原版的功能差距清单（22 项）按优先级逐批修复。第 0-1 批完成：

- **0. 实机冒烟** — `npx tauri dev` 首次跑通（窗口正常、终端可交互）。
  注意：1420 端口被占用是残留的 vite 进程，`taskkill` 后即可。
- **1. 终端内容保持** — 新增 `src/lib/terminalRegistry.ts`：xterm 实例按 sessionId 全局保活
  （监听器/输入都只注册一次），`TerminalPane` 只负责 attach/detach DOM 元素；
  `App.tsx` 用 `pruneSessions` 在 state-changed 时回收已关闭会话的实例。
  切 tab / zoom / 切项目不再丢终端内容。
- **2. 会话持久化恢复** — 新增 `AppState::restore()`（重建 projects/tabs/splits/cwd/选中态），
  bootstrap 改为全量恢复 + 每 5s 后台自动保存（原来只恢复侧栏可见性）。
  **顺带修了潜伏 bug**：setup 创建的初始 session 从不 spawn PTY（首个终端是死的），
  现在 bootstrap 统一为未 spawn 的 session 补 spawn + 读循环。
- **3. OSC 7/9;9 cwd 跟踪** — PowerShell 系 shell 改为通过
  `%APPDATA%/kero-dev/shell-integration.ps1`（自动生成，dot-source 用户 $PROFILE 后包装
  `prompt` 发 OSC 9;9）启动；PTY 读循环扫描 OSC 7/9;9 更新 `working_directory`。
- **4. 脏文件关闭提示** — 新命令 `tab_dirty_files` / `project_dirty_files` / `save_file`；
  关 tab/项目时若有未保存文件弹三键对话框（Save & Close / Don't Save / Cancel）。
- **5. 小修合集** — `resizeTerminal` 包装器 `rows: cols` bug 修复（删掉多余的
  `resizeTerminalRows`）；Ctrl+K Clear Terminal 实现（新命令 `clear_terminal`，
  前端清 xterm 缓冲 + 后端发 `clear`/`cls`）；git discard 支持 untracked 文件
  （走回收站，`explorer::trash` 从 `trash_file` 命令抽为共享函数）。

验证：`npm run build` ✅、`cargo check` ✅、单实例 argv 路由已在 bootstrap 确认存在（第 18 项大半已完成）

---

## 本轮已修复（2026-07-27 第五轮：第 2-4 批，路线图 6-16 项）

全部委托子代理实现、主会话验收。每批 `npm run build` + `cargo check` 均绿。

- **6. Monaco 编辑器** — `@monaco-editor/react` 4.7 + monaco-editor 0.56，全离线
  （`loader.config({ monaco })`，5 个 worker 走 Vite `?worker`，注意 0.56 的 exports map
  要求 `monaco-editor/editor/editor.worker?worker` 这种无前缀路径）。语言按扩展名映射
  （`src/lib/monaco.ts`，toml→ini）。编辑 300ms 防抖回写；`file-saved` 事件清除脏标记；
  Ctrl+S 在 Monaco 内部绑定（全局快捷键监听器会跳过它的内部 textarea）。
- **7. Monaco diff** — `<DiffEditor>` 只读，Split/Unified 切换 + Reload 按钮。
- **8. 上下文菜单三件套** — 全局 menuStore（useSyncExternalStore）+ 单一 `<ContextMenu>`。
  终端（Copy/Paste/Select All/Split）、标签头（Rename/自动标题/Reveal/Copy Path/
  Close/Close Others/Close to Right/Close All）、项目行（Rename/Set Directory/自动目录/Close）。
  新后端命令：`rename_project`、`set_project_directory`、`close_other_tabs`、
  `close_tabs_to_right`、`close_all_tabs`、`pane_context_path`。批量关闭走脏文件检查。
  顺带修复：点非选中标签的 × 原来会关错标签。
- **9. Ctrl+Tab 切换器** — 按住循环、松开提交、Esc 取消、单击直达；
  独立 capture 监听器（xterm 内部 textarea 不再挡路）；窗口失焦自动关闭不提交。
- **10. 分屏调整** — 分隔条拖拽（新命令 `resize_pane_divider`，rAF 节流）+
  Ctrl+Alt+Shift+方向键 resize + Ctrl+Alt+方向键 focus。
  **顺带修复**：NAV_MAP 修饰键顺序错误导致 ctrl+alt+[ / ] 一直是死键。
- **11. 命令面板补全** — 新增 18 条（Split Left/Up、Resize×4、Clear Terminal、
  Close Project、Save File、Switch Project 1-9、Settings）。
- **12. Ctrl+1-9 切换项目** — 快捷键穿透 INPUT/TEXTAREA 白名单。
- **13. 主题** — 592 个 Ghostty 主题（iTerm2-Color-Schemes 仓库 ghostty 目录，
  生成器 `scripts/generate-theme-catalog.mjs`，输出 `theme/catalog_ghostty.rs`；
  accent=palette[4]、sidebar=bg×0.7、divider=palette[8]）+ 6 个内置主题置顶去重；
  xterm 终端实时换肤（`terminalRegistry.applyTerminalTheme`），设置保存后全量应用。
- **14. 文件树右键菜单 + notify 监听** — Open/Open to Side/Open in Default App/
  Reveal/Copy Path/cd Here/新建/Rename/Trash；`notify` crate 非递归监听已展开目录，
  300ms 防抖后 `fs-changed` 事件驱动局部刷新（新模块 `services/watch.rs`）。
- **15. 拖拽** — 文件树→终端粘贴路径（带空格自动加引号，不落回车）；
  pane 顶部 6px 握把拖拽换位（新命令 `move_pane`，四象限最近边吸附高亮）。仅限 tab 内。
- **16. bell 通知 + OSC 9;4** — 读循环改为增量式 OscScanner 状态机（顺带替换了
  原来的滚动缓冲区扫描，并修了 BEL 被 OSC 终止符误计的问题）；窗口失焦才通知、
  每会话 2s 节流（tauri-plugin-notification）；OSC 9;4 进度 → 标签条 2px 进度条 +
  Windows 任务栏进度（最近活跃会话优先）。附 6 个单元测试（项目首批测试）。

### 路线图（剩余）

- 第 5 批：17 多窗口（L）→ 18 Explorer 集成验证（S）→ 19 托盘（S）→
  20 内置字体（S）→ 21 自动更新（S，等有发布渠道）→ 22 测试体系（M）

---

## 本轮已修复（2026-07-27 第六轮：第 5 批，路线图 17-22 项）

- **17. 多窗口** — `SharedState` 重构为「窗口 label → AppState」注册表（Settings 全局共享）；
  ~35 个命令改为按调用窗口解析状态；`state-changed` / `pty:*` 事件全部按窗口隔离
  （`emit_to(label)`）；任务栏进度/通知归属会话所在窗口；Ctrl+Shift+N 开新窗
  （`bootstrap::spawn_window`，chrome 配置与主窗一致）；按窗口快照
  （`sessions-<label>.json`）、关窗只清理本窗会话、最后一窗关闭时隐藏进托盘。
- **18. Explorer 集成入口** — Settings 新增 Integrations 区，一键安装
  「Open in Kero」右键菜单（需管理员权限，失败时显示错误）。
- **19. 托盘 + 后台存活** — 托盘菜单 New Window / Quit；关闭最后一个窗口 = 隐藏而非退出
  （会话保持存活，快照照存）；Quit 时保存全部快照并终止所有会话；
  二次启动（单实例）自动取消隐藏主窗。
- **20. 内置字体** — JetBrains Mono Regular/Bold + Symbols Nerd Font 打入
  `src-tauri/assets/fonts/`，启动时 `AddFontMemResourceEx` 进程私有注册。
- **21. 自动更新** — **推迟**：需要发布渠道（GitHub Releases 端点 + pubkey），届时再配。
- **22. 测试体系** — `cargo clippy --all-targets` 零警告（修 12 处）；单测从 6 → **35 个**
  （pane 分割/移动/调整不变量、restore 快照往返、config 容错、base64）；
  顺带修复测试发现的 `PaneTab::focus(Left)` 越界 panic。

至此路线图 22 项全部闭环（21 项推迟有因）。验证：`cargo test` 35/35 ✅、`clippy` 0 警告 ✅、
`npm run build` ✅

---

## 本轮已修复（2026-07-27 第七轮：前端重设计 A·分层无框）

设计文档：`docs/superpowers/specs/2026-07-27-ui-redesign-design.md`（头脑风暴确认：
macOS 原生感、暗色优先、视觉+局部结构、分两阶段）。

**阶段一（令牌 + 主布局）**：
- `globals.css` 令牌重构：新增 `--kero-bg-float`（L0 窗口底 #24292f）/ `--kero-panel` /
  `--kero-hover` / `--kero-selected` / `--kero-motion`（140ms cubic-bezier(.2,.8,.3,1)）；
  `--kero-bg` 语义改为 L1 内容区；全局 6px 细滚动条；antialiased
- `theme.ts` 派生：L0=bg→白6%、panel=L0→白3%、L1=bg→黑12%（592 主题零改动）
- 全去边框：侧栏/右面板/Header/pane 分割线删除，层级靠底色差；
  pane 间 2px 间隙露 L0 做隐形分隔；分隔条 handle 隐形、hover 显示 accent；
  终端聚焦改 40% accent inset 微光（`.kero-pane-focused`）
- Monaco 定义 `kero-dark` 主题（编辑器底色对齐 L1；静态值，不随主题切换）

**阶段二（组件细节）**：
- 五个弹层（ContextMenu/CommandPalette/TabSwitcher/Settings/脏文件弹窗）统一：
  10px 圆角、8% 白边、`0 12px 32px rgba(0,0,0,.5)` 阴影、`.kero-pop` 出现动画
  （scale(.97)→1 + fade；Settings 拆分层避免 translate 冲突）
- 按钮/行项 hover 统一进令牌（行 5% / 按钮 8% / active scale(.97)）；
  清除全部 `border-white/10`、`rounded-xl`、`shadow-2xl` 残留

**自动项目 Files/Git 面板为空修复（补充）**：新增 `lib/useProjectCwd.ts`
（固定目录 → 会话实时 cwd 回退），FileTree/GitPanel 接入。

**图标重绘 + 窗口控制（补充）**：
- 新增 `components/icons.tsx`：19 个手绘 SVG 图标（stroke 1.5、round caps、currentColor、
  Lucide 风格），替换全部 Unicode/emoji 图标（侧栏底部、右面板 tab、Header 按钮、
  文件树文件夹/文件/箭头、GitPanel、命令面板搜索框）
- 新增 `components/WindowControls.tsx`：无边框窗口的 Windows 风格控制按钮
  （最小化/最大化-还原切换/关闭，close hover 红底 #e81123），挂在 Header 最右侧

验证：`npm run build` ✅（两阶段均零 TS 错误）；实机目检通过

---

## 下一步

在跑的开发实例里抽查第 2-4 批（tauri watch 自动重编译）：

1. 打开 .ts 文件 → Monaco 高亮/查找；改动后 Ctrl+S 保存、脏标记消失
2. Git 面板点改动文件 → Monaco diff，切换 Split/Unified
3. 右键：终端（Copy/Paste/Split）、标签头（Close Others 等）、项目行（Rename）
4. Ctrl+Tab 按住循环切换；Ctrl+1-9 切项目
5. 拖分隔条调整分屏；拖 pane 顶部握把换位；拖文件到终端粘贴路径
6. 设置里换主题（592 个），终端颜色实时变化
7. 终端里 `"$([char]27)]9;4;1;50$([char]27)\"` → 标签进度条 + 任务栏进度

---

## Rust 构建环境

**已根治（2026-07-27）**：`src-tauri/.cargo/config.toml` 现在内置了本机的工具链路径，
`cargo build` / `npx tauri dev` 可以直接运行，不再需要 vcvars 环境或 `kero_check.bat`：

- `[build] rustflags` 加了 `-L native=` 指向 SDK 的 `um\x64` / `ucrt\x64`（修复 `kernel32.lib` LNK1181）
- `[env] INCLUDE` / `LIB` 指向 MSVC 14.44 + SDK 10.0.22621.0 的头文件/库
  （修复 cc-rs 调 `cl.exe` 编译 `vswhom-sys` 等 C 依赖时找不到头文件）
- `[env]` 不覆盖已存在的同名环境变量，在 vcvars shell 中是无副作用的

参考路径：VS 2022 BuildTools 在 `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`，
Windows 10/11 SDK 在 `D:\Windows Kits\10\`（非标准盘符，这是 rustc 自动探测失败的根本原因）。
若 SDK 或 MSVC 版本升级，需同步更新 `.cargo/config.toml` 里的版本号路径。

---

## 已完成的文件清单

### Rust 后端 (`src-tauri/src/`)
- `main.rs` — Tauri 入口点
- `lib.rs` — crate root
- `base64_util.rs` — base64 编解码工具
- `bootstrap.rs` — Tauri Builder 配置、插件注册、状态管理、快照持久化
- `commands.rs` — 全部 ~50 个 Tauri 命令（通过 `register_all()` 注册）
- `models/app.rs` — `AppState`、`Project` 实时模型 + `AppStateView` 序列化
- `models/pane.rs` — `Pane`、`PaneColumn`、`PaneTab`、`PaneContent`、分割/缩放/调整大小逻辑
- `models/session.rs` — `TerminalSession`（PTY 生命周期、生成、发送文本、调整大小、读循环）
- `models/file.rs` — `FileTab`（文本缓冲区、脏状态、保存/加载、二元检测/图片路径）
- `models/diff.rs` — `DiffTab`（git blob 加载）
- `models/project.rs` — `SessionSnapshot`、`ProjectSnapshot` 等（持久化类型）
- `services/config.rs` — 位于 `~/.config/kero/config.toml` 下的 TOML 设置文件
- `services/persist.rs` — JSON 快照保存/加载至 `%APPDATA%\kero\sessions.json`
- `services/shell.rs` — Shell 检测（pwsh > powershell > wsl > cmd）
- `services/git.rs` — git2 状态、阶段、提交、分支、远程、贮藏操作 + `git show` 的 diff 内容加载
- `services/history.rs` — VT 回滚存储，按 UUID 键值
- `services/explorer.rs` — 通过 `reg.exe` 子进程注册 Explorer 右键菜单 + `trash()` 回收站共享函数
- `services/watch.rs` — `notify` 文件系统监听（非递归、300ms 防抖、`fs-changed` 事件）
- `theme/mod.rs` — `ThemeColors` 解析
- `theme/catalog.rs` — 6 个内置主题（Default Dark、Default Light、Dracula、Tokyo Night、Gruvbox Dark、Monokai Pro）
- `theme/catalog_ghostty.rs` — 592 个 Ghostty 主题（生成器：`scripts/generate-theme-catalog.mjs`）
- `theme/fonts.rs` — 内置字体注册（JetBrains Mono + Symbols Nerd Font，AddFontMemResourceEx）

### React 前端 (`src/`)
- `main.tsx` — ReactDOM 入口点
- `App.tsx` — 主布局：侧边栏 + 顶部栏 + 面板区域 + 右侧面板，键盘快捷键（Ctrl+N/T/W/D/P/B 等）
- `components/Sidebar.tsx` — 左侧项目列表，支持拖拽、右键菜单、inline 重命名
- `components/Header.tsx` — 标签页栏（水平滚动，拖拽，重命名，右键菜单，OSC 9;4 进度条）
- `components/PaneLayout.tsx` — niri 风格的列 × 行网格布局渲染，分隔条拖拽，pane 拖拽换位
- `components/TerminalPane.tsx` — xterm.js 终端（实例由 terminalRegistry 保活），右键菜单，文件拖入粘贴路径
- `components/FilePane.tsx` — Monaco 编辑器（高亮、查找替换、防抖回写、脏标记）
- `components/DiffPane.tsx` — Monaco diff（Split/Unified 切换、Reload）
- `components/RightSidebar.tsx` — 标签页容器（Files / Git / Info）
- `components/FileTree.tsx` — 文件树（懒加载、inline 重命名、右键菜单、notify 自动刷新）
- `components/GitPanel.tsx` — Git 状态，stage/discard/commit，分支切换/新建，stash
- `components/InfoPanel.tsx` — 会话元数据（PID、cwd、git 信息）
- `components/CommandPalette.tsx` — Ctrl+P 覆盖层，~38 条命令 fuzzy 搜索
- `components/TabSwitcher.tsx` — Ctrl+Tab 覆盖层（按住循环、松开提交）
- `components/ContextMenu.tsx` — 全局右键菜单（menuStore 驱动）
- `components/Settings.tsx` — 设置弹窗（外观、字体、592 主题、编辑器/终端开关、Explorer 集成）
- `lib/types.ts` — Rust 结构体的 TypeScript 镜像
- `lib/invoke.ts` — 所有 Tauri 命令的强类型包装器
- `lib/fuzzy.ts` — 区分大小写的字符子序列匹配器的模糊匹配
- `lib/theme.ts` — 主题全量应用（CSS 变量 + xterm）
- `lib/monaco.ts` — Monaco 离线加载配置、worker、语言映射
- `lib/terminalRegistry.ts` — xterm 实例保活注册表（切 tab 不丢内容）
- `lib/menuStore.ts` — 右键菜单全局状态
- `hooks/useTauriEvent.ts` — Tauri 事件订阅的 React hook

---

## 已知限制（v0.1 收尾状态）

1. **终端渲染**：使用 xterm.js（WebView），而非文档中的 wgpu + 原生 HWND。在 WebView 内部可以工作，延迟稍高，但比在两个完全不同的渲染管线之间搭建桥梁要更可行。
2. ~~文件树只显示顶层~~（已修复，第三轮）/ ~~文件编辑器是 textarea~~（已修复，Monaco，第五轮）/ ~~Diff 是纯文本~~（已修复，Monaco diff，第五轮）/ ~~Ctrl+Tab 切换器~~（已实现，第五轮）/ ~~只有 6 个主题~~（已修复，592 个，第五轮）
3. ~~终端回滚恢复~~：`history.rs` 基础设施从未接入，第十一轮已整体删除（含假开关）。
4. ~~自动更新器~~：第十一轮已整体移除（插件/依赖/配置/前端包）。将来有发布渠道时重新引入。
5. **多窗口恢复**：次窗口（`win-*`）不会在启动时自动重开；第十一轮已修复快照文件泄漏
   （启动清孤儿 + Destroyed 时删除），但 label 随机导致副窗布局本身仍不恢复。
6. **Git 安全层**：破坏性操作没有原版的前置 fingerprint 检查；操作错误只走 alert，没有 banner。
7. **跨 tab 拖拽 pane** 未实现（仅 tab 内换位）；Monaco 编辑器主题固定 vs-dark，不随应用主题。
8. **实机验证不足**：第 2 批之后的功能（Monaco、菜单、多窗口、托盘等）均只通过构建/单测验证，未逐一人工实测。

---

## 构建命令

```pwsh
cd D:\agents\kero\kero-windows

# 前端构建
npm run build

# Rust 后端（直接可用，.cargo/config.toml 已内置工具链路径）
cargo build --manifest-path src-tauri\Cargo.toml

# 开发模式（前端 + Tauri 后端热重载）
npx tauri dev
```
