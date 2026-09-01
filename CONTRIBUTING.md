# 贡献指南 / Contributing

感谢你对 Muster 的兴趣！欢迎提交 Issue 和 Pull Request。

## 开发环境

- Windows 10/11（依赖 WebView2 与 ConPTY）
- Node.js 18+
- Rust 1.77+（MSVC 工具链）

```sh
npm install
npm run tauri dev
```

## 提交前检查

```sh
npm run build        # 前端类型检查 + 构建（tsc && vite build）
cargo check          # 后端编译检查（src-tauri/ 下）
cargo test           # 后端单元测试（src-tauri/ 下）
```

## 代码约定

- 前端：React 18 + TypeScript，invoke 封装集中在 `src/lib/`
- 后端：Tauri 命令按域拆分到 `src-tauri/src/commands/`，状态在 `models/`，业务逻辑在 `services/`
- 用户可见的字符串需要走 i18n（中文 / English 双语）
- Commit message 使用约定式提交（`feat:` / `fix:` / `docs:` / `chore:` / `perf:` 等）

## Pull Request

1. Fork 并新建分支
2. 保持改动聚焦，一个 PR 做一件事
3. 涉及 UI 变更请附截图
4. 确保上述检查全部通过
