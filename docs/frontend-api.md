# 前端接口（Frontend API）

本文档说明前端如何调用后端，以及 TS 插件如何在 `api.ts` 层路由。代码位于 `v2/src/lib/api.ts`、`v2/src/lib/plugin-loader.ts`、`v2/src/components/`。

## 1. 总体分工

- **Panel 组件**（`GlobalPanels.tsx` / `ProviderForm.tsx` / `McpPanel.tsx` / `UsagePanel.tsx` / `SessionList.tsx`…）负责 UI，只调 `api.ts` 的函数。
- **`api.ts`** 是前端 → 后端 IPC 的唯一封装（`invoke`），同时负责 **TS 插件路由**。
- **`plugin-loader.ts`** 负责加载/缓存 TS 插件脚本。

## 2. TS 插件路由机制

核心函数 `loadTsPluginIfTs(pluginId)`（`plugin-loader.ts`）：

```ts
// 若插件 entryType === "ts" 且声明了 main，则加载并缓存脚本导出对象；否则返回 null
export async function loadTsPluginIfTs(pluginId): Promise<TsPluginExports | null>
```

`api.ts` 中每个「live 类」函数都是这样写的：

```ts
export async function readLiveConfig(pluginId) {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.readLive) return ts.readLive();        // TS 插件：直接调脚本
  return invoke("plugin_read_live", { id: pluginId }); // native/shell：走后端命令
}
```

受影响的操作（TS 插件时绕过后端，改调脚本方法）：

| api.ts 函数 | TS 插件调用 | native/shell 后端命令 |
|-------------|------------|----------------------|
| `readLiveConfig` | `ts.readLive()` | `plugin_read_live` |
| `importFromLive` | `ts.import()` | `plugin_import` |
| `applyProvider` | `ts.apply(provider, current)` | `plugin_apply` |
| `removeProviderFromLive` | `ts.removeProvider(id)` | `plugin_remove_provider` |
| `listSessions` | `ts.sessions()` | `plugin_sessions` |
| `loadSessionMessages` | `ts.loadMessages(source)` | `plugin_load_messages` |
| `deleteSession` | `ts.deleteSession(id, source)` | `plugin_delete_session` |
| `getMcpServers` | `ts.getMcpServers()` | `plugin_mcp_get` |
| `setMcpServer` | `ts.setMcpServer(server)` | `plugin_mcp_set` |
| `removeMcpServer` | `ts.removeMcpServer(id)` | `plugin_mcp_remove` |
| `readRawConfig` | `ts.readRawConfig()` | `plugin_read_raw_config` |
| `writeRawConfig` | `ts.writeRawConfig(content)` | `plugin_write_raw_config` |
| `pluginSyncUsage` | `ts.syncUsage()` + `usage_insert_records` | `plugin_sync_usage` |

**DB 写操作特殊处理**（`addProvider` / `updateProvider`）：TS 插件时，后端 DB 写入与 live 投影分离——先以 `addToLive=false` / `applyLive=false` 落库，再在前端调 `ts.apply(provider, false)` 投影 live。

> TS 插件脚本内部如需访问 DB（暂存 provider、写用量），用 `TsHost` 的类型化 DB 方法（`host.providers()` / `host.upsertProvider()` / `host.saveUsageRecords()` 等，自动绑定当前插件 id），无需手写 `invoke`（见 [ts-plugin.md](ts-plugin.md)）。

## 3. `api.ts` 函数分组

### Provider
`getProviders(pluginId?)` / `getProvider(id)` / `addProvider(input, addToLive?)` / `updateProvider(id, input)` / `deleteProvider(id)` / `getCurrentProvider(pluginId)` / `setCurrentProvider(pluginId, providerId?)` / `switchProvider(providerId)` / `removeProviderFromLiveConfig(providerId)` / `updateProvidersSortOrder(pluginId, ids)` / `syncAllProvidersToLive(pluginId?)` / `importProvidersFromLive(pluginId)`

### 插件
`getPlugins()` / `installPlugin(source)` / `uninstallPlugin(id)`

### 插件能力（见上文路由表）
`readLiveConfig` / `importFromLive` / `applyProvider` / `removeProviderFromLive` / `listSessions` / `loadSessionMessages` / `deleteSession` / `getMcpServers` / `setMcpServer` / `removeMcpServer` / `readRawConfig` / `writeRawConfig`

### MCP（统一管理）
`mcpList()` / `mcpUpsert(server)` / `mcpDelete(id)` / `mcpToggleApp(id, pluginId, enabled)` / `importMcpFromPlugin(pluginId)` / `importMcpServersFromPlugin(pluginId)`

### 用量
`pluginSyncUsage(pluginId)` / `usageInsertRecords(pluginId, records)` / `usageListRequestLogs(pluginId?, limit?)` / `usageDailySummary(pluginId?)`

### Skills / Prompts
`skillsList()` / `skillsInstallLocalDir(source)` / `skillsInstallFromRepo(skill, currentPlugin)` / `skillsInstallFromZip(filePath, currentPlugin)` / `skillsUninstall(id)` / `skillsTogglePlugin(id, pluginId, enabled)` / `skillsDiscover()` / `skillsListRepos/addRepo/removeRepo` / `skillsSearchSkillsh(query, limit, offset)` / `skillsCheckUpdates()` / `skillsUpdateSkill(id)` / `skillsListBackups/deleteBackup/restoreBackup` / `skillsScanUnmanaged()` / `skillsImport(imports)` / `skillsGetSyncSettings()/setSyncMethod()/migrateStorage(target)`；`promptsList(pluginId?)` / `promptsUpsert(...)` / `promptsDelete(id)` / `promptsToggle(id, enabled)`

错误文案：后端结构化 JSON 错误经 `skillErrorText(t, err)`（`src/lib/skillsUtils.ts`）映射到 `skillsError.*` 文案。

### Profiles / Backup / 设置
`profilesList/current/upsert/delete/apply/clearCurrent`；`backupCreate/List/Delete`、`exportConfigJson/exportConfigToFile/parseExportJson/importConfig/importConfigFromFile`；`getSetting/setSetting`；`settingsGetOverrides/settingsSetOverride`、`getAppDataDirOverride/setAppDataDirOverride`；`updateTrayMenu()`（provider 变更后 `api.ts` 自动刷新托盘菜单）

## 4. Panel 组件职责

| 组件 | 对应能力 | 说明 |
|------|----------|------|
| `ProvidersPanel` + `ProviderForm` | Provider 配置 | provider 列表、增删改查、切换、回填（从 live 导入）、编辑 live 原始配置、同步全部到 live |
| `McpPanel` | MCP | 某插件的 MCP 服务器增删改查（走 `getMcpServers` 等） |
| `McpGlobalPanel` | MCP（全局） | 全部服务器的统一管理 + 各插件启用开关（走 `mcp_list/upsert/toggle`） |
| `SessionsPanel` + `SessionList` | 会话 | 会话列表、加载消息、删除（走 `listSessions/loadSessionMessages/deleteSession`） |
| `UsagePanel` | 用量查询 | 同步用量 + 每日汇总表格（走 `pluginSyncUsage/usageDailySummary`） |
| `SkillsPanel`（`components/skills/`） | Skill | 管理视图（已安装/更新/卸载/导入/备份/恢复/ZIP/发现入口）+ `SkillsDiscovery`（仓库/skills.sh）+ `RepoManagerPanel` + 恢复/导入对话框 |
| `PromptPanel`（`components/prompts/`） | Prompt 管理 | 列表（switch 开关/搜索/计数-已启用 header）+ 全屏编辑对话框（名称/描述/内容 Markdown）；启用走互斥+回填 |
| `ProfilesPanel` / `BackupPanel` | 配置方案 / 备份 | 方案 CRUD + 应用；备份与导入导出 |
| `SettingsPanel` | 设置 | Skills 存储位置（迁移）与同步方式 |

## 5. 应用入口（App.tsx）

- 左侧导航：providers / sessions / mcp / skills / usage / prompts / profiles / backup / **settings**。
- 选中插件后进入 `plugin-detail` 视图，**统一渲染 `PluginDetail`**（含 provider 管理、live 配置视图）。TS 插件的操作经 `api.ts` 的 TS 路由自动调用加载的脚本（见上文第 2 节），因此 **native / shell / ts 三形态共用同一套功能面板**。
- 未进入详情时，`GlobalPanels` 按当前 `pluginId` 渲染各 Panel；`settings` 视图由 App.tsx 直接渲染 `SettingsPanel`（与插件无关）。
