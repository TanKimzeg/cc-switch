/** 单个供应商记录（对应后端 providers 表）。 */
export interface Provider {
  id: string;
  pluginId: string;
  name: string;
  category: string;
  icon?: string;
  website?: string;
  apiKey?: string;
  settingsConfig?: string;
  meta?: Record<string, unknown>;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

/** 新增/编辑供应商的入参。 */
export interface ProviderInput {
  pluginId: string;
  name: string;
  category?: string;
  icon?: string;
  website?: string;
  apiKey?: string;
  settingsConfig?: string;
  meta?: Record<string, unknown>;
  sortOrder?: number;
}

/** 插件清单信息（manifest 的镜像）。 */
export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: string;
  author?: string;
  description?: string;
  icon?: string;
  capabilities?: PluginCapabilities;
  settingsSchema?: Record<string, unknown>;
}

/** 插件能力声明。 */
export interface PluginCapabilities {
  readLive?: boolean;
  apply?: boolean;
  import?: boolean;
  sessions?: boolean;
}

/** 已安装插件：清单字段 + 安装来源信息。 */
export interface InstalledPlugin extends PluginManifest {
  /** 安装来源：builtin（内置）| local（本地目录安装） */
  source: string;
  installedAt: string;
}

/** 插件 live 配置视图（plugin_read_live 返回值）。 */
export interface LiveConfig {
  providers: LiveProvider[];
  current?: string | null;
}

/** live 配置中的单个 provider。 */
export interface LiveProvider {
  id: string;
  name: string;
  settingsConfig: Record<string, unknown>;
}

/** 从 live 配置导入的 provider 候选（plugin_import 返回值）。 */
export interface ImportCandidate {
  id: string;
  name: string;
  settingsConfig: Record<string, unknown>;
}

/** 会话元信息（plugin_sessions 返回值）。 */
export interface SessionMeta {
  sessionId: string;
  title?: string | null;
  projectDir?: string | null;
  createdAt?: number | null;
  lastActiveAt?: number | null;
  sourcePath?: string | null;
  resumeCommand?: string | null;
}
