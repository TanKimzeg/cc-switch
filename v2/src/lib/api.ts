import { invoke } from "@tauri-apps/api/core";
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
  PromptRecord,
  Profile,
  RequestLogRow,
  DailyUsageRow,
  BackupRecord,
  ExportPayload,
} from "@/types";

export function getProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers");
}

export function getProvidersByPlugin(pluginId: string): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers", { pluginId });
}

export function getProvider(id: string): Promise<Provider | null> {
  return invoke<Provider | null>("get_provider", { id });
}

export function addProvider(
  input: ProviderInput,
  addToLive?: boolean,
): Promise<Provider> {
  return invoke<Provider>("add_provider", { input, addToLive });
}

export function updateProvider(
  id: string,
  input: ProviderInput,
): Promise<Provider> {
  return invoke<Provider>("update_provider", { id, input });
}

export function deleteProvider(id: string): Promise<void> {
  return invoke<void>("delete_provider", { id });
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

export function switchProvider(providerId: string): Promise<void> {
  return invoke<void>("switch_provider", { providerId });
}

export function removeProviderFromLiveConfig(
  providerId: string,
): Promise<void> {
  return invoke<void>("remove_provider_from_live_config", { providerId });
}

export function updateProvidersSortOrder(
  pluginId: string,
  ids: string[],
): Promise<void> {
  return invoke<void>("update_providers_sort_order", { pluginId, ids });
}

export function syncAllProvidersToLive(pluginId?: string): Promise<number> {
  return invoke<number>("sync_all_providers_to_live", { pluginId });
}

export function importProvidersFromLive(pluginId: string): Promise<number> {
  return invoke<number>("import_providers_from_live", { pluginId });
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

export function readLiveConfig(pluginId: string): Promise<LiveConfig> {
  return invoke<LiveConfig>("plugin_read_live", { id: pluginId });
}

export function importFromLive(pluginId: string): Promise<ImportCandidate[]> {
  return invoke<ImportCandidate[]>("plugin_import", { id: pluginId });
}

export function listSessions(pluginId: string): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>("plugin_sessions", { id: pluginId });
}

export function applyProvider(
  pluginId: string,
  providerId: string,
  current?: boolean,
): Promise<void> {
  return invoke<void>("plugin_apply", {
    id: pluginId,
    providerId,
    current,
  });
}

export function removeProviderFromLive(
  pluginId: string,
  providerId: string,
): Promise<void> {
  return invoke<void>("plugin_remove_provider", { id: pluginId, providerId });
}

export function loadSessionMessages(
  pluginId: string,
  source: string,
): Promise<SessionMessage[]> {
  return invoke<SessionMessage[]>("plugin_load_messages", {
    id: pluginId,
    source,
  });
}

export function deleteSession(
  pluginId: string,
  sessionId: string,
  source: string,
): Promise<boolean> {
  return invoke<boolean>("plugin_delete_session", {
    id: pluginId,
    sessionId,
    source,
  });
}

export function getMcpServers(pluginId: string): Promise<McpServerSpec[]> {
  return invoke<McpServerSpec[]>("plugin_mcp_get", { id: pluginId });
}

export function setMcpServer(
  pluginId: string,
  server: McpServerSpec,
): Promise<void> {
  return invoke<void>("plugin_mcp_set", { id: pluginId, server });
}

export function removeMcpServer(
  pluginId: string,
  serverId: string,
): Promise<void> {
  return invoke<void>("plugin_mcp_remove", { id: pluginId, serverId });
}

export function getPluginSubPlugins(pluginId: string): Promise<string[]> {
  return invoke<string[]>("plugin_get_plugins", { id: pluginId });
}

export function addPluginSubPlugin(
  pluginId: string,
  name: string,
): Promise<void> {
  return invoke<void>("plugin_add_plugin", { id: pluginId, name });
}

export function removePluginSubPlugin(
  pluginId: string,
  name: string,
): Promise<void> {
  return invoke<void>("plugin_remove_plugin", { id: pluginId, name });
}

export function readRawConfig(pluginId: string): Promise<string> {
  return invoke<string>("plugin_read_raw_config", { id: pluginId });
}

export function writeRawConfig(
  pluginId: string,
  content: string,
): Promise<void> {
  return invoke<void>("plugin_write_raw_config", { id: pluginId, content });
}

export function mcpList(): Promise<McpServer[]> {
  return invoke<McpServer[]>("mcp_list");
}

export function mcpUpsert(server: McpServer): Promise<void> {
  return invoke<void>("mcp_upsert", { server });
}

export function mcpDelete(id: string): Promise<void> {
  return invoke<void>("mcp_delete", { id });
}

export function mcpToggleApp(
  id: string,
  pluginId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("mcp_toggle_app", { id, pluginId, enabled });
}

export function importMcpFromPlugin(
  pluginId: string,
): Promise<McpServerSpec[]> {
  return invoke<McpServerSpec[]>("import_mcp_from_plugin", { id: pluginId });
}

export function importMcpServersFromPlugin(pluginId: string): Promise<number> {
  return invoke<number>("import_mcp_servers_from_plugin", { id: pluginId });
}

export function pluginSyncUsage(pluginId: string): Promise<number> {
  return invoke<number>("plugin_sync_usage", { pluginId });
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

export function skillsInstall(source: string): Promise<SkillRecord> {
  return invoke<SkillRecord>("skills_install", { source });
}

export function skillsUninstall(id: string): Promise<void> {
  return invoke<void>("skills_uninstall", { id });
}

export function skillsTogglePlugin(
  id: string,
  pluginId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("skills_toggle_plugin", { id, pluginId, enabled });
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
