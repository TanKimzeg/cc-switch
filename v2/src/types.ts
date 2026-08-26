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
  /** 是否投影到 live 配置（SSOT 标记） */
  liveConfigManaged?: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 新增/编辑供应商的入参。 */
export interface ProviderInput {
  /** additive 插件（如 opencode）的用户自定义 provider id（live 键） */
  id?: string;
  pluginId: string;
  name: string;
  category?: string;
  icon?: string;
  website?: string;
  apiKey?: string;
  settingsConfig?: string;
  meta?: Record<string, unknown>;
  sortOrder?: number;
  /** 是否投影到 live 配置（默认 true） */
  liveConfigManaged?: boolean;
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
  /** 提示词文件路径（相对 home，如 ~/.claude/CLAUDE.md） */
  promptFile?: string | null;
  /** Skills 同步目录（相对 home，如 ~/.claude/skills） */
  skillsDir?: string | null;
  /** 入口类型：native | shell | ts */
  entryType?: string;
  /** TS 插件主脚本（相对插件目录） */
  main?: string | null;
}

/** 插件能力声明。 */
export interface PluginCapabilities {
  readLive?: boolean;
  apply?: boolean;
  remove?: boolean;
  import?: boolean;
  sessions?: boolean;
  mcp?: boolean;
}

/** 已安装插件：清单字段 + 安装来源信息。 */
export interface InstalledPlugin extends PluginManifest {
  /** 安装来源：builtin（内置）| local（本地目录安装） */
  source: string;
  installedAt: string;
  /** 可后端切换（apply 且非 TS 入口）：托盘/目录覆盖等场景的统一判定依据 */
  backendSwitchable: boolean;
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
  sourcePath?: string | null;
  repoOwner?: string | null;
  repoName?: string | null;
  repoBranch?: string | null;
  readmeUrl?: string | null;
  enabledPlugins: string[];
  installedAt: number;
  contentHash?: string | null;
  updatedAt: number;
}

/** 技能分发（同步）方式。 */
export type SyncMethod = "auto" | "symlink" | "copy";

/** 技能存储位置。 */
export type SkillStorageLocation = "cc_switch" | "unified";

/** 仓库配置。 */
export interface SkillRepo {
  owner: string;
  name: string;
  branch: string;
  enabled: boolean;
}

/** 从仓库中发现的、可安装的技能。 */
export interface DiscoverableSkill {
  /** 唯一标识：owner/name:directory */
  key: string;
  name: string;
  description: string;
  directory: string;
  readmeUrl?: string | null;
  repoOwner: string;
  repoName: string;
  repoBranch: string;
}

/** skills.sh 搜索结果。 */
export interface SkillsShSearchResult {
  skills: SkillsShDiscoverableSkill[];
  totalCount: number;
  query: string;
}

/** skills.sh 可安装技能。 */
export interface SkillsShDiscoverableSkill extends DiscoverableSkill {
  installs: number;
}

/** 技能更新检测结果。 */
export interface SkillUpdateInfo {
  id: string;
  name: string;
  currentHash?: string | null;
  remoteHash: string;
}

/** 技能备份条目。 */
export interface SkillBackupEntry {
  backupId: string;
  backupPath: string;
  createdAt: number;
  name: string;
  directory: string;
  description?: string | null;
}

/** 未管理的技能（在应用/SSOT 目录中发现但未入库）。 */
export interface UnmanagedSkill {
  directory: string;
  name: string;
  description?: string | null;
  foundIn: string[];
  path: string;
}

/** 导入已有技能时，前端显式提交的目录 + 启用插件选择。 */
export interface ImportSkillSelection {
  directory: string;
  plugins: string[];
}

/** 同步设置（同步方式 + 存储位置）。 */
export interface SyncSettings {
  syncMethod: SyncMethod;
  storageLocation: SkillStorageLocation;
}

/** 存储位置迁移结果。 */
export interface MigrationResult {
  migratedCount: number;
  skippedCount: number;
  errors: string[];
}

/** 已配置的工具配置目录覆盖。 */
export interface OverrideDir {
  pluginId: string;
  path: string;
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

/** 单条用量记录（plugin_sync_usage / TS 插件 syncUsage 返回值）。 */
export interface UsageRecord {
  sourceId: string;
  sessionId: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  cost: number;
  timestampMs: number;
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

/** 用量查看的时间范围预设。 */
export type UsageRangePreset = "today" | "1d" | "7d" | "14d" | "30d" | "custom";

/** 用量时间范围选择（preset 快捷项或自定义起止日期）。 */
export interface UsageRangeSelection {
  preset: UsageRangePreset;
  customStartDate?: number;
  customEndDate?: number;
}

/** 数据库备份记录。 */
export interface BackupRecord {
  id: string;
  name: string;
  filePath: string;
  sizeBytes: number;
  createdAt: number;
}

/** 应用行为设置（托盘/关闭行为/自启/静默启动）。 */
export interface AppBehavior {
  showInTray: boolean;
  minimizeToTrayOnClose: boolean;
  silentStartup: boolean;
  launchOnStartup: boolean;
}

/** 模型定价（PricingService 唯一成本计算来源）。 */
export interface ModelPricing {
  id: string;
  /** 模型匹配键：精确名或前缀。 */
  modelMatch: string;
  /** 供应商限定（NULL=通用默认；中转站差价场景）。 */
  providerScope: string | null;
  displayName: string;
  inputCostPerMillion: string;
  outputCostPerMillion: string;
  cacheReadCostPerMillion: string;
  cacheCreationCostPerMillion: string;
  /** 错峰折扣百分比（如 50 = 半价）；null = 无峰谷。 */
  offPeakDiscountPercent: number | null;
  /** UTC "HH:MM" 窗口（可跨午夜）。 */
  offPeakStart: string | null;
  offPeakEnd: string | null;
  /** user（手填，同步不覆盖）| models_dev（目录同步）。 */
  source: "user" | "models_dev";
  updatedAt: number;
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
