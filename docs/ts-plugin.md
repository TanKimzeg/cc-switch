# TypeScript 插件（TS Plugin）

TS 插件由**前端（WebView）动态加载脚本执行**，通过注入的宿主 API（`TsHost`）与后端交互。本文档说明契约、宿主 API、加载机制、沙箱模型、写插件教程，以及当前沙箱限制与演进方向。

## 1. 为什么需要 TS 插件

- **无需重编译**：第三方可以分发一个 `.js` 脚本就完成插件接入，不需要用户编译 Rust。
- **轻量自包含**：适合自包含插件，也适合通过 manifest `resources` 白名单管理用户目录配置的真实 Agent（如 claudecode-ts 示例）。
- **统一功能面板**：TS 插件与 native/shell 共用同一套 `PluginDetail` 面板，操作经 `api.ts` 的 TS 路由自动调用加载的脚本。

> **定位边界**：TS 插件的宿主文件操作限定在「插件目录 + manifest `resources` 白名单」内。未声明 `resources` 的脚本无法读写用户目录；声明后可管理 `~/.claude/` 等真实 Agent 配置（见下文「沙箱模型与演进」）。

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
  /** 读取 manifest `resources` 白名单内声明的资源文件（后端执行，可访问用户目录）。 */
  readResource(name: string, rel?: string): Promise<string>;
  /** 写入 manifest `resources` 白名单内声明的资源文件。 */
  writeResource(name: string, content: string, rel?: string): Promise<void>;
  /** 列出 manifest `resources` 白名单内声明的资源目录。 */
  listResource(name: string, rel?: string): Promise<string[]>;
  /** 读取当前插件的全部 provider（SSOT）。 */
  providers(): Promise<Provider[]>;
  /** 新增/更新一个 provider（写入 SSOT，不投影 live）。 */
  upsertProvider(input: ProviderInput): Promise<Provider>;
  /** 删除一个 provider（DB 记录，不碰 live）。 */
  deleteProvider(providerId: string): Promise<void>;
  /** 写入用量记录（INSERT OR IGNORE 去重），返回导入条数。 */
  saveUsageRecords(records: UsageRecord[]): Promise<number>;
  /** 按日汇总当前插件用量。 */
  usageDailySummary(): Promise<DailyUsageRow[]>;
  /** 调用任意已注册的 Tauri 命令（由调用方保证参数合法）。 */
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}
```

- `readFile` / `writeFile` / `listFiles` 对应后端 `host_read_file` / `host_write_file` / `host_list_files` 命令，**路径被校验限定在 `plugins/<id>/` 内**。
- `readResource` / `writeResource` / `listResource` 对应后端 `host_read_resource` / `host_write_resource` / `host_list_resource`，**路径被校验限定在 manifest `resources` 声明的白名单根内**（可指向用户目录，如 `~/.claude/`）。
- **DB 方法**（`providers` / `upsertProvider` / `deleteProvider` / `saveUsageRecords` / `usageDailySummary`）：**自动绑定当前插件 id**，脚本无需手写 `invoke` 参数，也不会写错其他插件的数据。脚本保持纯逻辑，落库由宿主交给后端命令（`get_providers` / `add_provider` / `delete_provider` / `usage_insert_records` / `usage_daily_summary`）。
- `invoke` 可调用任何已注册 Tauri 命令（如 `usage_insert_records` 把用量写库）。

## 4. 加载机制

`loadTsPlugin`（`plugin-loader.ts`）：

1. 后端 `plugin_get_script` 返回插件主脚本内容（manifest `entry.main`）。
2. 用 `new Function("host", "pluginExports", source + return ...)` 构造执行，**仅注入 `host`**，脚本不可访问 DOM/进程。
3. 脚本通过全局 `plugin` 对象或 `pluginExports` 变量导出契约。
4. 结果缓存（`loadTsPluginIfTs`），后续调用复用。

> ⚠️ **不转译 TypeScript**：宿主用 `new Function` 直接执行，**不编译 TS**。因此脚本必须是**合法 JavaScript**（可用 JSDoc 标注类型），文件名建议用 `.js`。不能用 `interface` / `type` / `declare` / 参数类型注解等 TS 语法。

## 5. 沙箱模型

两级访问，均在后端做路径校验（canonicalize 后校验命中白名单，防符号链接/`..` 越界）：

- **插件目录**：`readFile`/`writeFile`/`listFiles` 限定在 `plugins/<id>/` 内。
- **资源白名单**：`readResource`/`writeResource`/`listResource` 限定在 manifest `resources` 声明的根内（可指向用户目录，如 `~/.claude/`）。

**前端路由**：`api.ts` 通过 `loadTsPluginIfTs(pluginId)` 判断插件是否为 TS（`entryType === "ts"`）；若是，则**直接调脚本方法**（`readLive/apply/...`），跳过后端命令（后端只有 `TsPluginStub` 占位，会报「请通过前端宿主执行」）。TS 插件与 native/shell 共用同一套 `PluginDetail` 面板。

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
  "resources": {
    "demo": "~/.cc-switch-demo"   // 资源白名单：TS 插件只能访问这里声明的根
  },
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
    // 资源白名单示例：把当前 provider 记到 manifest `resources.demo` 声明的文件
    await host.writeResource("demo", `current provider: ${id}`, "note.txt");
  },
};
```

### 数据库读写（暂存 Provider / 用量同步）

TS 插件**不直接碰 SQL**，脚本只做解析/转换，落库交给宿主自动绑定当前插件 id 的方法：

```js
// 用量同步：解析自己的会话存储 → 交给宿主写 request_logs（去重）
async function syncUsage() {
  const records = JSON.parse(await host.readFile("usage.json") || "[]");
  await host.saveUsageRecords(records);
  return records;
}

// 暂存 Provider：写入 SSOT，不投影 live
await host.upsertProvider({
  name: "My Provider",
  settingsConfig: JSON.stringify({ npm: "@ai-sdk/openai", options: { baseURL: "..." } }),
});

// 读取自己插件的 provider 列表
const providers = await host.providers();
```

> 安全边界：DB 方法在 `makeHost` 里强制注入 `pluginId`，脚本无法用 `upsertProvider` 写别的插件的 provider；`deleteProvider` 也只删自己的记录。

### 安装与使用

1. 把 `ts-demo` 目录通过「添加插件」从本地目录安装。
2. 应用将其注册为 TS 插件；前端 `api.ts` 会在操作时自动加载 `main.js` 并调用其方法。

> **真实示例**：`examples/plugins/claudecode-ts/` 是一个完整的 TS 插件，用资源白名单管理 Claude Code 的真实配置——`config` → `~/.claude/settings.json`、`mcp` → `~/.claude.json`、`projects` → `~/.claude/projects/**/*.jsonl`，实现 readLive/apply/import/sessions/loadMessages/deleteSession/MCP/rawConfig/syncUsage 全套能力。
> **注意**：Claude Code 的正式支持由内置 native 插件（`claudecode`，Rust 实现）提供；此示例改用 `claudecode-ts` id，用于 TS 插件开发参考，两者可共存。

## 7. 沙箱模型与演进

### 现状：两级访问

- **插件目录**（`readFile`/`writeFile`/`listFiles`）：沙箱限定 `plugins/<id>/` 内。
- **资源白名单**（`readResource`/`writeResource`/`listResource`）：**已实现（方案 A）**。manifest `resources` 声明允许访问的用户目录根（`~` 展开），后端校验路径命中声明的根后执行文件 I/O。TS 插件因此能管理真实 Agent 配置（如 `~/.claude/settings.json`），安全性由白名单保证。

### 方案 B：声明式插件（更激进，80% 场景免写代码，未实现）

- manifest 声明 config 路径 + 格式（JSON / JSON5 / YAML）+ 会话目录 + 会话格式（jsonl 等）。
- 后端用**通用解析器**直接实现 `read_live` / `apply` / `import` / `sessions` / `sync_usage`，无需任何脚本。
- TS / native 只留给需要自定义逻辑的 Agent。

> 实现细节与更多背景见 [plugin-protocol.md](plugin-protocol.md) 与 [v1-gap-analysis.md](v1-gap-analysis.md)。
