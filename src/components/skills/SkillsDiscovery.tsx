import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  Loader2,
  RefreshCw,
  Search,
  SearchX,
  Settings,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { skillErrorText, isSkillInstalled } from "@/lib/skillsUtils";
import { EmptyState } from "@/components/PanelHeader";
import { SkillCard } from "./SkillCard";
import { RepoManagerPanel } from "./RepoManagerPanel";
import {
  skillsDiscover,
  skillsInstallFromRepo,
  skillsListRepos,
  skillsSearchSkillsh,
} from "@/lib/api";
import type {
  DiscoverableSkill,
  SkillRecord,
  SkillsShDiscoverableSkill,
} from "@/types";

const SKILLSH_PAGE_SIZE = 20;

type Source = "repos" | "skillssh";

interface SkillsDiscoveryProps {
  currentPlugin: string;
  installed: SkillRecord[];
  onChanged: () => void;
  onBack: () => void;
}

export function SkillsDiscovery({
  currentPlugin,
  installed,
  onChanged,
  onBack,
}: SkillsDiscoveryProps) {
  const { t } = useTranslation();
  const [source, setSource] = useState<Source>("repos");
  const [repos, setRepos] = useState<{ owner: string; name: string }[]>([]);
  const [discoverable, setDiscoverable] = useState<DiscoverableSkill[]>([]);
  const [loading, setLoading] = useState(false);
  const [repoFilter, setRepoFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState("all");
  const [search, setSearch] = useState("");

  // skills.sh
  const [skillshQuery, setSkillshQuery] = useState("");
  const [skillshLoading, setSkillshLoading] = useState(false);
  const [skillshResults, setSkillshResults] = useState<
    SkillsShDiscoverableSkill[]
  >([]);
  const [skillshTotal, setSkillshTotal] = useState(0);
  const [skillshOffset, setSkillshOffset] = useState(0);

  const [repoManagerOpen, setRepoManagerOpen] = useState(false);
  const [busyKey, setBusyKey] = useState<string | null>(null);

  const loadReposAndDiscover = async () => {
    setLoading(true);
    try {
      const [repoList, discovered] = await Promise.all([
        skillsListRepos(),
        skillsDiscover().catch(() => []),
      ]);
      setRepos(repoList.map((r) => ({ owner: r.owner, name: r.name })));
      setDiscoverable(discovered);
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (source === "repos") void loadReposAndDiscover();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source]);

  // 无仓库时自动切到 skills.sh
  const effectiveSource: Source =
    source === "repos" && repos.length === 0 && !loading ? "skillssh" : source;

  const skillshQueryRef = useRef("");
  const doSearchSkillsh = async (query: string, offset: number) => {
    setSkillshLoading(true);
    try {
      const result = await skillsSearchSkillsh(
        query,
        SKILLSH_PAGE_SIZE,
        offset,
      );
      if (skillshQueryRef.current !== query) return;
      if (offset === 0) {
        setSkillshResults(result.skills);
      } else {
        setSkillshResults((prev) => {
          const seen = new Set(prev.map((s) => s.key));
          return [...prev, ...result.skills.filter((s) => !seen.has(s.key))];
        });
      }
      setSkillshTotal(result.totalCount);
      setSkillshOffset(offset + result.skills.length);
    } catch (e) {
      toast.error(skillErrorText(t, e));
    } finally {
      setSkillshLoading(false);
    }
  };

  const handleSearchSkillsh = () => {
    const q = skillshQuery.trim();
    if (q.length < 2) return;
    skillshQueryRef.current = q;
    setSkillshResults([]);
    setSkillshTotal(0);
    setSkillshOffset(0);
    void doSearchSkillsh(q, 0);
  };

  // 过滤（repos 模式）
  const filtered = useMemo(() => {
    let list = discoverable;
    if (repoFilter !== "all") {
      const [o, n] = repoFilter.split("/");
      list = list.filter((s) => s.repoOwner === o && s.repoName === n);
    }
    const q = search.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.directory.toLowerCase().includes(q) ||
          `${s.repoOwner}/${s.repoName}`.toLowerCase().includes(q),
      );
    }
    if (statusFilter === "installed") {
      list = list.filter((s) =>
        isSkillInstalled(installed, s.directory, s.repoOwner, s.repoName),
      );
    } else if (statusFilter === "uninstalled") {
      list = list.filter(
        (s) =>
          !isSkillInstalled(installed, s.directory, s.repoOwner, s.repoName),
      );
    }
    return list;
  }, [discoverable, repoFilter, statusFilter, search, installed]);

  const handleInstall = async (skill: DiscoverableSkill) => {
    setBusyKey(skill.key);
    try {
      const record = await skillsInstallFromRepo(skill, currentPlugin);
      toast.success(t("skills.installSuccess", { name: record.name }));
      onChanged();
    } catch (e) {
      toast.error(skillErrorText(t, e) || t("skills.installFailed"));
    } finally {
      setBusyKey(null);
    }
  };

  const handleUninstall = (_skill: DiscoverableSkill) => {
    // v1：发现面板卸载引导到主面板
    toast.info(t("skills.uninstallInMainPanel"));
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="icon" onClick={onBack}>
            <Settings className="hidden" />
            <span className="text-base">←</span>
          </Button>
          <div className="inline-flex items-center rounded-md border border-border-default p-0.5">
            {(
              [
                ["repos", t("skills.searchSourceRepos")],
                ["skillssh", t("skills.searchSourceSkillsh")],
              ] as [Source, string][]
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => setSource(id)}
                className={`rounded-md px-3 py-1 text-xs transition-colors ${
                  effectiveSource === id
                    ? "bg-primary/10 text-primary shadow-sm"
                    : "text-muted-foreground hover:bg-muted"
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          {effectiveSource === "repos" && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void loadReposAndDiscover()}
              disabled={loading}
            >
              {loading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              {t("skills.refresh")}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setRepoManagerOpen(true)}
          >
            <Settings className="h-3.5 w-3.5" />
            {t("skills.repoManager")}
          </Button>
        </div>
      </div>

      {effectiveSource === "repos" ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative min-w-[220px] flex-1">
              <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="pl-8"
                placeholder={t("skills.searchPlaceholder")}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>
            <Select value={repoFilter} onValueChange={setRepoFilter}>
              <SelectTrigger className="w-[160px]">
                <SelectValue placeholder={t("skills.filterRepo")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">
                  {t("skills.filterAllRepos")}
                </SelectItem>
                {repos.map((r) => (
                  <SelectItem
                    key={`${r.owner}/${r.name}`}
                    value={`${r.owner}/${r.name}`}
                  >
                    {r.owner}/{r.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-[130px]">
                <SelectValue placeholder={t("skills.filterPlaceholder")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("skills.filterAll")}</SelectItem>
                <SelectItem value="installed">
                  {t("skills.filterInstalled")}
                </SelectItem>
                <SelectItem value="uninstalled">
                  {t("skills.filterUninstalled")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          {search.trim() && (
            <p className="text-xs text-muted-foreground">
              {t("skills.count", { count: filtered.length })}
            </p>
          )}
          {loading ? (
            <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t("skills.loading")}
            </div>
          ) : filtered.length === 0 ? (
            <EmptyState
              icon={<Sparkles className="h-8 w-8" />}
              message={
                search.trim() || repoFilter !== "all" || statusFilter !== "all"
                  ? t("skills.noResults")
                  : t("skills.empty")
              }
            >
              {!search.trim() &&
                repoFilter === "all" &&
                statusFilter === "all" && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setRepoManagerOpen(true)}
                  >
                    {t("skills.addRepo")}
                  </Button>
                )}
            </EmptyState>
          ) : (
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
              {filtered.map((skill) => (
                <SkillCard
                  key={skill.key}
                  skill={skill}
                  installed={isSkillInstalled(
                    installed,
                    skill.directory,
                    skill.repoOwner,
                    skill.repoName,
                  )}
                  installing={busyKey === skill.key}
                  onInstall={() => void handleInstall(skill)}
                  onUninstall={() => void handleUninstall(skill)}
                  onView={() =>
                    skill.readmeUrl && window.open(skill.readmeUrl, "_blank")
                  }
                />
              ))}
            </div>
          )}
        </>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <div className="relative min-w-[220px] flex-1">
              <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="pl-8"
                placeholder={t("skills.skillshSearchPlaceholder")}
                value={skillshQuery}
                onChange={(e) => setSkillshQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSearchSkillsh()}
              />
            </div>
            <Button onClick={handleSearchSkillsh} disabled={skillshLoading}>
              {skillshLoading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Search className="h-3.5 w-3.5" />
              )}
              {t("skills.search")}
            </Button>
          </div>
          {skillshLoading && skillshResults.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t("skills.skillshLoading")}
            </div>
          ) : skillshResults.length === 0 ? (
            <EmptyState
              icon={<SearchX className="h-8 w-8" />}
              message={
                skillshQuery.trim()
                  ? t("skills.skillshNoResults", { query: skillshQuery })
                  : t("skills.skillshSearchPlaceholder")
              }
            />
          ) : (
            <>
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
                {skillshResults.map((skill) => (
                  <SkillCard
                    key={skill.key}
                    skill={skill}
                    installs={skill.installs}
                    installed={isSkillInstalled(
                      installed,
                      skill.directory,
                      skill.repoOwner,
                      skill.repoName,
                    )}
                    installing={busyKey === skill.key}
                    onInstall={() => void handleInstall(skill)}
                    onUninstall={() => void handleUninstall(skill)}
                    onView={() =>
                      skill.readmeUrl && window.open(skill.readmeUrl, "_blank")
                    }
                  />
                ))}
              </div>
              <div className="flex flex-col items-center gap-2">
                {skillshResults.length < skillshTotal && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      void doSearchSkillsh(
                        skillshQueryRef.current,
                        skillshOffset,
                      )
                    }
                    disabled={skillshLoading}
                  >
                    {skillshLoading ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : null}
                    {t("skills.loadMore")}
                  </Button>
                )}
                <p className="text-xs text-muted-foreground">
                  {t("skills.poweredBy")}
                </p>
              </div>
            </>
          )}
        </>
      )}

      <RepoManagerPanel
        open={repoManagerOpen}
        onClose={() => setRepoManagerOpen(false)}
        onChanged={() => void loadReposAndDiscover()}
      />
    </div>
  );
}
