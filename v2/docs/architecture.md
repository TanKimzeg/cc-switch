# 架构与设计目的

## 1. 为什么有 v2

v1 的每个 Agent（Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build、OpenCode、OpenClaw、Hermes）的配置读写、MCP 同步、Skill 同步、Prompt 路径等逻辑**散落分布**在各处：`app_config.rs` 里一个巨大的 `match app`、`services/provider/live.rs` 里又一个 `match app_type`、`services/skill.rs` 里 `get_app_skills_dir` 又 match 一次……新增一个 Agent 往往要同时改多处核心代码，违背开闭原则。

v2 的核心目标是：**把 v1 散落分布的 Agent 特点，用「插件设计模式」收敛为一份统一的协议（`AgentPlugin` trait）**。核心代码只依赖抽象；每个 Agent 是一个插件，负责把自己的配置格式翻译成协议要求的数据结构。新增 Agent = 写一个新插件 + 注册，核心代码零改动。

## 2. 核心能力（五大能力 + 会话）

协议聚焦于用户真正关心的能力。与 v1 一致，共 5 大能力 + 会话管理：

| 能力 | 协议方法 | 说明 |
|------|----------|------|
| **Provider 配置** | `read_live` / `apply` / `remove_provider` / `import` | 读取/写入 live 配置、切换、移除、反向导入 |
| **MCP** | `McpPlugin`（`get/set/remove_mcp_server`） | 统一格式 ↔ 各 Agent 原生格式的转换同步 |
| **Skill** | `skills_dir()` | 返回该 Agent 的 skills 目录，软件层复制/移除 |
| **Prompt 管理** | `prompt_file_path()` | 返回提示词文件路径（CLAUDE.md / AGENTS.md…），软件层写入/删除 |
| **用量查询** | `sync_usage()` | 从插件自己的会话存储解析 token/cost，软件层入库 |
| **会话** | `sessions()` / `load_messages()` / `delete_session()` | 浏览、加载、删除历史会话 |

软件层负责「与协议无关」的部分：DB 存储、去重、按日汇总、把 provider 写入 live 后记录 current 等。插件只负责「把协议操作翻译为该 Agent 的真实文件/SQL」。

## 3. 插件三形态

| 形态 | 执行位置 | 沙箱 | 适用场景 | v2 现状 |
|------|----------|------|----------|---------|
| **native** | 后端二进制内（Rust） | 无限制 | 真实 Agent，配置在用户目录 | ✅ `opencode` |
| **shell** | 后端调用外部命令 | 无限制（后端执行） | 外部 CLI（`openclaw config ...`） | ✅ `openclaw` |
| **ts** | 前端 WebView 脚本 | 插件目录 + manifest `resources` 白名单 | 自包含 / 资源白名单声明的第三方插件 | ✅ `claudecode`（示例）、`ts-demo` |

> **关键结论**：`native` / `shell` 都在**后端**执行，没有沙箱限制，能读写 `~/.claude/`、`~/.config/opencode/` 等真实配置。**TS 插件**的宿主文件操作限定在「插件目录 + manifest `resources` 白名单」内（见 [ts-plugin.md](ts-plugin.md)），白名单声明后后端代劳文件 I/O——claudecode 示例正是用 TS + 资源白名单管理 `~/.claude/`。

## 4. 数据流

```
┌──────────────────────────────────────────────────────────────┐
│ 前端 (React + TS)                                            │
│   Panel（ProvidersPanel / McpPanel / UsagePanel …）          │
│     │ 调 api.ts 函数                                          │
│     ▼                                                        │
│   api.ts（readLiveConfig / applyProvider / …）               │
│     │ 对 TS 插件：loadTsPluginIfTs → 直接调脚本方法           │
│     │ 对 native/shell：invoke 后端命令                        │
└──────────────┬───────────────────────────────────────────────┘
               │ Tauri IPC（invoke）
┌──────────────▼───────────────────────────────────────────────┐
│ 后端 (Rust)                                                  │
│   commands/（providers / plugins / mcp / usage / …）         │
│     │ registry.resolve_plugin(id) → Box<dyn AgentPlugin>     │
│     ▼                                                        │
│   plugin/（AgentPlugin trait 实现）                           │
│     ├─ native: opencode.rs                                    │
│     ├─ shell:  process.rs（调外部命令）                        │
│     └─ ts:     ts.rs（TsPluginStub，实际逻辑在前端）           │
│     ▼                                                        │
│   services/（DB、去重、汇总、复制）                           │
└──────────────────────────────────────────────────────────────┘
```

要点：
- **协议统一**：无论 native/shell/ts，`commands/` 都通过 `registry.resolve_plugin(id)` 拿到 `Box<dyn AgentPlugin>` 再调方法，命令层不关心插件形态。
- **TS 特判**：前端 `api.ts` 用 `loadTsPluginIfTs` 判断插件是否为 TS；若是则**跳过后端命令**，直接把脚本加载到 WebView 调其方法（因为后端只有 `TsPluginStub` 占位）。这导致命令层与前端有两套路由逻辑（详见 [frontend-api.md](frontend-api.md)）。

## 5. SSOT 与投影

- **`providers` 表是单一事实源（SSOT）**：用户录入的 provider（含 `settings_config`）存在 SQLite。
- **live 配置是投影**：`apply()` 把 provider 的 `settings_config` 写入 Agent 的真实配置文件（`~/.claude/settings.json`、`~/.config/opencode/opencode.json`…）。
- **回填**：`import()` 从 live 配置反读 provider，写回 DB，实现「从 live 导入」。
- 默认 `live_config_managed = 1`：DB 记录与 live 双向同步；改为 0 则 DB 记录不再投影。

## 6. 目录结构

```
v2/
├── src-tauri/src/
│   ├── commands/     # Tauri 命令（IPC API 层）
│   ├── plugin/       # 插件协议（trait + 各实现）
│   │   ├── mod.rs    # AgentPlugin trait、PluginCapabilities、数据类型
│   │   ├── opencode.rs    # 原生插件：~/.config/opencode/opencode.json
│   │   ├── claudecode.rs  # 原生参考实现（示例已切换为 TS 插件）
│   │   ├── process.rs     # shell 插件：调用外部命令
│   │   ├── mcp.rs        # McpPlugin trait + 格式转换
│   │   ├── ts.rs         # TS 插件占位（TsPluginStub）
│   │   └── error.rs      # PluginError
│   ├── registry.rs   # 插件注册表：解析 manifest、resolve_plugin、安装
│   ├── services/     # 与协议无关的业务：providers/mcp/skills/prompts/usage/backup/profiles
│   ├── db.rs         # SQLite schema 与访问
│   └── lib.rs        # 状态初始化、命令注册
├── src/              # 前端 React
│   ├── lib/api.ts    # 前端 → 后端 IPC 封装
│   ├── lib/plugin-loader.ts  # TS 插件加载器
│   └── components/   # 各 Panel
└── examples/plugins/ # 示例插件（claudecode 为 TS + 资源白名单，ts-demo）
```

## 7. 演进方向（摘要）

- **TS 插件沙箱放宽（部分完成）**：manifest `resources` 白名单 + 后端通用资源命令 `host_read/write/list_resource` 已实现，TS 插件可管理声明的用户目录资源（详见 [ts-plugin.md](ts-plugin.md)）；声明式插件（方案 B，后端通用解析器）仍为规划。
- 与 v1 的能力差距清单见 [v1-gap-analysis.md](v1-gap-analysis.md)。
