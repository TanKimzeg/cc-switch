import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Download,
  ExternalLink,
  FolderArchive,
  History,
  Loader2,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { PanelHeader, EmptyState } from "@/components/PanelHeader";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { filterInstalledSkills, skillErrorText } from "@/lib/skillsUtils";
import { SkillsDiscovery } from "./SkillsDiscovery";
import { RestoreSkillsDialog } from "./RestoreSkillsDialog";
import { ImportSkillsDialog } from "./ImportSkillsDialog";
import {
  getPlugins,
  skillsCheckUpdates,
  skillsInstallFromZip,
  skillsList,
  skillsScanUnmanaged,
  skillsTogglePlugin,
  skillsUninstall,
  skillsUpdateSkill,
} from "@/lib/api";
import type { SkillRecord, SkillUpdateInfo, UnmanagedSkill } from "@/types";

interface SkillsPanelProps {
  pluginId: string;
}

export default function SkillsPanel({ pluginId }: SkillsPanelProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [view, setView] = useState<"manage" | "discover">("manage");
  const [search, setSearch] = useState("");

  const skillsQuery = useQuery({ queryKey: ["skills"], queryFn: skillsList });
  const skills = skillsQuery.data ?? [];
  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: getPlugins });
  const skillsPlugins = (pluginsQuery.data ?? []).filter((p) => p.skillsDir);

  // 更新检测
  const [updates, setUpdates] = useState<SkillUpdateInfo[]>([]);
  const [checking, setChecking] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updatingId, setUpdatingId] = useState<string | null>(null);

  // 对话框
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [unmanaged, setUnmanaged] = useState<UnmanagedSkill[]>([]);
  const [confirmUninstall, setConfirmUninstall] = useState<SkillRecord | null>(
    null,
  );
  const [mutationPending, setMutationPending] = useState(false);

  const invalidateSkills = () =>
    queryClient.invalidateQueries({ queryKey: ["skills"] });

  const appliedUpdates = useMemo(() => {
    const installedIds = new Set(skills.map((s) => s.id));
    return updates.filter((u) => installedIds.has(u.id));
  }, [updates, skills]);

  const filtered = filterInstalledSkills(skills, search);

  const handleInstallZip = async () => {
    const file = await open({
      multiple: false,
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (typeof file !== "string") return;
    setMutationPending(true);
    try {
      const installed = await skillsInstallFromZip(file, pluginId);
      if (installed.length === 0) {
        toast.error(t("skills.installFailed"));
      } else if (installed.length === 1) {
        toast.success(t("skills.installSuccess", { name: installed[0].name }));
      } else {
        toast.success(
          t("skills.installSuccessCount", { count: installed.length }),
        );
      }
      await invalidateSkills();
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.installFailed"));
    } finally {
      setMutationPending(false);
    }
  };

  const handleToggle = async (skill: SkillRecord, targetPlugin: string) => {
    const enabled = skill.enabledPlugins.includes(targetPlugin);
    setMutationPending(true);
    try {
      await skillsTogglePlugin(skill.id, targetPlugin, !enabled);
      await invalidateSkills();
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setMutationPending(false);
    }
  };

  const handleBulkToggle = async (pluginId: string, enable: boolean) => {
    setMutationPending(true);
    let failed = 0;
    for (const skill of skills) {
      const on = skill.enabledPlugins.includes(pluginId);
      if (on === enable) continue;
      try {
        await skillsTogglePlugin(skill.id, pluginId, enable);
      } catch {
        failed += 1;
      }
    }
    await invalidateSkills();
    setMutationPending(false);
    if (failed > 0) {
      toast.error(t("bulkToggleFailed", { count: failed }));
    }
  };

  const handleUninstall = async () => {
    const skill = confirmUninstall;
    if (!skill) return;
    setConfirmUninstall(null);
    setMutationPending(true);
    try {
      const backupPath = await skillsUninstall(skill.id);
      toast.success(t("skills.uninstalled", { name: skill.name }), {
        description: backupPath
          ? t("skills.backupLocation", { path: backupPath })
          : undefined,
      });
      // 更新缓存：移除该技能的更新条目
      setUpdates((prev) => prev.filter((u) => u.id !== skill.id));
      await invalidateSkills();
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setMutationPending(false);
    }
  };

  const handleCheckUpdates = async () => {
    setChecking(true);
    try {
      const result = await skillsCheckUpdates();
      setUpdates(result);
      if (result.length === 0) {
        toast.success(t("skills.noUpdates"));
      } else {
        toast.info(t("skills.updatesFound", { count: result.length }));
      }
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setChecking(false);
    }
  };

  const handleUpdateOne = async (id: string) => {
    setUpdatingId(id);
    try {
      const record = await skillsUpdateSkill(id);
      toast.success(t("skills.updateSuccess", { name: record.name }));
      setUpdates((prev) => prev.filter((u) => u.id !== id));
      await invalidateSkills();
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.updateFailed"));
    } finally {
      setUpdatingId(null);
    }
  };

  const handleUpdateAll = async () => {
    setUpdating(true);
    let success = 0;
    for (const u of appliedUpdates) {
      try {
        await skillsUpdateSkill(u.id);
        success += 1;
      } catch (e) {
        toast.error(skillErrorText(t, e) || t("skills.updateFailed"), {
          description: u.name,
        });
      }
    }
    setUpdates([]);
    await invalidateSkills();
    if (success > 0) {
      toast.success(t("skills.updateAllSuccess", { count: success }));
    }
    setUpdating(false);
  };

  const handleScanImport = async () => {
    setMutationPending(true);
    try {
      const found = await skillsScanUnmanaged();
      if (found.length === 0) {
        toast.info(t("skills.noUnmanagedFound"));
      } else {
        setUnmanaged(found);
        setImportOpen(true);
      }
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setMutationPending(false);
    }
  };

  if (view === "discover") {
    return (
      <SkillsDiscovery
        currentPlugin={pluginId}
        installed={skills}
        onChanged={() => void invalidateSkills()}
        onBack={() => setView("manage")}
      />
    );
  }

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Sparkles className="h-5 w-5" />}
        title={t("features.skillsTitle")}
        subtitle={t("features.skillsSubtitle")}
      >
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void handleCheckUpdates()}
          disabled={checking || skills.length === 0}
        >
          {checking ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          {checking ? t("skills.checkingUpdates") : t("skills.checkUpdates")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setRestoreOpen(true)}
          disabled={mutationPending}
        >
          <History className="h-3.5 w-3.5" />
          {t("skills.restoreFromBackup")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void handleInstallZip()}
          disabled={mutationPending}
        >
          <FolderArchive className="h-3.5 w-3.5" />
          {t("skills.installFromZip")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void handleScanImport()}
          disabled={mutationPending}
        >
          <Download className="h-3.5 w-3.5" />
          {t("skills.import")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setView("discover")}
          disabled={mutationPending}
        >
          <Search className="h-3.5 w-3.5" />
          {t("skills.discover")}
        </Button>
      </PanelHeader>

      {skillsQuery.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("skills.loading")}
          </CardContent>
        </Card>
      ) : skills.length === 0 ? (
        <EmptyState
          icon={<Sparkles className="h-8 w-8" />}
          message={t("skills.noInstalled")}
        >
          <p className="text-xs text-muted-foreground">
            {t("skills.noInstalledDescription")}
          </p>
        </EmptyState>
      ) : (
        <Card>
          {/* 计数条 + 按插件批量开关 + 全部更新 */}
          <div className="flex flex-wrap items-center gap-2 border-b border-border-default px-4 py-3">
            <Badge variant="secondary">{t("skills.installed")}</Badge>
            {skillsPlugins.map((plugin) => {
              const count = skills.filter((s) =>
                s.enabledPlugins.includes(plugin.id),
              ).length;
              const total = skills.length;
              const enabled = count === total && total > 0;
              const partial = count > 0 && count < total;
              return (
                <button
                  key={plugin.id}
                  type="button"
                  title={enabled ? `${plugin.name} ✓` : plugin.name}
                  onClick={() => void handleBulkToggle(plugin.id, !enabled)}
                  disabled={mutationPending}
                  className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs transition-colors ${
                    enabled
                      ? "border-primary bg-primary/10 text-primary"
                      : partial
                        ? "border-primary/50 text-muted-foreground"
                        : "border-border-default text-muted-foreground opacity-60 hover:bg-muted"
                  }`}
                >
                  {plugin.name}: {count}
                </button>
              );
            })}
            <div className="flex-1" />
            {appliedUpdates.length > 0 && (
              <Button
                size="sm"
                onClick={() => void handleUpdateAll()}
                disabled={updating}
              >
                {updating ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : null}
                {updating
                  ? t("skills.updatingAll")
                  : t("skills.updateAll", { count: appliedUpdates.length })}
              </Button>
            )}
          </div>

          {/* 搜索 */}
          <div className="relative px-4 pt-3">
            <Search className="absolute left-7 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="pl-8"
              placeholder={t("skills.installedSearchPlaceholder")}
              aria-label={t("skills.installedSearchAriaLabel")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            {search && (
              <button
                type="button"
                onClick={() => setSearch("")}
                className="absolute right-8 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                title={t("common.clear")}
                aria-label={t("common.clear")}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          {filtered.length === 0 ? (
            <div className="py-10 text-center text-xs text-muted-foreground">
              {t("skills.noInstalledSearchResults")}
            </div>
          ) : (
            <ul className="divide-y divide-border">
              {filtered.map((skill) => {
                const hasUpdate = appliedUpdates.some((u) => u.id === skill.id);
                const sourceLabel =
                  skill.repoOwner && skill.repoName
                    ? `${skill.repoOwner}/${skill.repoName}`
                    : t("skills.local");
                return (
                  <li
                    key={skill.id}
                    className="flex items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <span className="truncate text-sm font-medium">
                          {skill.name}
                        </span>
                        {skill.readmeUrl && (
                          <button
                            type="button"
                            onClick={() =>
                              window.open(skill.readmeUrl!, "_blank")
                            }
                            className="text-muted-foreground transition-colors hover:text-foreground"
                            title={t("skills.view")}
                          >
                            <ExternalLink className="h-3 w-3" />
                          </button>
                        )}
                        <span className="text-[11px] text-muted-foreground">
                          {sourceLabel}
                        </span>
                        {hasUpdate && (
                          <Badge
                            variant="outline"
                            className="border-amber-500 text-amber-600 dark:text-amber-400"
                          >
                            {t("skills.updateAvailable")}
                          </Badge>
                        )}
                      </div>
                      {skill.description && (
                        <div className="truncate text-xs text-muted-foreground">
                          {skill.description}
                        </div>
                      )}
                      <div className="mt-1.5 flex flex-wrap gap-1.5">
                        {skillsPlugins.map((plugin) => {
                          const on = skill.enabledPlugins.includes(plugin.id);
                          return (
                            <button
                              key={plugin.id}
                              type="button"
                              onClick={() =>
                                void handleToggle(skill, plugin.id)
                              }
                              disabled={mutationPending}
                              title={`${plugin.name}${on ? " ✓" : ""}`}
                              className={`rounded-full border px-2 py-0.5 text-[11px] transition-colors ${
                                on
                                  ? "border-primary bg-primary/10 text-primary"
                                  : "border-border-default text-muted-foreground opacity-40 hover:opacity-100 hover:bg-muted"
                              }`}
                            >
                              {plugin.name}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      {hasUpdate && (
                        <Button
                          variant="ghost"
                          size="icon"
                          title={t("skills.update")}
                          disabled={updatingId !== null || updating}
                          onClick={() => void handleUpdateOne(skill.id)}
                        >
                          {updatingId === skill.id ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : (
                            <RefreshCw className="h-4 w-4" />
                          )}
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        title={t("skills.uninstall")}
                        disabled={mutationPending}
                        onClick={() => setConfirmUninstall(skill)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </Card>
      )}

      <RestoreSkillsDialog
        open={restoreOpen}
        onClose={() => setRestoreOpen(false)}
        currentPlugin={pluginId}
        onChanged={() => void invalidateSkills()}
      />
      <ImportSkillsDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        unmanaged={unmanaged}
        plugins={skillsPlugins}
        onChanged={() => void invalidateSkills()}
      />
      <ConfirmDialog
        isOpen={confirmUninstall !== null}
        title={t("skills.uninstall")}
        message={t("skills.uninstallConfirm", { name: confirmUninstall?.name })}
        confirmText={t("skills.uninstall")}
        pending={mutationPending}
        onConfirm={() => void handleUninstall()}
        onCancel={() => setConfirmUninstall(null)}
      />
    </div>
  );
}
