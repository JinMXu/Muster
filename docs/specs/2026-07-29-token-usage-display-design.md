# Token 消耗展示 - 设计文档

日期：2026-07-29
状态：已确认，待实现

## 背景

Muster 是纯终端/Git 工作区，自身不消耗 token，也不发 LLM 请求。但用户日常使用的 AI 编码 CLI 工具（OpenCode、Claude Code、Codex、Kimi Code）都会在本地留下每次会话的 token 消耗数据。本功能在 Muster 中聚合展示这些工具的 token 消耗，让用户在一个面板内看到各工具、各会话的用量。

### 数据来源确认

四款 CLI 工具都在本地存储了 session 级 token 数据，但格式与完整度不同：

| 工具 | Windows 路径 | 格式 | 存了美元成本？ |
|------|-------------|------|--------------|
| OpenCode | `%USERPROFILE%\.local\share\opencode\opencode.db` | SQLite (WAL) | 是（`session.cost`，USD） |
| Claude Code | `%USERPROFILE%\.claude\projects\<slug>\<uuid>.jsonl` | JSONL | 否，仅 token 数 |
| Codex | `%USERPROFILE%\.codex\sessions\YYYY\MM\DD\rollout-*.jsonl` | JSONL | 否，仅 token 数 |
| Kimi Code | `%USERPROFILE%\.kimi-code\sessions\<workDirKey>\<sid>\agents\main\wire.jsonl` | JSONL | 否，仅 token 数 |

路径支持环境变量覆盖：OpenCode 支持 `XDG_DATA_HOME` / `OPENCODE_DB`；Codex 支持 `CODEX_HOME`；Kimi Code 支持 `KIMI_CODE_HOME`；Claude Code 支持 `CLAUDE_CONFIG_DIR`。

### 范围决策

- **成本展示**：只展 token 数，不算钱。OpenCode 顺带展示其自带的美元成本（不用我们算）。其余三家不显示成本。
- **明细粒度**：会话级汇总（每个 session 一行），不做 per-message 细分。
- **刷新方式**：后台定时扫描（60 秒），面板始终显示最新数据。
- **明细表**：不支持展开消息级，仅会话列表 + 按工具/时间筛选。

## 架构方案对比

考虑过三种方案，选定 A：

| 方案 | 说明 | 评价 |
|------|------|------|
| **A. Rust 原生解析器** ✅ | 后端 `services/usage/` 模块，每工具一个解析器，tokio 后台扫描，前端只渲染 | 与 Muster 现有架构一致，性能好，无额外依赖 |
| B. shell 调 Node 脚本 | Rust 编排，Node 脚本解析 JSONL | 需用户装 Node，进程启动慢，与原生应用定位冲突 |
| C. Rust 编排 + 前端 TS 解析 | 大 JSONL 传到前端解析 | 内存膨胀，解析跨两层，后台扫描难做 |

## 1. 数据模型与归一化

四款工具的 token 字段名各异，归一化为统一结构。

### 统一结构

```rust
struct TokenUsage {
    input: u64,       // 非缓存输入 token
    output: u64,      // 输出/补全 token
    reasoning: u64,   // 推理 token（部分模型独有）
    cache_read: u64,  // 缓存命中读取
    cache_write: u64, // 写入缓存
}

struct UsageSession {
    tool: ToolKind,
    session_id: String,
    title: String,
    cwd: String,
    model: String,
    started_at: i64,   // epoch ms
    updated_at: i64,
    tokens: TokenUsage,
    cost_usd: Option<f64>,  // 仅 OpenCode 有
}

struct ToolSummary {
    tool: ToolKind,
    total_tokens: u64,         // input+output+reasoning+cache_read+cache_write
    tokens: TokenUsage,        // 分项明细
    session_count: usize,
    cost_usd: Option<f64>,     // 仅 opencode 非 None
    last_updated: i64,         // 该工具最后一条 session 的 updated_at
}

struct UsageSummary {
    tools: Vec<ToolSummary>,
}

enum ToolKind { OpenCode, ClaudeCode, Codex, KimiCode }
```

### 字段映射

| 归一化字段 | OpenCode (`session` 表) | Claude Code (`message.usage`) | Codex (`token_count.info`) | Kimi Code (`usage.record`) |
|-----------|------------------------|------------------------------|---------------------------|---------------------------|
| input | `tokens_input` | `input_tokens` | `input_tokens - cached_input_tokens` | `inputOther` |
| output | `tokens_output` | `output_tokens` | `output_tokens` | `output` |
| reasoning | `tokens_reasoning` | — | `reasoning_output_tokens` | — |
| cache_read | `tokens_cache_read` | `cache_read_input_tokens` | `cached_input_tokens` | `inputCacheRead` |
| cache_write | `tokens_cache_write` | `cache_creation_input_tokens` | 0（固定） | `inputCacheCreation` |

**Codex 关键注意**：`input_tokens` **包含** `cached_input_tokens`，非缓存输入 = `input_tokens - cached_input_tokens`，不能直接用 `input_tokens`。

**总 token** = `input + output + reasoning + cache_read + cache_write`，四款工具可横向比较。

## 2. 解析器架构与后台扫描

### 模块结构

```
src-tauri/src/services/usage/
├── mod.rs            // 对外接口：collect_summary()、collect_sessions()
├── model.rs          // TokenUsage、UsageSession、ToolKind 等
├── opencode.rs       // SQLite 只读查询
├── claude.rs         // JSONL 流式解析
├── codex.rs          // JSONL 流式解析
├── kimi.rs           // JSONL 流式解析
└── scanner.rs        // 后台定时扫描 + 文件监听
```

### 各解析器策略

**OpenCode（`opencode.rs`）**：
- 路径：`%USERPROFILE%\.local\share\opencode\opencode.db`，支持 `XDG_DATA_HOME` / `OPENCODE_DB` 覆盖
- 用 `rusqlite` 以 `file:...?mode=ro&immutable=1` 打开（opencode 运行时持有 WAL 连接，必须只读避免锁冲突）
- 单条 SQL 拿所有 session 聚合：
  ```sql
  SELECT id, title, directory, cost,
         tokens_input, tokens_output, tokens_reasoning,
         tokens_cache_read, tokens_cache_write,
         json_extract(model,'$.id'), json_extract(model,'$.providerID'),
         time_created, time_updated
  FROM session
  WHERE time_archived IS NULL
  ORDER BY time_updated DESC;
  ```
- `model` 列是 JSON，用 `json_extract` 提取 id 和 providerID

**Claude Code（`claude.rs`）**：
- Glob `~\.claude\projects\*\*.jsonl`，支持 `CLAUDE_CONFIG_DIR` 覆盖
- `BufReader` 逐行流式读取，过滤 `type == "assistant"`，累加 `message.usage.*`
- session_id 从文件名（UUID）提取，cwd 从文件夹名反解（`D--agents-Foo` → `D:\agents\Foo`）
- model 取最后一条 assistant 消息的 `message.model`
- title 取首条 user 消息前若干字符

**Codex（`codex.rs`）**：
- Glob `~\.codex\sessions\**\rollout-*.jsonl` + `archived_sessions\**\`，支持 `CODEX_HOME` 覆盖
- 首行 `session_meta` 拿 cwd、git 信息
- 只取最后一条 `payload.type == "token_count"` 事件的 `total_token_usage`（累计值，无需自行加总）
- model 从 `turn_context` 事件取

**Kimi Code（`kimi.rs`）**：
- 支持 `KIMI_CODE_HOME` 覆盖路径
- 先读 `~\.kimi-code\session_index.jsonl` 拿 session 目录映射
- 对每个 session 的 `agents/main/wire.jsonl` 逐行累加 `type == "usage.record"` 事件的 `usage.*`
- model 从 `usage.record.model` 或 `llm.request.model` 取

### 统一 trait

```rust
pub trait UsageProvider {
    fn tool_kind(&self) -> ToolKind;
    fn discover(&self) -> Vec<DiscoveredSession>;   // 找到哪些文件/记录
    fn parse(&self, source: &DiscoveredSession) -> Option<UsageSession>;
}
```

四个解析器各自实现，`scanner.rs` 统一调度。

### 后台扫描策略（`scanner.rs`）

- **启动时机**：Tauri 启动时在 `bootstrap.rs` 中 `tokio::spawn` 后台任务
- **频率**：每 60 秒全量扫描一次
- **增量优化**：记录每个源文件的 `(path, mtime, size)`，下次 mtime/size 未变的跳过解析、复用缓存结果。对 OpenCode 大库和累积 JSONL 尤其重要
- **WAL 安全**：OpenCode 用只读模式打开；Claude/Codex/Kimi 的 JSONL 是 append-only，读时写端不阻塞
- **结果存储**：`AppState` 的 `RwLock<HashMap<ToolKind, Vec<UsageSession>>>`，Tauri 命令直接读缓存返回，零延迟
- **前端通知**：扫描完成后 emit `usage-updated` 事件，前端监听刷新

### 容错原则

- 单个解析器出错（文件不存在、格式变更、解析异常）只 `log::warn` 并跳过，不影响其他工具
- 工具未安装（路径不存在）返回空列表，不算错误
- 全部解析并行执行（`tokio::join!`），总耗时 ≈ 最慢的那个

## 3. Tauri 命令与前端 API

### 新增 Tauri 命令（`commands.rs`）

```rust
// 汇总卡片：各工具 token 总数 + session 数 + OpenCode 成本
#[tauri::command]
fn usage_summary(state: State<AppState>) -> UsageSummary

// 会话明细列表：按工具筛选 + 时间范围
#[tauri::command]
fn usage_sessions(
    state: State<AppState>,
    tool: Option<ToolKind>,      // None = 全部工具
    since: Option<i64>,          // epoch ms，None = 不限
    limit: Option<usize>,        // 默认 200
) -> Vec<UsageSession>

// 手动触发刷新
#[tauri::command]
async fn usage_refresh(state: State<'_, AppState>) -> Result<()>
```

命令读 `AppState` 的 `RwLock` 缓存，同步返回，无 IO 等待。

### 前端 API（`invoke.ts`）

在 `api` 对象中新增 `usage` 命名空间：

```typescript
usage: {
  summary: () => invoke<UsageSummary>('usage_summary'),
  sessions: (opts?: { tool?: ToolKind; since?: number; limit?: number })
    => invoke<UsageSession[]>('usage_sessions', opts ?? {}),
  refresh: () => invoke('usage_refresh'),
}
```

### 前端类型（`types.ts`）

```typescript
type ToolKind = 'opencode' | 'claude_code' | 'codex' | 'kimi_code';

interface TokenUsage {
  input: number;
  output: number;
  reasoning: number;
  cache_read: number;
  cache_write: number;
}

interface UsageSummary {
  tools: Array<{
    tool: ToolKind;
    total_tokens: number;
    tokens: TokenUsage;
    session_count: number;
    cost_usd: number | null;
    last_updated: number;
  }>;
}

interface UsageSession {
  tool: ToolKind;
  session_id: string;
  title: string;
  cwd: string;
  model: string;
  started_at: number;
  updated_at: number;
  tokens: TokenUsage;
  cost_usd: number | null;
}
```

### 实时更新

前端监听 `usage-updated` 事件，扫描完成时自动刷新面板数据；保留手动刷新按钮兜底。

## 4. UsagePanel UI 设计

### 入口

- 命令面板新增 "Usage" 项
- 快捷键 `Ctrl+Shift+U` 打开
- 独立全屏覆盖面板（与 Settings 弹窗同层级）

### 布局

```
┌─ Usage ──────────────────────────────────────────────┐
│                                                       │
│  [Today] [7 Days] [30 Days] [All]      [↻ Refresh]   │
│                                                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐│
│  │ OpenCode │ │ Claude   │ │ Codex    │ │ Kimi Code││
│  │          │ │ Code     │ │          │ │          ││
│  │ 1.2M     │ │ 480K     │ │ 95K      │ │ 760K     ││
│  │ tokens   │ │ tokens   │ │ tokens   │ │ tokens   ││
│  │          │ │          │ │          │ │          ││
│  │ $24.74   │ │ -        │ │ -        │ │ -        ││
│  │ 33 sess  │ │ 12 sess  │ │ 4 sess   │ │ 8 sess   ││
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘│
│                                                       │
│  SESSIONS                          [All ▾] [Sort ▾]  │
│  ─────────────────────────────────────────────────── │
│  Time         Tool        Model        Tokens   $   │
│  ─────────────────────────────────────────────────── │
│  14:30 today  OpenCode    deepseek-v4  21.4K    $0.3│
│  13:15 today  Claude Code sonnet-4     46.5K    -   │
│  11:02 today  Kimi Code   kimi-coding  18.2K    -   │
│  09:45 today  Codex       gpt-5        8.1K     -   │
│  ...                                                 │
└───────────────────────────────────────────────────────┘
```

### 组件结构

```
UsagePanel.tsx
├── 顶部时间筛选条：Today / 7 Days / 30 Days / All
├── 汇总卡片区：UsageSummaryCard × 4
│   ├── 工具名 + 色块标识（各工具固定颜色）
│   ├── 总 token 数（大号字体）
│   ├── 分项 token（小字，hover 展示 input/output/cache）
│   ├── 美元成本（仅有则显示，无则 "-"）
│   └── session 数量
├── 会话明细表：UsageSessionTable
│   ├── 工具筛选下拉（All / 单个工具）
│   ├── 排序下拉（按时间 / 按 token 数）
│   ├── 列：时间、工具、模型、token 数、成本
│   └── 虚拟滚动（session 可能上千条）
└── 刷新按钮（右上角）
```

### Token 格式化

- `< 1000`：原样，如 `853`
- `1K–1M`：K 单位，如 `21.4K`
- `> 1M`：M 单位，如 `1.2M`
- 成本：`$24.74`，两位小数；无则 `-`

### 空状态

工具未安装或无数据时卡片显示 "Not found"（可选附带安装说明链接）。

### 配色（各工具固定标识色）

- OpenCode：紫色
- Claude Code：橙棕色
- Codex：绿色
- Kimi Code：蓝色

### i18n

新增 `usage.*` 命名空间，en/zh 各一套：`usage.title`、`usage.refresh`、`usage.today`、`usage.week`、`usage.month`、`usage.all`、`usage.tokens`、`usage.sessions`、`usage.cost`、`usage.model`、`usage.not_found`、`usage.empty`、`usage.no_sessions`。

## 改动文件

**后端（Rust）**：
- 新增：`src-tauri/src/services/usage/{mod,model,opencode,claude,codex,kimi,scanner}.rs`
- 修改：`src-tauri/src/services/mod.rs`（声明 usage 模块）
- 修改：`src-tauri/src/commands.rs`（3 个 `#[tauri::command]`）
- 修改：`src-tauri/src/bootstrap.rs`（注册命令 + 启动后台扫描）
- 修改：`src-tauri/src/models/app.rs`（AppState 加 usage 缓存字段）
- 修改：`src-tauri/Cargo.toml`（加 `rusqlite` 依赖，启用 `bundled` feature）

**前端（TS/React）**：
- 新增：`src/components/UsagePanel.tsx`（及子组件 `UsageSummaryCard`、`UsageSessionTable`）
- 修改：`src/lib/invoke.ts`（`api.usage` 命名空间）
- 修改：`src/lib/types.ts`（`ToolKind`、`TokenUsage`、`UsageSummary`、`UsageSession`）
- 修改：`src/components/CommandPalette.tsx`（新增 Usage 命令）
- 修改：`src/App.tsx`（`showUsage` 状态、NAV_MAP 加 `Ctrl+Shift+U`、渲染面板）
- 修改：`src/lib/i18n/{en,zh}.ts`（`usage.*` 键）

## 验证

- `cargo clippy` 通过、`cargo test` 通过（解析器单元测试用 fixture 文件）
- `npx tsc --noEmit` 通过
- 手动：四款工具数据均正确展示；时间筛选/工具筛选生效；手动刷新生效；后台扫描后数据自动更新；某工具未安装时正确显示空状态
