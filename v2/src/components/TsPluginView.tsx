import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, TerminalSquare } from "lucide-react";
import { loadTsPluginById, type TsPluginExports } from "@/lib/plugin-loader";
import type { InstalledPlugin } from "@/types";

/**
 * TS 插件视图：加载并显示 TypeScript 插件导出的能力，供开发调试与验证。
 * 实际的生产面板将由各能力对应的通用组件驱动（后续接入）。
 */
export default function TsPluginView({ plugin }: { plugin: InstalledPlugin }) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<TsPluginExports | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (plugin.entryType !== "ts" || !plugin.main) return;
    let cancelled = false;
    loadTsPluginById(plugin.id, plugin.main)
      .then((p) => {
        if (!cancelled) setLoaded(p);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [plugin.id, plugin.main, plugin.entryType]);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <TerminalSquare className="h-5 w-5 text-blue-500" />
        <h2 className="text-lg font-semibold">
          {plugin.name} · TypeScript 插件
        </h2>
      </div>

      {error ? (
        <p className="text-sm text-destructive">
          {t("common.error")}: {error}
        </p>
      ) : !loaded ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("common.loading")}
        </p>
      ) : (
        <div className="space-y-3">
          <div className="rounded-lg border border-border p-4">
            <div className="text-sm font-medium">{loaded.id}</div>
            <div className="mt-2 flex flex-wrap gap-2">
              {Object.entries(loaded.capabilities)
                .filter(([, v]) => v)
                .map(([key]) => (
                  <span
                    key={key}
                    className="rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary"
                  >
                    {key}
                  </span>
                ))}
            </div>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("features.tsPluginHint")}
          </p>
        </div>
      )}
    </div>
  );
}
