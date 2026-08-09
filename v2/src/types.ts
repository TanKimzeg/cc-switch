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
  remove?: boolean;
  import?: boolean;
  sessions?: boolean;
  mcp?: boolean;
  plugins?: boolean;
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

/** 会话消息（plugin_load_messages 返回值）。 */
export interface SessionMessage {
  role: string;
  content: string;
  ts?: number | null;
}

/** MCP 服务器（统一格式）。 */
export interface McpServerSpec {
  id: string;
  name: string;
  spec: Record<string, unknown>;
}

/** MCP 服务器记录（含各插件启用状态）。 */
export interface McpServer {
  id: string;
  name: string;
  spec: Record<string, unknown>;
  description?: string | null;
  homepage?: string | null;
  docs?: string | null;
  tags?: string[];
  apps: Array<[string, boolean]>;
}

/** 技能清单记录。 */
export interface SkillRecord {
  id: string;
  name: string;
  description?: string | null;
  directory: string;
  sourcePath: string;
  enabledPlugins: string[];
  installedAt: number;
}

/** Prompt 记录。 */
export interface PromptRecord {
  id: string;
  pluginId: string;
  name: string;
  content: string;
  description?: string | null;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 配置方案（profile）。 */
export interface Profile {
  id: string;
  name: string;
  payload: Record<string, unknown>;
  sortOrder?: number | null;
  createdAt?: number | null;
  updatedAt?: number | null;
}

/** 请求日志行。 */
export interface RequestLogRow {
  requestId: string;
  pluginId: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalCostUsd: string;
  sessionId?: string | null;
  createdAt: number;
}

/** 每日用量汇总。 */
export interface DailyUsageRow {
  day: string;
  pluginId: string;
  model: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  costUsd: number;
}

/** 数据库备份记录。 */
export interface BackupRecord {
  id: string;
  name: string;
  filePath: string;
  sizeBytes: number;
  createdAt: number;
}

/** 完整配置导出负载。 */
export interface ExportPayload {
  version: number;
  providers: Record<string, unknown>[];
  mcpServers: Record<string, unknown>[];
  skills: Record<string, unknown>[];
  prompts: Record<string, unknown>[];
  profiles: Record<string, unknown>[];
}
