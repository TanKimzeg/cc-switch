//! TypeScript 插件宿主：前端动态加载插件脚本并注入宿主 API。
//!
//! TS 插件是一个自包含脚本（`main.ts`/`main.js`，由后端 `plugin_get_script`
//! 提供内容）。脚本导出一个 [`TsPluginExports`] 对象，各能力函数通过
//! 注入的 [`TsHost`] 调用 Tauri 命令读写配置。
//!
//! 加载机制：为避免 CSP 与动态 `import` 限制，脚本在 `new Function` 作用域
//! 内执行，仅暴露 `host`（TsHost）作为唯一接口，脚本不可访问任意 DOM/进程。

import { invoke } from "@tauri-apps/api/core";
import type {
  DailyUsageRow,
  ImportCandidate,
  LiveConfig,
  McpServerSpec,
  PluginCapabilities,
  Provider,
  ProviderInput,
  SessionMessage,
  SessionMeta,
  UsageRecord,
} from "@/types";

/** 宿主 API：TS 插件可调用的全部命令。 */
export interface TsHost {
  /** 读取插件目录内的文件。 */
  readFile(path: string): Promise<string>;
  /** 写入插件目录内的文件。 */
  writeFile(path: string, content: string): Promise<void>;
  /** 列出插件目录内容。 */
  listFiles(dir?: string): Promise<string[]>;
  /** 读取 manifest `resources` 白名单内声明的资源文件。 */
  readResource(name: string, rel?: string): Promise<string>;
  /** 写入 manifest `resources` 白名单内声明的资源文件。 */
  writeResource(name: string, content: string, rel?: string): Promise<void>;
  /** 列出 manifest `resources` 白名单内声明的资源目录。 */
  listResource(name: string, rel?: string): Promise<string[]>;

  // ---- 数据库（绑定当前插件 id，脚本无需手写参数）----
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

/** TS 插件导出契约。 */
export interface TsPluginExports {
  id: string;
  capabilities: PluginCapabilities;
  readLive?: () => Promise<LiveConfig>;
  apply?: (provider: unknown, current: boolean) => Promise<void>;
  removeProvider?: (id: string) => Promise<void>;
  import?: () => Promise<ImportCandidate[]>;
  sessions?: () => Promise<SessionMeta[]>;
  loadMessages?: (source: string) => Promise<SessionMessage[]>;
  deleteSession?: (sessionId: string, source: string) => Promise<boolean>;
  getMcpServers?: () => Promise<McpServerSpec[]>;
  setMcpServer?: (server: McpServerSpec) => Promise<void>;
  removeMcpServer?: (id: string) => Promise<void>;
  /** 读取/写入 live 配置原始文本。 */
  readRawConfig?: () => Promise<string>;
  writeRawConfig?: (content: string) => Promise<void>;
  /** 从插件自己的会话存储解析用量（对应后端 sync_usage）。 */
  syncUsage?: () => Promise<UsageRecord[]>;
}

/** 构建宿主对象（绑定插件 id，文件操作限定插件目录 + manifest 资源白名单）。 */
export function makeHost(pluginId: string): TsHost {
  return {
    readFile: (path) => invoke("host_read_file", { id: pluginId, path }),
    writeFile: (path, content) =>
      invoke("host_write_file", { id: pluginId, path, content }),
    listFiles: (dir) => invoke("host_list_files", { id: pluginId, dir }),
    readResource: (name, rel) =>
      invoke("host_read_resource", { id: pluginId, name, rel }),
    writeResource: (name, content, rel) =>
      invoke("host_write_resource", { id: pluginId, name, content, rel }),
    listResource: (name, rel) =>
      invoke("host_list_resource", { id: pluginId, name, rel }),

    providers: () => invoke("get_providers", { pluginId }),
    upsertProvider: (input) =>
      invoke("add_provider", {
        input: { ...input, pluginId },
        addToLive: false,
      }),
    deleteProvider: (providerId) =>
      invoke("delete_provider", { id: providerId }),
    saveUsageRecords: (records) =>
      invoke("usage_insert_records", { pluginId, records }),
    usageDailySummary: () => invoke("usage_daily_summary", { pluginId }),

    invoke: (command, args) => invoke(command, args),
  };
}

/**
 * 从脚本源码加载 TS 插件。
 *
 * 脚本末尾通过 `return` 或赋值给全局 `pluginExports` 导出契约。
 * 加载器在受限作用域执行脚本，仅注入 `host`。
 */
export async function loadTsPlugin(
  pluginId: string,
  source: string,
  hostOverride?: TsHost,
): Promise<TsPluginExports> {
  const host = hostOverride ?? makeHost(pluginId);

  // 用 Function 构造器执行脚本；脚本可通过 `host` 调用命令。
  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  const factory = new Function(
    "host",
    "pluginExports",
    `${source}
      return typeof pluginExports !== "undefined" && pluginExports
        ? pluginExports
        : typeof plugin !== "undefined" && plugin
          ? plugin
          : null;`,
  );

  const result = factory(host, null);
  if (!result || typeof result !== "object" || !result.id) {
    throw new Error(`TS 插件 '${pluginId}' 未导出有效的 plugin 对象`);
  }
  return result as TsPluginExports;
}

/** 从后端获取并加载一个 TS 插件。 */
export async function loadTsPluginById(
  pluginId: string,
  main: string,
): Promise<TsPluginExports> {
  const source = await invoke<string>("plugin_get_script", {
    id: pluginId,
    main,
  });
  return loadTsPlugin(pluginId, source);
}

const tsCache = new Map<string, Promise<TsPluginExports | null>>();

/**
 * 若插件是 TS 插件（entryType === "ts"），加载并缓存其导出对象；
 * 否则返回 null（调用方回退到后端命令）。
 */
export async function loadTsPluginIfTs(
  pluginId: string,
): Promise<TsPluginExports | null> {
  if (!tsCache.has(pluginId)) {
    tsCache.set(
      pluginId,
      (async () => {
        const plugins =
          await invoke<
            Array<{ id: string; entryType?: string; main?: string | null }>
          >("get_plugins");
        const p = plugins.find((x) => x.id === pluginId);
        if (p?.entryType !== "ts" || !p.main) return null;
        return loadTsPluginById(pluginId, p.main);
      })(),
    );
  }
  return tsCache.get(pluginId)!;
}
