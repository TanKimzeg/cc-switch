# 插件协议（Plugin Protocol）

本文档描述 v2 的插件协议接口：`AgentPlugin` trait、`McpPlugin` trait、能力声明、数据类型与 `manifest.json` schema。代码位于 `v2/src-tauri/src/plugin/` 与 `v2/src-tauri/src/registry.rs`。

## 1. 核心概念

插件 = 「把 Agent 的真实配置格式翻译成协议数据结构」的适配器。核心代码通过 `registry.resolve_plugin(id)` 拿到 `Box<dyn AgentPlugin>`，只调用 trait 方法，不关心插件是 native / shell / ts。

## 2. `AgentPlugin` trait

定义在 `v2/src-tauri/src/plugin/mod.rs`。实现方必须 `Send + Sync`（Tauri 全局状态并发访问）。

```rust
pub trait AgentPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> &PluginCapabilities;

    // ---- Provider 配置 ----
    fn read_live(&self) -> Result<LiveConfig, PluginError>;
    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError>;
    fn remove_provider(&self, id: &str) -> Result<(), PluginError>;   // 默认：不支持
    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError>;

    // ---- 会话 ----
    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError>;
    fn load_messages(&self, source: &str) -> Result<Vec<SessionMessage>, PluginError>; // 默认：不支持
    fn delete_session(&self, session_id: &str, source: &str) -> Result<bool, PluginError>; // 默认：不支持

    // ---- 可选能力 ----
    fn as_mcp(&self) -> Option<&dyn McpPlugin>;   // 默认 None
    fn prompt_file_path(&self) -> Option<PathBuf>; // 默认 None（不支持 Prompts）
    fn skills_dir(&self) -> Option<PathBuf>;       // 默认 None（不支持 Skills 同步）
    fn read_raw_config(&self) -> Result<String, PluginError>;   // 默认：不支持
    fn write_raw_config(&self, content: &str) -> Result<(), PluginError>; // 默认：不支持
    fn sync_usage(&self) -> Result<Vec<UsageRecord>, PluginError>; // 默认：不支持
}
```

### 方法语义

- `read_live()`：返回该 Agent live 配置中的全部 provider 与当前选中项。
  - **additive**（如 opencode）：live 配置保留全部 provider，`providers` 多个。
  - **非 additive**（如 claudecode）：live 配置只有一个生效配置，返回单个 `default` provider。
- `apply(provider, current)`：把 provider 的 `settings_config` 写入 live 配置。`current=true` 表示同时标记为当前生效。
- `remove_provider(id)`：从 live 配置移除。非 additive 插件通常清空相关字段（保留用户其他配置）。
- `import()`：从 live 配置反向读回 provider 候选（`ImportCandidate`），供软件层写入 DB（回填）。
- `sessions()` / `load_messages(source)` / `delete_session(id, source)`：会话浏览。`source` 是 `SessionMeta::source_path` 返回的来源引用（文件路径或 `sqlite:<db>:<session>`）。
- `prompt_file_path()` / `skills_dir()`：声明该 Agent 的提示词文件 / skills 目录。软件层据此写入/复制/删除。返回 `None` = 不支持。
- `read_raw_config()` / `write_raw_config()`：live 配置原始文本（JSON/JSON5），供用户手动编辑（"编辑 live" 按钮）。
- `sync_usage()`：从插件自己的会话存储解析 token/cost，返回 `UsageRecord` 列表；软件层写入 `request_logs` 并去重。

## 3. `McpPlugin` trait

定义在 `v2/src-tauri/src/plugin/mcp.rs`。通过 `AgentPlugin::as_mcp()` 获取。

```rust
pub trait McpPlugin: Send + Sync {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError>;
    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError>;
    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError>;
}
```

`McpServerSpec` 使用 **CC Switch 统一格式**：

| 字段 | 类型 |
|------|------|
| `id` | String |
| `name` | String |
| `spec` | JSON（`type` / `command` / `args` / `env` / `url` / `headers`） |

`spec.type` 取值：`stdio`（`command`+`args`）、`sse`/`http`（`url`）。插件负责在统一格式 ↔ 各 Agent 原生格式间转换（如 opencode 的 `local`/`remote`、claude 的 `~/.claude.json` 的 `mcpServers` 映射）。

## 4. 能力声明 `PluginCapabilities`

manifest 中 `capabilities` 字段，用于命令层校验「该插件是否支持某操作」：

```rust
pub struct PluginCapabilities {
    pub read_live: bool,
    pub apply: bool,
    pub remove: bool,
    pub import: bool,
    pub sessions: bool,
    pub mcp: bool,
}
```

命令层用 `require_capability(plugin, capability, action)` 在调用前校验；不声明则返回 `PluginError::Capability`。

## 5. 数据类型

| 类型 | 字段 | 用途 |
|------|------|------|
| `LiveConfig` | `providers: Vec<LiveProvider>`, `current: Option<String>` | `read_live` 返回值 |
| `LiveProvider` | `id`, `name`, `settings_config: Value` | live 中单个 provider 视图 |
| `ImportCandidate` | `id`, `name`, `settings_config: Value` | `import` 返回的可导入候选 |
| `SessionMeta` | `session_id`, `title?`, `project_dir?`, `created_at?`, `last_active_at?`, `source_path?`, `resume_command?` | `sessions` 返回的会话元信息 |
| `SessionMessage` | `role`, `content`, `ts?` | `load_messages` 返回的单条消息 |
| `UsageRecord` | `source_id`, `session_id`, `model`, `input_tokens`, `output_tokens`, `reasoning_tokens`, `cache_read_tokens`, `cache_write_tokens`, `cost`, `timestamp_ms` | `sync_usage` 返回的单条用量记录（`source_id` 为去重键） |
| `PluginError` | `Io` / `Json` / `Config` / `Capability` / `Process` / `Other` | 协议错误 |

## 6. `manifest.json` schema

插件目录：`{app_data_dir}/plugins/<id>/`，必须含 `manifest.json`。解析见 `registry.rs` 的 `ManifestFile`。

```jsonc
{
  "id": "claudecode",          // 必填，唯一
  "name": "Claude Code",       // 必填，显示名
  "version": "0.1.0",          // 必填
  "apiVersion": "1",           // 必填，当前支持 "1"
  "author": "cc-switch",       // 可选
  "description": "...",        // 可选
  "icon": "opencode",          // 可选
  "capabilities": {            // 可选，能力声明
    "readLive": true,
    "apply": true,
    "remove": true,
    "import": true,
    "sessions": true,
    "mcp": true
  },
  "promptFile": "~/.claude/CLAUDE.md",  // 可选，prompt 文件路径（~ 展开）
  "skillsDir": "~/.claude/skills",      // 可选，skills 目录（~ 展开）
  "resources": {                        // 可选，TS 插件资源白名单（~ 展开）
    "config":   "~/.claude/settings.json",
    "projects": "~/.claude/projects",
    "mcp":      "~/.claude.json"
  },
  "entry": {                  // 必填，入口（tag = type）
    "type": "native",          // native | shell | ts
    "module": "claudecode"     // native：原生模块标识
  }
}
```

### 入口类型

| `entry.type` | 额外字段 | 解析结果 |
|--------------|----------|----------|
| `native` | `module`（默认取 `id`） | `resolve_plugin` 匹配内置模块：`opencode` → `OpenCodePlugin`，`claudecode` → `ClaudeCodePlugin`，`codex` → `CodexPlugin`，`grokbuild` → `GrokBuildPlugin`，`hermes` → `HermesPlugin`；未知模块报错 |
| `shell` | `command`, `args` | 包装为 `ProcessPlugin`，调用外部命令的子命令（`read-live` / `apply` / `import` / `sessions`） |
| `ts` | `main`（如 `main.js`） | `TsPluginStub` 占位；实际逻辑由前端加载脚本执行 |

### `promptFile` / `skillsDir` / `resources` 解析

- **native 插件**：可直接在 trait 实现中计算（如 opencode 返回 `~/.config/opencode/AGENTS.md`），也可用 manifest 声明。
- **TS / shell 插件**：后端无法执行其脚本，因此这些路径必须在 manifest 声明（`~` 展开为 home 目录）：
  - `promptFile` / `skillsDir` → `TsPluginStub` / registry 暴露 `prompt_file_path()` / `skills_dir()`。
  - `resources` → `registry.resource_roots()` 暴露资源白名单，供 TS 宿主命令 `host_read/write/list_resource` 校验路径（见 [ts-plugin.md](ts-plugin.md)）。

> **前端可见性**：`to_manifest()` 将 `promptFile` / `skillsDir` 一并暴露到前端 `PluginManifest`（`get_plugins` 返回）。Skills 面板据此筛选「支持 skills 同步」的插件做启用开关。**native 插件由 `list_installed` 以 trait 实现回填 `capabilities` / `skillsDir` / `promptFile`**（含目录覆盖动态解析），manifest 漏声明不会导致前端隐藏；TS 插件以 manifest 声明为准。

## 7. 插件生命周期（registry）

- **发现**：`discover()` 扫描 `plugins/` 目录，解析并校验所有 manifest。
- **安装**：`install_plugin(source)` 从本地目录复制到 `plugins/<id>/`。
- **卸载**：`uninstall(id)` 删除目录与 `plugin_installs` 记录；内置插件拒绝卸载。
- **内置 seed**：`seed_builtin()` 每次启动覆盖写入 `openclaw` / `opencode` / `claudecode` / `codex` / `grokbuild` / `hermes` 内置 manifest。
- **安装来源**：`plugin_installs.source` = `builtin` | `local`；`sync_installs` 仅把内置 id 标记为 builtin，手动安装的插件保留 local（避免重启被覆盖成 builtin）。例外：id 属于内置清单的既有 local 记录（如 TS 示例升级为 native）同步改标 builtin。

## 8. 内置插件

| id | 形态 | 配置位置 | 能力 |
|----|------|----------|------|
| `opencode` | native | `~/.config/opencode/opencode.json`（additive）+ `~/.config/opencode/opencode.db`（会话/用量） | Provider/MCP/Sessions/Usage/Prompt/Skills |
| `openclaw` | shell | 外部 `openclaw` 命令 | Provider（read_live/apply/import） |
| `claudecode` | native | `~/.claude/settings.json`（非 additive）+ `~/.claude.json`（MCP）+ `~/.claude/projects/**/*.jsonl`（会话/用量） | Provider/MCP/Sessions/Usage/Prompt/Skills |
| `codex` | native | `~/.codex/config.toml` + `auth.json`（非 additive，settings_config 形状 `{"auth":{},"config":"<toml>"}`）+ `~/.codex/sessions`（rollout jsonl） | Provider/MCP/Sessions/Prompt/Skills（Usage 暂缺，见 gap-analysis §3.20） |
| `grokbuild` | native | `~/.grok/config.toml`（非 additive，settings_config 形状 `{"config":"<toml>"}`；官方条目允许空文档）+ `~/.grok/sessions`（summary.json + chat_history.jsonl） | Provider/MCP/Sessions/Prompt/Skills（Usage 暂缺） |
| `hermes` | native | `~/.hermes/config.yaml`（additive `custom_providers`，Windows 默认 `%LOCALAPPDATA%\hermes`）+ `state.db` + `sessions/*.jsonl` | Provider/MCP/Sessions（含 remove）/Prompt/Skills（Usage 暂缺） |
| `claudecode-ts`（示例，需手动安装） | ts | 同 claudecode（经 manifest `resources` 白名单，由前端脚本宿主执行） | Provider/MCP/Sessions/Usage/Prompt/Skills |
| `ts-demo`（示例） | ts | 插件目录内 `state.json` + `~/.cc-switch-demo`（资源白名单） | Provider（readLive/apply） |

> 三个 TOML/YAML 配置型插件的目录覆盖：`overrideDir.<id>`（设置页）优先，其次环境变量（codex/grokbuild：`CC_SWITCH_<NAME>_CONFIG_DIR`；hermes：`HERMES_HOME`），最后平台默认。
