import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Loader2, Settings as SettingsIcon } from "lucide-react";
import { PanelHeader } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { skillErrorText } from "@/lib/skillsUtils";
import {
  skillsGetSyncSettings,
  skillsList,
  skillsMigrateStorage,
  skillsSetSyncMethod,
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

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [s, skills] = await Promise.all([
        skillsGetSyncSettings(),
        skillsList(),
      ]);
      setSettings(s);
      setInstalledCount(skills.length);
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
    </div>
  );
}
