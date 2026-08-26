import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ArrowLeft, ExternalLink, Loader2, Plus, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { skillErrorText, parseRepoUrl } from "@/lib/skillsUtils";
import {
  skillsAddRepo,
  skillsDiscover,
  skillsListRepos,
  skillsRemoveRepo,
} from "@/lib/api";
import type { SkillRepo } from "@/types";

interface RepoManagerPanelProps {
  open: boolean;
  onClose: () => void;
  onChanged: () => void;
}

export function RepoManagerPanel({
  open,
  onClose,
  onChanged,
}: RepoManagerPanelProps) {
  const { t } = useTranslation();
  const [repos, setRepos] = useState<SkillRepo[]>([]);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [url, setUrl] = useState("");
  const [branch, setBranch] = useState("");
  const [urlError, setUrlError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [scanning, setScanning] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const [list, discovered] = await Promise.all([
        skillsListRepos(),
        skillsDiscover().catch(() => []),
      ]);
      setRepos(list);
      const c: Record<string, number> = {};
      for (const d of discovered) {
        const key = `${d.repoOwner}/${d.repoName}`;
        c[key] = (c[key] ?? 0) + 1;
      }
      setCounts(c);
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const handleAdd = async () => {
    const parsed = parseRepoUrl(url);
    if (!parsed) {
      setUrlError(t("skills.repoInvalidUrl"));
      return;
    }
    setUrlError(null);
    setScanning(true);
    try {
      const repo = await skillsAddRepo(
        parsed.owner,
        parsed.name,
        branch.trim() || "main",
      );
      await load();
      const count = counts[`${repo.owner}/${repo.name}`] ?? 0;
      toast.success(
        t("skills.repoAdded", {
          owner: repo.owner,
          name: repo.name,
          count,
        }),
      );
      setUrl("");
      setBranch("");
      onChanged();
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.repoAddFailed"));
    } finally {
      setScanning(false);
    }
  };

  const handleRemove = async (repo: SkillRepo) => {
    try {
      await skillsRemoveRepo(repo.owner, repo.name);
      toast.success(
        t("skills.repoRemoved", { owner: repo.owner, name: repo.name }),
      );
      setRepos((prev) => prev.filter((r) => r !== repo));
      onChanged();
    } catch (e) {
      toast.error(skillErrorText(t, e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent variant="fullscreen" className="overflow-y-auto">
        <DialogHeader className="flex-row items-center justify-between border-b border-border-default px-6 py-4">
          <div className="flex items-center gap-3">
            <Button
              variant="ghost"
              size="icon"
              onClick={onClose}
              title={t("common.close")}
            >
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <DialogTitle>{t("skills.repoTitle")}</DialogTitle>
          </div>
        </DialogHeader>

        <div className="mx-auto w-full max-w-3xl flex-1 space-y-6 px-6 py-6">
          {/* 添加仓库表单 */}
          <div className="rounded-xl border border-border-default p-6">
            <h3 className="text-sm font-semibold">{t("skills.addRepo")}</h3>
            <div className="mt-4 space-y-3">
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">
                  {t("skills.repoUrl")}
                </label>
                <Input
                  value={url}
                  onChange={(e) => {
                    setUrl(e.target.value);
                    setUrlError(null);
                  }}
                  placeholder={t("skills.repoUrlPlaceholder")}
                  onKeyDown={(e) => e.key === "Enter" && handleAdd()}
                />
                {urlError && (
                  <p className="mt-1 text-xs text-red-500">{urlError}</p>
                )}
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">
                  {t("skills.repoBranch")}
                </label>
                <Input
                  value={branch}
                  onChange={(e) => setBranch(e.target.value)}
                  placeholder={t("skills.repoBranchPlaceholder")}
                  onKeyDown={(e) => e.key === "Enter" && handleAdd()}
                />
              </div>
              <Button onClick={handleAdd} disabled={scanning}>
                {scanning ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Plus className="h-4 w-4" />
                )}
                {t("skills.repoAdd")}
              </Button>
            </div>
          </div>

          {/* 仓库列表 */}
          <div>
            <h3 className="text-sm font-semibold">{t("skills.repoList")}</h3>
            {loading ? (
              <div className="flex items-center gap-2 py-6 text-xs text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {t("skills.loading")}
              </div>
            ) : repos.length === 0 ? (
              <div className="py-6 text-center text-xs text-muted-foreground">
                {t("skills.repoEmpty")}
              </div>
            ) : (
              <div className="mt-3 space-y-2">
                {repos.map((repo) => {
                  const key = `${repo.owner}/${repo.name}`;
                  const count = counts[key];
                  return (
                    <div
                      key={key}
                      className="flex items-center justify-between gap-3 rounded-lg border border-border-default p-3"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium">
                          {repo.owner}/{repo.name}
                        </div>
                        <div className="mt-0.5 text-xs text-muted-foreground">
                          {t("skills.repoBranch")}: {repo.branch || "main"}
                          {typeof count === "number" && (
                            <Badge
                              variant="secondary"
                              className="ml-2 text-[10px]"
                            >
                              {t("skills.repoSkillCount", { count })}
                            </Badge>
                          )}
                        </div>
                      </div>
                      <div className="flex shrink-0 gap-1.5">
                        <Button
                          variant="ghost"
                          size="icon"
                          title={t("common.view")}
                          onClick={() =>
                            window.open(
                              `https://github.com/${repo.owner}/${repo.name}`,
                              "_blank",
                            )
                          }
                        >
                          <ExternalLink className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="text-red-500 hover:text-red-600"
                          title={t("common.delete")}
                          onClick={() => handleRemove(repo)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
