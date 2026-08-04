<div align="center">

# Muster

**Windows 原生的终端工作区 — 终端、文件、Diff、Git，一窗之内。**

基于 **Tauri 2 + React + xterm.js**，Rust 后端直接驱动 ConPTY 与 libgit2。

[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D4?style=flat-square&logo=windows)](https://github.com/JinMXu/Muster)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-FFC131?style=flat-square&logo=tauri)](https://tauri.app/)

![Muster 主界面](docs/screenshots/main-window.png)

</div>

---

## 亮点

<table>
<tr>
<td width="33%" valign="top">

**🖥️ 终端工作区**

niri 风格分屏布局，拖拽重排、焦点跟随、单 pane 最大化。ConPTY 驱动的完整终端仿真，PowerShell 深度集成 — 每条命令自动上报 cwd。

</td>
<td width="33%" valign="top">

**📂 文件与 Git 内建**

Monaco 编辑器 + Diff 视图，主题跟随全局。Git 面板支持暂存/提交/push/pull/stash/分支管理，**检查点**功能持续追踪 HEAD 之后的全部变更。

</td>
<td width="33%" valign="top">

**🤖 Agent 感知**

自动识别 Claude Code / OpenCode / Codex / Kimi Code 等 coding agent，标签显示状态点（绿=运行、黄=等待、蓝=完成），失焦时发系统通知。CLI 桥让 AI 代理驱动终端。

</td>
</tr>
</table>

---

## 功能一览

| 模块 | 能力 |
|:---|:---|
| **多窗口** | 多开窗口并行，布局定时快照，重启完整恢复。最后窗口关闭后驻留系统托盘，终端会话不中断 |
| **分屏布局** | 列×行网格，拖拽 pane 到任意边缘重排，跨标签页拖拽，焦点跟随，zoom 最大化 |
| **终端** | ConPTY 完整仿真（xterm.js），PowerShell 自动上报 cwd，响铃通知 |
| **Worktree 追踪** | `cd` 进 worktree 后面板即时重锚定（事件驱动），Info 面板显示标记 |
| **文件编辑器** | Monaco 语法高亮/查找替换/自动换行；内联 diff vs HEAD（实时更新）、git blame、文件历史；终端报错行点击直达 |
| **差异查看器** | Monaco Diff 内联/分栏对比，支持任意两提交对比 |
| **文件树** | 懒加载目录树，内联重命名/新建，拖拽文件到终端粘贴路径 |
| **Git 面板** | porcelain v2 状态，暂存/提交（`Ctrl+Enter`）/push/pull/fetch/分支/stash/冲突检测/最近提交；检查点锚定快照，持续追踪后续变更 |
| **项目搜索** | `Ctrl+Shift+F` 全文搜索，gitignore 感知，结果按文件分组、命中高亮 |
| **终端搜索** | 终端焦点下 `Ctrl+F` 滚动回滚搜索，增量高亮 |
| **命令面板** | `Ctrl+P` 模糊搜索项目文件、最近文件、命令、会话 |
| **标签切换器** | `Ctrl+Tab` 循环切换标签，带预览 |
| **主题** | 内置 GitHub 风格 + 完整 Ghostty 主题目录（598 款），深色/浅色独立设置，跟随系统 |
| **国际化** | 中文 / English，可跟随系统语言 |
| **会话持久化** | 布局/项目/选中状态每 5 秒自动快照，重启完整恢复 |
| **Agent 总览** | 左下角 mini-bar 跨窗口汇总所有 agent 状态，点击跳转（`Ctrl+Shift+A`） |
| **Agent CLI 桥** | `muster` 命令驱动终端：`split`/`send`/`capture`/`run`/`wait`/`watch` 等 |
| **CLI 工具** | `muster <path>` 从任意终端打开项目，支持 `--cmd` 执行命令 |
| **剪贴板安全** | 粘贴疑似可执行命令时弹窗确认 |
| **信息面板** | Shell PID、cwd、Git 分支/远程、进程树、监听端口 |
| **用量面板** | AI 编程工具 token 用量统计，按日堆叠柱状图 |
| **资源管理器集成** | 一键安装「在 Muster 中打开」右键菜单 |
| **无障碍** | 跟随系统 `prefers-reduced-motion` |

---

## 技术栈

| 层 | 技术 |
|:---|:---|
| 外壳 / 窗口 | Tauri 2（Rust），多窗口 + 系统托盘 + CLI PATH 注册 |
| 前端 UI | React 18 + TypeScript + Vite + Tailwind CSS |
| 终端仿真 | xterm.js（fit 插件） |
| PTY | 手动 ConPTY 驱动（bundled OpenConsole.exe） |
| 编辑器 / Diff | Monaco Editor |
| Git | git2（libgit2，含 SSH） |
| 剪贴板安全 | 前端粘贴拦截 + 危险内容检测 |
| 其他 | notify、sysinfo、ignore、rusqlite、toml、winreg、本地 TCP JSON IPC |

---

## 快速开始

### 环境要求

- Windows 10/11（依赖 WebView2 与 ConPTY）
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77+（MSVC 工具链）

### 开发

```sh
npm install
npm run tauri dev
```

### 构建

```sh
npm run tauri build    # 产出 MSI 和 NSIS 安装包
```

### 常用命令

```sh
npm run dev            # 仅启动前端 Vite dev server
npm run build          # 前端类型检查 + 产物构建（tsc && vite build）
cargo check            # 后端编译检查（在 src-tauri/ 下执行）
cargo test             # 后端单元测试（在 src-tauri/ 下执行）
```

> 部分后端测试会真实创建进程和监听端口，默认被标记为 `#[ignore]`，需手动运行：
> `cargo test --lib -- --ignored`

<details>
<summary><b>本机工具链配置</b>（可选，仅非标准安装时需要）</summary>

`src-tauri/.cargo/config.toml` 不入库（已 gitignore），因为它是**单机有效**的 workaround：只有当 Windows SDK / MSVC 装在非标准位置、rustc 自动探测失败（典型报错：`LNK1181: cannot open input file 'kernel32.lib'`，或 cc-rs 找不到头文件）时才需要创建，内容形如：

```toml
[build]
rustflags = ["-L", "native=D:\\Windows Kits\\10\\Lib\\10.0.22621.0\\um\\x64"]

[env]
INCLUDE = "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC\\14.44.35207\\include;..."
LIB = "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC\\14.44.35207\\lib\\x64;..."
```

路径里的 MSVC/SDK 版本号需按本机实际安装调整；工具链在标准位置时不需要此文件。

</details>

---

## 配置

配置文件为 TOML 格式：

- 正式版：`%APPDATA%\muster\config.toml`
- 开发版（debug 构建）：`%APPDATA%\muster-dev\config.toml`

会话快照保存在同目录下的 `sessions*.json`。

---

## 命令行

在设置中选择 "Add Muster to PATH" 后：

```sh
muster .                              # 在当前目录打开项目
muster C:\projects\myapp              # 在指定目录打开项目
muster --cmd "npm run dev" .          # 打开项目并运行命令
muster --cmd "python hello.py"        # 在当前目录运行命令
```

快捷键 `-c` 和 `-d` 可分别替代 `--cmd` 和 `--cwd`。

### Agent 驱动的 CLI 桥

`muster <verb>` 通过本地 IPC 与正在运行的 Muster 通信（未运行时自动启动），供 AI 编码代理和其他脚本驱动终端：

```sh
muster doctor                         # 检查桥接状态
muster new C:\projects\myapp          # 打开项目，打印首个终端会话 id
muster split --v                      # 向下分屏，打印新会话 id
muster send <id> "npm run dev" --enter    # 向会话发送按键
muster capture <id> --lines 200           # 读取会话最近输出
muster procs <id>                     # 会话进程树 + 监听端口
muster agents                         # 各会话的 coding agent 状态
muster run -- cargo test              # 新标签 PTY 运行命令，等待完成
```

- `run` 默认 600s 超时（`--timeout N`）；`--` 之后的内容原样作为命令
- 任何 verb 加 `--json` 输出结构化结果
- 为 AI 代理准备的完整使用说明见 `skills/muster/SKILL.md`

---

## 快捷键

<details open>
<summary><b>窗口与项目</b></summary>

| 快捷键 | 功能 |
|:---|:---|
| `Ctrl+N` | 新建项目 |
| `Ctrl+Shift+N` | 新建窗口 |
| `Ctrl+1` ~ `Ctrl+9` | 切换到第 N 个项目 |
| `Ctrl+Alt+[` / `Ctrl+Alt+]` | 上一个 / 下一个项目 |
| `Ctrl+,` | 打开设置 |
| `Ctrl+/` | 快捷键帮助 |

</details>

<details open>
<summary><b>标签与分屏</b></summary>

| 快捷键 | 功能 |
|:---|:---|
| `Ctrl+T` | 新建终端标签 |
| `Ctrl+W` | 关闭当前标签 |
| `Ctrl+Shift+T` | 重开最近关闭的标签 |
| `Ctrl+Tab` | 标签切换器 |
| `Ctrl+Shift+[` / `Ctrl+Shift+]` | 上一个 / 下一个标签 |
| `Ctrl+D` | 向右分屏 |
| `Ctrl+Shift+D` | 向下分屏 |
| `Ctrl+[` / `Ctrl+]` | 上一个 / 下一个 pane |
| `Ctrl+Alt+方向键` | 按方向切换 pane 焦点 |
| `Ctrl+Alt+Shift+方向键` | 按方向调整 pane 尺寸 |
| `Ctrl+Shift+Enter` | 当前 pane 最大化 / 还原 |

</details>

<details open>
<summary><b>面板与工具</b></summary>

| 快捷键 | 功能 |
|:---|:---|
| `Ctrl+P` | 命令面板 |
| `Ctrl+B` | 切换左侧项目栏 |
| `Ctrl+Shift+B` | 切换右侧边栏 |
| `Ctrl+Shift+E` | 文件树面板 |
| `Ctrl+Shift+F` | 项目全文搜索 |
| `Ctrl+F` | 终端滚动回滚搜索 |
| `Ctrl+Shift+A` | Agents 总览 |
| `Ctrl+Shift+G` | Git 面板 |
| `Ctrl+Shift+I` | 信息面板 |
| `Ctrl+Shift+U` | 用量面板 |
| `Ctrl+S` | 保存文件（编辑器中） |
| `Ctrl+K` | 清空终端 |

</details>

---

## 目录结构

```
kero-windows/
├── src/                    # React 前端
│   ├── components/         # 界面组件（终端、文件树、Git 面板、设置等）
│   ├── lib/                # invoke 封装、设置存储、主题、i18n、模糊匹配等
│   ├── hooks/              # Tauri 事件订阅等 React hooks
│   └── styles/             # 全局样式与设计变量
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── commands/       # 全部 Tauri 命令，按域拆分（前端 invoke 入口）
│   │   ├── models/         # 应用状态：项目、标签、pane、终端会话
│   │   ├── services/       # PTY/shell、git、配置、持久化、进程、CLI 等
│   │   └── theme/          # 主题目录与解析（含 Ghostty 主题）
│   ├── capabilities/       # Tauri 权限声明
│   └── assets/fonts/       # 内嵌字体（JetBrains Mono + Nerd Font 符号）
├── scripts/                # 主题目录生成脚本
└── docs/                   # 设计文档与截图
```

---

## 致谢

Muster 是 [Kero](https://github.com/egoist/kero) 的 Windows 重实现 — Kero 是由 [@egoist](https://github.com/egoist) 开发的 macOS 原生终端工作区（Swift + libghostty）。本项目在产品形态与交互设计上深受其启发，在此致谢。

主题目录来源于 [Ghostty](https://ghostty.org/) 项目；终端字体为 [JetBrains Mono](https://www.jetbrains.com/lp/mono/) 与 [Nerd Fonts](https://www.nerdfonts.com/) 符号字体。
