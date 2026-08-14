import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  Blocks,
  Download,
  History,
  Layers,
  Pencil,
  Puzzle,
  Plus,
  RefreshCw,
  ScrollText,
  Send,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  addProvider,
  applyProvider,
  deleteProvider,
  getPlugins,
  getProviders,
  importFromLive,
  installPlugin,
  readLiveConfig,
  removeProviderFromLive,
  uninstallPlugin,
} from "@/lib/api";
import type { InstalledPlugin, Provider } from "@/types";
import GlobalPanels from "@/components/GlobalPanels";
import ProviderForm from "@/components/ProviderForm";

type View =
  | "providers"
  | "sessions"
  | "mcp"
  | "skills"
  | "usage"
  | "prompts"
  | "profiles"
  | "backup"
  | "plugin-detail";

const NAV_ITEMS: { id: View; label: string; icon: typeof Blocks }[] = [
  { id: "providers", label: "nav.providers", icon: Blocks },
  { id: "sessions", label: "nav.sessions", icon: History },
  { id: "mcp", label: "nav.mcp", icon: Puzzle },
  { id: "skills", label: "nav.skills", icon: Sparkles },
  { id: "usage", label: "nav.usage", icon: RefreshCw },
  { id: "prompts", label: "nav.prompts", icon: ScrollText },
  { id: "profiles", label: "nav.profiles", icon: Layers },
  { id: "backup", label: "nav.backup", icon: Archive },
];

export default function App() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [view, setView] = useState<View>("providers");
  const [selectedPluginId, setSelectedPluginId] = useState<string | null>(null);

  const pluginsQuery = useQuery({
    queryKey: ["plugins"],
    queryFn: getPlugins,
  });
  const providersQuery = useQuery({
    queryKey: ["providers"],
    queryFn: () => getProviders(),
  });

  const plugins = pluginsQuery.data ?? [];
  const selectedPlugin = plugins.find((p) => p.id === selectedPluginId) ?? null;
  const providers = selectedPluginId
    ? (providersQuery.data?.filter((p) => p.pluginId === selectedPluginId) ??
      [])
    : [];

  const handleAddPlugin = async () => {
    const dir = await open({
      directory: true,
      multiple: false,
      title: t("shell.pickPluginDir"),
    });
    if (typeof dir !== "string") return;
    try {
      await installPlugin(dir);
      await queryClient.invalidateQueries({ queryKey: ["plugins"] });
      toast.success(t("shell.pluginInstalled"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleUninstall = async (plugin: InstalledPlugin) => {
    try {
      await uninstallPlugin(plugin.id);
      if (selectedPluginId === plugin.id) setSelectedPluginId(null);
      await queryClient.invalidateQueries({ queryKey: ["plugins"] });
      toast.success(t("shell.pluginUninstalled"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const renderContent = () => {
    if (view === "plugin-detail" && selectedPlugin) {
      return (
        <PluginDetail
          plugin={selectedPlugin}
          providers={providers}
          onProvidersChanged={() =>
            queryClient.invalidateQueries({ queryKey: ["providers"] })
          }
        />
      );
    }
    const activePluginId = selectedPluginId ?? plugins[0]?.id ?? "opencode";
    return <GlobalPanels view={view} pluginId={activePluginId} />;
  };

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
        <Puzzle className="h-5 w-5 text-blue-500" />
        <span className="font-semibold">{t("app.name")}</span>
        <span className="text-xs text-muted-foreground">
          {t("app.tagline")}
        </span>
        <nav className="ml-4 flex items-center gap-1">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active =
              view === item.id ||
              (item.id === "providers" && view === "plugin-detail");
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => {
                  if (item.id === "providers") {
                    setView(selectedPlugin ? "plugin-detail" : "providers");
                  } else {
                    setView(item.id);
                  }
                }}
                className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm transition-colors ${
                  active
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <span className="hidden lg:inline">{t(item.label)}</span>
              </button>
            );
          })}
        </nav>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-48 shrink-0 border-r border-border p-3">
          <div className="mb-2 flex items-center justify-between px-2">
            <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t("shell.installed")}
            </span>
            <button
              type="button"
              onClick={handleAddPlugin}
              title={t("shell.addPlugin")}
              className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            >
              <Plus className="h-4 w-4" />
            </button>
          </div>
          <nav className="space-y-1">
            {plugins.map((plugin) => (
              <div key={plugin.id} className="group flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => {
                    setSelectedPluginId(plugin.id);
                    setView("plugin-detail");
                  }}
                  className={`flex flex-1 items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors ${
                    view === "plugin-detail" && selectedPluginId === plugin.id
                      ? "bg-primary/10 text-primary"
                      : "text-foreground hover:bg-muted"
                  }`}
                >
                  <Blocks className="h-4 w-4 shrink-0" />
                  <span className="truncate">{plugin.name}</span>
                </button>
                {plugin.source !== "builtin" && (
                  <button
                    type="button"
                    onClick={() => handleUninstall(plugin)}
                    title={t("shell.uninstall")}
                    className="rounded p-1 text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
            ))}
            {plugins.length === 0 && (
              <p className="px-2 py-1 text-xs text-muted-foreground">
                {t("shell.noPluginsTitle")}
              </p>
            )}
          </nav>
        </aside>

        <main className="min-w-0 flex-1 overflow-auto p-6">
          {renderContent()}
        </main>
      </div>
    </div>
  );
}

function CapabilityBadge({ label, on }: { label: string; on: boolean }) {
  const { t } = useTranslation();
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs ${
        on
          ? "border-primary/30 bg-primary/10 text-primary"
          : "border-border text-muted-foreground"
      }`}
    >
      {label}: {on ? "✓" : t("shell.capabilityOff")}
    </span>
  );
}

function PluginDetail({
  plugin,
  providers,
  onProvidersChanged,
}: {
  plugin: InstalledPlugin;
  providers: Provider[];
  onProvidersChanged: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [formOpen, setFormOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);
  const caps = plugin.capabilities ?? {};
  const canRead = caps.readLive ?? false;
  const canApply = caps.apply ?? false;
  const canImport = caps.import ?? false;
  const canSessions = caps.sessions ?? false;

  const liveQuery = useQuery({
    queryKey: ["live", plugin.id],
    queryFn: () => readLiveConfig(plugin.id),
    enabled: canRead,
  });

  const hasAnyCapability = canRead || canApply || canImport || canSessions;

  const handleApply = async (providerId: string) => {
    try {
      await applyProvider(plugin.id, providerId, true);
      toast.success(t("shell.applied"));
      if (canRead) {
        await queryClient.invalidateQueries({ queryKey: ["live", plugin.id] });
      }
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDeleteProvider = async (provider: Provider) => {
    try {
      await deleteProvider(provider.id);
      if (plugin.capabilities?.remove) {
        await removeProviderFromLive(plugin.id, provider.id);
      }
      await queryClient.invalidateQueries({ queryKey: ["providers"] });
      if (canRead) {
        await queryClient.invalidateQueries({ queryKey: ["live", plugin.id] });
      }
      toast.success(t("shell.providerDeleted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImport = async () => {
    try {
      const candidates = await importFromLive(plugin.id);
      if (candidates.length === 0) {
        toast.info(t("shell.noLiveProviders"));
        return;
      }
      for (const c of candidates) {
        try {
          await addProvider({
            // 保留 live 配置中的 provider id（additive 键一致）。
            id: c.id,
            pluginId: plugin.id,
            name: c.name,
            category: "imported",
            settingsConfig: JSON.stringify(c.settingsConfig),
            meta: { liveId: c.id },
          });
        } catch {
          // 跳过已存在或失败的条目
        }
      }
      await queryClient.invalidateQueries({ queryKey: ["providers"] });
      await onProvidersChanged();
      toast.success(t("shell.importedCount", { count: candidates.length }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-3">
          <Blocks className="h-6 w-6 text-blue-500" />
          <div>
            <h1 className="text-lg font-semibold">{plugin.name}</h1>
            <p className="text-xs text-muted-foreground">
              v{plugin.version} · {plugin.id} · {plugin.source}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-muted-foreground">
            {t("shell.capabilities")}:
          </span>
          <CapabilityBadge label={t("shell.readLive")} on={canRead} />
          <CapabilityBadge label={t("shell.applyCap")} on={canApply} />
          <CapabilityBadge label={t("shell.importCap")} on={canImport} />
          <CapabilityBadge label={t("shell.sessionsCap")} on={canSessions} />
        </div>
      </div>

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium">{plugin.name}</h3>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => {
                setEditingProvider(null);
                setFormOpen(true);
              }}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("shell.addProvider")}
            </button>
            {canImport && (
              <button
                type="button"
                onClick={handleImport}
                className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
                title={t("shell.importLiveHint")}
              >
                <Download className="h-3.5 w-3.5" />
                {t("shell.importLive")}
              </button>
            )}
          </div>
        </div>
        {formOpen && (
          <ProviderForm
            pluginId={plugin.id}
            existing={editingProvider}
            onDone={() => {
              setFormOpen(false);
              setEditingProvider(null);
              onProvidersChanged();
            }}
          />
        )}
        {providers.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("shell.noProviders")}
          </p>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {providers.map((provider) => (
              <div
                key={provider.id}
                className="flex flex-col gap-2 rounded-lg border border-border p-4"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="truncate font-medium">{provider.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {provider.category}
                    </div>
                  </div>
                  <div className="flex shrink-0 gap-0.5">
                    <button
                      type="button"
                      onClick={() => {
                        setEditingProvider(provider);
                        setFormOpen(true);
                      }}
                      className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                      title={t("shell.editProvider")}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteProvider(provider)}
                      className="rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                      title={t("shell.deleteProvider")}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>
                {canApply && (
                  <button
                    type="button"
                    onClick={() => handleApply(provider.id)}
                    className="mt-auto inline-flex items-center justify-center gap-1 rounded-md bg-primary px-2 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
                  >
                    <Send className="h-3.5 w-3.5" />
                    {t("shell.applyProvider")}
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      {!hasAnyCapability && (
        <p className="text-sm text-muted-foreground">
          {t("shell.noCapabilities")}
        </p>
      )}

      {canRead && (
        <section className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium">{t("shell.liveProviders")}</h3>
            <button
              type="button"
              onClick={() =>
                queryClient.invalidateQueries({ queryKey: ["live", plugin.id] })
              }
              className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
              title={t("common.refresh")}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          </div>
          {liveQuery.isLoading ? (
            <p className="text-xs text-muted-foreground">
              {t("common.loading")}
            </p>
          ) : liveQuery.isError ? (
            <p className="text-xs text-destructive">
              {t("common.error")}: {String(liveQuery.error)}
            </p>
          ) : (liveQuery.data?.providers.length ?? 0) === 0 ? (
            <p className="text-xs text-muted-foreground">
              {t("shell.noLiveProviders")}
            </p>
          ) : (
            <ul className="divide-y divide-border rounded-lg border border-border">
              {liveQuery.data!.providers.map((lp) => (
                <li
                  key={lp.id}
                  className="flex items-center justify-between gap-3 px-3 py-2"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm">{lp.name}</span>
                      {liveQuery.data!.current === lp.id && (
                        <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary">
                          {t("shell.liveCurrent")}
                        </span>
                      )}
                    </div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground">
                      {lp.id}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}
