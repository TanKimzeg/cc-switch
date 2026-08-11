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
  ImportCandidate,
  LiveConfig,
  McpServerSpec,
  PluginCapabilities,
  SessionMessage,
  SessionMeta,
} from "@/types";

/** 宿主 API：TS 插件可调用的全部命令。 */
export interface TsHost {
  /** 读取插件目录内的文件。 */
  readFile(path: string): Promise<string>;
  /** 写入插件目录内的文件。 */
  writeFile(path: string, content: string): Promise<void>;
  /** 列出插件目录内容。 */
  listFiles(dir?: string): Promise<string[]>;
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
}

/** 构建宿主对象（绑定插件 id，文件操作限定插件目录）。 */
export function makeHost(pluginId: string): TsHost {
  return {
    readFile: (path) => invoke("host_read_file", { id: pluginId, path }),
    writeFile: (path, content) =>
      invoke("host_write_file", { id: pluginId, path, content }),
    listFiles: (dir) => invoke("host_list_files", { id: pluginId, dir }),
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
): Promise<TsPluginExports> {
  const host = makeHost(pluginId);

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
