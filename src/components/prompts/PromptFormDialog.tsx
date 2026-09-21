import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ArrowLeft, Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import MarkdownEditor from "@/components/MarkdownEditor";
import { promptsUpsert } from "@/lib/api";
import type { PromptRecord } from "@/types";

interface PromptFormDialogProps {
  open: boolean;
  pluginId: string;
  pluginName: string;
  filename: string;
  /** 编辑对象；null = 新增。 */
  prompt: PromptRecord | null;
  onClose: () => void;
  onChanged: () => void;
}

export function PromptFormDialog({
  open,
  pluginId,
  pluginName,
  filename,
  prompt,
  onClose,
  onChanged,
}: PromptFormDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(prompt?.name ?? "");
  const [description, setDescription] = useState(prompt?.description ?? "");
  const [content, setContent] = useState(prompt?.content ?? "");
  const [saving, setSaving] = useState(false);

  // 打开/切换编辑对象时重置表单。
  useEffect(() => {
    if (open) {
      setName(prompt?.name ?? "");
      setDescription(prompt?.description ?? "");
      setContent(prompt?.content ?? "");
    }
  }, [open, prompt?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSave = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setSaving(true);
    try {
      const id = prompt?.id ?? `prompt-${Date.now()}`;
      await promptsUpsert(
        id,
        pluginId,
        trimmed,
        content.trim(),
        description.trim() || undefined,
      );
      toast.success(t("prompts.saveSuccess"));
      onChanged();
      onClose();
    } catch (e) {
      toast.error(String(e) || t("prompts.saveFailed"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && !saving && onClose()}>
      <DialogContent variant="fullscreen" className="overflow-y-auto">
        <DialogHeader className="flex-row items-center justify-between border-b border-border-default px-6 py-4">
          <div className="flex items-center gap-3">
            <Button
              variant="ghost"
              size="icon"
              onClick={onClose}
              disabled={saving}
              title={t("common.close")}
            >
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <DialogTitle>
              {prompt
                ? t("prompts.editTitle", { name: pluginName })
                : t("prompts.addTitle", { name: pluginName })}
            </DialogTitle>
          </div>
        </DialogHeader>

        <div className="mx-auto w-full max-w-3xl flex-1 space-y-4 px-6 py-6">
          <div className="grid gap-4 sm:grid-cols-2">
            <div>
              <Label className="mb-1 block text-xs text-muted-foreground">
                {t("prompts.name")}
              </Label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("prompts.namePlaceholder")}
              />
            </div>
            <div>
              <Label className="mb-1 block text-xs text-muted-foreground">
                {t("prompts.description")}
              </Label>
              <Input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("prompts.descriptionPlaceholder")}
              />
            </div>
          </div>
          <div>
            <Label className="mb-1 block text-xs text-muted-foreground">
              {t("prompts.content")}
            </Label>
            <MarkdownEditor
              value={content}
              onChange={setContent}
              placeholder={t("prompts.contentPlaceholder", { filename })}
              minHeight="167px"
            />
          </div>
          <div className="flex justify-end">
            <Button
              onClick={() => void handleSave()}
              disabled={!name.trim() || saving}
            >
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              {t("common.save")}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
