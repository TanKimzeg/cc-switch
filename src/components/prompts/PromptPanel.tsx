import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Edit3, FileText, Plus, Search, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent } from "@/components/ui/card";
import { PanelHeader, EmptyState } from "@/components/PanelHeader";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { PromptFormDialog } from "./PromptFormDialog";
import {
  getPlugins,
  promptsDelete,
  promptsList,
  promptsToggle,
} from "@/lib/api";
import type { PromptRecord } from "@/types";

const DEFAULT_FILENAMES: Record<string, string> = {
  claudecode: "CLAUDE.md",
  opencode: "AGENTS.md",
  openclaw: "AGENTS.md",
};

interface PromptPanelProps {
  pluginId: string;
}

export default function PromptPanel({ pluginId }: PromptPanelProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const [form, setForm] = useState<{
    open: boolean;
    prompt: PromptRecord | null;
  }>({ open: false, prompt: null });
  const [deleteTarget, setDeleteTarget] = useState<PromptRecord | null>(null);
  const [pending, setPending] = useState(false);

  const query = useQuery({
    queryKey: ["prompts", pluginId],
    queryFn: () => promptsList(pluginId),
  });
  const prompts = query.data ?? [];
  const enabled = prompts.find((p) => p.enabled);

  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: getPlugins });
  const plugin = (pluginsQuery.data ?? []).find((p) => p.id === pluginId);
  const pluginName = plugin?.name ?? pluginId;
  const filename = plugin?.promptFile
    ? plugin.promptFile.split(/[\\/]/).pop() ||
      DEFAULT_FILENAMES[pluginId] ||
      "AGENTS.md"
    : DEFAULT_FILENAMES[pluginId] || "AGENTS.md";

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return prompts;
    return prompts.filter((p) =>
      [p.name, p.id, p.description, p.content]
        .join(" ")
        .toLowerCase()
        .includes(q),
    );
  }, [prompts, search]);

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["prompts", pluginId] });

  const handleToggle = async (p: PromptRecord) => {
    setPending(true);
    try {
      await promptsToggle(p.id, !p.enabled);
      await invalidate();
      toast.success(
        t(p.enabled ? "prompts.disableSuccess" : "prompts.enableSuccess"),
      );
    } catch (e) {
      toast.error(
        String(e) ||
          t(p.enabled ? "prompts.disableFailed" : "prompts.enableFailed"),
      );
    } finally {
      setPending(false);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    setPending(true);
    try {
      await promptsDelete(deleteTarget.id);
      toast.success(t("prompts.deleteSuccess"));
      await invalidate();
    } catch (e) {
      toast.error(String(e) || t("prompts.deleteFailed"));
    } finally {
      setPending(false);
      setDeleteTarget(null);
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<FileText className="h-5 w-5" />}
        title={t("features.promptsTitle")}
        subtitle={t("features.promptsSubtitle")}
      >
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setForm({ open: true, prompt: null })}
          disabled={pending}
        >
          <Plus className="h-3.5 w-3.5" />
          {t("prompts.add")}
        </Button>
      </PanelHeader>

      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("prompts.loading")}
          </CardContent>
        </Card>
      ) : prompts.length === 0 ? (
        <EmptyState
          icon={<FileText className="h-8 w-8" />}
          message={t("prompts.empty")}
        >
          <p className="text-xs text-muted-foreground">
            {t("prompts.emptyDescription")}
          </p>
        </EmptyState>
      ) : (
        <Card>
          {/* 计数条 + 当前启用 */}
          <div className="flex flex-wrap items-center gap-2 border-b border-border-default px-4 py-3 text-xs text-muted-foreground">
            <span className="font-medium text-foreground">
              {t("prompts.count", { count: prompts.length })}
            </span>
            <span>·</span>
            <span>
              {enabled
                ? t("prompts.enabledName", { name: enabled.name })
                : t("prompts.noneEnabled")}
            </span>
          </div>

          {/* 搜索 */}
          <div className="relative px-4 pt-3">
            <Search className="absolute left-7 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="pl-8"
              placeholder={t("prompts.searchPlaceholder")}
              aria-label={t("prompts.searchAriaLabel")}
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
              {t("prompts.noSearchResults")}
            </div>
          ) : (
            <ul className="divide-y divide-border">
              {filtered.map((p) => (
                <li
                  key={p.id}
                  className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/40"
                >
                  <Switch
                    checked={p.enabled}
                    disabled={pending}
                    onCheckedChange={() => void handleToggle(p)}
                    title={t(
                      p.enabled
                        ? "prompts.disableSuccess"
                        : "prompts.enableSuccess",
                    )}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{p.name}</div>
                    {p.description && (
                      <div className="truncate text-xs text-muted-foreground">
                        {p.description}
                      </div>
                    )}
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    title={t("common.edit")}
                    disabled={pending}
                    onClick={() => setForm({ open: true, prompt: p })}
                  >
                    <Edit3 className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    title={t("common.delete")}
                    disabled={pending}
                    onClick={() => setDeleteTarget(p)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}

      <PromptFormDialog
        open={form.open}
        pluginId={pluginId}
        pluginName={pluginName}
        filename={filename}
        prompt={form.prompt}
        onClose={() => setForm({ open: false, prompt: null })}
        onChanged={() => void invalidate()}
      />
      <ConfirmDialog
        isOpen={deleteTarget !== null}
        title={t("prompts.confirm.deleteTitle")}
        message={t("prompts.confirm.deleteMessage", {
          name: deleteTarget?.name,
        })}
        confirmText={t("common.delete")}
        pending={pending}
        onConfirm={() => void handleDelete()}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
