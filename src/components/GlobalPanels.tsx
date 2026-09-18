import { useEffect, useState } from "react";
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
  Pencil,
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
  backupRename,
  backupRestore,
  exportConfigToFile,
  getSetting,
  importConfigFromFile,
  profilesApply,
  profilesCreate,
  profilesDelete,
  profilesList,
  profilesUpdate,
  applyProvider,
  addProvider,
  deleteProvider,
  getProviders,
  importFromLive,
  importProvidersFromLive,
  readRawConfig,
  removeProviderFromLive,
  setSetting,
  syncAllProvidersToLive,
  writeRawConfig,
} from "@/lib/api";
import type { BackupRecord, Provider, ProfileWithCurrent } from "@/types";
import SkillsPanel from "@/components/skills/SkillsPanel";
import PromptPanel from "@/components/prompts/PromptPanel";
import McpGlobalPanel from "@/components/mcp/McpGlobalPanel";
import ProviderForm from "@/components/ProviderForm";
import SessionList from "@/components/SessionList";
import UsagePanel from "@/components/UsagePanel";
import JsonEditor from "@/components/JsonEditor";
import { PanelHeader, EmptyState } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ConfirmDialog";

function ProfilesPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<ProfileWithCurrent | null>(null);
  const [renameText, setRenameText] = useState("");
  const [deleting, setDeleting] = useState<ProfileWithCurrent | null>(null);
  const [busy, setBusy] = useState(false);

  const query = useQuery({
    queryKey: ["profiles", pluginId],
    queryFn: () => profilesList(pluginId),
  });
  const profiles = query.data ?? [];

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["profiles", pluginId] });

  const handleCreate = async () => {
    if (!name.trim()) {
      toast.error(t("common.error"));
      return;
    }
    setBusy(true);
    try {
      await profilesCreate(name.trim(), pluginId);
      await invalidate();
      setShowCreate(false);
      setName("");
      toast.success(t("profiles.created"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleApply = async (p: ProfileWithCurrent) => {
    setBusy(true);
    try {
      const warnings = await profilesApply(p.id, pluginId);
      await invalidate();
      if (warnings.length === 0) {
        toast.success(t("profiles.applied", { name: p.name }));
      } else {
        toast.warning(
          t("profiles.appliedWithWarnings", {
            name: p.name,
            warnings: warnings.join("\n"),
          }),
          { duration: 8000 },
        );
      }
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleResnapshot = async (p: ProfileWithCurrent) => {
    setBusy(true);
    try {
      await profilesUpdate(p.id, undefined, pluginId);
      await invalidate();
      toast.success(t("profiles.resnapshotted", { name: p.name }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRename = async () => {
    if (!renaming) return;
    try {
      await profilesUpdate(renaming.id, renameText.trim());
      await invalidate();
      toast.success(t("profiles.renamed"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRenaming(null);
    }
  };

  const handleDelete = async (p: ProfileWithCurrent) => {
    try {
      await profilesDelete(p.id);
      await invalidate();
      toast.success(t("common.delete"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeleting(null);
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
          onClick={() => setShowCreate((v) => !v)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Plus className="h-3 w-3" />
          {t("profiles.createFromCurrent")}
        </button>
      </PanelHeader>

      <p className="text-xs text-muted-foreground">
        {t("profiles.description")}
      </p>

      {showCreate && (
        <div className="flex gap-2 rounded-xl border border-border bg-card p-3 shadow-sm">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("profiles.namePlaceholder")}
            className="flex-1"
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleCreate();
            }}
          />
          <Button size="sm" disabled={busy} onClick={() => void handleCreate()}>
            {t("profiles.snapshotNow")}
          </Button>
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
            {profiles.map((p) => (
              <li
                key={p.id}
                className="flex items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">
                      {p.name}
                    </span>
                    {p.isCurrent && (
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary">
                        {t("features.profilesCurrent")}
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 text-xs text-muted-foreground">
                    {new Date((p.updatedAt ?? 0) * 1000).toLocaleString()}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  {!p.isCurrent && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void handleApply(p)}
                      className="rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
                    >
                      {t("features.profilesApply")}
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void handleResnapshot(p)}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                    title={t("profiles.updateFromCurrent")}
                  >
                    <RefreshCw className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setRenaming(p);
                      setRenameText(p.name);
                    }}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                    title={t("profiles.rename")}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => setDeleting(p)}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    title={t("common.delete")}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <ConfirmDialog
        isOpen={deleting !== null}
        title={t("profiles.deleteTitle")}
        message={
          deleting ? t("profiles.deleteMessage", { name: deleting.name }) : ""
        }
        confirmText={t("common.delete")}
        cancelText={t("common.cancel")}
        variant="destructive"
        onConfirm={() => deleting && void handleDelete(deleting)}
        onCancel={() => setDeleting(null)}
      />

      <Dialog
        open={renaming !== null}
        onOpenChange={(o) => !o && setRenaming(null)}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("profiles.rename")}</DialogTitle>
          </DialogHeader>
          <Input
            value={renameText}
            onChange={(e) => setRenameText(e.target.value)}
            placeholder={t("profiles.namePlaceholder")}
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setRenaming(null)}>
              {t("common.cancel")}
            </Button>
            <Button size="sm" onClick={() => void handleRename()}>
              {t("common.save")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function BackupPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ["backups"], queryFn: backupList });
  const backups = query.data ?? [];

  // 自动备份设置（settings 表：backup.intervalHours / backup.retainCount）
  const [intervalHours, setIntervalHours] = useState<string>("24");
  const [retainCount, setRetainCount] = useState<string>("10");
  useEffect(() => {
    void (async () => {
      try {
        const [i, r] = await Promise.all([
          getSetting("backup.intervalHours"),
          getSetting("backup.retainCount"),
        ]);
        if (i !== null) setIntervalHours(i);
        if (r !== null) setRetainCount(r);
      } catch {
        // 缺省值即可
      }
    })();
  }, []);

  const handleIntervalChange = async (value: string) => {
    setIntervalHours(value);
    try {
      await setSetting("backup.intervalHours", value);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleRetainChange = async (value: string) => {
    setRetainCount(value);
    try {
      await setSetting("backup.retainCount", value);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const [renaming, setRenaming] = useState<BackupRecord | null>(null);
  const [renameText, setRenameText] = useState("");
  const [restoring, setRestoring] = useState<BackupRecord | null>(null);
  const [deleting, setDeleting] = useState<BackupRecord | null>(null);

  const handleCreate = async () => {
    try {
      await backupCreate();
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
      toast.success(t("settings.backupManager.createSuccess"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleRestore = async (b: BackupRecord) => {
    try {
      await backupRestore(b.id);
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
      toast.success(t("settings.backupManager.restoreSuccess"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRestoring(null);
    }
  };

  const handleRename = async () => {
    if (!renaming) return;
    try {
      await backupRename(renaming.id, renameText);
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
      toast.success(t("settings.backupManager.renameSuccess"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRenaming(null);
    }
  };

  const handleDelete = async (b: BackupRecord) => {
    try {
      await backupDelete(b.id);
      await queryClient.invalidateQueries({ queryKey: ["backups"] });
      toast.success(t("settings.backupManager.deleteSuccess"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeleting(null);
    }
  };

  const handleExport = async () => {
    try {
      const filePath = await save({
        defaultPath: "agentswitch-export.json",
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

  const intervalOptions = ["0", "1", "6", "12", "24", "48", "72"];
  const retainOptions = ["1", "3", "5", "10", "20", "50"];

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Archive className="h-5 w-5" />}
        title={t("settings.backupManager.title")}
        subtitle={t("settings.backupManager.description")}
      >
        <button
          type="button"
          onClick={handleCreate}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Archive className="h-3 w-3" />
          {t("settings.backupManager.createBackup")}
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

      <Card>
        <CardContent className="flex flex-wrap items-center gap-x-6 gap-y-3 p-4">
          <label className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground">
              {t("settings.backupManager.intervalLabel")}
            </span>
            <select
              value={intervalHours}
              onChange={(e) => void handleIntervalChange(e.target.value)}
              className="h-8 rounded-md border border-border-default bg-background px-2 text-sm"
            >
              {intervalOptions.map((v) => (
                <option key={v} value={v}>
                  {v === "0"
                    ? t("settings.backupManager.intervalDisabled")
                    : Number(v) % 24 === 0
                      ? t("settings.backupManager.intervalDays", {
                          days: Number(v) / 24,
                        })
                      : t("settings.backupManager.intervalHours", {
                          hours: v,
                        })}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground">
              {t("settings.backupManager.retainLabel")}
            </span>
            <select
              value={retainCount}
              onChange={(e) => void handleRetainChange(e.target.value)}
              className="h-8 rounded-md border border-border-default bg-background px-2 text-sm"
            >
              {retainOptions.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
          </label>
        </CardContent>
      </Card>

      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : backups.length === 0 ? (
        <EmptyState
          icon={<Archive className="h-8 w-8" />}
          message={t("settings.backupManager.empty")}
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
                <div className="flex shrink-0 items-center gap-0.5">
                  <button
                    type="button"
                    onClick={() => setRestoring(b)}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                    title={t("settings.backupManager.restore")}
                  >
                    <History className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setRenaming(b);
                      setRenameText(b.name);
                    }}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                    title={t("settings.backupManager.rename")}
                  >
                    <Pencil className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    onClick={() => setDeleting(b)}
                    className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    title={t("common.delete")}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <ConfirmDialog
        isOpen={restoring !== null}
        title={t("settings.backupManager.confirmTitle")}
        message={t("settings.backupManager.confirmMessage")}
        confirmText={t("settings.backupManager.restore")}
        cancelText={t("common.cancel")}
        variant="info"
        onConfirm={() => {
          if (restoring) void handleRestore(restoring);
        }}
        onCancel={() => setRestoring(null)}
      />
      <ConfirmDialog
        isOpen={deleting !== null}
        title={t("settings.backupManager.deleteConfirmTitle")}
        message={t("settings.backupManager.deleteConfirmMessage")}
        confirmText={t("common.delete")}
        cancelText={t("common.cancel")}
        variant="destructive"
        onConfirm={() => {
          if (deleting) void handleDelete(deleting);
        }}
        onCancel={() => setDeleting(null)}
      />

      <Dialog
        open={renaming !== null}
        onOpenChange={(o) => !o && setRenaming(null)}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("settings.backupManager.rename")}</DialogTitle>
          </DialogHeader>
          <Input
            value={renameText}
            onChange={(e) => setRenameText(e.target.value)}
            placeholder={t("settings.backupManager.namePlaceholder")}
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setRenaming(null)}>
              {t("common.cancel")}
            </Button>
            <Button onClick={() => void handleRename()}>
              {t("common.save")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
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
      return <PromptPanel pluginId={pluginId} />;
    case "profiles":
      return <ProfilesPanel pluginId={pluginId} />;
    case "backup":
      return <BackupPanel />;
    default:
      return null;
  }
}
