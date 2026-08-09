import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import { addProvider, updateProvider } from "@/lib/api";
import type { Provider } from "@/types";

const NPM_PACKAGES = [
  { value: "@ai-sdk/openai", label: "OpenAI Responses" },
  { value: "@ai-sdk/openai-compatible", label: "OpenAI Compatible" },
  { value: "@ai-sdk/anthropic", label: "Anthropic" },
  { value: "@ai-sdk/google", label: "Google (Gemini)" },
  { value: "@ai-sdk/amazon-bedrock", label: "Amazon Bedrock" },
];

interface ModelEntry {
  id: string;
  name: string;
  context: string;
  output: string;
}

function parseSettings(json: string): Record<string, unknown> {
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}

export default function ProviderForm({
  pluginId,
  existing,
  onDone,
}: {
  pluginId: string;
  existing?: Provider | null;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const isEdit = !!existing;

  const initialSettings = existing
    ? parseSettings(existing.settingsConfig ?? "")
    : {};

  const [id, setId] = useState(existing?.id ?? "");
  const [name, setName] = useState(existing?.name ?? "");
  const [npm, setNpm] = useState(
    String(initialSettings.npm ?? "@ai-sdk/openai-compatible"),
  );
  const [baseUrl, setBaseUrl] = useState(
    String((initialSettings.options as Record<string, unknown>)?.baseURL ?? ""),
  );
  const [apiKey, setApiKey] = useState(
    String((initialSettings.options as Record<string, unknown>)?.apiKey ?? ""),
  );
  const [headers, setHeaders] = useState<Record<string, string>>(
    () =>
      ((initialSettings.options as Record<string, unknown>)?.headers as
        Record<string, string> | undefined) ?? {},
  );
  const [models, setModels] = useState<Record<string, ModelEntry>>(() => {
    const raw =
      (initialSettings.models as Record<
        string,
        { name?: string; limit?: { context?: number; output?: number } }
      >) ?? {};
    const out: Record<string, ModelEntry> = {};
    for (const [mid, m] of Object.entries(raw)) {
      out[mid] = {
        id: mid,
        name: m.name ?? "",
        context: String(m.limit?.context ?? ""),
        output: String(m.limit?.output ?? ""),
      };
    }
    return out;
  });

  const updateModel = (mid: string, patch: Partial<ModelEntry>) => {
    setModels((prev) => ({ ...prev, [mid]: { ...prev[mid], ...patch } }));
  };

  const addModel = () => {
    const mid = `model-${Date.now()}`;
    setModels((prev) => ({
      ...prev,
      [mid]: { id: mid, name: "", context: "", output: "" },
    }));
  };

  const removeModel = (mid: string) => {
    setModels((prev) => {
      const next = { ...prev };
      delete next[mid];
      return next;
    });
  };

  const handleSave = async () => {
    if (!name.trim()) {
      toast.error(t("common.error"));
      return;
    }
    const options: Record<string, unknown> = {};
    if (baseUrl.trim()) options.baseURL = baseUrl.trim();
    if (apiKey.trim()) options.apiKey = apiKey.trim();
    if (Object.keys(headers).length > 0) options.headers = headers;

    const settingsConfig: Record<string, unknown> = { npm };
    settingsConfig.options = options;
    const modelsOut: Record<string, unknown> = {};
    for (const m of Object.values(models)) {
      const entry: Record<string, unknown> = {};
      if (m.name.trim()) entry.name = m.name.trim();
      const limit: Record<string, number> = {};
      if (m.context.trim()) limit.context = Number(m.context);
      if (m.output.trim()) limit.output = Number(m.output);
      if (Object.keys(limit).length > 0) entry.limit = limit;
      modelsOut[m.id] = entry;
    }
    if (Object.keys(modelsOut).length > 0) settingsConfig.models = modelsOut;

    const input = {
      pluginId,
      name: name.trim(),
      category: "custom",
      settingsConfig: JSON.stringify(settingsConfig),
      sortOrder: 0,
    };

    try {
      if (isEdit && existing) {
        await updateProvider(existing.id, input);
      } else {
        await addProvider(input);
      }
      toast.success(t("common.save"));
      onDone();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-3 rounded-lg border border-border p-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">
          {isEdit ? t("shell.providerEdit") : t("shell.providerAdd")}
        </h3>
        <button
          type="button"
          onClick={onDone}
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {!isEdit && (
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">ID</label>
          <input
            value={id}
            onChange={(e) => setId(e.target.value)}
            placeholder="provider-id"
            className="w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
          />
        </div>
      )}

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">
          {t("shell.providerName")}
        </label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
        />
      </div>

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">
          {t("shell.providerNpm")}
        </label>
        <select
          value={npm}
          onChange={(e) => setNpm(e.target.value)}
          className="w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
        >
          {NPM_PACKAGES.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">
          {t("shell.providerBaseUrl")}
        </label>
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.example.com/v1"
          className="w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
        />
      </div>

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">
          {t("shell.providerApiKey")}
        </label>
        <input
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          type="password"
          placeholder="sk-…"
          className="w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
        />
      </div>

      <div>
        <div className="mb-1 flex items-center justify-between">
          <label className="text-xs text-muted-foreground">
            {t("shell.providerHeaders")}
          </label>
          <button
            type="button"
            onClick={() =>
              setHeaders((h) => ({ ...h, [`h-${Date.now()}`]: "" }))
            }
            className="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-xs transition-colors hover:bg-accent"
          >
            <Plus className="h-3 w-3" />
          </button>
        </div>
        {Object.entries(headers).map(([k, v]) => (
          <div key={k} className="mb-1 flex items-center gap-1">
            <input
              value={k.startsWith("h-") ? "" : k}
              onChange={(e) => {
                const val = e.target.value.trim();
                setHeaders((prev) => {
                  const next = { ...prev };
                  if (val) {
                    next[val] = prev[k];
                    if (val !== k) delete next[k];
                  } else {
                    delete next[k];
                  }
                  return next;
                });
              }}
              placeholder="X-Header"
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <input
              value={v}
              onChange={(e) =>
                setHeaders((prev) => ({ ...prev, [k]: e.target.value }))
              }
              placeholder="value"
              className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <button
              type="button"
              onClick={() =>
                setHeaders((prev) => {
                  const next = { ...prev };
                  delete next[k];
                  return next;
                })
              }
              className="rounded p-1 text-muted-foreground hover:text-destructive"
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        ))}
      </div>

      <div>
        <div className="mb-1 flex items-center justify-between">
          <label className="text-xs text-muted-foreground">
            {t("shell.providerModels")}
          </label>
          <button
            type="button"
            onClick={addModel}
            className="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-xs transition-colors hover:bg-accent"
          >
            <Plus className="h-3 w-3" />
          </button>
        </div>
        {Object.values(models).map((m) => (
          <div
            key={m.id}
            className="mb-1 grid grid-cols-[1fr_1fr_1fr_auto] gap-1"
          >
            <input
              value={m.id.startsWith("model-") ? "" : m.id}
              onChange={(e) =>
                updateModel(m.id, { id: e.target.value.trim() || m.id })
              }
              placeholder={t("shell.providerModelId")}
              className="rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <input
              value={m.name}
              onChange={(e) => updateModel(m.id, { name: e.target.value })}
              placeholder={t("shell.providerModelName")}
              className="rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <input
              value={m.context}
              onChange={(e) => updateModel(m.id, { context: e.target.value })}
              placeholder="context"
              className="rounded-md border border-border bg-background px-2 py-1 text-xs"
            />
            <button
              type="button"
              onClick={() => removeModel(m.id)}
              className="rounded p-1 text-muted-foreground hover:text-destructive"
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        ))}
      </div>

      <button
        type="button"
        onClick={handleSave}
        className="w-full rounded-md bg-primary px-2 py-1.5 text-sm text-primary-foreground transition-colors hover:bg-primary/90"
      >
        {t("common.save")}
      </button>
    </div>
  );
}
