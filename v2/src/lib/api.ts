import { invoke } from "@tauri-apps/api/core";
import type {
  Provider,
  ProviderInput,
  InstalledPlugin,
  LiveConfig,
  ImportCandidate,
  SessionMeta,
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

export function addProvider(input: ProviderInput): Promise<Provider> {
  return invoke<Provider>("add_provider", { input });
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

export function getCurrentProvider(pluginId: string): Promise<string | null> {
  return invoke<string | null>("get_current_provider", { pluginId });
}

export function setCurrentProvider(
  pluginId: string,
  providerId: string,
): Promise<void> {
  return invoke<void>("set_current_provider", { pluginId, providerId });
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
