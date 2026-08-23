import { invoke } from "@tauri-apps/api/core";
import { loadTsPluginIfTs } from "@/lib/plugin-loader";
import type {
  Provider,
  ProviderInput,
  InstalledPlugin,
  LiveConfig,
  ImportCandidate,
  SessionMeta,
  SessionMessage,
  McpServerSpec,
  McpServer,
  SkillRecord,
  DiscoverableSkill,
  SkillRepo,
  SkillsShSearchResult,
  SkillUpdateInfo,
  SkillBackupEntry,
  UnmanagedSkill,
  ImportSkillSelection,
  SyncSettings,
  MigrationResult,
  OverrideDir,
  PromptRecord,
  Profile,
  RequestLogRow,
  DailyUsageRow,
  UsageRecord,
  BackupRecord,
  ExportPayload,
  AppBehavior,
} from "@/types";

export function getProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers");
}

export function getProvidersByPlugin(pluginId: string): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers", { pluginId });
}

/** 重建托盘菜单（provider 变更后调用，失败静默）。 */
export function updateTrayMenu(): Promise<void> {
  return invoke<void>("update_tray_menu");
}

function refreshTray() {
  void updateTrayMenu().catch(() => {});
}

export function getProvider(id: string): Promise<Provider | null> {
  return invoke<Provider | null>("get_provider", { id });
}

export async function addProvider(
  input: ProviderInput,
  addToLive?: boolean,
): Promise<Provider> {
  const ts = await loadTsPluginIfTs(input.pluginId);
  if (ts?.apply) {
    // TS 插件：DB 写入与 live 投影分离（live 由前端脚本执行）。
    const provider = await invoke<Provider>("add_provider", {
      input,
      addToLive: false,
    });
    if (addToLive !== false) {
      await ts.apply(
        {
          id: provider.id,
          name: provider.name,
          settingsConfig: provider.settingsConfig,
        },
        false,
      );
    }
    refreshTray();
    return provider;
  }
  const provider = await invoke<Provider>("add_provider", { input, addToLive });
  refreshTray();
  return provider;
}

export async function updateProvider(
  id: string,
  input: ProviderInput,
): Promise<Provider> {
  const ts = await loadTsPluginIfTs(input.pluginId);
  if (ts?.apply) {
    const provider = await invoke<Provider>("update_provider", {
      id,
      input,
      applyLive: false,
    });
    await ts.apply(
      {
        id: provider.id,
        name: provider.name,
        settingsConfig: provider.settingsConfig,
      },
      false,
    );
    refreshTray();
    return provider;
  }
  const provider = await invoke<Provider>("update_provider", { id, input });
  refreshTray();
  return provider;
}

export async function deleteProvider(id: string): Promise<void> {
  await invoke<void>("delete_provider", { id });
  refreshTray();
}

export function getCurrentProvider(pluginId: string): Promise<Provider | null> {
  return invoke<Provider | null>("get_current_provider", { pluginId });
}

export function setCurrentProvider(
  pluginId: string,
  providerId: string | null,
): Promise<void> {
  return invoke<void>("set_current_provider", { pluginId, providerId });
}

export async function switchProvider(providerId: string): Promise<void> {
  await invoke<void>("switch_provider", { providerId });
  refreshTray();
}

export async function removeProviderFromLiveConfig(
  providerId: string,
): Promise<void> {
  await invoke<void>("remove_provider_from_live_config", { providerId });
  refreshTray();
}

export async function updateProvidersSortOrder(
  pluginId: string,
  ids: string[],
): Promise<void> {
  await invoke<void>("update_providers_sort_order", { pluginId, ids });
  refreshTray();
}

export async function syncAllProvidersToLive(
  pluginId?: string,
): Promise<number> {
  const n = await invoke<number>("sync_all_providers_to_live", { pluginId });
  refreshTray();
  return n;
}

export async function importProvidersFromLive(
  pluginId: string,
): Promise<number> {
  const n = await invoke<number>("import_providers_from_live", { pluginId });
  refreshTray();
  return n;
}

export function getPlugins(): Promise<InstalledPlugin[]> {
  return invoke<InstalledPlugin[]>("get_plugins");
}

export function installPlugin(source: string): Promise<InstalledPlugin> {
  return invoke<InstalledPlugin>("install_plugin", { source });
}

export function uninstallPlugin(id: string): Promise<void> {
  return invoke<void>("uninstall_plugin", { id });
}

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

export function settingsGetOverrides(): Promise<OverrideDir[]> {
  return invoke<OverrideDir[]>("settings_get_overrides");
}

export function settingsSetOverride(
  pluginId: string,
  path: string | null,
): Promise<void> {
  return invoke<void>("settings_set_override", { pluginId, path });
}

export function getAppDataDirOverride(): Promise<string | null> {
  return invoke<string | null>("get_app_data_dir_override");
}

export function setAppDataDirOverride(path: string | null): Promise<boolean> {
  return invoke<boolean>("set_app_data_dir_override", { path });
}

export function settingsGetAppBehavior(): Promise<AppBehavior> {
  return invoke<AppBehavior>("settings_get_app_behavior");
}

export function settingsSetMinimizeToTrayOnClose(
  enabled: boolean,
): Promise<void> {
  return invoke<void>("settings_set_minimize_to_tray_on_close", { enabled });
}

export function settingsSetSilentStartup(enabled: boolean): Promise<void> {
  return invoke<void>("settings_set_silent_startup", { enabled });
}

export function settingsSetLaunchOnStartup(enabled: boolean): Promise<void> {
  return invoke<void>("settings_set_launch_on_startup", { enabled });
}

export function settingsSetShowInTray(enabled: boolean): Promise<void> {
  return invoke<void>("settings_set_show_in_tray", { enabled });
}

export async function readLiveConfig(pluginId: string): Promise<LiveConfig> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.readLive) return ts.readLive();
  return invoke<LiveConfig>("plugin_read_live", { id: pluginId });
}

export async function importFromLive(
  pluginId: string,
): Promise<ImportCandidate[]> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.import) return ts.import();
  return invoke<ImportCandidate[]>("plugin_import", { id: pluginId });
}

export async function listSessions(pluginId: string): Promise<SessionMeta[]> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.sessions) return ts.sessions();
  return invoke<SessionMeta[]>("plugin_sessions", { id: pluginId });
}

export async function applyProvider(
  pluginId: string,
  providerId: string,
  current?: boolean,
): Promise<void> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.apply) {
    const provider = await getProvider(providerId);
    if (!provider) throw new Error(`provider not found: ${providerId}`);
    await ts.apply(
      {
        id: provider.id,
        name: provider.name,
        settingsConfig: provider.settingsConfig,
      },
      current ?? true,
    );
    return;
  }
  return invoke<void>("plugin_apply", {
    id: pluginId,
    providerId,
    current,
  });
}

export async function removeProviderFromLive(
  pluginId: string,
  providerId: string,
): Promise<void> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.removeProvider) {
    await ts.removeProvider(providerId);
    return;
  }
  return invoke<void>("plugin_remove_provider", { id: pluginId, providerId });
}

export async function loadSessionMessages(
  pluginId: string,
  source: string,
): Promise<SessionMessage[]> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.loadMessages) return ts.loadMessages(source);
  return invoke<SessionMessage[]>("plugin_load_messages", {
    id: pluginId,
    source,
  });
}

export async function deleteSession(
  pluginId: string,
  sessionId: string,
  source: string,
): Promise<boolean> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.deleteSession) return ts.deleteSession(sessionId, source);
  return invoke<boolean>("plugin_delete_session", {
    id: pluginId,
    sessionId,
    source,
  });
}

export async function getMcpServers(
  pluginId: string,
): Promise<McpServerSpec[]> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.getMcpServers) return ts.getMcpServers();
  return invoke<McpServerSpec[]>("plugin_mcp_get", { id: pluginId });
}

export async function setMcpServer(
  pluginId: string,
  server: McpServerSpec,
): Promise<void> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.setMcpServer) {
    await ts.setMcpServer(server);
    return;
  }
  return invoke<void>("plugin_mcp_set", { id: pluginId, server });
}

export async function removeMcpServer(
  pluginId: string,
  serverId: string,
): Promise<void> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.removeMcpServer) {
    await ts.removeMcpServer(serverId);
    return;
  }
  return invoke<void>("plugin_mcp_remove", { id: pluginId, serverId });
}

export async function readRawConfig(pluginId: string): Promise<string> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.readRawConfig) return ts.readRawConfig();
  return invoke<string>("plugin_read_raw_config", { id: pluginId });
}

export async function writeRawConfig(
  pluginId: string,
  content: string,
): Promise<void> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.writeRawConfig) {
    await ts.writeRawConfig(content);
    return;
  }
  return invoke<void>("plugin_write_raw_config", { id: pluginId, content });
}

export function mcpList(): Promise<McpServer[]> {
  return invoke<McpServer[]>("mcp_list");
}

/** 新增/更新 MCP 服务器并同步到启用的插件；TS 插件的 live 同步由前端脚本执行。 */
export async function mcpUpsert(server: McpServer): Promise<void> {
  // 编辑前快照：用于对比本次被取消勾选的插件。
  let prevApps: [string, boolean][] = [];
  try {
    const all = await mcpList();
    const prev = all.find((s) => s.id === server.id);
    if (prev) prevApps = prev.apps;
  } catch {
    // 快照失败不阻塞保存
  }

  await invoke<void>("mcp_upsert", { server });
  for (const [pluginId, enabled] of server.apps) {
    if (!enabled) continue;
    const ts = await loadTsPluginIfTs(pluginId);
    if (ts?.setMcpServer) {
      await ts.setMcpServer({
        id: server.id,
        name: server.name,
        spec: server.spec,
      });
    }
  }
  // 对齐后端语义：编辑时被取消勾选的 TS 插件，从其 live 配置移除该服务器。
  const disabled = prevApps
    .filter(([, enabled]) => enabled)
    .map(([pluginId]) => pluginId)
    .filter((pluginId) => !server.apps.some(([p, en]) => p === pluginId && en));
  for (const pluginId of disabled) {
    const ts = await loadTsPluginIfTs(pluginId);
    if (ts?.removeMcpServer) {
      await ts.removeMcpServer(server.id);
    }
  }
}

export function mcpDelete(id: string): Promise<void> {
  return invoke<void>("mcp_delete", { id });
}

/** 切换某 MCP 服务器在指定插件的启用状态；TS 插件同步由前端脚本执行。 */
export async function mcpToggleApp(
  id: string,
  pluginId: string,
  enabled: boolean,
): Promise<void> {
  await invoke<void>("mcp_toggle_app", { id, pluginId, enabled });
  const ts = await loadTsPluginIfTs(pluginId);
  if (!ts) return;
  if (enabled) {
    if (ts.setMcpServer) {
      const all = await mcpList();
      const server = all.find((s) => s.id === id);
      if (server) {
        await ts.setMcpServer({
          id: server.id,
          name: server.name,
          spec: server.spec,
        });
      }
    }
  } else if (ts.removeMcpServer) {
    await ts.removeMcpServer(id);
  }
}

export function importMcpFromPlugin(
  pluginId: string,
): Promise<McpServerSpec[]> {
  return invoke<McpServerSpec[]>("import_mcp_from_plugin", { id: pluginId });
}

/** 从插件 live 导入 MCP 服务器到统一表；TS 插件经前端脚本读取。返回导入数。 */
export async function importMcpServersFromPlugin(
  pluginId: string,
): Promise<number> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.getMcpServers) {
    const servers = await ts.getMcpServers();
    let imported = 0;
    const all = (await mcpList().catch(() => [] as McpServer[])) ?? [];
    for (const spec of servers) {
      try {
        const existing = all.find((s) => s.id === spec.id);
        if (existing) {
          // 合并语义（对齐 v1 与后端）：仅置位当前插件标志，不覆盖其它插件状态与已有配置。
          const apps: [string, boolean][] = existing.apps.some(
            ([p]) => p === pluginId,
          )
            ? existing.apps.map(([p, en]) =>
                p === pluginId ? [p, true] : [p, en],
              )
            : [...existing.apps, [pluginId, true] as [string, boolean]];
          // 直接落库（不经 mcpUpsert 的 live 同步：数据本就来自该插件的 live）。
          await invoke<void>("mcp_upsert", {
            server: { ...existing, apps },
          });
        } else {
          await invoke<void>("mcp_upsert", {
            server: {
              id: spec.id,
              name: spec.name,
              spec: spec.spec,
              description: null,
              homepage: null,
              docs: null,
              tags: [],
              apps: [[pluginId, true]],
            },
          });
        }
        imported += 1;
      } catch {
        // 跳过冲突条目
      }
    }
    return imported;
  }
  return invoke<number>("import_mcp_servers_from_plugin", { id: pluginId });
}

/** 从全部已安装插件导入 MCP 服务器，返回总导入数（best-effort）。 */
export async function importMcpServersFromAllPlugins(): Promise<number> {
  const plugins = await getPlugins();
  let total = 0;
  for (const p of plugins) {
    try {
      total += await importMcpServersFromPlugin(p.id);
    } catch {
      // best-effort：单个插件失败不阻断其余
    }
  }
  return total;
}

export async function pluginSyncUsage(pluginId: string): Promise<number> {
  const ts = await loadTsPluginIfTs(pluginId);
  if (ts?.syncUsage) {
    const records = (await ts.syncUsage()) ?? [];
    return usageInsertRecords(pluginId, records);
  }
  return invoke<number>("plugin_sync_usage", { pluginId });
}

/** 持久化 TS 插件在前端解析出的用量记录（INSERT OR IGNORE 去重）。 */
export function usageInsertRecords(
  pluginId: string,
  records: UsageRecord[],
): Promise<number> {
  return invoke<number>("usage_insert_records", { pluginId, records });
}

export function usageListRequestLogs(
  pluginId?: string,
  limit?: number,
): Promise<RequestLogRow[]> {
  return invoke<RequestLogRow[]>("usage_list_request_logs", {
    pluginId,
    limit,
  });
}

export function usageDailySummary(pluginId?: string): Promise<DailyUsageRow[]> {
  return invoke<DailyUsageRow[]>("usage_daily_summary", { pluginId });
}

export function skillsList(): Promise<SkillRecord[]> {
  return invoke<SkillRecord[]>("skills_list");
}

export function skillsInstallLocalDir(source: string): Promise<SkillRecord> {
  return invoke<SkillRecord>("skills_install_local_dir", { source });
}

export function skillsInstallFromRepo(
  skill: DiscoverableSkill,
  currentPlugin: string,
): Promise<SkillRecord> {
  return invoke<SkillRecord>("skills_install_skill", { skill, currentPlugin });
}

export function skillsInstallFromZip(
  filePath: string,
  currentPlugin: string,
): Promise<SkillRecord[]> {
  return invoke<SkillRecord[]>("skills_install_from_zip", {
    filePath,
    currentPlugin,
  });
}

export function skillsUninstall(id: string): Promise<string | null> {
  return invoke<string | null>("skills_uninstall", { id });
}

export function skillsTogglePlugin(
  id: string,
  pluginId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("skills_toggle_plugin", { id, pluginId, enabled });
}

export function skillsDiscover(): Promise<DiscoverableSkill[]> {
  return invoke<DiscoverableSkill[]>("skills_discover");
}

export function skillsListRepos(): Promise<SkillRepo[]> {
  return invoke<SkillRepo[]>("skills_list_repos");
}

export function skillsAddRepo(
  owner: string,
  name: string,
  branch: string,
): Promise<SkillRepo> {
  return invoke<SkillRepo>("skills_add_repo", { owner, name, branch });
}

export function skillsRemoveRepo(owner: string, name: string): Promise<void> {
  return invoke<void>("skills_remove_repo", { owner, name });
}

export function skillsSearchSkillsh(
  query: string,
  limit: number,
  offset: number,
): Promise<SkillsShSearchResult> {
  return invoke<SkillsShSearchResult>("skills_search_skillsh", {
    query,
    limit,
    offset,
  });
}

export function skillsCheckUpdates(): Promise<SkillUpdateInfo[]> {
  return invoke<SkillUpdateInfo[]>("skills_check_updates");
}

export function skillsUpdateSkill(id: string): Promise<SkillRecord> {
  return invoke<SkillRecord>("skills_update_skill", { id });
}

export function skillsListBackups(): Promise<SkillBackupEntry[]> {
  return invoke<SkillBackupEntry[]>("skills_list_backups");
}

export function skillsDeleteBackup(backupId: string): Promise<void> {
  return invoke<void>("skills_delete_backup", { backupId });
}

export function skillsRestoreBackup(
  backupId: string,
  currentPlugin: string,
): Promise<SkillRecord> {
  return invoke<SkillRecord>("skills_restore_backup", {
    backupId,
    currentPlugin,
  });
}

export function skillsScanUnmanaged(): Promise<UnmanagedSkill[]> {
  return invoke<UnmanagedSkill[]>("skills_scan_unmanaged");
}

export function skillsImport(
  imports: ImportSkillSelection[],
): Promise<SkillRecord[]> {
  return invoke<SkillRecord[]>("skills_import", { imports });
}

export function skillsGetSyncSettings(): Promise<SyncSettings> {
  return invoke<SyncSettings>("skills_get_sync_settings");
}

export function skillsSetSyncMethod(method: string): Promise<void> {
  return invoke<void>("skills_set_sync_method", { method });
}

export function skillsMigrateStorage(target: string): Promise<MigrationResult> {
  return invoke<MigrationResult>("skills_migrate_storage", { target });
}

export function promptsList(pluginId?: string): Promise<PromptRecord[]> {
  return invoke<PromptRecord[]>("prompts_list", { pluginId });
}

export function promptsUpsert(
  id: string,
  pluginId: string,
  name: string,
  content: string,
  description?: string,
): Promise<void> {
  return invoke<void>("prompts_upsert", {
    id,
    pluginId,
    name,
    content,
    description,
  });
}

export function promptsDelete(id: string): Promise<void> {
  return invoke<void>("prompts_delete", { id });
}

export function promptsToggle(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("prompts_toggle", { id, enabled });
}

export function profilesList(): Promise<Profile[]> {
  return invoke<Profile[]>("profiles_list");
}

export function profilesCurrent(): Promise<string | null> {
  return invoke<string | null>("profiles_current");
}

export function profilesUpsert(profile: Profile): Promise<void> {
  return invoke<void>("profiles_upsert", { profile });
}

export function profilesDelete(id: string): Promise<void> {
  return invoke<void>("profiles_delete", { id });
}

export function profilesApply(id: string): Promise<void> {
  return invoke<void>("profiles_apply", { id });
}

export function profilesClearCurrent(): Promise<void> {
  return invoke<void>("profiles_clear_current");
}

export function backupCreate(): Promise<BackupRecord> {
  return invoke<BackupRecord>("backup_create");
}

export function backupList(): Promise<BackupRecord[]> {
  return invoke<BackupRecord[]>("backup_list");
}

export function backupDelete(id: string): Promise<void> {
  return invoke<void>("backup_delete", { id });
}

export function backupRename(id: string, name: string): Promise<void> {
  return invoke<void>("backup_rename", { id, name });
}

export function backupRestore(id: string): Promise<string> {
  return invoke<string>("backup_restore", { id });
}

export function exportConfigJson(): Promise<ExportPayload> {
  return invoke<ExportPayload>("export_config_json");
}

export function exportConfigToFile(path: string): Promise<void> {
  return invoke<void>("export_config_to_file", { path });
}

export function importConfig(payload: ExportPayload): Promise<number> {
  return invoke<number>("import_config", { payload });
}

export function importConfigFromFile(path: string): Promise<number> {
  return invoke<number>("import_config_from_file", { path });
}
