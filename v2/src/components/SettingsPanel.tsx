import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  AppWindow,
  EyeOff,
  FolderSearch,
  Loader2,
  MonitorUp,
  Power,
  Settings as SettingsIcon,
  Undo2,
} from "lucide-react";
import { PanelHeader } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleRow } from "@/components/ui/toggle-row";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ThemeSettings } from "@/components/settings/ThemeSettings";
import {
  LanguageSettings,
  type LanguageOption,
} from "@/components/settings/LanguageSettings";
import { cn } from "@/lib/utils";
import { skillErrorText } from "@/lib/skillsUtils";
import {
  getAppDataDirOverride,
  getPlugins,
  setAppDataDirOverride,
  settingsGetAppBehavior,
  settingsGetOverrides,
  settingsSetLaunchOnStartup,
  settingsSetMinimizeToTrayOnClose,
  settingsSetOverride,
  settingsSetShowInTray,
  settingsSetSilentStartup,
  skillsGetSyncSettings,
  skillsList,
  skillsMigrateStorage,
  skillsSetSyncMethod,
  syncAllProvidersToLive,
} from "@/lib/api";
import type { AppBehavior, SkillStorageLocation, SyncSettings } from "@/types";

export default function SettingsPanel() {
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useState<SyncSettings | null>(null);
  const [behavior, setBehavior] = useState<AppBehavior | null>(null);
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
      const [s, skills, pluginList, overridesList, appDir, b] =
        await Promise.all([
          skillsGetSyncSettings(),
          skillsList(),
          getPlugins(),
          settingsGetOverrides(),
          getAppDataDirOverride(),
          settingsGetAppBehavior(),
        ]);
      setSettings(s);
      setBehavior(b);
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

  const currentLanguage = i18n.language?.startsWith("en")
    ? "en"
    : ("zh" as LanguageOption);

  const handleBehavior =
    (key: keyof AppBehavior, apply: (enabled: boolean) => Promise<void>) =>
    async (enabled: boolean) => {
      const prev = behavior;
      setBehavior((b) => (b ? { ...b, [key]: enabled } : b));
      try {
        await apply(enabled);
        toast.success(t("common.save"));
      } catch (e) {
        toast.error(skillErrorText(t, e));
        setBehavior(prev);
      }
    };

  const onLaunchOnStartup = handleBehavior(
    "launchOnStartup",
    settingsSetLaunchOnStartup,
  );
  const onSilentStartup = handleBehavior(
    "silentStartup",
    settingsSetSilentStartup,
  );
  const onMinimizeToTray = handleBehavior(
    "minimizeToTrayOnClose",
    settingsSetMinimizeToTrayOnClose,
  );
  const onShowInTray = handleBehavior("showInTray", settingsSetShowInTray);

  const handleLanguageChange = (lang: LanguageOption) => {
    if (lang === currentLanguage) return;
    void i18n.changeLanguage(lang);
    try {
      window.localStorage.setItem("language", lang);
    } catch {
      // localStorage 不可用时仅本次会话生效
    }
  };

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
    if (target === settings?.storageLocation) return;
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
        <Tabs defaultValue="general" className="space-y-6">
          <TabsList className="grid w-full grid-cols-2 glass rounded-lg">
            <TabsTrigger value="general">
              {t("settings.tabGeneral")}
            </TabsTrigger>
            <TabsTrigger value="advanced">
              {t("settings.tabAdvanced")}
            </TabsTrigger>
          </TabsList>

          <TabsContent value="general" className="space-y-6 mt-0">
            <LanguageSettings
              value={currentLanguage}
              onChange={handleLanguageChange}
            />
            <ThemeSettings />
            {behavior && (
              <section className="space-y-4">
                <div className="flex items-center gap-2 pb-2 border-b border-border/40">
                  <AppWindow className="h-4 w-4 text-primary" />
                  <h3 className="text-sm font-medium">
                    {t("settings.windowBehavior")}
                  </h3>
                </div>
                <div className="space-y-3">
                  <ToggleRow
                    icon={<Power className="h-4 w-4 text-orange-500" />}
                    title={t("settings.launchOnStartup")}
                    description={t("settings.launchOnStartupDescription")}
                    checked={behavior.launchOnStartup}
                    onCheckedChange={(v) => void onLaunchOnStartup(v)}
                  />
                  {behavior.launchOnStartup && (
                    <ToggleRow
                      icon={<EyeOff className="h-4 w-4 text-green-500" />}
                      title={t("settings.silentStartup")}
                      description={t("settings.silentStartupDescription")}
                      checked={behavior.silentStartup}
                      onCheckedChange={(v) => void onSilentStartup(v)}
                    />
                  )}
                  <ToggleRow
                    icon={<MonitorUp className="h-4 w-4 text-blue-500" />}
                    title={t("settings.minimizeToTray")}
                    description={t("settings.minimizeToTrayDescription")}
                    checked={behavior.minimizeToTrayOnClose}
                    onCheckedChange={(v) => void onMinimizeToTray(v)}
                  />
                  <ToggleRow
                    icon={<EyeOff className="h-4 w-4 text-cyan-500" />}
                    title={t("settings.showInTray")}
                    description={t("settings.showInTrayDescription")}
                    checked={behavior.showInTray}
                    onCheckedChange={(v) => void onShowInTray(v)}
                  />
                </div>
              </section>
            )}
            <section className="space-y-2">
              <header className="space-y-1">
                <h3 className="text-sm font-medium">
                  {t("settings.skillStorageTitle")}
                </h3>
                <p className="text-xs text-muted-foreground">
                  {t("settings.skillStorageDescription")}
                </p>
              </header>
              <div className="inline-flex gap-1 rounded-md border border-border-default bg-background p-1">
                {(
                  [
                    ["cc_switch", t("settings.skillStorageCcSwitch")],
                    ["unified", t("settings.skillStorageUnified")],
                  ] as const
                ).map(([value, label]) => (
                  <SegmentButton
                    key={value}
                    active={settings.storageLocation === value}
                    disabled={pending}
                    onClick={() => requestMigrate(value)}
                  >
                    {label}
                  </SegmentButton>
                ))}
              </div>
              <p className="text-xs text-muted-foreground">
                {settings.storageLocation === "unified"
                  ? t("settings.skillStorageUnifiedHint")
                  : t("settings.skillStorageCcSwitchHint")}
              </p>
            </section>

            <section className="space-y-2">
              <header className="space-y-1">
                <h3 className="text-sm font-medium">
                  {t("settings.skillSyncTitle")}
                </h3>
                <p className="text-xs text-muted-foreground">
                  {t("settings.skillSyncDescription")}
                </p>
              </header>
              <div className="inline-flex gap-1 rounded-md border border-border-default bg-background p-1">
                {(
                  [
                    ["auto", t("settings.skillSyncSymlink")],
                    ["copy", t("settings.skillSyncCopy")],
                  ] as const
                ).map(([value, label]) => (
                  <SegmentButton
                    key={value}
                    active={settings.syncMethod === value}
                    disabled={pending}
                    onClick={() => void handleSyncMethod(value)}
                  >
                    {label}
                  </SegmentButton>
                ))}
              </div>
              {settings.syncMethod === "auto" && (
                <p className="text-xs text-muted-foreground">
                  {t("settings.skillSyncSymlinkHint")}
                </p>
              )}
            </section>
          </TabsContent>

          <TabsContent value="advanced" className="mt-0 pb-4">
            <Accordion
              type="multiple"
              defaultValue={[]}
              className="w-full space-y-4"
            >
              <AccordionItem
                value="directory"
                className="rounded-xl glass-card overflow-hidden"
              >
                <AccordionTrigger className="px-6 py-4 hover:no-underline hover:bg-muted/50 data-[state=open]:bg-muted/50">
                  <div className="flex items-center gap-3">
                    <FolderSearch className="h-5 w-5 text-primary" />
                    <div className="text-left">
                      <h3 className="text-base font-semibold">
                        {t("settings.advanced.configDir.title")}
                      </h3>
                      <p className="text-sm text-muted-foreground font-normal">
                        {t("settings.advanced.configDir.description")}
                      </p>
                    </div>
                  </div>
                </AccordionTrigger>
                <AccordionContent className="px-6 pb-6 pt-4 border-t border-border/50">
                  <div className="space-y-6">
                    <section className="space-y-2">
                      <h3 className="text-sm font-medium">
                        {t("settings.appDataDir")}
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        {t("settings.appDataDirDescription")}
                      </p>
                      <div className="flex gap-2">
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
                      <p className="text-[11px] text-muted-foreground">
                        {t("settings.restartRequiredMessage")}
                      </p>
                    </section>

                    <section className="space-y-2">
                      <h3 className="text-sm font-medium">
                        {t("settings.configDirOverride")}
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        {t("settings.configDirOverrideDescription")}
                      </p>
                      <div className="space-y-3">
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
                                    void handleOverrideChange(
                                      plugin.id,
                                      current,
                                    );
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
                                onClick={() =>
                                  void handleOverrideChange(plugin.id, "")
                                }
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
                    </section>
                  </div>
                </AccordionContent>
              </AccordionItem>
            </Accordion>
          </TabsContent>
        </Tabs>
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

interface SegmentButtonProps {
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}

function SegmentButton({
  active,
  disabled,
  onClick,
  children,
}: SegmentButtonProps) {
  return (
    <Button
      type="button"
      onClick={onClick}
      disabled={disabled}
      size="sm"
      variant={active ? "default" : "ghost"}
      className={cn(
        "min-w-[96px]",
        active
          ? "shadow-sm"
          : "text-muted-foreground hover:text-foreground hover:bg-muted",
      )}
    >
      {children}
    </Button>
  );
}
