import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  ArrowDownToLine,
  ArrowUpFromLine,
  BarChart3,
  ChevronLeft,
  ChevronRight,
  Coins,
  Database,
  ListFilter,
  Loader2,
  RefreshCw,
  Sparkles,
  Zap,
} from "lucide-react";
import { toast } from "sonner";
import { pluginSyncUsage, usageListRequestLogs } from "@/lib/api";
import {
  formatTokensShort,
  fmtInt,
  fmtUsd,
  getLocaleFromLanguage,
  parseFiniteNumber,
} from "@/lib/formatters";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import UsageTrendChart, { type TrendPoint } from "@/components/UsageTrendChart";
import { UsageDateRangePicker } from "@/components/UsageDateRangePicker";
import {
  getRangeDayStrings,
  getRangeHourKeys,
  getUsageRangePresetLabel,
  logDayKey,
  logHourKey,
  resolveUsageRange,
} from "@/lib/usageRange";
import type { UsageRangeSelection } from "@/types";

const DAY_SECONDS = 86400;
const LOG_FETCH_LIMIT = 10000;
const LOG_PAGE_SIZE = 20;

interface ModelStat {
  model: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  costUsd: number;
}

function zeroPoint(day: string): TrendPoint {
  return {
    day,
    requests: 0,
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    cost: 0,
  };
}

/** 生成分页页码列表（含省略号）。 */
function pageNumbers(current: number, total: number): (number | string)[] {
  const pages: (number | string)[] = [];
  if (total <= 9) {
    for (let i = 0; i < total; i++) pages.push(i);
    return pages;
  }
  const set = new Set<number>();
  for (let i = 0; i < 3; i++) set.add(i);
  for (let i = total - 3; i < total; i++) set.add(i);
  for (
    let i = Math.max(0, current - 1);
    i <= Math.min(total - 1, current + 1);
    i++
  )
    set.add(i);
  const sorted = Array.from(set).sort((a, b) => a - b);
  for (let i = 0; i < sorted.length; i++) {
    if (i > 0 && sorted[i] - sorted[i - 1] > 1) pages.push(`e-${i}`);
    pages.push(sorted[i]);
  }
  return pages;
}

export default function UsagePanel({ pluginId }: { pluginId: string }) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const lang = i18n.resolvedLanguage || i18n.language || "en";
  const locale = getLocaleFromLanguage(lang);
  const [range, setRange] = useState<UsageRangeSelection>({ preset: "30d" });

  const logsQuery = useQuery({
    queryKey: ["usage-logs", pluginId],
    queryFn: () => usageListRequestLogs(pluginId, LOG_FETCH_LIMIT),
  });

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["usage-logs", pluginId] });

  const handleSync = async () => {
    try {
      const count = await pluginSyncUsage(pluginId);
      await invalidate();
      toast.success(t("features.usageSynced", { count }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const logs = logsQuery.data ?? [];

  const resolvedRange = useMemo(() => resolveUsageRange(range), [range]);
  const isHourly =
    resolvedRange.endDate - resolvedRange.startDate <= DAY_SECONDS;
  const rangeKeys = useMemo(
    () =>
      isHourly
        ? getRangeHourKeys(resolvedRange.startDate, resolvedRange.endDate)
        : getRangeDayStrings(resolvedRange.startDate, resolvedRange.endDate),
    [isHourly, resolvedRange],
  );
  const rangeLabel = useMemo(() => {
    if (range.preset !== "custom") {
      return getUsageRangePresetLabel(range.preset, t);
    }
    const fmt = (ts: number) =>
      new Date(ts * 1000).toLocaleDateString(locale, {
        year: "2-digit",
        month: "2-digit",
        day: "2-digit",
      });
    return `${fmt(resolvedRange.startDate)} - ${fmt(resolvedRange.endDate)}`;
  }, [range, resolvedRange, t, locale]);

  const rangeLogs = useMemo(
    () =>
      logs.filter(
        (l) =>
          l.createdAt >= resolvedRange.startDate &&
          l.createdAt <= resolvedRange.endDate,
      ),
    [logs, resolvedRange],
  );

  const totals = useMemo(() => {
    let requests = 0;
    let input = 0;
    let output = 0;
    let cacheRead = 0;
    let cacheWrite = 0;
    let cost = 0;
    for (const l of rangeLogs) {
      requests += 1;
      input += l.inputTokens;
      output += l.outputTokens;
      cacheRead += l.cacheReadTokens;
      cacheWrite += l.cacheCreationTokens;
      cost += parseFiniteNumber(l.totalCostUsd) ?? 0;
    }
    return { requests, input, output, cacheRead, cacheWrite, cost };
  }, [rangeLogs]);

  const dayPoints = useMemo<TrendPoint[]>(() => {
    const map = new Map<string, TrendPoint>();
    for (const l of rangeLogs) {
      const key = isHourly ? logHourKey(l.createdAt) : logDayKey(l.createdAt);
      const p = map.get(key) ?? zeroPoint(key);
      p.requests += 1;
      p.input += l.inputTokens;
      p.output += l.outputTokens;
      p.cacheRead += l.cacheReadTokens;
      p.cacheWrite += l.cacheCreationTokens;
      p.cost += parseFiniteNumber(l.totalCostUsd) ?? 0;
      map.set(key, p);
    }
    return rangeKeys.map((key) => map.get(key) ?? zeroPoint(key));
  }, [rangeLogs, rangeKeys, isHourly]);

  const modelStats = useMemo<ModelStat[]>(() => {
    const map = new Map<string, ModelStat>();
    for (const l of rangeLogs) {
      const m =
        map.get(l.model) ??
        ({
          model: l.model,
          requests: 0,
          inputTokens: 0,
          outputTokens: 0,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          costUsd: 0,
        } as ModelStat);
      m.requests += 1;
      m.inputTokens += l.inputTokens;
      m.outputTokens += l.outputTokens;
      m.cacheReadTokens += l.cacheReadTokens;
      m.cacheCreationTokens += l.cacheCreationTokens;
      m.costUsd += parseFiniteNumber(l.totalCostUsd) ?? 0;
      map.set(l.model, m);
    }
    return Array.from(map.values()).sort((a, b) => b.costUsd - a.costUsd);
  }, [rangeLogs]);

  const [page, setPage] = useState(0);
  useEffect(() => {
    setPage(0);
  }, [resolvedRange.startDate, resolvedRange.endDate]);
  const totalLogs = rangeLogs.length;
  const totalPages = Math.max(1, Math.ceil(totalLogs / LOG_PAGE_SIZE));
  const safePage = Math.min(page, totalPages - 1);
  const pageLogs = useMemo(
    () =>
      rangeLogs.slice(safePage * LOG_PAGE_SIZE, (safePage + 1) * LOG_PAGE_SIZE),
    [rangeLogs, safePage],
  );

  const totalTokens =
    totals.input + totals.output + totals.cacheRead + totals.cacheWrite;
  const cacheableInput = totals.input + totals.cacheWrite + totals.cacheRead;
  const hitRate =
    cacheableInput > 0 ? (totals.cacheRead / cacheableInput) * 100 : 0;
  const hitRateLabel = hitRate.toFixed(hitRate >= 99.95 ? 0 : 1);

  if (logsQuery.isLoading) {
    return (
      <Card>
        <CardContent className="flex min-h-[220px] items-center justify-center">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground/50" />
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {t("features.usageSubtitle")}
        </p>
        <div className="flex items-center gap-2">
          <UsageDateRangePicker
            selection={range}
            onApply={setRange}
            triggerLabel={rangeLabel}
          />
          <button
            type="button"
            onClick={handleSync}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2.5 py-1.5 text-xs transition-colors hover:bg-accent"
          >
            <RefreshCw className="h-3 w-3" />
            {t("features.usageSync")}
          </button>
        </div>
      </div>

      {logs.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center gap-3 py-14 text-center">
            <Zap className="h-8 w-8 text-muted-foreground/30" />
            <p className="text-sm text-muted-foreground">
              {t("features.usageEmptyDetail")}
            </p>
            <button
              type="button"
              onClick={handleSync}
              className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
            >
              <RefreshCw className="h-3 w-3" />
              {t("features.usageSync")}
            </button>
          </CardContent>
        </Card>
      ) : (
        <>
          <Card className="relative overflow-hidden">
            <CardContent className="p-4 md:p-5">
              <div className="flex flex-col gap-4">
                <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
                  <div className="flex items-center gap-3">
                    <div className="rounded-xl bg-primary/10 p-2.5">
                      <Zap className="h-5 w-5 text-primary" />
                    </div>
                    <div>
                      <div className="mb-0.5 text-xs font-medium text-muted-foreground">
                        {t("features.usageTotalTokens")}
                      </div>
                      <div className="flex items-baseline gap-2">
                        <span className="text-2xl font-bold leading-none tracking-tight tabular-nums md:text-3xl">
                          {totalTokens.toLocaleString(locale)}
                        </span>
                        <span className="rounded-md bg-muted/40 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                          ≈ {formatTokensShort(totalTokens, lang, 2)}
                        </span>
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-5 rounded-xl border bg-muted/30 px-4 py-2.5 shadow-sm">
                    <div className="flex flex-col">
                      <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                        {t("features.usageTotalRequests")}
                      </span>
                      <span className="flex items-center gap-1.5 text-sm font-semibold tabular-nums">
                        <Activity className="h-3.5 w-3.5 text-blue-500" />
                        {fmtInt(totals.requests, locale)}
                      </span>
                    </div>
                    <div className="h-8 w-px bg-border/60" />
                    <div className="flex flex-col">
                      <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                        {t("features.usageTotalCost")}
                      </span>
                      <span className="flex items-center gap-1.5 text-sm font-semibold tabular-nums text-green-500">
                        <Coins className="h-3.5 w-3.5" />
                        {fmtUsd(totals.cost, 4)}
                      </span>
                    </div>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
                  <MiniStat
                    icon={<ArrowDownToLine className="h-3.5 w-3.5" />}
                    label={t("features.usageInput")}
                    value={formatTokensShort(totals.input, lang)}
                    accent="text-blue-500"
                  />
                  <MiniStat
                    icon={<ArrowUpFromLine className="h-3.5 w-3.5" />}
                    label={t("features.usageOutput")}
                    value={formatTokensShort(totals.output, lang)}
                    accent="text-purple-500"
                  />
                  <MiniStat
                    icon={<Database className="h-3.5 w-3.5" />}
                    label={t("features.usageCacheWrite")}
                    value={formatTokensShort(totals.cacheWrite, lang)}
                    accent="text-amber-500"
                  />
                  <MiniStat
                    icon={<Sparkles className="h-3.5 w-3.5" />}
                    label={t("features.usageCacheRead")}
                    value={formatTokensShort(totals.cacheRead, lang)}
                    accent="text-emerald-500"
                  />
                  <div className="col-span-2 flex flex-col justify-center rounded-xl border bg-muted/20 p-3 shadow-sm lg:col-span-1">
                    <div className="mb-2 flex items-center justify-between text-[11px]">
                      <span className="font-medium text-muted-foreground">
                        {t("features.usageCacheHitRate")}
                      </span>
                      <span className="font-bold tabular-nums text-emerald-500">
                        {hitRateLabel}%
                      </span>
                    </div>
                    <div className="relative h-1.5 overflow-hidden rounded-full bg-muted/60">
                      <div
                        className="absolute inset-y-0 left-0 rounded-full bg-emerald-500 transition-all duration-700 ease-out"
                        style={{ width: `${hitRate}%` }}
                      />
                    </div>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent className="p-4 md:p-5">
              <div className="mb-4 flex items-center justify-between">
                <h3 className="text-sm font-medium">
                  {t("features.usageDailyTrend")}
                </h3>
                <span className="text-xs text-muted-foreground">
                  {rangeLabel}
                </span>
              </div>
              <UsageTrendChart dayPoints={dayPoints} />
            </CardContent>
          </Card>

          <Tabs defaultValue="logs" className="w-full">
            <TabsList>
              <TabsTrigger value="logs" className="gap-2">
                <ListFilter className="h-4 w-4" />
                {t("features.usageLogs")}
              </TabsTrigger>
              <TabsTrigger value="models" className="gap-2">
                <BarChart3 className="h-4 w-4" />
                {t("features.usageModelStats")}
              </TabsTrigger>
            </TabsList>

            <TabsContent value="logs" className="mt-3">
              <Card>
                <CardHeader className="p-4 pb-0 md:p-5 md:pb-0">
                  <CardTitle className="text-sm font-medium">
                    {t("features.usageLogs")}
                  </CardTitle>
                  <CardDescription>
                    {t("features.usageLogsCount", {
                      count: totalLogs,
                    })}
                  </CardDescription>
                </CardHeader>
                <CardContent className="p-4 pt-4 md:p-5">
                  {logsQuery.isLoading ? (
                    <p className="text-xs text-muted-foreground">
                      {t("common.loading")}
                    </p>
                  ) : pageLogs.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {t("features.usageEmpty")}
                    </p>
                  ) : (
                    <>
                      <div className="overflow-x-auto">
                        <Table>
                          <TableHeader>
                            <TableRow>
                              <TableHead>{t("features.usageTime")}</TableHead>
                              <TableHead>{t("features.usageModel")}</TableHead>
                              <TableHead className="text-right">
                                {t("features.inputTokens")}
                              </TableHead>
                              <TableHead className="text-right">
                                {t("features.outputTokens")}
                              </TableHead>
                              <TableHead className="text-right">
                                {t("features.usageCache")}
                              </TableHead>
                              <TableHead className="text-right">
                                {t("features.cost")}
                              </TableHead>
                              <TableHead>
                                {t("features.usageSession")}
                              </TableHead>
                            </TableRow>
                          </TableHeader>
                          <TableBody>
                            {pageLogs.map((log) => {
                              const cacheParts = [
                                log.cacheReadTokens > 0 &&
                                  `R${fmtInt(log.cacheReadTokens, locale)}`,
                                log.cacheCreationTokens > 0 &&
                                  `W${fmtInt(log.cacheCreationTokens, locale)}`,
                              ].filter(Boolean);
                              return (
                                <TableRow key={log.requestId}>
                                  <TableCell className="whitespace-nowrap text-xs tabular-nums">
                                    {new Date(
                                      log.createdAt * 1000,
                                    ).toLocaleString(locale, {
                                      month: "2-digit",
                                      day: "2-digit",
                                      hour: "2-digit",
                                      minute: "2-digit",
                                    })}
                                  </TableCell>
                                  <TableCell className="font-medium">
                                    {log.model}
                                  </TableCell>
                                  <TableCell className="text-right tabular-nums">
                                    {fmtInt(log.inputTokens, locale)}
                                  </TableCell>
                                  <TableCell className="text-right tabular-nums">
                                    {fmtInt(log.outputTokens, locale)}
                                  </TableCell>
                                  <TableCell className="text-right text-xs text-muted-foreground">
                                    {cacheParts.length > 0
                                      ? cacheParts.join("·")
                                      : "—"}
                                  </TableCell>
                                  <TableCell className="text-right tabular-nums">
                                    {fmtUsd(log.totalCostUsd, 4)}
                                  </TableCell>
                                  <TableCell className="max-w-[160px] truncate text-xs text-muted-foreground">
                                    {log.sessionId ?? "—"}
                                  </TableCell>
                                </TableRow>
                              );
                            })}
                          </TableBody>
                        </Table>
                      </div>
                      <div className="mt-4 flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
                        <span>
                          {t("features.usageLogsTotal", { total: totalLogs })}
                        </span>
                        <div className="flex items-center gap-1">
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={safePage === 0}
                            onClick={() => setPage((p) => Math.max(0, p - 1))}
                          >
                            <ChevronLeft className="h-4 w-4" />
                          </Button>
                          {pageNumbers(safePage, totalPages).map((p) =>
                            typeof p === "number" ? (
                              <Button
                                key={p}
                                type="button"
                                variant={p === safePage ? "default" : "outline"}
                                size="sm"
                                className="h-8 w-8 p-0"
                                onClick={() => setPage(p)}
                              >
                                {p + 1}
                              </Button>
                            ) : (
                              <span key={p} className="px-1">
                                …
                              </span>
                            ),
                          )}
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={safePage >= totalPages - 1}
                            onClick={() =>
                              setPage((p) => Math.min(totalPages - 1, p + 1))
                            }
                          >
                            <ChevronRight className="h-4 w-4" />
                          </Button>
                        </div>
                      </div>
                    </>
                  )}
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="models" className="mt-3">
              <Card>
                <CardHeader className="p-4 pb-0 md:p-5 md:pb-0">
                  <CardTitle className="text-sm font-medium">
                    {t("features.usageModelStats")}
                  </CardTitle>
                  <CardDescription>
                    {t("features.usageModelCount", {
                      count: modelStats.length,
                    })}
                  </CardDescription>
                </CardHeader>
                <CardContent className="p-4 pt-4 md:p-5">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>{t("features.usageModel")}</TableHead>
                        <TableHead className="text-right">
                          {t("features.requests")}
                        </TableHead>
                        <TableHead className="text-right">
                          {t("features.inputTokens")}
                        </TableHead>
                        <TableHead className="text-right">
                          {t("features.outputTokens")}
                        </TableHead>
                        <TableHead className="text-right">
                          {t("features.usageCache")}
                        </TableHead>
                        <TableHead className="text-right">
                          {t("features.cost")}
                        </TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {modelStats.map((m) => (
                        <TableRow key={m.model}>
                          <TableCell className="font-medium">
                            {m.model}
                          </TableCell>
                          <TableCell className="text-right tabular-nums">
                            {fmtInt(m.requests, locale)}
                          </TableCell>
                          <TableCell className="text-right tabular-nums">
                            {fmtInt(m.inputTokens, locale)}
                          </TableCell>
                          <TableCell className="text-right tabular-nums">
                            {fmtInt(m.outputTokens, locale)}
                          </TableCell>
                          <TableCell className="text-right tabular-nums">
                            {formatTokensShort(
                              m.cacheReadTokens + m.cacheCreationTokens,
                              lang,
                            )}
                          </TableCell>
                          <TableCell className="text-right tabular-nums">
                            {fmtUsd(m.costUsd, 4)}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                    <TableFooter>
                      <TableRow>
                        <TableCell>{t("features.usageTotal")}</TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmtInt(totals.requests, locale)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmtInt(totals.input, locale)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmtInt(totals.output, locale)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {formatTokensShort(
                            totals.cacheRead + totals.cacheWrite,
                            lang,
                          )}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmtUsd(totals.cost, 4)}
                        </TableCell>
                      </TableRow>
                    </TableFooter>
                  </Table>
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
        </>
      )}
    </div>
  );
}

function MiniStat({
  icon,
  label,
  value,
  accent,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  accent: string;
}) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border bg-muted/20 p-3 shadow-sm">
      <div
        className={`flex items-center gap-1.5 text-[11px] font-medium ${accent}`}
      >
        {icon}
        <span className="tracking-wide text-foreground/70">{label}</span>
      </div>
      <div className="text-sm font-semibold tabular-nums">{value}</div>
    </div>
  );
}
