import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  Blocks,
  Copy,
  Download,
  FileJson,
  History,
  Layers,
  Plus,
  Puzzle,
  RefreshCw,
  ScrollText,
  Sparkles,
  Trash2,
  Upload,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  backupCreate,
  backupDelete,
  backupList,
  exportConfigToFile,
  importConfigFromFile,
  profilesApply,
  profilesClearCurrent,
  profilesCurrent,
  profilesDelete,
  profilesList,
  profilesUpsert,
  promptsDelete,
  promptsList,
  promptsToggle,
  promptsUpsert,
  skillsInstall,
  skillsList,
  skillsTogglePlugin,
  skillsUninstall,
  getPlugins,
  applyProvider,
  addProvider,
  deleteProvider,
  getProviders,
  importFromLive,
  importMcpServersFromAllPlugins,
  importProvidersFromLive,
  mcpDelete,
  mcpList,
  mcpToggleApp,
  mcpUpsert,
  readRawConfig,
  removeProviderFromLive,
  syncAllProvidersToLive,
  writeRawConfig,
} from "@/lib/api";
import type {
  BackupRecord,
  Profile,
  PromptRecord,
  SkillRecord,
  Provider,
  McpServer,
} from "@/types";
import ProviderForm from "@/components/ProviderForm";
import SessionList from "@/components/SessionList";
import UsagePanel from "@/components/UsagePanel";
import JsonEditor from "@/components/JsonEditor";
import MarkdownEditor from "@/components/MarkdownEditor";
import { PanelHeader, EmptyState } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

function SkillsPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ["skills"], queryFn: skillsList });
  const skills = query.data ?? [];

  const handleInstall = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    try {
      await skillsInstall(dir);
      await queryClient.invalidateQueries({ queryKey: ["skills"] });
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleUninstall = async (id: string) => {
    try {
      await skillsUninstall(id);
      await queryClient.invalidateQueries({ queryKey: ["skills"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleToggle = async (s: SkillRecord, pluginId: string) => {
    const enabled = s.enabledPlugins.includes(pluginId);
    try {
      await skillsTogglePlugin(s.id, pluginId, !enabled);
      await queryClient.invalidateQueries({ queryKey: ["skills"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Sparkles className="h-5 w-5" />}
        title={t("features.skillsTitle")}
        subtitle={t("features.skillsSubtitle")}
      >
        <button
          type="button"
          onClick={handleInstall}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.skillsInstall")}
        </button>
      </PanelHeader>
      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : skills.length === 0 ? (
        <EmptyState
          icon={<Sparkles className="h-8 w-8" />}
          message={t("features.skillsEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {skills.map((s: SkillRecord) => (
              <li
                key={s.id}
                className="flex items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{s.name}</div>
                  {s.description && (
                    <div className="truncate text-xs text-muted-foreground">
                      {s.description}
                    </div>
                  )}
                  <button
                    type="button"
                    onClick={() => handleToggle(s, pluginId)}
                    className={`mt-1.5 rounded-full px-2 py-0.5 text-xs transition-colors ${
                      s.enabledPlugins.includes(pluginId)
                        ? "bg-primary/10 text-primary hover:bg-primary/20"
                        : "border border-border text-muted-foreground hover:bg-muted"
                    }`}
                  >
                    {pluginId} ·{" "}
                    {s.enabledPlugins.includes(pluginId)
                      ? t("features.skillsEnable")
                      : t("features.skillsDisable")}
                  </button>
                </div>
                <button
                  type="button"
                  onClick={() => handleUninstall(s.id)}
                  className="shrink-0 rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("common.delete")}
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function PromptsPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [content, setContent] = useState("");
  const query = useQuery({
    queryKey: ["prompts"],
    queryFn: () => promptsList(),
  });
  const prompts = query.data ?? [];

  const handleAdd = async () => {
    const id = name.trim() || `prompt_${Date.now()}`;
    if (!content.trim()) {
      toast.error(t("common.error"));
      return;
    }
    try {
      await promptsUpsert(id, pluginId, name.trim() || id, content, undefined);
      await queryClient.invalidateQueries({ queryKey: ["prompts"] });
      setShowForm(false);
      setName("");
      setContent("");
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleToggle = async (p: PromptRecord) => {
    try {
      await promptsToggle(p.id, !p.enabled);
      await queryClient.invalidateQueries({ queryKey: ["prompts"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await promptsDelete(id);
      await queryClient.invalidateQueries({ queryKey: ["prompts"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<ScrollText className="h-5 w-5" />}
        title={t("features.promptsTitle")}
        subtitle={t("features.promptsSubtitle")}
      >
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.promptsAdd")}
        </button>
      </PanelHeader>
      {showForm && (
        <div className="space-y-2 rounded-xl border border-border bg-card p-3 shadow-sm">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("features.promptsTitle")}
            className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
          />
          <MarkdownEditor
            value={content}
            onChange={setContent}
            placeholder="Content…"
            minHeight="120px"
          />
          <button
            type="button"
            onClick={handleAdd}
            className="w-full rounded-md bg-primary px-2 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
          >
            {t("common.save")}
          </button>
        </div>
      )}
      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : prompts.length === 0 ? (
        <EmptyState
          icon={<ScrollText className="h-8 w-8" />}
          message={t("features.promptsEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {prompts.map((p: PromptRecord) => (
              <li
                key={p.id}
                className="px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{p.name}</div>
                    <div className="truncate text-xs text-muted-foreground">
                      {p.pluginId}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleToggle(p)}
                    className={`shrink-0 rounded-full px-2 py-0.5 text-xs transition-colors ${
                      p.enabled
                        ? "bg-primary/10 text-primary hover:bg-primary/20"
                        : "border border-border text-muted-foreground hover:bg-muted"
                    }`}
                  >
                    {p.enabled
                      ? t("features.promptsEnable")
                      : t("features.promptsDisable")}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDelete(p.id)}
                    className="shrink-0 rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    title={t("common.delete")}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
                {p.enabled && (
                  <pre className="mt-2 whitespace-pre-wrap break-words rounded bg-muted/50 p-2 text-xs">
                    {p.content}
                  </pre>
                )}
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function ProfilesPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ["profiles"], queryFn: profilesList });
  const currentQuery = useQuery({
    queryKey: ["profiles-current"],
    queryFn: profilesCurrent,
  });
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [payloadJson, setPayloadJson] = useState("{}");
  const profiles = query.data ?? [];

  const handleAdd = async () => {
    if (!name.trim()) {
      toast.error(t("common.error"));
      return;
    }
    let payload: Record<string, unknown>;
    try {
      payload = JSON.parse(payloadJson || "{}");
    } catch {
      toast.error(t("jsonEditor.invalidJson"));
      return;
    }
    try {
      await profilesUpsert({
        id: `profile_${Date.now()}`,
        name: name.trim(),
        payload,
      });
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      setShowForm(false);
      setName("");
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleApply = async (id: string) => {
    try {
      await profilesApply(id);
      await queryClient.invalidateQueries({ queryKey: ["profiles-current"] });
      toast.success(t("features.profilesApply"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleClear = async () => {
    try {
      await profilesClearCurrent();
      await queryClient.invalidateQueries({ queryKey: ["profiles-current"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await profilesDelete(id);
      await queryClient.invalidateQueries({ queryKey: ["profiles"] });
      await queryClient.invalidateQueries({ queryKey: ["profiles-current"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Layers className="h-5 w-5" />}
        title={t("features.profilesTitle")}
        subtitle={t("features.profilesSubtitle")}
      >
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.profilesAdd")}
        </button>
      </PanelHeader>
      {currentQuery.data && (
        <div className="flex items-center justify-between rounded-lg border border-primary/30 bg-primary/5 px-4 py-2.5 text-xs">
          <span className="flex items-center gap-2 text-primary">
            <span className="h-2 w-2 rounded-full bg-primary" />
            {t("features.profilesCurrent")}
          </span>
          <button
            type="button"
            onClick={handleClear}
            className="text-muted-foreground hover:text-foreground"
          >
            {t("features.profilesClear")}
          </button>
        </div>
      )}
      {showForm && (
        <div className="space-y-2 rounded-xl border border-border bg-card p-3 shadow-sm">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("features.profilesTitle")}
            className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
          />
          <JsonEditor value={payloadJson} onChange={setPayloadJson} rows={8} />
          <button
            type="button"
            onClick={handleAdd}
            className="rounded-md bg-primary px-3 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
          >
            {t("common.save")}
          </button>
        </div>
      )}
      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : profiles.length === 0 ? (
        <EmptyState
          icon={<Layers className="h-8 w-8" />}
          message={t("features.profilesEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {profiles.map((p: Profile) => (
              <li
                key={p.id}
                className="flex items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{p.name}</div>
                </div>
                <button
                  type="button"
                  onClick={() => handleApply(p.id)}
                  className="shrink-0 rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
                >
                  {t("features.profilesApply")}
                </button>
                <button
                  type="button"
                  onClick={() => handleDelete(p.id)}
                  className="shrink-0 rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("common.delete")}
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function BackupPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ["backups"], queryFn: backupList });
  const backups = query.data ?? [];

  const handleCreate = async () => {
    try {
      await backupCreate();
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleExport = async () => {
    try {
      const filePath = await save({
        defaultPath: "cc-switch-export.json",
      });
      if (typeof filePath !== "string") return;
      await exportConfigToFile(filePath);
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImport = async () => {
    const filePath = await open({
      multiple: false,
      title: t("features.backupImportHint"),
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof filePath !== "string") return;
    try {
      const n = await importConfigFromFile(filePath);
      await queryClient.invalidateQueries({ queryKey: ["providers"] });
      toast.success(t("features.backupImported", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (b: BackupRecord) => {
    try {
      await backupDelete(b.id);
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Archive className="h-5 w-5" />}
        title={t("features.backupTitle")}
        subtitle={t("features.backupSubtitle")}
      >
        <button
          type="button"
          onClick={handleCreate}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Archive className="h-3 w-3" />
          {t("features.backupCreate")}
        </button>
        <button
          type="button"
          onClick={handleExport}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Download className="h-3 w-3" />
          {t("features.backupExport")}
        </button>
        <button
          type="button"
          onClick={handleImport}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Upload className="h-3 w-3" />
          {t("features.backupImport")}
        </button>
      </PanelHeader>
      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : backups.length === 0 ? (
        <EmptyState
          icon={<Archive className="h-8 w-8" />}
          message={t("features.backupEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {backups.map((b: BackupRecord) => (
              <li
                key={b.id}
                className="flex items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{b.name}</div>
                  <div className="mt-0.5 text-xs text-muted-foreground">
                    {new Date(b.createdAt * 1000).toLocaleString()} ·{" "}
                    {(b.sizeBytes / 1024).toFixed(1)} KB
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => handleDelete(b)}
                  className="shrink-0 rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("common.delete")}
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function ProvidersPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<Provider | null>(null);
  const [liveEditOpen, setLiveEditOpen] = useState(false);
  const [liveRaw, setLiveRaw] = useState("");
  const [liveLoading, setLiveLoading] = useState(false);

  const query = useQuery({
    queryKey: ["providers"],
    queryFn: () => getProviders(),
  });
  const providers = (query.data ?? []).filter((p) => p.pluginId === pluginId);

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["providers"] });

  const openLiveEdit = async () => {
    setLiveLoading(true);
    try {
      const raw = await readRawConfig(pluginId);
      setLiveRaw(raw);
      setLiveEditOpen(true);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLiveLoading(false);
    }
  };

  const handleSaveLive = async () => {
    try {
      await writeRawConfig(pluginId, liveRaw);
      setLiveEditOpen(false);
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleSyncAll = async () => {
    try {
      const n = await syncAllProvidersToLive(pluginId);
      toast.success(t("shell.syncedCount", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleBackfill = async () => {
    try {
      const n = await importProvidersFromLive(pluginId);
      await invalidate();
      toast.success(t("shell.importedCount", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleApply = async (id: string) => {
    try {
      await applyProvider(pluginId, id, true);
      toast.success(t("shell.applied"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (p: Provider) => {
    try {
      await deleteProvider(p.id);
      await removeProviderFromLive(pluginId, p.id);
      await invalidate();
      toast.success(t("shell.providerDeleted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDuplicate = async (p: Provider) => {
    // 生成唯一 id：`{id}-copy-{n}`（仿 v1 generateUniqueProviderCopyKey）。
    const base = p.id.includes("-copy") ? p.id.split("-copy")[0] : p.id;
    let n = 1;
    let newId = `${base}-copy-${n}`;
    const existingIds = new Set(providers.map((x) => x.id));
    while (existingIds.has(newId)) {
      n += 1;
      newId = `${base}-copy-${n}`;
    }
    try {
      // addToLive=false：仅复制到 DB，不立即投影（仿 v1）。
      await addProvider(
        {
          id: newId,
          pluginId,
          name: `${p.name} (copy)`,
          category: p.category,
          settingsConfig: p.settingsConfig,
          meta: p.meta,
        },
        false,
      );
      await invalidate();
      toast.success(t("shell.providerDuplicated"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImport = async () => {
    try {
      const candidates = await importFromLive(pluginId);
      for (const c of candidates) {
        try {
          await addProvider({
            // 保留 live 配置中的 provider id 作为 DB id（additive 键一致）。
            id: c.id,
            pluginId,
            name: c.name,
            category: "imported",
            settingsConfig: JSON.stringify(c.settingsConfig),
            meta: { liveId: c.id },
          });
        } catch {
          // 跳过已存在条目
        }
      }
      await invalidate();
      toast.success(t("shell.importedCount", { count: candidates.length }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Blocks className="h-5 w-5" />}
        title={t("nav.providers")}
        subtitle={pluginId}
      >
        <button
          type="button"
          onClick={handleBackfill}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
          title={t("shell.backfillHint")}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("shell.backfill")}
        </button>
        <button
          type="button"
          onClick={handleSyncAll}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
          title={t("shell.syncAllHint")}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("shell.syncAll")}
        </button>
        <button
          type="button"
          onClick={openLiveEdit}
          disabled={liveLoading}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
          title={t("shell.editLiveHint")}
        >
          <FileJson className="h-3.5 w-3.5" />
          {t("shell.editLive")}
        </button>
        <button
          type="button"
          onClick={handleImport}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Download className="h-3.5 w-3.5" />
          {t("shell.importLive")}
        </button>
        <button
          type="button"
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
          className="inline-flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Plus className="h-3.5 w-3.5" />
          {t("shell.addProvider")}
        </button>
      </PanelHeader>

      {formOpen && (
        <ProviderForm
          pluginId={pluginId}
          existing={editing}
          onDone={() => {
            setFormOpen(false);
            setEditing(null);
            invalidate();
          }}
        />
      )}

      {providers.length === 0 ? (
        <EmptyState
          icon={<Blocks className="h-8 w-8" />}
          message={t("shell.noProviders")}
        />
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {providers.map((p) => (
            <div
              key={p.id}
              className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4 shadow-sm transition-shadow hover:shadow-md"
            >
              <div className="truncate font-medium">{p.name}</div>
              <div className="text-xs text-muted-foreground">{p.category}</div>
              <div className="mt-auto flex gap-1">
                <button
                  type="button"
                  onClick={() => handleApply(p.id)}
                  className="flex-1 rounded-md bg-primary px-2 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
                >
                  {t("shell.applyProvider")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setEditing(p);
                    setFormOpen(true);
                  }}
                  className="rounded-md border border-border px-2 py-1.5 text-xs transition-colors hover:bg-accent"
                  title={t("shell.editProvider")}
                >
                  {t("shell.editProvider")}
                </button>
                <button
                  type="button"
                  onClick={() => handleDuplicate(p)}
                  className="rounded-md border border-border px-2 py-1.5 text-xs transition-colors hover:bg-accent"
                  title={t("shell.duplicateProvider")}
                >
                  <Copy className="h-3.5 w-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => handleDelete(p)}
                  className="rounded-md border border-border px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("shell.deleteProvider")}
                >
                  {t("shell.deleteProvider")}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={liveEditOpen}
        onOpenChange={(o) => {
          if (!o) setLiveEditOpen(false);
        }}
      >
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>
              {t("shell.editLive")} · {pluginId}
            </DialogTitle>
          </DialogHeader>
          <JsonEditor value={liveRaw} onChange={setLiveRaw} height={360} />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setLiveEditOpen(false)}
              className="rounded-md border border-border px-3 py-1.5 text-xs transition-colors hover:bg-accent"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={handleSaveLive}
              className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
            >
              {t("common.save")}
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function SessionsPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<History className="h-5 w-5" />}
        title={t("nav.sessions")}
        subtitle={pluginId}
      />
      <SessionList pluginId={pluginId} />
    </div>
  );
}

function McpGlobalPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [newId, setNewId] = useState("");
  const [newName, setNewName] = useState("");
  const [mcpTab, setMcpTab] = useState<"structured" | "raw">("structured");
  // 结构化表单字段（对齐 v1 引导式配置）。
  const [mcpType, setMcpType] = useState<"stdio" | "sse">("stdio");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpEnv, setMcpEnv] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpHeaders, setMcpHeaders] = useState("");
  const [rawSpec, setRawSpec] = useState("{}");
  const [enabledApps, setEnabledApps] = useState<Record<string, boolean>>({});

  const query = useQuery({ queryKey: ["mcp-all"], queryFn: mcpList });
  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: getPlugins });
  const servers = query.data ?? [];
  const plugins = pluginsQuery.data ?? [];

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["mcp-all"] });

  /** 把结构化表单字段组装成统一 spec（对齐 v1 的 wizard 逻辑）。 */
  const buildSpec = (validate: boolean): Record<string, unknown> | null => {
    if (mcpType === "stdio") {
      if (!mcpCommand.trim()) {
        if (validate) toast.error(t("mcpRequired"));
        return null;
      }
      const spec: Record<string, unknown> = {
        type: "stdio",
        command: mcpCommand.trim(),
      };
      const args = mcpArgs
        .split(/\r?\n/)
        .map((a) => a.trim())
        .filter(Boolean);
      if (args.length > 0) spec.args = args;
      const env: Record<string, string> = {};
      for (const line of mcpEnv.split(/\r?\n/)) {
        const idx = line.indexOf("=");
        if (idx > 0) {
          const k = line.slice(0, idx).trim();
          const v = line.slice(idx + 1).trim();
          if (k) env[k] = v;
        }
      }
      if (Object.keys(env).length > 0) spec.env = env;
      return spec;
    }
    if (!mcpUrl.trim()) {
      if (validate) toast.error(t("mcpRequired"));
      return null;
    }
    const spec: Record<string, unknown> = { type: "sse", url: mcpUrl.trim() };
    const headers: Record<string, string> = {};
    for (const line of mcpHeaders.split(/\r?\n/)) {
      const idx = line.indexOf(":");
      if (idx > 0) {
        const k = line.slice(0, idx).trim();
        const v = line.slice(idx + 1).trim();
        if (k) headers[k] = v;
      }
    }
    if (Object.keys(headers).length > 0) spec.headers = headers;
    return spec;
  };

  const handleUpsert = async () => {
    if (!newId.trim()) {
      toast.error(t("common.error"));
      return;
    }
    let spec: Record<string, unknown>;
    if (mcpTab === "structured") {
      const built = buildSpec(true);
      if (!built) return;
      spec = built;
    } else {
      try {
        const parsed = JSON.parse(rawSpec || "{}");
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
          toast.error(t("jsonEditor.invalidJson"));
          return;
        }
        spec = parsed as Record<string, unknown>;
      } catch {
        toast.error(t("jsonEditor.invalidJson"));
        return;
      }
    }
    const apps = plugins.map((p): [string, boolean] => [
      p.id,
      enabledApps[p.id] ?? false,
    ]);
    try {
      await mcpUpsert({
        id: newId.trim(),
        name: newName.trim() || newId.trim(),
        spec,
        apps,
      });
      await invalidate();
      setShowForm(false);
      setNewId("");
      setNewName("");
      setRawSpec("{}");
      setEnabledApps({});
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const resetForm = () => {
    setNewId("");
    setNewName("");
    setMcpType("stdio");
    setMcpCommand("");
    setMcpArgs("");
    setMcpEnv("");
    setMcpUrl("");
    setMcpHeaders("");
    setRawSpec("{}");
    setEnabledApps({});
  };

  const handleDelete = async (id: string) => {
    try {
      await mcpDelete(id);
      await invalidate();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleToggleApp = async (
    id: string,
    pluginId: string,
    enabled: boolean,
  ) => {
    try {
      await mcpToggleApp(id, pluginId, enabled);
      await invalidate();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImport = async () => {
    try {
      const n = await importMcpServersFromAllPlugins();
      await invalidate();
      toast.success(t("features.mcpImported", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Puzzle className="h-5 w-5" />}
        title={t("nav.mcp")}
        subtitle={t("features.mcpSubtitle")}
      >
        <button
          type="button"
          onClick={handleImport}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Download className="h-3 w-3" />
          {t("features.mcpImport")}
        </button>
        <button
          type="button"
          onClick={() => {
            if (!showForm) resetForm();
            setShowForm((v) => !v);
          }}
          className="inline-flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Plus className="h-3.5 w-3.5" />
          {t("features.mcpAdd")}
        </button>
      </PanelHeader>

      {showForm && (
        <div className="space-y-3 rounded-xl border border-border bg-card p-3 shadow-sm">
          <div className="flex gap-2">
            <input
              value={newId}
              onChange={(e) => setNewId(e.target.value)}
              placeholder={t("features.mcpId")}
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t("features.mcpName")}
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
          </div>

          <Tabs
            value={mcpTab}
            onValueChange={(v) => {
              if (v === "raw") {
                // 从结构化切到 JSON：预填当前表单生成的 spec（不校验必填）。
                const built = buildSpec(false);
                if (built) setRawSpec(JSON.stringify(built, null, 2));
              }
              setMcpTab(v as "structured" | "raw");
            }}
          >
            <TabsList>
              <TabsTrigger value="structured">
                {t("features.mcpFormStructured")}
              </TabsTrigger>
              <TabsTrigger value="raw">{t("features.mcpFormRaw")}</TabsTrigger>
            </TabsList>

            <TabsContent value="structured" className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">
                  {t("features.mcpType")}:
                </span>
                <select
                  value={mcpType}
                  onChange={(e) =>
                    setMcpType(e.target.value as "stdio" | "sse")
                  }
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs"
                >
                  <option value="stdio">{t("features.mcpTypeStdio")}</option>
                  <option value="sse">{t("features.mcpTypeRemote")}</option>
                </select>
              </div>
              {mcpType === "stdio" ? (
                <>
                  <input
                    value={mcpCommand}
                    onChange={(e) => setMcpCommand(e.target.value)}
                    placeholder={t("features.mcpCommand")}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                  <textarea
                    value={mcpArgs}
                    onChange={(e) => setMcpArgs(e.target.value)}
                    placeholder={t("features.mcpArgs")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                  <textarea
                    value={mcpEnv}
                    onChange={(e) => setMcpEnv(e.target.value)}
                    placeholder={t("features.mcpEnv")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                </>
              ) : (
                <>
                  <input
                    value={mcpUrl}
                    onChange={(e) => setMcpUrl(e.target.value)}
                    placeholder={t("features.mcpUrl")}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                  <textarea
                    value={mcpHeaders}
                    onChange={(e) => setMcpHeaders(e.target.value)}
                    placeholder={t("features.mcpHeaders")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                </>
              )}

              {/* 启用插件勾选 */}
              {plugins.length > 0 && (
                <div className="flex flex-wrap gap-3 pt-1">
                  {plugins.map((p) => (
                    <label
                      key={p.id}
                      className="flex items-center gap-1.5 text-xs text-muted-foreground"
                    >
                      <Checkbox
                        checked={enabledApps[p.id] ?? false}
                        onCheckedChange={(v) =>
                          setEnabledApps((prev) => ({
                            ...prev,
                            [p.id]: v === true,
                          }))
                        }
                      />
                      {p.name}
                    </label>
                  ))}
                </div>
              )}
            </TabsContent>

            <TabsContent value="raw">
              <JsonEditor value={rawSpec} onChange={setRawSpec} rows={8} />
            </TabsContent>
          </Tabs>

          <button
            type="button"
            onClick={handleUpsert}
            className="w-full rounded-md bg-primary px-2 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
          >
            {t("common.save")}
          </button>
        </div>
      )}

      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : servers.length === 0 ? (
        <EmptyState
          icon={<Puzzle className="h-8 w-8" />}
          message={t("features.mcpEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {servers.map((s: McpServer) => (
              <li
                key={s.id}
                className="px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{s.name}</div>
                    <div className="truncate text-xs text-muted-foreground">
                      {s.id}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleDelete(s.id)}
                    className="shrink-0 rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    title={t("common.delete")}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
                <div className="mt-2 flex flex-wrap gap-3">
                  {plugins.map((p) => {
                    const enabled =
                      s.apps.find(([pid]) => pid === p.id)?.[1] ?? false;
                    return (
                      <label
                        key={p.id}
                        className="flex items-center gap-1.5 text-xs text-muted-foreground"
                      >
                        <Checkbox
                          checked={enabled}
                          onCheckedChange={(v) =>
                            handleToggleApp(s.id, p.id, v === true)
                          }
                        />
                        {p.name}
                      </label>
                    );
                  })}
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

function UsageGlobalPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<RefreshCw className="h-5 w-5" />}
        title={t("nav.usage")}
        subtitle={pluginId}
      />
      <UsagePanel pluginId={pluginId} />
    </div>
  );
}

export default function GlobalPanels({
  view,
  pluginId,
}: {
  view: string;
  pluginId: string;
}) {
  switch (view) {
    case "providers":
      return <ProvidersPanel pluginId={pluginId} />;
    case "sessions":
      return <SessionsPanel pluginId={pluginId} />;
    case "mcp":
      return <McpGlobalPanel />;
    case "usage":
      return <UsageGlobalPanel pluginId={pluginId} />;
    case "skills":
      return <SkillsPanel pluginId={pluginId} />;
    case "prompts":
      return <PromptsPanel pluginId={pluginId} />;
    case "profiles":
      return <ProfilesPanel />;
    case "backup":
      return <BackupPanel />;
    default:
      return null;
  }
}
