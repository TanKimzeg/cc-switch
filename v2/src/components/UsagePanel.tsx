import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { syncOpencodeUsage, usageDailySummary } from "@/lib/api";

export default function UsagePanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const summaryQuery = useQuery({
    queryKey: ["usage-daily", pluginId],
    queryFn: () => usageDailySummary(pluginId),
  });

  const handleSync = async () => {
    try {
      const count = await syncOpencodeUsage();
      await queryClient.invalidateQueries({ queryKey: ["usage-daily"] });
      toast.success(t("features.usageSynced", { count }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const rows = summaryQuery.data ?? [];
  const totalCost = rows.reduce((acc, r) => acc + (r.costUsd ?? 0), 0);

  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.usageTitle")}</h3>
        <button
          type="button"
          onClick={handleSync}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <RefreshCw className="h-3 w-3" />
          {t("features.usageSync")}
        </button>
      </div>
      <div className="flex items-center gap-3 rounded-lg border border-border px-3 py-2 text-xs">
        <span className="text-muted-foreground">{t("features.cost")}:</span>
        <span className="font-medium">${totalCost.toFixed(6)}</span>
      </div>
      {summaryQuery.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : rows.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.usageEmpty")}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border">
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border text-left text-muted-foreground">
                <th className="px-3 py-1.5">{t("features.usageDaily")}</th>
                <th className="px-3 py-1.5">model</th>
                <th className="px-3 py-1.5 text-right">
                  {t("features.requests")}
                </th>
                <th className="px-3 py-1.5 text-right">
                  {t("features.inputTokens")}
                </th>
                <th className="px-3 py-1.5 text-right">
                  {t("features.outputTokens")}
                </th>
                <th className="px-3 py-1.5 text-right">{t("features.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={i} className="border-b border-border last:border-0">
                  <td className="px-3 py-1.5">{r.day}</td>
                  <td className="px-3 py-1.5">{r.model}</td>
                  <td className="px-3 py-1.5 text-right">{r.requests}</td>
                  <td className="px-3 py-1.5 text-right">{r.inputTokens}</td>
                  <td className="px-3 py-1.5 text-right">{r.outputTokens}</td>
                  <td className="px-3 py-1.5 text-right">
                    ${r.costUsd.toFixed(6)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
