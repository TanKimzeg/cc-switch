import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  Download,
  Layers,
  Plus,
  ScrollText,
  Sparkles,
  Trash2,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  backupCreate,
  backupDelete,
  backupList,
  exportConfigJson,
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
} from "@/lib/api";
import type { BackupRecord, Profile, PromptRecord, SkillRecord } from "@/types";

function SkillsPanel() {
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
                  onClick={() => handleToggle(s, "opencode")}
                  className={`mt-1 rounded-full px-2 py-0.5 text-xs ${
                    s.enabledPlugins.includes("opencode")
                      ? "bg-primary/10 text-primary"
                      : "border border-border text-muted-foreground"
                  }`}
                >
                  opencode ·{" "}
                  {s.enabledPlugins.includes("opencode")
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

function PromptsPanel() {
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
      await promptsUpsert(
        id,
        "opencode",
        name.trim() || id,
        content,
        undefined,
      );
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
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            placeholder="Content…"
            rows={4}
            className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
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
  const profiles = query.data ?? [];

  const handleAdd = async () => {
    if (!name.trim()) {
      toast.error(t("common.error"));
      return;
    }
    try {
      await profilesUpsert({
        id: `profile_${Date.now()}`,
        name: name.trim(),
        payload: {},
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
        <div className="flex gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("features.profilesTitle")}
            className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
          />
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
      const payload = await exportConfigJson();
      const filePath = await save({
        defaultPath: "cc-switch-export.json",
      });
      if (typeof filePath !== "string") return;
      const fs = await import("@tauri-apps/plugin-fs");
      await fs.writeTextFile(filePath, JSON.stringify(payload, null, 2));
      toast.success(t("common.save"));
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

export default function GlobalPanels({ view }: { view: string }) {
  switch (view) {
    case "skills":
      return <SkillsPanel />;
    case "prompts":
      return <PromptsPanel />;
    case "profiles":
      return <ProfilesPanel />;
    case "backup":
      return <BackupPanel />;
    default:
      return null;
  }
}

export function globalPanelTabs() {
  return [
    { id: "skills", label: "features.skillsTitle", icon: Sparkles },
    { id: "prompts", label: "features.promptsTitle", icon: ScrollText },
    { id: "profiles", label: "features.profilesTitle", icon: Layers },
    { id: "backup", label: "features.backupTitle", icon: Archive },
  ];
}
