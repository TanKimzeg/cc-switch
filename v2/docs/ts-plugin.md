# TypeScript 插件（TS Plugin）

TS 插件由**前端（WebView）动态加载脚本执行**，通过注入的宿主 API（`TsHost`）与后端交互。本文档说明契约、宿主 API、加载机制、沙箱模型、写插件教程，以及当前沙箱限制与演进方向。

## 1. 为什么需要 TS 插件

- **无需重编译**：第三方可以分发一个 `.js` 脚本就完成插件接入，不需要用户编译 Rust。
- **轻量自包含**：适合「配置就放在自己插件目录内」的简单 Agent 或实验性插件。

> ⚠️ **定位边界**：TS 插件的宿主文件操作被**沙箱限制在插件目录内**，因此它**无法**管理配置在用户目录（`~/.claude/`、`~/.config/...`）的真实 Agent。这类 Agent 请用 **native**（Rust）插件。详见下文「沙箱演进方向」。

## 2. 导出契约 `TsPluginExports`

定义在 `v2/src/lib/plugin-loader.ts`。脚本需导出（或声明全局）一个对象：

```ts
interface TsPluginExports {
  id: string;
  capabilities: PluginCapabilities; // readLive/apply/remove/import/sessions/mcp
  readLive?: () => Promise<LiveConfig>;
  apply?: (provider: { id: string; name?: string; settingsConfig?: string }, current: boolean) => Promise<void>;
  removeProvider?: (id: string) => Promise<void>;
  import?: () => Promise<ImportCandidate[]>;
  sessions?: () => Promise<SessionMeta[]>;
  loadMessages?: (source: string) => Promise<SessionMessage[]>;
  deleteSession?: (sessionId: string, source: string) => Promise<boolean>;
  getMcpServers?: () => Promise<McpServerSpec[]>;
  setMcpServer?: (server: McpServerSpec) => Promise<void>;
  removeMcpServer?: (id: string) => Promise<void>;
  readRawConfig?: () => Promise<string>;
  writeRawConfig?: (content: string) => Promise<void>;
  syncUsage?: () => Promise<UsageRecord[]>;
}
```

> 契约与后端 `AgentPlugin` trait 一一对应，TS 插件作者可按能力声明实现其中的方法。

## 3. 宿主 API `TsHost`

脚本通过注入的 `host` 变量与后端交互：

```ts
interface TsHost {
  /** 读取插件目录内的文件（沙箱：仅插件目录）。 */
  readFile(path: string): Promise<string>;
  /** 写入插件目录内的文件（沙箱：仅插件目录，自动建父目录）。 */
  writeFile(path: string, content: string): Promise<void>;
  /** 列出插件目录内容。 */
  listFiles(dir?: string): Promise<string[]>;
  /** 调用任意已注册的 Tauri 命令（由调用方保证参数合法）。 */
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}
```

- `readFile` / `writeFile` / `listFiles` 对应后端 `host_read_file` / `host_write_file` / `host_list_files` 命令，**路径被校验限定在 `plugins/<id>/` 内**。
- `invoke` 可调用任何已注册 Tauri 命令（如 `usage_insert_records` 把用量写库）。

## 4. 加载机制

`loadTsPlugin`（`plugin-loader.ts`）：

1. 后端 `plugin_get_script` 返回插件主脚本内容（manifest `entry.main`）。
2. 用 `new Function("host", "pluginExports", source + return ...)` 构造执行，**仅注入 `host`**，脚本不可访问 DOM/进程。
3. 脚本通过全局 `plugin` 对象或 `pluginExports` 变量导出契约。
4. 结果缓存（`loadTsPluginIfTs`），后续调用复用。

> ⚠️ **不转译 TypeScript**：宿主用 `new Function` 直接执行，**不编译 TS**。因此脚本必须是**合法 JavaScript**（可用 JSDoc 标注类型），文件名建议用 `.js`。不能用 `interface` / `type` / `declare` / 参数类型注解等 TS 语法。

## 5. 沙箱模型

- 后端 `host.rs` 的 `resolve_plugin_path` 会 **canonicalize 后校验目标路径必须位于 `plugins/<id>/` 内**，越界（如 `../`、绝对路径）一律拒绝。
- 好处：第三方脚本无法读写用户敏感文件。
- 代价：TS 插件无法管理真实 Agent 的配置/会话/用量（那些都在用户目录）。

**前端路由**：`api.ts` 通过 `loadTsPluginIfTs(pluginId)` 判断插件是否为 TS（`entryType === "ts"`）；若是，则**直接调脚本方法**（`readLive/apply/...`），跳过后端命令（后端只有 `TsPluginStub` 占位，会报「请通过前端宿主执行」）。

## 6. 写一个 TS 插件（教程）

以 `examples/plugins/ts-demo/` 为例：

### manifest.json

```jsonc
{
  "id": "ts-demo",
  "name": "TS Demo",
  "version": "0.1.0",
  "apiVersion": "1",
  "capabilities": { "readLive": true, "apply": true },
  "entry": { "type": "ts", "main": "main.js" }
}
```

### main.js（合法 JS，用 JSDoc）

```js
// 通过注入的 host 读写插件目录内的 state.json
const CONFIG_PATH = "state.json";

async function readConfig() {
  try {
    const raw = await host.readFile(CONFIG_PATH);
    return raw ? JSON.parse(raw) : { providers: [] };
  } catch {
    return { providers: [] };
  }
}

const plugin = {
  id: "ts-demo",
  capabilities: { readLive: true, apply: true },
  async readLive() {
    const config = await readConfig();
    return {
      providers: (config.providers || []).map((p) => ({
        id: p.id,
        name: p.name || p.id,
        settingsConfig: p,
      })),
      current: config.current || null,
    };
  },
  async apply(provider, current) {
    const config = await readConfig();
    const id = provider.id;
    const existing = config.providers || [];
    const idx = existing.findIndex((p) => p.id === id);
    let parsed = {};
    try { parsed = JSON.parse(provider.settingsConfig || "{}"); } catch {}
    if (idx >= 0) existing[idx] = { ...existing[idx], ...parsed, id };
    else existing.push({ ...parsed, id, name: provider.name || id });
    await host.writeFile(CONFIG_PATH, JSON.stringify({ ...config, providers: existing, current: id }, null, 2));
  },
};
```

### 安装与使用

1. 把 `ts-demo` 目录通过「添加插件」从本地目录安装。
2. 应用将其注册为 TS 插件；前端 `api.ts` 会在操作时自动加载 `main.js` 并调用其方法。

## 7. 沙箱演进方向（规划，未实现）

TS 插件沙箱过窄，无法管理真实 Agent。两条演进路径：

### 方案 A：manifest 声明「资源白名单」+ 后端通用资源命令（推荐中间态）

- manifest 新增 `resources` 声明允许访问的用户目录根，如：
  ```jsonc
  "resources": {
    "config":  "~/.claude/settings.json",
    "projects": "~/.claude/projects",
    "mcp":     "~/.claude.json"
  }
  ```
- 后端新增通用命令 `host_read_resource` / `host_write_resource` / `host_list_resource`，路径校验从「仅插件目录」放宽为「命中 manifest 声明的根」。
- TS 插件仍写解析/转换逻辑，文件 I/O 全走后端；安全性不丢（白名单显式声明、可审计）。
- 需要后端在 `host.rs` 增加基于 manifest `resources` 的路径校验，并在 `plugin-loader.ts` 的 `TsHost` 暴露对应方法。

### 方案 B：声明式插件（更激进，80% 场景免写代码）

- manifest 声明 config 路径 + 格式（JSON / JSON5 / YAML）+ 会话目录 + 会话格式（jsonl 等）。
- 后端用**通用解析器**直接实现 `read_live` / `apply` / `import` / `sessions` / `sync_usage`，无需任何脚本。
- TS / native 只留给需要自定义逻辑的 Agent。

> 两者不冲突：A 是 B 的中间态。实现优先级与更多背景见 [v1-gap-analysis.md](v1-gap-analysis.md)。
