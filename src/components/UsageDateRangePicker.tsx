import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CalendarDays,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { getLocaleFromLanguage } from "@/lib/formatters";
import { getUsageRangePresetLabel, resolveUsageRange } from "@/lib/usageRange";
import type { UsageRangePreset, UsageRangeSelection } from "@/types";

type DraftField = "start" | "end";

const PRESETS: UsageRangePreset[] = ["today", "1d", "7d", "14d", "30d"];

function toTs(d: Date): number {
  return Math.floor(d.getTime() / 1000);
}

function fromTs(ts: number): Date {
  return new Date(ts * 1000);
}

function startOfMonth(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), 1);
}

function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function fmtDate(ts: number): string {
  const d = fromTs(ts);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
    2,
    "0",
  )}-${String(d.getDate()).padStart(2, "0")}`;
}

function parseDateInput(value: string): number {
  const [y, m, d] = value.split("-").map(Number);
  if (!Number.isFinite(y) || !Number.isFinite(m) || !Number.isFinite(d))
    return 0;
  return toTs(new Date(y, m - 1, d));
}

function getCalendarDays(month: Date): Date[] {
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const gridStart = new Date(first);
  gridStart.setDate(first.getDate() - first.getDay());
  return Array.from({ length: 42 }, (_, i) => {
    const d = new Date(gridStart);
    d.setDate(gridStart.getDate() + i);
    return d;
  });
}

interface UsageDateRangePickerProps {
  selection: UsageRangeSelection;
  onApply: (selection: UsageRangeSelection) => void;
  triggerLabel: string;
}

export function UsageDateRangePicker({
  selection,
  onApply,
  triggerLabel,
}: UsageDateRangePickerProps) {
  const { t, i18n } = useTranslation();
  const [open, setOpen] = useState(false);
  const resolvedRange = useMemo(
    () => resolveUsageRange(selection),
    [selection],
  );
  const [draftStart, setDraftStart] = useState(resolvedRange.startDate);
  const [draftEnd, setDraftEnd] = useState(resolvedRange.endDate);
  const [activeField, setActiveField] = useState<DraftField>("start");
  const [displayMonth, setDisplayMonth] = useState(() =>
    startOfMonth(fromTs(resolvedRange.startDate)),
  );

  const locale = getLocaleFromLanguage(
    i18n.resolvedLanguage || i18n.language || "en",
  );

  useEffect(() => {
    if (!open) return;
    const r = resolveUsageRange(selection);
    setDraftStart(r.startDate);
    setDraftEnd(r.endDate);
    setDisplayMonth(startOfMonth(fromTs(r.startDate)));
    setActiveField("start");
  }, [open, selection]);

  const calendarDays = useMemo(
    () => getCalendarDays(displayMonth),
    [displayMonth],
  );
  const weekdayLabels = useMemo(
    () =>
      Array.from({ length: 7 }, (_, i) =>
        new Intl.DateTimeFormat(locale, { weekday: "narrow" }).format(
          new Date(2024, 0, 7 + i),
        ),
      ),
    [locale],
  );

  const startDay = fromTs(draftStart);
  const endDay = fromTs(draftEnd);
  const today = new Date();

  const handleDayPick = (day: Date) => {
    const ts = toTs(new Date(day.getFullYear(), day.getMonth(), day.getDate()));
    if (activeField === "start") {
      setDraftStart(ts);
      if (ts > draftEnd) setDraftEnd(ts);
      setActiveField("end");
    } else if (ts < draftStart) {
      setDraftStart(ts);
      setActiveField("end");
    } else {
      setDraftEnd(ts);
    }
    if (
      day.getMonth() !== displayMonth.getMonth() ||
      day.getFullYear() !== displayMonth.getFullYear()
    ) {
      setDisplayMonth(new Date(day.getFullYear(), day.getMonth(), 1));
    }
  };

  const handleApply = () => {
    if (draftStart > draftEnd) return;
    onApply({
      preset: "custom",
      customStartDate: draftStart,
      customEndDate: draftEnd,
    });
    setOpen(false);
  };

  const renderField = (field: DraftField) => {
    const isActive = activeField === field;
    const ts = field === "start" ? draftStart : draftEnd;
    const setTs = field === "start" ? setDraftStart : setDraftEnd;
    const label =
      field === "start"
        ? t("features.usageStartDate", { defaultValue: "Start date" })
        : t("features.usageEndDate", { defaultValue: "End date" });
    return (
      <div
        className={cn(
          "cursor-pointer rounded-lg border px-3 py-2 transition-all",
          isActive
            ? "border-primary bg-primary/5 ring-1 ring-primary/30"
            : "border-border/50 hover:border-border",
        )}
        onClick={() => setActiveField(field)}
      >
        <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {label}
        </div>
        <Input
          type="date"
          className="h-7 flex-1 border-0 bg-transparent p-0 text-sm shadow-none focus-visible:ring-0"
          value={fmtDate(ts)}
          onChange={(e) => {
            const next = parseDateInput(e.target.value);
            if (next === 0) return;
            setTs(next);
            const d = fromTs(next);
            setDisplayMonth(new Date(d.getFullYear(), d.getMonth(), 1));
          }}
          onFocus={() => setActiveField(field)}
        />
      </div>
    );
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant={selection.preset === "custom" ? "default" : "outline"}
          className="h-9 justify-start gap-1.5 text-xs"
          title={triggerLabel}
        >
          <CalendarDays className="h-4 w-4 shrink-0" />
          <span className="flex-1 truncate">{triggerLabel}</span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        className="w-[620px] max-w-[calc(100vw-2rem)] p-3"
        align="end"
      >
        <div className="flex flex-wrap gap-1.5 border-b border-border/40 pb-2">
          {PRESETS.map((preset) => (
            <Button
              key={preset}
              type="button"
              size="sm"
              variant={selection.preset === preset ? "default" : "outline"}
              className="h-7 px-2.5 text-xs"
              onClick={() => {
                onApply({ preset });
                setOpen(false);
              }}
            >
              {getUsageRangePresetLabel(preset, t)}
            </Button>
          ))}
        </div>

        <div className="flex flex-col gap-3 pt-3 lg:flex-row">
          <div className="flex-1 space-y-2">
            {renderField("start")}
            {renderField("end")}
            <div className="flex gap-2 pt-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="flex-1"
                onClick={() => setOpen(false)}
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                size="sm"
                className="flex-1"
                onClick={handleApply}
              >
                {t("common.save")}
              </Button>
            </div>
          </div>

          <div className="rounded-lg border border-border/50 bg-muted/30 p-2.5">
            <div className="mb-1.5 flex items-center justify-between">
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() =>
                  setDisplayMonth(
                    new Date(
                      displayMonth.getFullYear(),
                      displayMonth.getMonth() - 1,
                      1,
                    ),
                  )
                }
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </Button>
              <button
                type="button"
                className="text-sm font-medium transition-colors hover:text-primary"
                onClick={() =>
                  setDisplayMonth(
                    new Date(today.getFullYear(), today.getMonth(), 1),
                  )
                }
              >
                {displayMonth.toLocaleDateString(locale, {
                  year: "numeric",
                  month: "long",
                })}
              </button>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() =>
                  setDisplayMonth(
                    new Date(
                      displayMonth.getFullYear(),
                      displayMonth.getMonth() + 1,
                      1,
                    ),
                  )
                }
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
            </div>

            <div className="mb-0.5 grid grid-cols-7 text-center text-[11px] text-muted-foreground">
              {weekdayLabels.map((label, i) => (
                <div key={i} className="py-0.5">
                  {label}
                </div>
              ))}
            </div>

            <div className="grid grid-cols-7 gap-px">
              {calendarDays.map((day) => {
                const isCurrentMonth =
                  day.getMonth() === displayMonth.getMonth();
                const isToday = isSameDay(day, today);
                const isStart = isSameDay(day, startDay);
                const isEnd = isSameDay(day, endDay);
                const dayStart = new Date(
                  day.getFullYear(),
                  day.getMonth(),
                  day.getDate(),
                );
                const startOfStart = new Date(
                  startDay.getFullYear(),
                  startDay.getMonth(),
                  startDay.getDate(),
                );
                const startOfEnd = new Date(
                  endDay.getFullYear(),
                  endDay.getMonth(),
                  endDay.getDate(),
                );
                const inRange =
                  dayStart >= startOfStart && dayStart <= startOfEnd;
                const isEndpoint = isStart || isEnd;

                return (
                  <button
                    key={day.toISOString()}
                    type="button"
                    aria-label={day.toLocaleDateString(locale)}
                    aria-pressed={isEndpoint}
                    className={cn(
                      "relative h-7 rounded text-xs transition-colors",
                      !isCurrentMonth && "text-muted-foreground/30",
                      isCurrentMonth && !inRange && "hover:bg-muted",
                      inRange && !isEndpoint && "bg-primary/10 text-primary",
                      isEndpoint &&
                        "bg-primary font-medium text-primary-foreground",
                      isToday && !isEndpoint && "ring-1 ring-primary/40",
                    )}
                    onClick={() => handleDayPick(day)}
                  >
                    {day.getDate()}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
