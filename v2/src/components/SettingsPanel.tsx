import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  FolderSearch,
  Loader2,
  Settings as SettingsIcon,
  Undo2,
} from "lucide-react";
import { PanelHeader } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { skillErrorText } from "@/lib/skillsUtils";
import {
  getAppDataDirOverride,
  getPlugins,
  setAppDataDirOverride,
  settingsGetOverrides,
  settingsSetOverride,
  skillsGetSyncSettings,
  skillsList,
  skillsMigrateStorage,
  skillsSetSyncMethod,
  syncAllProvidersToLive,
} from "@/lib/api";
import type { SkillStorageLocation, SyncSettings } from "@/types";

export default function SettingsPanel() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<SyncSettings | null>(null);
  const [installedCount, setInstalledCount] = useState(0);
  const [loading, setLoading] = useState(false);
  const [pending, setPending] = useState(false);
  const [migrateTarget, setMigrateTarget] =
    useState<SkillStorageLocation | null>(null);

  // 目录覆盖
  const [plugins, setPlugins] = useState<
    { id: string; name: string; promptFile?: string | null }[]
  >([]);
  const [overrides, setOverrides] = useState<Record<string, string>>({});
  const [appDataDir, setAppDataDir] = useState("");
  const [restartOpen, setRestartOpen] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [s, skills, pluginList, overridesList, appDir] = await Promise.all([
        skillsGetSyncSettings(),
        skillsList(),
        getPlugins(),
        settingsGetOverrides(),
        getAppDataDirOverride(),
      ]);
      setSettings(s);
      setInstalledCount(skills.length);
      setPlugins(
        pluginList
          .filter((p) => p.capabilities?.apply && p.entryType !== "ts")
          .map((p) => ({ id: p.id, name: p.name, promptFile: p.promptFile })),
      );
      setOverrides(
        Object.fromEntries(overridesList.map((o) => [o.pluginId, o.path])),
      );
      setAppDataDir(appDir ?? "");
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleSyncMethod = async (method: string) => {
    setPending(true);
    try {
      await skillsSetSyncMethod(method);
      setSettings((prev) =>
        prev
          ? { ...prev, syncMethod: method as SyncSettings["syncMethod"] }
          : prev,
      );
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setPending(false);
    }
  };

  const requestMigrate = (target: SkillStorageLocation) => {
    if (installedCount > 0) {
      setMigrateTarget(target);
    } else {
      void runMigrate(target);
    }
  };

  const runMigrate = async (target: SkillStorageLocation) => {
    setPending(true);
    try {
      const result = await skillsMigrateStorage(target);
      setSettings((prev) =>
        prev ? { ...prev, storageLocation: target } : prev,
      );
      if (result.errors.length === 0) {
        toast.success(
          t("settings.skillStorageMigrated", { count: result.migratedCount }),
        );
      } else {
        toast.error(
          t("settings.skillStorageMigratedPartial", {
            migrated: result.migratedCount,
            errors: result.errors.length,
          }),
        );
      }
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setPending(false);
    }
  };

  // ===== 目录覆盖 =====

  const browseDir = async (current: string): Promise<string | null> => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: current || undefined,
      });
      return typeof picked === "string" ? picked : null;
    } catch {
      return null;
    }
  };

  const handleAppDataDirBrowse = async () => {
    const picked = await browseDir(appDataDir);
    if (!picked) return;
    setAppDataDir(picked);
    setPending(true);
    try {
      await setAppDataDirOverride(picked);
      setRestartOpen(true);
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setPending(false);
    }
  };

  const handleAppDataDirReset = async () => {
    setAppDataDir("");
    setPending(true);
    try {
      await setAppDataDirOverride(null);
      setRestartOpen(true);
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setPending(false);
    }
  };

  const handleOverrideChange = async (pluginId: string, value: string) => {
    const next = value.trim();
    setOverrides((prev) => ({ ...prev, [pluginId]: next }));
    setPending(true);
    try {
      await settingsSetOverride(pluginId, next || null);
      await syncAllProvidersToLive(pluginId).catch(() => {});
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(skillErrorText(t, e));
      void load();
    } finally {
      setPending(false);
    }
  };

  const handleOverrideBrowse = async (pluginId: string, current: string) => {
    const picked = await browseDir(current);
    if (!picked) return;
    await handleOverrideChange(pluginId, picked);
  };

  const locationOptions: {
    value: SkillStorageLocation;
    label: string;
    hint: string;
  }[] = [
    {
      value: "cc_switch",
      label: t("settings.skillStorageCcSwitch"),
      hint: t("settings.skillStorageCcSwitchHint"),
    },
    {
      value: "unified",
      label: t("settings.skillStorageUnified"),
      hint: t("settings.skillStorageUnifiedHint"),
    },
  ];

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<SettingsIcon className="h-5 w-5" />}
        title={t("settings.title")}
      />

      {loading || !settings ? (
        <Card>
          <CardContent className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t("skills.loading")}
          </CardContent>
        </Card>
      ) : (
        <>
          {/* Skills 存储位置 */}
          <Card>
            <CardContent className="p-6">
              <h3 className="text-sm font-semibold">
                {t("settings.skillStorageTitle")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("settings.skillStorageDescription")}
              </p>
              <div className="mt-4 flex flex-wrap gap-2">
                {locationOptions.map((opt) => {
                  const active = settings.storageLocation === opt.value;
                  return (
                    <button
                      key={opt.value}
                      type="button"
                      disabled={pending}
                      onClick={() => requestMigrate(opt.value)}
                      className={`rounded-md border px-3 py-1.5 text-sm transition-colors ${
                        active
                          ? "border-primary bg-primary/10 text-primary"
                          : "border-border-default text-muted-foreground hover:bg-muted"
                      }`}
                    >
                      {opt.label}
                    </button>
                  );
                })}
              </div>
              <p className="mt-2 text-xs text-muted-foreground">
                {
                  locationOptions.find(
                    (o) => o.value === settings.storageLocation,
                  )?.hint
                }
              </p>
            </CardContent>
          </Card>

          {/* Skills 同步方式 */}
          <Card>
            <CardContent className="p-6">
              <h3 className="text-sm font-semibold">
                {t("settings.skillSyncTitle")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("settings.skillSyncDescription")}
              </p>
              <div className="mt-4 flex flex-wrap gap-2">
                {(
                  [
                    ["auto", t("settings.skillSyncSymlink")],
                    ["copy", t("settings.skillSyncCopy")],
                  ] as const
                ).map(([value, label]) => {
                  const active = settings.syncMethod === value;
                  return (
                    <button
                      key={value}
                      type="button"
                      disabled={pending}
                      onClick={() => void handleSyncMethod(value)}
                      className={`rounded-md border px-3 py-1.5 text-sm transition-colors ${
                        active
                          ? "border-primary bg-primary/10 text-primary"
                          : "border-border-default text-muted-foreground hover:bg-muted"
                      }`}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
              {settings.syncMethod === "auto" && (
                <p className="mt-2 text-xs text-muted-foreground">
                  {t("settings.skillSyncSymlinkHint")}
                </p>
              )}
            </CardContent>
          </Card>

          {/* CC Switch 配置目录 */}
          <Card>
            <CardContent className="p-6">
              <h3 className="text-sm font-semibold">
                {t("settings.appDataDir")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("settings.appDataDirDescription")}
              </p>
              <div className="mt-4 flex gap-2">
                <Input
                  className="flex-1"
                  value={appDataDir}
                  onChange={(e) => setAppDataDir(e.target.value)}
                  placeholder="~/.cc-switch"
                  disabled={pending}
                />
                <Button
                  variant="outline"
                  size="icon"
                  title={t("settings.browseDirectory")}
                  disabled={pending}
                  onClick={() => void handleAppDataDirBrowse()}
                >
                  <FolderSearch className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  title={t("settings.resetDefault")}
                  disabled={pending}
                  onClick={() => void handleAppDataDirReset()}
                >
                  <Undo2 className="h-4 w-4" />
                </Button>
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground">
                {t("settings.restartRequiredMessage")}
              </p>
            </CardContent>
          </Card>

          {/* 配置目录覆盖（高级） */}
          <Card>
            <CardContent className="p-6">
              <h3 className="text-sm font-semibold">
                {t("settings.configDirOverride")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("settings.configDirOverrideDescription")}
              </p>
              <div className="mt-4 space-y-3">
                {plugins.map((plugin) => {
                  const current = overrides[plugin.id] ?? "";
                  return (
                    <div
                      key={plugin.id}
                      className="flex flex-wrap items-center gap-2"
                    >
                      <span className="w-32 shrink-0 text-sm">
                        {plugin.name}
                      </span>
                      <Input
                        className="min-w-[220px] flex-1"
                        value={current}
                        onChange={(e) =>
                          setOverrides((prev) => ({
                            ...prev,
                            [plugin.id]: e.target.value,
                          }))
                        }
                        onBlur={() =>
                          void handleOverrideChange(plugin.id, current)
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            void handleOverrideChange(plugin.id, current);
                          }
                        }}
                        disabled={pending}
                        placeholder="~/.config/opencode"
                      />
                      <Button
                        variant="outline"
                        size="icon"
                        title={t("settings.browseDirectory")}
                        disabled={pending}
                        onClick={() =>
                          void handleOverrideBrowse(plugin.id, current)
                        }
                      >
                        <FolderSearch className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("settings.resetDefault")}
                        disabled={pending}
                        onClick={() => void handleOverrideChange(plugin.id, "")}
                      >
                        <Undo2 className="h-4 w-4" />
                      </Button>
                    </div>
                  );
                })}
                {plugins.length === 0 && (
                  <p className="text-xs text-muted-foreground">
                    {t("skills.noInstalled")}
                  </p>
                )}
              </div>
            </CardContent>
          </Card>
        </>
      )}

      <ConfirmDialog
        isOpen={migrateTarget !== null}
        title={t("settings.skillStorageMigrateTitle")}
        message={t("settings.skillStorageMigrateMessage", {
          count: installedCount,
        })}
        confirmText={t("common.confirm")}
        variant="info"
        pending={pending}
        onConfirm={() => {
          const target = migrateTarget;
          setMigrateTarget(null);
          if (target) void runMigrate(target);
        }}
        onCancel={() => setMigrateTarget(null)}
      />
      <ConfirmDialog
        isOpen={restartOpen}
        title={t("settings.restartRequired")}
        message={t("settings.restartRequiredMessage")}
        confirmText={t("settings.restartNow")}
        cancelText={t("settings.restartLater")}
        variant="info"
        pending={false}
        onConfirm={() => {
          setRestartOpen(false);
          void relaunch().catch(() => toast.error(t("settings.restartFailed")));
        }}
        onCancel={() => setRestartOpen(false)}
      />
    </div>
  );
}
