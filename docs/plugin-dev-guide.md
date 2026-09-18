# 插件开发指南（Developer Guide）

面向插件作者。读完本文你应能：
- 判断你的 Agent 该用 **TS 插件** 还是 **native 插件**
- 从零写一个可安装、可验证的插件（TS 与 native 各一例）
- 实现 Provider / MCP / 会话 / 用量 / Prompt / Skills 等能力

前置阅读：[plugin-protocol.md](plugin-protocol.md)（协议接口）、[architecture.md](architecture.md)（设计意图）、[ts-plugin.md](ts-plugin.md)（TS 宿主细节）。

---

## 1. 选形态：TS / native / shell

| 形态 | 代码位置 | 执行位置 | 沙箱 | 适用场景 |
|------|----------|----------|------|----------|
| **TS** | `main.js` 脚本（分发到插件目录） | 前端 WebView | 插件目录 + manifest `resources` 白名单 | 快速分发、免编译；逻辑可全部写在 JS 里 |
| **native** | Rust 源码（随二进制编译） | 后端 | 无 | 需要完整 Rust 能力（SQLite 解析、复杂格式）；随应用分发 |
| **shell** | 外部命令（manifest 声明 `command`+`args`） | 后端子进程 | 无 | 已有现成 CLI 可读写配置 |

**选择建议**
- 想快速写一个能管理真实 Agent 配置的插件，且愿意用 JS 表达解析/写入逻辑 → **TS 插件**（如 `claudecode-ts` 示例）。
- 需要高性能/复杂解析（如 opencode 的 SQLite 会话）、或希望插件随安装包分发 → **native 插件**（如 `opencode`）。
- 已有 CLI 能处理一切 → **shell 插件**（如 `openclaw`）。

> **关键**：无论哪种形态，命令层都通过 `registry.resolve_plugin(id)` 拿到 `Box<dyn AgentPlugin>` 统一调用。TS 插件在后端只有一个 `TsPluginStub` 占位，其真实逻辑由前端 `api.ts` 经 `loadTsPluginIfTs` 加载脚本执行——**但这对插件作者透明**：你写好 `main.js` 并声明能力即可，UI 与命令都会自动路由。

---

## 2. manifest.json（所有形态通用）

插件目录结构：`{app_data_dir}/plugins/<id>/manifest.json`（+ TS 插件的 `main.js`）。schema 详见 [plugin-protocol.md](plugin-protocol.md) §6，字段速览：

```jsonc
{
  "id": "my-agent",            // 必填，唯一
  "name": "My Agent",          // 必填，显示名
  "version": "0.1.0",          // 必填
  "apiVersion": "1",           // 必填，当前仅支持 "1"
  "author": "you",             // 可选
  "description": "...",        // 可选
  "capabilities": {            // 能力声明（缺省全 false）
    "readLive": true,          // 读取 live 配置
    "apply": true,             // 写入 live（切换）
    "remove": true,            // 从 live 移除 provider
    "import": true,            // 从 live 反向导入
    "sessions": true,          // 会话列表/加载/删除
    "mcp": true                // MCP 服务器管理
  },
  "promptFile": "~/.claude/CLAUDE.md",  // 可选，Prompt 文件路径（~ 展开）
  "skillsDir": "~/.claude/skills",      // 可选，Skills 目录（~ 展开）
  "resources": {               // 仅 TS 插件需要；声明可访问的用户目录根
    "config":   "~/.claude/settings.json",
    "projects": "~/.claude/projects"
  },
  "entry": {
    "type": "ts",              // "ts" | "native" | "shell"
    "main": "main.js"          // ts: 主脚本（相对插件目录）
    // native: "module": "my-agent"（二进制内置模块标识）
    // shell:  "command": "my-cli", "args": []
  }
}
```

---

## 3. 开发一个 TS 插件

### 3.1 完整示例：`examples/plugins/claudecode-ts/`

这是仓库里的真实示例：管理 Claude Code 的 `~/.claude/settings.json`（provider）、`~/.claude.json`（MCP）、`~/.claude/projects/**/*.jsonl`（会话/用量）。Claude Code 的正式支持由内置 native 插件提供，此示例（id `claudecode-ts`）用于 TS 插件开发参考。

**manifest.json**

```jsonc
{
  "id": "claudecode-ts",
  "name": "Claude Code (TS)",
  "version": "0.1.0",
  "apiVersion": "1",
  "capabilities": {
    "readLive": true, "apply": true, "remove": true,
    "import": true, "sessions": true, "mcp": true
  },
  "resources": {
    "config":   "~/.claude/settings.json",
    "mcp":      "~/.claude.json",
    "projects": "~/.claude/projects"
  },
  "promptFile": "~/.claude/CLAUDE.md",
  "skillsDir":  "~/.claude/skills",
  "entry": { "type": "ts", "main": "main.js" }
}
```

**main.js 骨架**（合法 JS；宿主用 `new Function` 执行，**不转译 TS**，不要用 `interface`/类型注解）

```js
// 宿主注入的 host 提供资源访问与 DB 方法（详见 ts-plugin.md §3）
const RES_CONFIG = "config";   // 对应 manifest resources.config
const RES_MCP = "mcp";
const RES_PROJECTS = "projects";

async function readSettings() {
  try { return JSON.parse(await host.readResource(RES_CONFIG) || "{}"); }
  catch { return {}; }
}

const plugin = {
  id: "claudecode-ts",
  capabilities: { readLive: true, apply: true, remove: true, import: true, sessions: true, mcp: true },

  async readLive() {
    const settings = await readSettings();
    return { providers: [{ id: "default", name: "Claude Code", settingsConfig: settings }], current: "default" };
  },

  async apply(provider, current) {
    // provider.settingsConfig 是 JSON 字符串；解析后写入 settings.json
    const parsed = JSON.parse(provider.settingsConfig || "{}");
    await host.writeResource(RES_CONFIG, JSON.stringify(parsed, null, 2));
  },

  async removeProvider(id) {
    const settings = await readSettings();
    delete settings.env; delete settings.apiProvider; delete settings.model;
    await host.writeResource(RES_CONFIG, JSON.stringify(settings, null, 2));
  },

  async import() {
    const settings = await readSettings();
    return [{ id: "default", name: "Claude Code", settingsConfig: settings }];
  },

  async sessions() { /* 扫描 RES_PROJECTS 下的 *.jsonl */ },
  async loadMessages(source) { /* 读 RES_PROJECTS/source 解析 */ },
  async deleteSession(sessionId, source) { /* 空文件标记删除 */ },

  async getMcpServers() {
    const root = JSON.parse(await host.readResource(RES_MCP) || "{}");
    const map = root.mcpServers || {};
    return Object.keys(map).map((id) => {
      const spec = { ...map[id] };
      if (spec.type == null) {
        if (spec.command != null) spec.type = "stdio";
        else if (spec.url != null) spec.type = "sse";
      }
      return { id, name: id, spec };
    });
  },
  async setMcpServer(server) { /* 写 RES_MCP 的 mcpServers[server.id] */ },
  async removeMcpServer(id) { /* 删 mcpServers[id] */ },

  async readRawConfig() { return JSON.stringify(await readSettings(), null, 2); },
  async writeRawConfig(content) { await host.writeResource(RES_CONFIG, content); },

  async syncUsage() {
    const records = []; // 解析会话 jsonl 聚合 assistant token
    if (records.length > 0) await host.saveUsageRecords(records); // 落库由宿主代劳
    return records;
  },
};
```

### 3.2 host API（脚本内可用）

| 方法 | 用途 | 沙箱 |
|------|------|------|
| `host.readFile/writeFile/listFiles` | 插件目录内文件 | 仅插件目录 |
| `host.readResource/writeResource/listResource(name, rel?)` | manifest `resources` 白名单内文件 | 白名单根 |
| `host.providers/upsertProvider/deleteProvider` | DB provider（绑定自身插件 id） | DB |
| `host.saveUsageRecords(records)` | 用量落库（去重） | DB |
| `host.usageDailySummary()` | 查询本插件日用量 | DB |
| `host.invoke(cmd, args)` | 调任意已注册 Tauri 命令 | 无（自行保证合法） |

> 资源读写由**后端代劳**（路径校验命中白名单），脚本只写解析/转换逻辑。DB 方法自动绑定当前插件 id，脚本无法越权操作其他插件数据。

### 3.3 安装与验证

1. 把插件目录（含 `manifest.json` + `main.js`）放到应用可安装位置，或用前端「添加插件」从本地目录安装。
2. 重启应用，`get_plugins` 应返回该插件（`entryType: "ts"`、`main: "main.js"`）。
3. 前端 `api.ts` 会自动加载脚本并路由所有能力操作（Provider/MCP/会话/用量/rawConfig 均可用，三形态共用同一套 UI 面板）。
4. 可参考 `v2/src/lib/plugin-loader.claudecode.test.ts` 用「内存资源宿主」做单测。

---

## 4. 开发一个 native 插件

native 插件是随二进制编译的 Rust 实现。仓库里现有 `opencode`、`claudecode`（均内置分发）。

### 4.1 步骤

1. **新建文件** `src-tauri/src/plugin/<id>.rs`，实现 `AgentPlugin`（可选 `McpPlugin`）。
2. **注册模块**：在 `src-tauri/src/plugin/mod.rs` 加 `pub mod <id>;` 与 `pub use <id>::<PluginName>;`。
3. **注册解析**：在 `registry.rs` 的 `resolve_plugin` 里 `ManifestEntry::Native { module }` 匹配 `<id>` → `Ok(Box::new(<PluginName>::new()))`。
4. **写 manifest**：`entry.type = "native"`, `module = "<id>"`。
5. **路径约定**：读用户目录用 `home_dir()`（支持 `CC_SWITCH_TEST_HOME` 测试覆盖）与 `override_dir("CC_SWITCH_<NAME>_CONFIG_DIR")`，见 opencode.rs 模式。

### 4.2 最小实现

```rust
// src-tauri/src/plugin/myagent.rs
use std::path::PathBuf;
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities, PluginError, SessionMeta};
use crate::types::Provider;

pub struct MyAgentPlugin;

impl MyAgentPlugin {
    pub fn new() -> Self { Self }
}

impl AgentPlugin for MyAgentPlugin {
    fn id(&self) -> &str { "my-agent" }
    fn capabilities(&self) -> &PluginCapabilities {
        static C: PluginCapabilities = PluginCapabilities {
            read_live: true, apply: true, remove: true, import: true,
            sessions: false, mcp: true,
        };
        &C
    }
    fn read_live(&self) -> Result<LiveConfig, PluginError> {
        // 读你的 live 配置文件，映射为 LiveConfig
        Ok(LiveConfig::default())
    }
    fn apply(&self, provider: &Provider, _current: bool) -> Result<(), PluginError> {
        // 把 provider.settings_config 写入 live 配置
        let _ = provider;
        Ok(())
    }
    fn remove_provider(&self, _id: &str) -> Result<(), PluginError> { Ok(()) }
    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> { Ok(vec![]) }
    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> { Ok(vec![]) }
    // 可选：MCP、prompt_file_path、skills_dir、sync_usage、read_raw_config 等
}
```

### 4.3 测试

- 路径解析：`CC_SWITCH_TEST_HOME` 指向临时目录，构造真实文件，断言 `read_live/apply/sessions/sync_usage` 行为（参考 opencode.rs / claudecode.rs 的 `#[cfg(test)] mod tests`）。
- 环境变量互斥：用 `crate::test_support::env_lock()` 串行化改环境变量的测试。
- 端到端：`cargo test <id>` 验证。

---

## 5. 能力实现对照

| 能力 | TS 插件写法 | native 插件写法 | 软件层动作 |
|------|-------------|-----------------|-----------|
| Provider 读取 | `readLive()` → LiveConfig | `read_live()` | 命令 `plugin_read_live` |
| Provider 切换 | `apply(provider, current)` 写配置 | `apply()` 写配置 | `applyProvider` |
| Provider 移除 | `removeProvider(id)` | `remove_provider(id)` | `removeProviderFromLive` |
| 从 live 导入 | `import()` → 候选 | `import()` | `importFromLive` + 落库 |
| 会话 | `sessions/loadMessages/deleteSession` | 同左 | `listSessions/loadSessionMessages/deleteSession` |
| MCP | `getMcpServers/setMcpServer/removeMcpServer` | 实现 `McpPlugin` | `getMcpServers/setMcpServer/...` |
| Prompt | manifest `promptFile`（后端代写文件） | `prompt_file_path()` | `prompts_toggle` 写文件 |
| Skills | manifest `skillsDir`（后端代复制） | `skills_dir()` | `skills_toggle_plugin` 复制/移除 |
| 用量 | `syncUsage()` + `host.saveUsageRecords` | `sync_usage()` | 写 `request_logs`（去重） |
| 原始配置编辑 | `readRawConfig/writeRawConfig` | `read/write_raw_config` | `readRawConfig/writeRawConfig` |

---

## 6. 常见坑

- **TS 脚本必须是合法 JS**：宿主用 `new Function` 执行，不转译 TS。用 `.js` 扩展名 + JSDoc。
- **resource 指向不存在的文件**：首次写入允许（后端会建父目录），无需 pre-create。
- **MCP spec 缺省 type**：Claude 等 Agent 的 `mcpServers` 条目常无 `type`，`getMcpServers` 应按字段补全（`command`→stdio、`url`→sse），否则展示与格式转换异常。
- **能力声明必须与 manifest 一致**：命令层用 `capabilities` 校验；漏声明会导致 UI 隐藏对应操作。
- **native 插件路径**：不要写死 home，用 `home_dir()`/`override_dir()` 支持测试覆盖。
- **并发安全**：`AgentPlugin` 要求 `Send + Sync`；文件读写注意原子性（参考 `atomic_write`）。

---

## 7. 参考实现

| 插件 | 形态 | 位置 | 亮点 |
|------|------|------|------|
| `opencode` | native（内置） | `src-tauri/src/plugin/opencode.rs` | additive 模型、SQLite 会话、MCP 格式转换 |
| `claudecode` | native（内置） | `src-tauri/src/plugin/claudecode.rs` | 非 additive、会话 jsonl、用量聚合、MCP 安装守卫 |
| `claudecode-ts` | ts（示例） | `examples/plugins/claudecode-ts/` | 同一 Agent 的 TS 实现对照（资源白名单） |
| `ts-demo` | ts（示例） | `examples/plugins/ts-demo/` | 最简 TS 插件 + 资源/DB 演示 |
| `openclaw` | shell | `src-tauri/plugins/openclaw/` | 外部命令约定 |

测试参考：`v2/src/lib/plugin-loader.claudecode.test.ts`（TS，内存宿主）、`src-tauri/src/plugin/opencode.rs` / `claudecode.rs` 的 `#[cfg(test)]`（native）。
