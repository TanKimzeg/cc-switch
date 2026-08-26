import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Download, Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { skillErrorText } from "@/lib/skillsUtils";
import { skillsImport } from "@/lib/api";
import type { InstalledPlugin, UnmanagedSkill } from "@/types";

interface ImportSkillsDialogProps {
  open: boolean;
  onClose: () => void;
  unmanaged: UnmanagedSkill[];
  plugins: InstalledPlugin[];
  onChanged: () => void;
}

export function ImportSkillsDialog({
  open,
  onClose,
  unmanaged,
  plugins,
  onChanged,
}: ImportSkillsDialogProps) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<Record<string, string[]>>({});
  const [pending, setPending] = useState(false);

  const selectedCount = useMemo(
    () =>
      Object.values(selected).filter((plugins) => plugins.length > 0).length,
    [selected],
  );

  const handleToggle = (directory: string, pluginId: string, on: boolean) => {
    setSelected((prev) => {
      const current = prev[directory] ?? [];
      return {
        ...prev,
        [directory]: on
          ? [...new Set([...current, pluginId])]
          : current.filter((p) => p !== pluginId),
      };
    });
  };

  const handleSelectSkill = (directory: string, checked: boolean) => {
    setSelected((prev) => {
      const next = { ...prev };
      if (checked) {
        // 默认选中该技能在 foundIn 中、且支持 skills 的插件
        const skill = unmanaged.find((u) => u.directory === directory);
        const targets = skill
          ? skill.foundIn.filter((id) => plugins.some((p) => p.id === id))
          : [];
        next[directory] = [
          ...new Set([...(prev[directory] ?? []), ...targets]),
        ];
      } else {
        delete next[directory];
      }
      return next;
    });
  };

  const handleImport = async () => {
    const selections = Object.entries(selected)
      .filter(([, plugins]) => plugins.length > 0)
      .map(([directory, plugins]) => ({ directory, plugins }));
    if (selections.length === 0) return;
    setPending(true);
    try {
      const imported = await skillsImport(selections);
      toast.success(t("skills.importedCount", { count: imported.length }));
      onChanged();
      onClose();
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && !pending && onClose()}>
      <DialogContent className="max-w-2xl max-h-[85vh]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download className="h-4 w-4" />
            {t("skills.importTitle")}
          </DialogTitle>
          <DialogDescription>{t("skills.importDescription")}</DialogDescription>
        </DialogHeader>
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {unmanaged.length === 0 ? (
            <div className="py-10 text-center text-xs text-muted-foreground">
              {t("skills.noUnmanagedFound")}
            </div>
          ) : (
            <div className="space-y-2">
              {unmanaged.map((skill) => {
                const isChecked = selected[skill.directory] !== undefined;
                return (
                  <div
                    key={skill.directory}
                    className="flex items-start gap-3 rounded-lg border border-border-default p-3"
                  >
                    <Checkbox
                      checked={isChecked}
                      onCheckedChange={(v) =>
                        handleSelectSkill(skill.directory, v === true)
                      }
                      className="mt-0.5"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">
                        {skill.name}
                      </div>
                      {skill.description && (
                        <div className="mt-0.5 line-clamp-1 text-xs text-muted-foreground">
                          {skill.description}
                        </div>
                      )}
                      <div className="mt-1 break-all text-[11px] text-muted-foreground">
                        {skill.path}
                      </div>
                      <div className="mt-1.5 flex flex-wrap gap-1.5">
                        {plugins.map((plugin) => {
                          const on = selected[skill.directory]?.includes(
                            plugin.id,
                          );
                          const recommended =
                            isChecked &&
                            skill.foundIn.includes(plugin.id) &&
                            on === undefined;
                          const active = on || recommended;
                          return (
                            <button
                              key={plugin.id}
                              type="button"
                              onClick={() =>
                                handleToggle(
                                  skill.directory,
                                  plugin.id,
                                  !active,
                                )
                              }
                              className={`rounded-full border px-2 py-0.5 text-[11px] transition-colors ${
                                active
                                  ? "border-primary bg-primary/10 text-primary"
                                  : "border-border-default text-muted-foreground hover:bg-muted"
                              }`}
                            >
                              {plugin.name}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={pending}>
            {t("common.cancel")}
          </Button>
          <Button
            onClick={handleImport}
            disabled={selectedCount === 0 || pending}
          >
            {pending ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {t("skills.importSelected", { count: selectedCount })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
