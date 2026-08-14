import { useState } from "react";
import { useTranslation } from "react-i18next";
import { fmtInt, fmtUsd, getLocaleFromLanguage } from "@/lib/formatters";

export interface TrendPoint {
  day: string;
  requests: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  cost: number;
}

const W = 600;
const H = 260;
const PAD = { top: 24, right: 48, bottom: 22, left: 44 };

const COST_COLOR = "#f43f5e";

function dayLabel(key: string, locale: string): string {
  if (key.includes(":")) {
    // 小时键 "YYYY-MM-DD HH:00"（UTC）
    const date = new Date(key.replace(" ", "T") + ":00Z");
    if (Number.isNaN(date.getTime())) return key;
    return date.toLocaleString(locale, {
      hour: "2-digit",
      minute: "2-digit",
    });
  }
  const date = new Date(`${key}T00:00:00`);
  if (Number.isNaN(date.getTime())) return key;
  return date.toLocaleDateString(locale, {
    month: "2-digit",
    day: "2-digit",
  });
}

/** Catmull-Rom → cubic Bézier，生成平滑曲线路径。 */
function smoothPath(pts: [number, number][]): string {
  if (pts.length === 0) return "";
  if (pts.length === 1) return `M ${pts[0][0]},${pts[0][1]}`;
  let d = `M ${pts[0][0].toFixed(2)},${pts[0][1].toFixed(2)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];
    const cp1x = p1[0] + (p2[0] - p0[0]) / 6;
    const cp1y = p1[1] + (p2[1] - p0[1]) / 6;
    const cp2x = p2[0] - (p3[0] - p1[0]) / 6;
    const cp2y = p2[1] - (p3[1] - p1[1]) / 6;
    d += ` C ${cp1x.toFixed(2)},${cp1y.toFixed(2)} ${cp2x.toFixed(
      2,
    )},${cp2y.toFixed(2)} ${p2[0].toFixed(2)},${p2[1].toFixed(2)}`;
  }
  return d;
}

function niceTicks(max: number, count = 4): number[] {
  if (max <= 0) return [0];
  const raw = max / count;
  const mag = Math.pow(10, Math.floor(Math.log10(raw)));
  const norm = raw / mag;
  const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
  const ticks: number[] = [];
  let v = 0;
  while (v <= max + 1e-9) {
    ticks.push(Math.round(v * 100) / 100);
    v += step;
  }
  return ticks;
}

function tickLabel(value: number): string {
  if (value >= 1000) return `${(value / 1000).toFixed(0)}k`;
  return value.toString();
}

export default function UsageTrendChart({
  dayPoints,
}: {
  dayPoints: TrendPoint[];
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage || i18n.language || "en";
  const locale = getLocaleFromLanguage(lang);

  if (dayPoints.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        {t("features.usageEmpty")}
      </p>
    );
  }

  const series = [
    {
      key: "input",
      label: t("features.usageInput"),
      color: "#3b82f6",
    },
    {
      key: "output",
      label: t("features.usageOutput"),
      color: "#22c55e",
    },
    {
      key: "cacheWrite",
      label: t("features.usageCacheWrite"),
      color: "#f97316",
    },
    {
      key: "cacheRead",
      label: t("features.usageCacheRead"),
      color: "#a855f7",
    },
  ] as const;

  const n = dayPoints.length;
  const plotW = W - PAD.left - PAD.right;
  const plotH = H - PAD.top - PAD.bottom;
  const baseline = PAD.top + plotH;

  const maxTokens = Math.max(
    1,
    ...dayPoints.flatMap((p) => series.map((s) => p[s.key] as number)),
  );
  const maxCost = Math.max(1e-9, ...dayPoints.map((p) => p.cost));

  const x = (i: number) =>
    PAD.left + (n === 1 ? plotW / 2 : (i / (n - 1)) * plotW);
  const yToken = (v: number) => PAD.top + (1 - v / maxTokens) * plotH;
  const yCost = (v: number) => PAD.top + (1 - v / maxCost) * plotH;

  const tokenTicks = niceTicks(maxTokens);
  const costTicks = niceTicks(maxCost);

  const labelEvery = Math.ceil(n / 8);
  const labels = dayPoints
    .map((p, i) => ({ p, i }))
    .filter(({ i }) => i === 0 || i === n - 1 || i % labelEvery === 0);

  const costPath = smoothPath(
    dayPoints.map((p, i) => [x(i), yCost(p.cost)] as [number, number]),
  );

  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const hoverPoint = hoverIdx != null ? dayPoints[hoverIdx] : null;
  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(
      Math.max((e.clientX - rect.left) / rect.width, 0),
      1,
    );
    setHoverIdx(Math.round(ratio * (n - 1)));
  };
  const tooltipLeft =
    hoverIdx != null ? Math.min(Math.max((x(hoverIdx) / W) * 100, 15), 85) : 0;

  return (
    <div
      className="relative cursor-crosshair"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverIdx(null)}
    >
      <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-1">
        {series.map((s) => (
          <span
            key={s.key}
            className="flex items-center gap-1.5 text-xs text-muted-foreground"
          >
            <span
              className="h-2 w-2 rounded-full"
              style={{ background: s.color }}
            />
            {s.label}
          </span>
        ))}
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span
            className="h-0 w-3 border-t-2 border-dashed"
            style={{ borderColor: COST_COLOR }}
          />
          {t("features.cost")}
        </span>
      </div>

      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="h-auto w-full"
        role="img"
        aria-label={t("features.usageDailyTrend")}
      >
        <defs>
          {series.map((s) => (
            <linearGradient
              key={s.key}
              id={`usage-area-${s.key}`}
              x1="0"
              y1="0"
              x2="0"
              y2="1"
            >
              <stop offset="5%" stopColor={s.color} stopOpacity="0.25" />
              <stop offset="95%" stopColor={s.color} stopOpacity="0" />
            </linearGradient>
          ))}
        </defs>

        {tokenTicks.map((v) => {
          const gy = yToken(v);
          return (
            <g key={v}>
              <line
                x1={PAD.left}
                y1={gy}
                x2={W - PAD.right}
                y2={gy}
                stroke="hsl(var(--border))"
                strokeDasharray="3 3"
                opacity="0.4"
              />
              <text
                x={PAD.left - 6}
                y={gy + 3}
                textAnchor="end"
                fontSize={10}
                fill="hsl(var(--muted-foreground))"
              >
                {tickLabel(v)}
              </text>
            </g>
          );
        })}

        {costTicks.map((v) => (
          <text
            key={v}
            x={W - PAD.right + 6}
            y={yCost(v) + 3}
            textAnchor="start"
            fontSize={10}
            fill="hsl(var(--muted-foreground))"
          >
            {fmtUsd(v, v < 0.01 ? 4 : 2)}
          </text>
        ))}

        <line
          x1={PAD.left}
          y1={baseline}
          x2={W - PAD.right}
          y2={baseline}
          stroke="hsl(var(--border))"
          strokeWidth={1}
        />

        {series.map((s) => {
          const pts = dayPoints.map(
            (p, i) => [x(i), yToken(p[s.key] as number)] as [number, number],
          );
          const line = smoothPath(pts);
          const area = `${line} L ${pts[n - 1][0].toFixed(2)},${baseline} L ${pts[0][0].toFixed(2)},${baseline} Z`;
          return (
            <g key={s.key}>
              <path d={area} fill={`url(#usage-area-${s.key})`} />
              <path
                d={line}
                fill="none"
                stroke={s.color}
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </g>
          );
        })}

        <path
          d={costPath}
          fill="none"
          stroke={COST_COLOR}
          strokeWidth={2}
          strokeDasharray="4 4"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {hoverIdx != null && hoverPoint && (
          <>
            <line
              x1={x(hoverIdx)}
              y1={PAD.top}
              x2={x(hoverIdx)}
              y2={baseline}
              stroke="hsl(var(--muted-foreground))"
              strokeOpacity={0.5}
              strokeWidth={1}
              strokeDasharray="3 3"
            />
            {series.map((s) => (
              <circle
                key={s.key}
                cx={x(hoverIdx)}
                cy={yToken(hoverPoint[s.key] as number)}
                r={3}
                fill={s.color}
                stroke="hsl(var(--card))"
                strokeWidth={1.5}
              />
            ))}
            <circle
              cx={x(hoverIdx)}
              cy={yCost(hoverPoint.cost)}
              r={3}
              fill={COST_COLOR}
              stroke="hsl(var(--card))"
              strokeWidth={1.5}
            />
          </>
        )}
      </svg>

      {hoverPoint && hoverIdx != null && (
        <div
          className="pointer-events-none absolute top-2 z-10 -translate-x-1/2 rounded-lg border border-border bg-background/95 px-3 py-2 text-xs shadow-lg backdrop-blur"
          style={{ left: `${tooltipLeft}%` }}
        >
          <div className="mb-1.5 whitespace-nowrap font-medium">
            {dayLabel(hoverPoint.day, locale)}
          </div>
          {series.map((s) => (
            <div
              key={s.key}
              className="flex items-center justify-between gap-4"
            >
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <span
                  className="h-2 w-2 rounded-full"
                  style={{ background: s.color }}
                />
                {s.label}
              </span>
              <span className="font-medium tabular-nums">
                {fmtInt(hoverPoint[s.key] as number, locale)}
              </span>
            </div>
          ))}
          <div className="mt-1 flex items-center justify-between gap-4 border-t border-border pt-1">
            <span className="flex items-center gap-1.5 text-muted-foreground">
              <span
                className="h-0 w-3 border-t-2 border-dashed"
                style={{ borderColor: COST_COLOR }}
              />
              {t("features.cost")}
            </span>
            <span className="font-medium tabular-nums">
              {fmtUsd(hoverPoint.cost, 4)}
            </span>
          </div>
        </div>
      )}

      <div className="relative mt-1 h-4">
        {labels.map(({ p, i }) => (
          <span
            key={i}
            className="absolute -translate-x-1/2 text-[10px] text-muted-foreground"
            style={{ left: `${(x(i) / W) * 100}%` }}
          >
            {dayLabel(p.day, locale)}
          </span>
        ))}
      </div>
    </div>
  );
}
