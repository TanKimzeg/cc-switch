import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Download, ExternalLink, Loader2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DiscoverableSkill } from "@/types";

interface SkillCardProps {
  skill: DiscoverableSkill;
  /** skills.sh 安装量（仅 skills.sh 结果展示）。 */
  installs?: number;
  installed: boolean;
  installing?: boolean;
  uninstalling?: boolean;
  onInstall?: () => void;
  onUninstall?: () => void;
  onView?: () => void;
}

export function SkillCard({
  skill,
  installs,
  installed,
  installing = false,
  uninstalling = false,
  onInstall,
  onUninstall,
  onView,
}: SkillCardProps) {
  const { t } = useTranslation();
  const showDirectory =
    skill.directory.toLowerCase() !== skill.name.toLowerCase();

  return (
    <Card className="flex flex-col overflow-hidden">
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <CardTitle className="text-sm leading-snug">{skill.name}</CardTitle>
          {installed && (
            <Badge className="bg-green-600/90 text-white">
              {t("skills.installed")}
            </Badge>
          )}
        </div>
        <CardDescription className="line-clamp-2 text-xs">
          {showDirectory && (
            <span className="mr-1 font-mono">{skill.directory}</span>
          )}
        </CardDescription>
        <div className="flex flex-wrap items-center gap-1.5">
          <Badge variant="outline" className="text-[10px]">
            {skill.repoOwner}/{skill.repoName}
          </Badge>
          {typeof installs === "number" && installs >= 0 && (
            <Badge variant="secondary" className="text-[10px]">
              <Download className="mr-1 h-2.5 w-2.5" />
              {installs.toLocaleString()}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent className="pb-2 text-xs text-muted-foreground">
        <p className="line-clamp-2">{skill.description}</p>
      </CardContent>
      <CardFooter className="mt-auto gap-1.5">
        {onView && skill.readmeUrl && (
          <Button
            variant="ghost"
            size="sm"
            className="flex-1"
            onClick={onView}
            title={t("skills.view")}
          >
            <ExternalLink className="h-3.5 w-3.5" />
            {t("skills.view")}
          </Button>
        )}
        {installed ? (
          <Button
            variant="outline"
            size="sm"
            className="flex-1 text-red-500 hover:text-red-600"
            onClick={onUninstall}
            disabled={uninstalling || installing}
          >
            {uninstalling ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Trash2 className="h-3.5 w-3.5" />
            )}
            {t("skills.uninstall")}
          </Button>
        ) : (
          <Button
            variant="mcp"
            size="sm"
            className="flex-1"
            onClick={onInstall}
            disabled={installing || uninstalling}
          >
            {installing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            {t("skills.install")}
          </Button>
        )}
      </CardFooter>
    </Card>
  );
}
