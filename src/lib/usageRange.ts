import type { UsageRangePreset, UsageRangeSelection } from "@/types";

const DAY_SECONDS = 24 * 60 * 60;
const DAY_MS = DAY_SECONDS * 1000;

export interface ResolvedUsageRange {
  startDate: number;
  endDate: number;
}

function getStartOfLocalDayDate(nowMs: number): Date {
  const date = new Date(nowMs);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function resolveUsageRange(
  selection: UsageRangeSelection,
  nowMs: number = Date.now(),
): ResolvedUsageRange {
  const endDate = Math.floor(nowMs / 1000);

  switch (selection.preset) {
    case "today":
      return {
        startDate: Math.floor(getStartOfLocalDayDate(nowMs).getTime() / 1000),
        endDate,
      };
    case "1d":
      return { startDate: endDate - DAY_SECONDS, endDate };
    case "7d":
    case "14d":
    case "30d": {
      const dayCount =
        selection.preset === "7d" ? 7 : selection.preset === "14d" ? 14 : 30;
      return {
        startDate: Math.floor(
          getStartOfLocalDayDate(nowMs - (dayCount - 1) * DAY_MS).getTime() /
            1000,
        ),
        endDate,
      };
    }
    case "custom":
      return {
        startDate: selection.customStartDate ?? endDate - DAY_SECONDS,
        endDate: selection.customEndDate ?? endDate,
      };
  }
}

export function getUsageRangePresetLabel(
  preset: UsageRangePreset,
  t: (key: string, options?: { defaultValue?: string }) => string,
): string {
  switch (preset) {
    case "today":
      return t("features.usagePresetToday", { defaultValue: "Today" });
    case "1d":
      return t("features.usagePreset1d", { defaultValue: "1d" });
    case "7d":
      return t("features.usagePreset7d", { defaultValue: "7d" });
    case "14d":
      return t("features.usagePreset14d", { defaultValue: "14d" });
    case "30d":
      return t("features.usagePreset30d", { defaultValue: "30d" });
    case "custom":
      return t("features.usageCustomRange", { defaultValue: "Custom" });
  }
}

/** 返回 [start, end] 区间内的全部日期字符串（UTC，格式 YYYY-MM-DD）。 */
export function getRangeDayStrings(
  startDate: number,
  endDate: number,
): string[] {
  if (endDate < startDate) return [];
  const start = new Date(startDate * 1000);
  const end = new Date(endDate * 1000);
  const startDay = Date.UTC(
    start.getUTCFullYear(),
    start.getUTCMonth(),
    start.getUTCDate(),
  );
  const endDay = Date.UTC(
    end.getUTCFullYear(),
    end.getUTCMonth(),
    end.getUTCDate(),
  );
  const days: string[] = [];
  for (let t = startDay; t <= endDay; t += DAY_MS) {
    days.push(new Date(t).toISOString().slice(0, 10));
  }
  return days;
}

const HOUR_MS = 60 * 60 * 1000;

/** 返回 [start, end] 区间内的全部整点键（UTC，格式 YYYY-MM-DD HH:00）。 */
export function getRangeHourKeys(startDate: number, endDate: number): string[] {
  if (endDate < startDate) return [];
  const start = Math.floor(startDate / 3600) * 3600;
  const end = Math.floor(endDate / 3600) * 3600;
  const keys: string[] = [];
  for (let t = start; t <= end; t += HOUR_MS / 1000) {
    keys.push(
      new Date(t * 1000).toISOString().slice(0, 13).replace("T", " ") + ":00",
    );
  }
  return keys;
}

/** 请求日志时间戳（秒）→ 日期键（UTC YYYY-MM-DD）。 */
export function logDayKey(createdAt: number): string {
  return new Date(createdAt * 1000).toISOString().slice(0, 10);
}

/** 请求日志时间戳（秒）→ 小时键（UTC YYYY-MM-DD HH:00）。 */
export function logHourKey(createdAt: number): string {
  return (
    new Date(createdAt * 1000).toISOString().slice(0, 13).replace("T", " ") +
    ":00"
  );
}
