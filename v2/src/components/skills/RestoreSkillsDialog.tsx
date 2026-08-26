import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { History, Loader2, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { skillErrorText } from "@/lib/skillsUtils";
import {
  skillsDeleteBackup,
  skillsListBackups,
  skillsRestoreBackup,
} from "@/lib/api";
import type { SkillBackupEntry } from "@/types";

interface RestoreSkillsDialogProps {
  open: boolean;
  onClose: () => void;
  currentPlugin: string;
  onChanged: () => void;
}

export function RestoreSkillsDialog({
  open,
  onClose,
  currentPlugin,
  onChanged,
}: RestoreSkillsDialogProps) {
  const { t } = useTranslation();
  const [backups, setBackups] = useState<SkillBackupEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<SkillBackupEntry | null>(
    null,
  );

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    skillsListBackups()
      .then(setBackups)
      .catch((e) => toast.error(skillErrorText(t, e)))
      .finally(() => setLoading(false));
  }, [open, t]);

  const handleRestore = async (backup: SkillBackupEntry) => {
    setBusyId(backup.backupId);
    try {
      const restored = await skillsRestoreBackup(
        backup.backupId,
        currentPlugin,
      );
      toast.success(t("skills.restoreSuccess", { name: restored.name }));
      onChanged();
      onClose();
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.restoreFailed"));
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (backup: SkillBackupEntry) => {
    setDeleteTarget(null);
    setBusyId(backup.backupId);
    try {
      await skillsDeleteBackup(backup.backupId);
      toast.success(t("skills.backupDeleted", { name: backup.name }));
      // 失败也重拉（remove_dir_all 可能部分删除）
      try {
        setBackups(await skillsListBackups());
      } catch {
        /* ignore */
      }
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.backupDeleteFailed"));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="max-w-2xl max-h-[85vh]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <History className="h-4 w-4" />
              {t("skills.restoreFromBackupTitle")}
            </DialogTitle>
            <DialogDescription>
              {t("skills.restoreFromBackupDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="flex-1 overflow-y-auto px-6 py-4">
            {loading ? (
              <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {t("skills.loading")}
              </div>
            ) : backups.length === 0 ? (
              <div className="py-10 text-center text-xs text-muted-foreground">
                {t("skills.restoreEmpty")}
              </div>
            ) : (
              <div className="space-y-2">
                {backups.map((backup) => (
                  <div
                    key={backup.backupId}
                    className="flex items-start justify-between gap-3 rounded-lg border border-border-default p-3"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium">
                          {backup.name}
                        </span>
                        <Badge
                          variant="outline"
                          className="font-mono text-[10px]"
                        >
                          {backup.directory}
                        </Badge>
                      </div>
                      {backup.description && (
                        <div className="mt-0.5 truncate text-xs text-muted-foreground">
                          {backup.description}
                        </div>
                      )}
                      <div className="mt-1 text-[11px] text-muted-foreground">
                        {t("skills.backupTime", {
                          time: new Date(
                            backup.createdAt * 1000,
                          ).toLocaleString(),
                        })}
                      </div>
                      <div className="break-all text-[11px] text-muted-foreground">
                        {t("skills.backupPath", { path: backup.backupPath })}
                      </div>
                    </div>
                    <div className="flex shrink-0 gap-1.5">
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => handleRestore(backup)}
                        disabled={busyId !== null}
                      >
                        {busyId === backup.backupId ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : null}
                        {t("skills.restoreBackup")}
                      </Button>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="text-red-500 hover:text-red-600"
                        onClick={() => setDeleteTarget(backup)}
                        disabled={busyId !== null}
                        title={t("skills.deleteBackup")}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={onClose}>
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        isOpen={deleteTarget !== null}
        title={t("skills.confirmDeleteBackupTitle")}
        message={t("skills.confirmDeleteBackup", { name: deleteTarget?.name })}
        confirmText={t("common.delete")}
        pending={false}
        onConfirm={() => deleteTarget && handleDelete(deleteTarget)}
        onCancel={() => setDeleteTarget(null)}
      />
    </>
  );
}
