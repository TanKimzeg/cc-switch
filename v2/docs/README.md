# CC Switch v2 文档

CC Switch v2 是基于插件协议（Plugin Protocol）重写的 Agent 配置切换器。本目录记录 v2 的架构、接口、数据模型、TS 插件机制，以及与 v1 的能力差距和演进方向。

> 本文档仅覆盖 `v2/`（当前在仓库根目录下独立维护的新版本）。旧版（v1）见仓库根目录的 `docs/` 与 `README_ZH.md`。

## 文档导航

| 文档 | 内容 | 阅读对象 |
|------|------|----------|
| [architecture.md](architecture.md) | 设计目的、插件三形态、核心能力、数据流、SSOT | 想理解 v2 为什么这么设计的人 |
| [plugin-protocol.md](plugin-protocol.md) | `AgentPlugin` / `McpPlugin` trait、数据类型、manifest schema | 插件作者、后端开发者 |
| [plugin-dev-guide.md](plugin-dev-guide.md) | 从零开发 TS / native / shell 插件的完整指南（含示例与测试） | **插件作者（推荐先读）** |
| [ts-plugin.md](ts-plugin.md) | TS 插件契约、宿主 API、沙箱模型、写插件教程、沙箱演进方向 | TS 插件作者 |
| [commands-api.md](commands-api.md) | 后端 Tauri 命令清单（按域分组、入参出参） | 前端/后端联调者 |
| [frontend-api.md](frontend-api.md) | `api.ts` 函数、TS/native 路由、各 Panel 职责 | 前端开发者 |
| [data-model.md](data-model.md) | SQLite 数据模型（15 张表字段与关系） | 后端开发者 |
| [v1-gap-analysis.md](v1-gap-analysis.md) | 与 v1 的能力差距对照 + 每项实现思路 | 规划 v2 路线的所有人 |

## 建议阅读顺序

1. **先读** [architecture.md](architecture.md) —— 理解 v2 的核心设计意图
2. **再读** [plugin-protocol.md](plugin-protocol.md) —— 协议是一切的基础
3. **写插件前读** [plugin-dev-guide.md](plugin-dev-guide.md) —— 从零开发的实操指南
4. 按需查阅 [commands-api.md](commands-api.md) / [frontend-api.md](frontend-api.md) / [data-model.md](data-model.md)
5. TS 插件细节参考 [ts-plugin.md](ts-plugin.md)
6. 规划功能补齐时参考 [v1-gap-analysis.md](v1-gap-analysis.md)

## 一句话定位

v2 把 v1 中「按 Agent 散落分布」的配置逻辑，收敛为**插件协议**：每个 Agent（Claude Code、OpenCode、OpenClaw…）是一个插件，通过统一的 `AgentPlugin` trait 暴露「Provider 配置、MCP、Skill、Prompt、用量查询、会话」能力；核心代码只依赖抽象，不再 `match` 具体 Agent，新增 Agent 只加插件、不动核心（开闭原则）。

**铁律**：除「插件模式」这一架构差异外，功能语义、UI 布局、交互、文案**一律向 v1 看齐**（v1 是 120k+ stars 的成熟项目）。开发任何功能前先查 v1 实现（`docs/user-manual/zh/`、`src/`、`src-tauri/src/`），抄其成熟实践而非重新发明。已知差距见 [v1-gap-analysis.md](v1-gap-analysis.md)。
