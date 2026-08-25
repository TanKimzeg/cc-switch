import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronDown,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  Undo2,
} from "lucide-react";
import { toast } from "sonner";
import {
  pricingDelete,
  pricingList,
  pricingSyncModelsDev,
  pricingUpsert,
  usageRecomputeCosts,
} from "@/lib/api";
import type { ModelPricing } from "@/types";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ConfirmDialog";

function emptyPricing(): ModelPricing {
  return {
    id: `user:${Date.now()}`,
    modelMatch: "",
    providerScope: null,
    displayName: "",
    inputCostPerMillion: "",
    outputCostPerMillion: "",
    cacheReadCostPerMillion: "0",
    cacheCreationCostPerMillion: "0",
    offPeakDiscountPercent: null,
    offPeakStart: null,
    offPeakEnd: null,
    source: "user",
    updatedAt: 0,
  };
}

export default function ModelPricingSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ModelPricing | null>(null);
  const [deleting, setDeleting] = useState<ModelPricing | null>(null);
  const [syncing, setSyncing] = useState(false);

  const query = useQuery({ queryKey: ["pricing"], queryFn: pricingList });
  const rows = query.data ?? [];

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["pricing"] });

  const handleSave = async () => {
    if (!editing) return;
    try {
      await pricingUpsert(editing);
      await invalidate();
      setEditing(null);
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (p: ModelPricing) => {
    try {
      await pricingDelete(p.id);
      await invalidate();
      toast.success(t("common.delete"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeleting(null);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    try {
      const [synced, skipped] = await pricingSyncModelsDev();
      await invalidate();
      toast.success(t("usage.pricing.synced", { synced, skipped }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSyncing(false);
    }
  };

  const handleBackfill = async () => {
    try {
      const n = await usageRecomputeCosts();
      await queryClient.invalidateQueries({ queryKey: ["usage-logs"] });
      toast.success(t("usage.pricing.backfilled", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <Card>
      <CardContent className="p-4">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="flex w-full items-center justify-between text-sm font-medium"
        >
          <span className="inline-flex items-center gap-1.5">
            <ChevronDown
              className={`h-4 w-4 transition-transform ${open ? "rotate-180" : ""}`}
            />
            {t("usage.pricing.title")}
            <span className="text-xs text-muted-foreground">
              ({rows.length})
            </span>
          </span>
        </button>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("usage.pricing.description")}
        </p>

        {open && (
          <div className="mt-3 space-y-3">
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={() => setEditing(emptyPricing())}
              >
                <Plus className="h-3.5 w-3.5" />
                {t("usage.pricing.add")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={syncing}
                onClick={() => void handleSync()}
              >
                <RefreshCw
                  className={`h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`}
                />
                {t("usage.pricing.sync")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => void handleBackfill()}
              >
                <Undo2 className="h-3.5 w-3.5" />
                {t("usage.pricing.backfill")}
              </Button>
            </div>

            {rows.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {t("usage.pricing.empty")}
              </p>
            ) : (
              <div className="overflow-x-auto rounded-md border border-border">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-border bg-muted/40 text-left text-muted-foreground">
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.model")}
                      </th>
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.input")}
                      </th>
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.output")}
                      </th>
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.cacheRead")}
                      </th>
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.offPeak")}
                      </th>
                      <th className="px-2 py-1.5">
                        {t("usage.pricing.source")}
                      </th>
                      <th className="px-2 py-1.5" />
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((p) => (
                      <tr
                        key={p.id}
                        className="border-b border-border last:border-0"
                      >
                        <td className="px-2 py-1.5">
                          <div className="font-medium">{p.modelMatch}</div>
                          {p.providerScope && (
                            <div className="text-muted-foreground">
                              @{p.providerScope}
                            </div>
                          )}
                        </td>
                        <td className="px-2 py-1.5">{p.inputCostPerMillion}</td>
                        <td className="px-2 py-1.5">
                          {p.outputCostPerMillion}
                        </td>
                        <td className="px-2 py-1.5">
                          {p.cacheReadCostPerMillion}
                        </td>
                        <td className="px-2 py-1.5">
                          {p.offPeakDiscountPercent != null
                            ? `-${p.offPeakDiscountPercent}% (${p.offPeakStart ?? ""}-${p.offPeakEnd ?? ""} UTC)`
                            : "—"}
                        </td>
                        <td className="px-2 py-1.5 text-muted-foreground">
                          {p.source}
                        </td>
                        <td className="px-2 py-1.5">
                          <div className="flex shrink-0 justify-end gap-0.5">
                            <button
                              type="button"
                              onClick={() => setEditing({ ...p })}
                              className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                              title={t("common.edit")}
                            >
                              <Pencil className="h-3.5 w-3.5" />
                            </button>
                            <button
                              type="button"
                              onClick={() => setDeleting(p)}
                              className="rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                              title={t("common.delete")}
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </CardContent>

      {editing && (
        <PricingEditDialog
          value={editing}
          onChange={setEditing}
          onSave={() => void handleSave()}
          onCancel={() => setEditing(null)}
        />
      )}

      <ConfirmDialog
        isOpen={deleting !== null}
        title={t("usage.pricing.deleteTitle")}
        message={
          deleting
            ? t("usage.pricing.deleteMessage", { name: deleting.modelMatch })
            : ""
        }
        confirmText={t("common.delete")}
        cancelText={t("common.cancel")}
        variant="destructive"
        onConfirm={() => deleting && void handleDelete(deleting)}
        onCancel={() => setDeleting(null)}
      />
    </Card>
  );
}

function PricingEditDialog({
  value,
  onChange,
  onSave,
  onCancel,
}: {
  value: ModelPricing;
  onChange: (v: ModelPricing) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const patch = (p: Partial<ModelPricing>) => onChange({ ...value, ...p });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onCancel}
    >
      <div
        className="mx-4 max-h-[85vh] w-full max-w-lg space-y-3 overflow-y-auto rounded-xl border border-border bg-card p-5 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-semibold">
          {t("usage.pricing.editTitle")}
        </h3>

        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">
            {t("usage.pricing.modelMatch")}
          </span>
          <Input
            value={value.modelMatch}
            onChange={(e) => patch({ modelMatch: e.target.value })}
            placeholder="deepseek-chat"
          />
        </label>

        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">
            {t("usage.pricing.providerScope")}
          </span>
          <Input
            value={value.providerScope ?? ""}
            onChange={(e) =>
              patch({ providerScope: e.target.value.trim() || null })
            }
            placeholder={t("usage.pricing.providerScopeHint")}
          />
        </label>

        <div className="grid grid-cols-2 gap-2">
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">
              {t("usage.pricing.input")}
            </span>
            <Input
              value={value.inputCostPerMillion}
              onChange={(e) => patch({ inputCostPerMillion: e.target.value })}
              placeholder="0.27"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">
              {t("usage.pricing.output")}
            </span>
            <Input
              value={value.outputCostPerMillion}
              onChange={(e) => patch({ outputCostPerMillion: e.target.value })}
              placeholder="1.1"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">
              {t("usage.pricing.cacheRead")}
            </span>
            <Input
              value={value.cacheReadCostPerMillion}
              onChange={(e) =>
                patch({ cacheReadCostPerMillion: e.target.value })
              }
              placeholder="0.07"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">
              {t("usage.pricing.cacheCreation")}
            </span>
            <Input
              value={value.cacheCreationCostPerMillion}
              onChange={(e) =>
                patch({ cacheCreationCostPerMillion: e.target.value })
              }
              placeholder="0"
            />
          </label>
        </div>

        <div className="rounded-md border border-border p-3">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={value.offPeakDiscountPercent != null}
              onChange={(e) =>
                patch({
                  offPeakDiscountPercent: e.target.checked ? 50 : null,
                  offPeakStart: e.target.checked
                    ? (value.offPeakStart ?? "16:30")
                    : null,
                  offPeakEnd: e.target.checked
                    ? (value.offPeakEnd ?? "00:30")
                    : null,
                })
              }
            />
            {t("usage.pricing.offPeakEnable")}
          </label>
          {value.offPeakDiscountPercent != null && (
            <div className="mt-2 grid grid-cols-3 gap-2">
              <label className="block space-y-1">
                <span className="text-xs text-muted-foreground">
                  {t("usage.pricing.discount")}
                </span>
                <Input
                  type="number"
                  min={1}
                  max={100}
                  value={value.offPeakDiscountPercent}
                  onChange={(e) =>
                    patch({
                      offPeakDiscountPercent:
                        Number.parseInt(e.target.value, 10) || null,
                    })
                  }
                />
              </label>
              <label className="block space-y-1">
                <span className="text-xs text-muted-foreground">
                  {t("usage.pricing.startUtc")}
                </span>
                <Input
                  value={value.offPeakStart ?? ""}
                  onChange={(e) => patch({ offPeakStart: e.target.value })}
                  placeholder="16:30"
                />
              </label>
              <label className="block space-y-1">
                <span className="text-xs text-muted-foreground">
                  {t("usage.pricing.endUtc")}
                </span>
                <Input
                  value={value.offPeakEnd ?? ""}
                  onChange={(e) => patch({ offPeakEnd: e.target.value })}
                  placeholder="00:30"
                />
              </label>
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" onClick={onSave}>
            {t("common.save")}
          </Button>
        </div>
      </div>
    </div>
  );
}
