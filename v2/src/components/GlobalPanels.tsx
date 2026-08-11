import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  Download,
  FileJson,
  Plus,
  RefreshCw,
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
  importMcpServersFromPlugin,
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
import { Checkbox } from "@/components/ui/checkbox";
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
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.skillsTitle")}</h3>
        <button
          type="button"
          onClick={handleInstall}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.skillsInstall")}
        </button>
      </div>
      {query.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : skills.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.skillsEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {skills.map((s: SkillRecord) => (
            <li
              key={s.id}
              className="flex items-center justify-between gap-2 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm">{s.name}</div>
                {s.description && (
                  <div className="truncate text-xs text-muted-foreground">
                    {s.description}
                  </div>
                )}
                <button
                  type="button"
                  onClick={() => handleToggle(s, pluginId)}
                  className={`mt-1 rounded-full px-2 py-0.5 text-xs ${
                    s.enabledPlugins.includes(pluginId)
                      ? "bg-primary/10 text-primary"
                      : "border border-border text-muted-foreground"
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
                className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                title={t("common.delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
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
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.promptsTitle")}</h3>
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.promptsAdd")}
        </button>
      </div>
      {showForm && (
        <div className="space-y-2 rounded-lg border border-border p-3">
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
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : prompts.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.promptsEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {prompts.map((p: PromptRecord) => (
            <li key={p.id} className="px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{p.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {p.pluginId}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => handleToggle(p)}
                  className={`shrink-0 rounded-full px-2 py-0.5 text-xs ${
                    p.enabled
                      ? "bg-primary/10 text-primary"
                      : "border border-border text-muted-foreground"
                  }`}
                >
                  {p.enabled
                    ? t("features.promptsEnable")
                    : t("features.promptsDisable")}
                </button>
                <button
                  type="button"
                  onClick={() => handleDelete(p.id)}
                  className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("common.delete")}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>
              {p.enabled && (
                <pre className="mt-1 whitespace-pre-wrap break-words rounded bg-muted/50 p-2 text-xs">
                  {p.content}
                </pre>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
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
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.profilesTitle")}</h3>
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("features.profilesAdd")}
        </button>
      </div>
      {currentQuery.data && (
        <div className="flex items-center justify-between rounded-lg border border-primary/30 bg-primary/5 px-3 py-2 text-xs">
          <span className="text-primary">{t("features.profilesCurrent")}</span>
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
        <div className="space-y-2">
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
            className="rounded-md bg-primary px-3 py-1 text-xs text-primary-foreground"
          >
            {t("common.save")}
          </button>
        </div>
      )}
      {query.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : profiles.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.profilesEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {profiles.map((p: Profile) => (
            <li
              key={p.id}
              className="flex items-center justify-between gap-2 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm">{p.name}</div>
              </div>
              <button
                type="button"
                onClick={() => handleApply(p.id)}
                className="shrink-0 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
              >
                {t("features.profilesApply")}
              </button>
              <button
                type="button"
                onClick={() => handleDelete(p.id)}
                className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                title={t("common.delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
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
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.backupTitle")}</h3>
        <div className="flex items-center gap-1">
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
        </div>
      </div>
      {query.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : backups.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.backupEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {backups.map((b: BackupRecord) => (
            <li
              key={b.id}
              className="flex items-center justify-between gap-2 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm">{b.name}</div>
                <div className="text-xs text-muted-foreground">
                  {new Date(b.createdAt * 1000).toLocaleString()} ·{" "}
                  {(b.sizeBytes / 1024).toFixed(1)} KB
                </div>
              </div>
              <button
                type="button"
                onClick={() => handleDelete(b)}
                className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                title={t("common.delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
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
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">
          {t("nav.providers")} · {pluginId}
        </h2>
        <div className="flex gap-1">
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
        </div>
      </div>

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
        <p className="text-sm text-muted-foreground">
          {t("shell.noProviders")}
        </p>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {providers.map((p) => (
            <div
              key={p.id}
              className="flex flex-col gap-2 rounded-lg border border-border p-4"
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
      <h2 className="text-lg font-semibold">
        {t("nav.sessions")} · {pluginId}
      </h2>
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
  const [newSpec, setNewSpec] = useState("{}");

  const query = useQuery({ queryKey: ["mcp-all"], queryFn: mcpList });
  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: getPlugins });
  const servers = query.data ?? [];
  const plugins = pluginsQuery.data ?? [];

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["mcp-all"] });

  const handleUpsert = async () => {
    if (!newId.trim()) {
      toast.error(t("common.error"));
      return;
    }
    let spec: Record<string, unknown>;
    try {
      spec = JSON.parse(newSpec || "{}");
    } catch {
      toast.error(t("jsonEditor.invalidJson"));
      return;
    }
    try {
      await mcpUpsert({
        id: newId.trim(),
        name: newName.trim() || newId.trim(),
        spec,
        apps: [],
      });
      await invalidate();
      setShowForm(false);
      setNewId("");
      setNewName("");
      setNewSpec("{}");
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
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
      const n = await importMcpServersFromPlugin("opencode");
      await invalidate();
      toast.success(t("features.mcpImported", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("nav.mcp")}</h2>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={handleImport}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
          >
            <Download className="h-3.5 w-3.5" />
            {t("features.mcpImport")}
          </button>
          <button
            type="button"
            onClick={() => setShowForm((v) => !v)}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("features.mcpAdd")}
          </button>
        </div>
      </div>

      {showForm && (
        <div className="space-y-2 rounded-lg border border-border p-3">
          <div className="flex gap-2">
            <input
              value={newId}
              onChange={(e) => setNewId(e.target.value)}
              placeholder="server-id"
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t("features.mcpName")}
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
          </div>
          <JsonEditor value={newSpec} onChange={setNewSpec} rows={8} />
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
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : servers.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.mcpEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {servers.map((s: McpServer) => (
            <li key={s.id} className="px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{s.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {s.id}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => handleDelete(s.id)}
                  className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("common.delete")}
                >
                  <Trash2 className="h-3.5 w-3.5" />
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
      )}
    </div>
  );
}

function UsageGlobalPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold">
        {t("nav.usage")} · {pluginId}
      </h2>
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
