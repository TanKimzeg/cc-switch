import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronDown,
  Download,
  Pencil,
  Plus,
  Puzzle,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import {
  getPlugins,
  importMcpServersFromAllPlugins,
  mcpDelete,
  mcpList,
  mcpToggleApp,
  mcpUpsert,
} from "@/lib/api";
import { getMcpSearchText, parseSmartMcpJson } from "@/lib/mcpUtils";
import mcpPresets from "@/config/mcpPresets";
import type { McpServer } from "@/types";
import { PanelHeader, EmptyState } from "@/components/PanelHeader";
import { Card, CardContent } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import JsonEditor from "@/components/JsonEditor";
import { ConfirmDialog } from "@/components/ConfirmDialog";

type RemoteType = "sse" | "http";

function specToForm(spec: Record<string, unknown>): {
  type: "stdio" | RemoteType;
  command: string;
  args: string;
  env: string;
  url: string;
  headers: string;
} {
  const type =
    (spec["type"] as string) === "stdio"
      ? "stdio"
      : (spec["type"] as string) === "http"
        ? "http"
        : (spec["type"] as string) === "sse"
          ? "sse"
          : "stdio";
  if (type === "stdio") {
    const env = (spec["env"] as Record<string, string>) ?? {};
    return {
      type,
      command: String(spec["command"] ?? ""),
      args: ((spec["args"] as string[]) ?? []).join("\n"),
      env: Object.entries(env)
        .map(([k, v]) => `${k}=${v}`)
        .join("\n"),
      url: "",
      headers: "",
    };
  }
  const headers = (spec["headers"] as Record<string, string>) ?? {};
  return {
    type,
    command: "",
    args: "",
    env: "",
    url: String(spec["url"] ?? ""),
    headers: Object.entries(headers)
      .map(([k, v]) => `${k}: ${v}`)
      .join("\n"),
  };
}

export default function McpGlobalPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const writeLock = useRef(false);

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formId, setFormId] = useState("");
  const [formName, setFormName] = useState("");
  const [mcpTab, setMcpTab] = useState<"structured" | "raw">("structured");
  const [mcpType, setMcpType] = useState<"stdio" | RemoteType>("stdio");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpEnv, setMcpEnv] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpHeaders, setMcpHeaders] = useState("");
  const [rawSpec, setRawSpec] = useState("{}");
  const [enabledApps, setEnabledApps] = useState<Record<string, boolean>>({});
  const [metaOpen, setMetaOpen] = useState(false);
  const [formDesc, setFormDesc] = useState("");
  const [formHomepage, setFormHomepage] = useState("");
  const [formDocs, setFormDocs] = useState("");
  const [formTags, setFormTags] = useState("");

  const [search, setSearch] = useState("");
  const [deleting, setDeleting] = useState<McpServer | null>(null);
  const [bulkPluginId, setBulkPluginId] = useState("");

  const query = useQuery({ queryKey: ["mcp-all"], queryFn: mcpList });
  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: getPlugins });
  const servers = query.data ?? [];
  const plugins = pluginsQuery.data ?? [];

  const filtered = search.trim()
    ? servers.filter((s) =>
        getMcpSearchText(s).includes(search.trim().toLowerCase()),
      )
    : servers;

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["mcp-all"] });

  const buildSpec = (validate: boolean): Record<string, unknown> | null => {
    if (mcpType === "stdio") {
      if (!mcpCommand.trim()) {
        if (validate) toast.error(t("mcp.error.commandRequired"));
        return null;
      }
      const spec: Record<string, unknown> = {
        type: "stdio",
        command: mcpCommand.trim(),
      };
      const args = mcpArgs
        .split(/\r?\n/)
        .map((a) => a.trim())
        .filter(Boolean);
      if (args.length > 0) spec.args = args;
      const env = parseKeyValueLines(mcpEnv, "=");
      if (Object.keys(env).length > 0) spec.env = env;
      return spec;
    }
    if (!mcpUrl.trim()) {
      if (validate) toast.error(t("mcp.error.commandRequired"));
      return null;
    }
    const spec: Record<string, unknown> = { type: mcpType, url: mcpUrl.trim() };
    const headers = parseKeyValueLines(mcpHeaders, ":");
    if (Object.keys(headers).length > 0) spec.headers = headers;
    return spec;
  };

  const resetForm = () => {
    setEditingId(null);
    setFormId("");
    setFormName("");
    setMcpType("stdio");
    setMcpCommand("");
    setMcpArgs("");
    setMcpEnv("");
    setMcpUrl("");
    setMcpHeaders("");
    setRawSpec("{}");
    setEnabledApps({});
    setMetaOpen(false);
    setFormDesc("");
    setFormHomepage("");
    setFormDocs("");
    setFormTags("");
  };

  const openAdd = () => {
    if (!showForm) resetForm();
    setShowForm((v) => !v);
  };

  const openEdit = (s: McpServer) => {
    resetForm();
    setEditingId(s.id);
    setFormId(s.id);
    setFormName(s.name);
    const f = specToForm(s.spec);
    setMcpType(f.type);
    setMcpCommand(f.command);
    setMcpArgs(f.args);
    setMcpEnv(f.env);
    setMcpUrl(f.url);
    setMcpHeaders(f.headers);
    setRawSpec(JSON.stringify(s.spec, null, 2));
    for (const [pid, en] of s.apps) enabledApps[pid] = en;
    setEnabledApps(Object.fromEntries(s.apps));
    if (s.description || s.homepage || s.docs || (s.tags?.length ?? 0) > 0) {
      setMetaOpen(true);
    }
    setFormDesc(s.description ?? "");
    setFormHomepage(s.homepage ?? "");
    setFormDocs(s.docs ?? "");
    setFormTags((s.tags ?? []).join(", "));
    setShowForm(true);
  };

  /** 智能粘贴：识别 mcpServers 包装 / 单键包装，自动回填 id/name。 */
  const handleRawSpecChange = (value: string) => {
    setRawSpec(value);
    if (!value.trim()) return;
    try {
      const parsed = JSON.parse(value);
      // 整份配置误粘（含 mcpServers 包装）时直接改写编辑器内容为首个条目
      if (
        parsed &&
        typeof parsed === "object" &&
        !Array.isArray(parsed) &&
        "mcpServers" in parsed
      ) {
        const result = parseSmartMcpJson(value);
        setRawSpec(JSON.stringify(result.config, null, 2));
        if (result.id && !formId.trim() && !editingId) {
          setFormId(result.id);
          if (!formName.trim()) setFormName(result.id);
        }
        toast.info(t("mcp.smartPasteDetected"));
      }
    } catch {
      // 输入过程中的中间态（不完整 JSON）不报错
    }
  };

  const handleUpsert = async () => {
    if (writeLock.current) return;
    const id = formId.trim();
    if (!id) {
      toast.error(t("mcp.error.idRequired"));
      return;
    }
    if (!editingId && servers.some((s) => s.id === id)) {
      toast.error(t("mcp.error.idExists"));
      return;
    }
    let spec: Record<string, unknown>;
    if (mcpTab === "structured") {
      const built = buildSpec(true);
      if (!built) return;
      spec = built;
    } else {
      try {
        const result = parseSmartMcpJson(rawSpec || "{}");
        if (
          !result.config ||
          typeof result.config !== "object" ||
          Array.isArray(result.config)
        ) {
          toast.error(t("mcp.error.jsonInvalid"));
          return;
        }
        spec = result.config;
      } catch {
        toast.error(t("mcp.error.jsonInvalid"));
        return;
      }
    }
    const apps = plugins.map((p): [string, boolean] => [
      p.id,
      enabledApps[p.id] ?? false,
    ]);
    writeLock.current = true;
    try {
      await mcpUpsert({
        id,
        name: formName.trim() || id,
        spec,
        description: formDesc.trim() || null,
        homepage: formHomepage.trim() || null,
        docs: formDocs.trim() || null,
        tags: formTags
          .split(/[,，]/)
          .map((s) => s.trim())
          .filter(Boolean),
        apps,
      });
      await invalidate();
      setShowForm(false);
      resetForm();
      toast.success(t("common.save"));
    } catch (e) {
      toast.error(`${t("mcp.error.saveFailed")}: ${String(e)}`);
    } finally {
      writeLock.current = false;
    }
  };

  const handleDelete = async (s: McpServer) => {
    if (writeLock.current) return;
    writeLock.current = true;
    try {
      await mcpDelete(s.id);
      await invalidate();
      toast.success(t("common.delete"));
    } catch (e) {
      toast.error(`${t("mcp.error.deleteFailed")}: ${String(e)}`);
    } finally {
      writeLock.current = false;
      setDeleting(null);
    }
  };

  const handleToggleApp = async (
    id: string,
    pluginId: string,
    enabled: boolean,
  ) => {
    try {
      await mcpToggleApp(id, pluginId, enabled);
      await invalidate();
    } catch (e) {
      toast.error(String(e));
    }
  };

  /** 按 app 批量一键开关全部服务器（对齐 v1，串行执行）。 */
  const handleBulkToggle = async (pluginId: string, enabled: boolean) => {
    if (writeLock.current || !pluginId) return;
    writeLock.current = true;
    let failed = 0;
    try {
      for (const s of servers) {
        const current = s.apps.find(([pid]) => pid === pluginId)?.[1] ?? false;
        if (current === enabled) continue;
        try {
          await mcpToggleApp(s.id, pluginId, enabled);
        } catch {
          failed += 1;
        }
      }
      await invalidate();
      if (failed > 0) {
        toast.error(t("mcp.bulkToggleFailed", { count: failed }));
      }
    } finally {
      writeLock.current = false;
    }
  };

  const applyPreset = (presetId: string) => {
    const preset = mcpPresets.find((p) => p.id === presetId);
    if (!preset) return;
    const f = specToForm(preset.server as Record<string, unknown>);
    setMcpType(f.type);
    setMcpCommand(f.command);
    setMcpArgs(f.args);
    setMcpEnv(f.env);
    setMcpUrl(f.url);
    setMcpHeaders(f.headers);
    if (!formName.trim()) setFormName(preset.name);
    if (!formId.trim()) setFormId(preset.id);
  };

  const handleImport = async () => {
    try {
      const n = await importMcpServersFromAllPlugins();
      await invalidate();
      toast.success(t("features.mcpImported", { count: n }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="space-y-4">
      <PanelHeader
        icon={<Puzzle className="h-5 w-5" />}
        title={t("nav.mcp")}
        subtitle={t("features.mcpSubtitle")}
      >
        <button
          type="button"
          onClick={handleImport}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
        >
          <Download className="h-3 w-3" />
          {t("features.mcpImport")}
        </button>
        <button
          type="button"
          onClick={openAdd}
          className="inline-flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Plus className="h-3.5 w-3.5" />
          {t("features.mcpAdd")}
        </button>
      </PanelHeader>

      {showForm && (
        <div className="space-y-3 rounded-xl border border-border bg-card p-4 shadow-sm">
          <div className="text-sm font-semibold">
            {editingId ? t("mcp.editServer") : t("mcp.addServer")}
          </div>

          {/* 预设 chips */}
          {!editingId && (
            <div className="flex flex-wrap gap-1.5">
              {mcpPresets.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  title={t(`mcp.presets.${p.id}.description`)}
                  onClick={() => applyPreset(p.id)}
                  className="rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/10 hover:text-primary"
                >
                  {t(`mcp.presets.${p.id}.name`)}
                </button>
              ))}
            </div>
          )}

          <div className="flex gap-2">
            <Input
              value={formId}
              onChange={(e) => setFormId(e.target.value)}
              placeholder={t("features.mcpId")}
              disabled={!!editingId}
              className="flex-1"
            />
            <Input
              value={formName}
              onChange={(e) => setFormName(e.target.value)}
              placeholder={t("features.mcpName")}
              className="flex-1"
            />
          </div>

          <Tabs
            value={mcpTab}
            onValueChange={(v) => {
              if (v === "raw") {
                const built = buildSpec(false);
                if (built) setRawSpec(JSON.stringify(built, null, 2));
              }
              setMcpTab(v as "structured" | "raw");
            }}
          >
            <TabsList>
              <TabsTrigger value="structured">
                {t("features.mcpFormStructured")}
              </TabsTrigger>
              <TabsTrigger value="raw">{t("features.mcpFormRaw")}</TabsTrigger>
            </TabsList>

            <TabsContent value="structured" className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">
                  {t("features.mcpType")}:
                </span>
                <select
                  value={mcpType}
                  onChange={(e) =>
                    setMcpType(e.target.value as "stdio" | RemoteType)
                  }
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs"
                >
                  <option value="stdio">{t("mcp.typeStdio")}</option>
                  <option value="http">{t("mcp.typeHttp")}</option>
                  <option value="sse">{t("mcp.typeSse")}</option>
                </select>
              </div>
              {mcpType === "stdio" ? (
                <>
                  <Input
                    value={mcpCommand}
                    onChange={(e) => setMcpCommand(e.target.value)}
                    placeholder={t("mcp.command")}
                  />
                  <textarea
                    value={mcpArgs}
                    onChange={(e) => setMcpArgs(e.target.value)}
                    placeholder={t("features.mcpArgs")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                  <textarea
                    value={mcpEnv}
                    onChange={(e) => setMcpEnv(e.target.value)}
                    placeholder={t("features.mcpEnv")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                </>
              ) : (
                <>
                  <Input
                    value={mcpUrl}
                    onChange={(e) => setMcpUrl(e.target.value)}
                    placeholder={t("features.mcpUrl")}
                  />
                  <textarea
                    value={mcpHeaders}
                    onChange={(e) => setMcpHeaders(e.target.value)}
                    placeholder={t("features.mcpHeaders")}
                    rows={2}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                </>
              )}
            </TabsContent>

            <TabsContent value="raw">
              <JsonEditor
                value={rawSpec}
                onChange={handleRawSpecChange}
                rows={8}
              />
            </TabsContent>
          </Tabs>

          {/* 元数据（可折叠，对齐 v1） */}
          <div>
            <button
              type="button"
              onClick={() => setMetaOpen((v) => !v)}
              className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
            >
              <ChevronDown
                className={`h-3.5 w-3.5 transition-transform ${metaOpen ? "rotate-180" : ""}`}
              />
              {t("mcp.metadataOptional")}
            </button>
            {metaOpen && (
              <div className="mt-2 grid gap-2 sm:grid-cols-2">
                <Input
                  value={formDesc}
                  onChange={(e) => setFormDesc(e.target.value)}
                  placeholder={t("mcp.description")}
                />
                <Input
                  value={formTags}
                  onChange={(e) => setFormTags(e.target.value)}
                  placeholder={t("mcp.tags")}
                />
                <Input
                  value={formHomepage}
                  onChange={(e) => setFormHomepage(e.target.value)}
                  placeholder={t("mcp.homepage")}
                />
                <Input
                  value={formDocs}
                  onChange={(e) => setFormDocs(e.target.value)}
                  placeholder={t("mcp.docs")}
                />
              </div>
            )}
          </div>

          {/* 启用插件勾选 */}
          {plugins.length > 0 && (
            <div className="flex flex-wrap gap-3 pt-1">
              {plugins.map((p) => (
                <label
                  key={p.id}
                  className="flex items-center gap-1.5 text-xs text-muted-foreground"
                >
                  <Checkbox
                    checked={enabledApps[p.id] ?? false}
                    onCheckedChange={(v) =>
                      setEnabledApps((prev) => ({
                        ...prev,
                        [p.id]: v === true,
                      }))
                    }
                  />
                  {p.name}
                </label>
              ))}
            </div>
          )}

          <Button onClick={() => void handleUpsert()} className="w-full">
            {t("common.save")}
          </Button>
        </div>
      )}

      {/* 搜索 + 按插件批量开关 */}
      {servers.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("mcp.searchPlaceholder")}
            className="max-w-xs"
          />
          <select
            value={bulkPluginId}
            onChange={(e) => setBulkPluginId(e.target.value)}
            className="rounded-md border border-border bg-background px-2 py-1 text-xs"
          >
            <option value="">{t("mcp.batchToggle")}</option>
            {plugins.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          {bulkPluginId && (
            <>
              <Button
                size="sm"
                variant="outline"
                onClick={() => void handleBulkToggle(bulkPluginId, true)}
              >
                {t("mcp.enableAll")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => void handleBulkToggle(bulkPluginId, false)}
              >
                {t("mcp.disableAll")}
              </Button>
            </>
          )}
        </div>
      )}

      {query.isLoading ? (
        <Card>
          <CardContent className="py-10 text-center text-xs text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<Puzzle className="h-8 w-8" />}
          message={search.trim() ? t("mcp.noResults") : t("features.mcpEmpty")}
        />
      ) : (
        <Card>
          <ul className="divide-y divide-border">
            {filtered.map((s: McpServer) => (
              <li
                key={s.id}
                className="px-4 py-3 transition-colors hover:bg-muted/40"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{s.name}</div>
                    <div className="truncate text-xs text-muted-foreground">
                      {s.id}
                      {s.description ? ` · ${s.description}` : ""}
                      {(s.tags?.length ?? 0) > 0
                        ? ` · ${s.tags!.join(", ")}`
                        : ""}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-0.5">
                    <button
                      type="button"
                      onClick={() => openEdit(s)}
                      className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                      title={t("mcp.editServer")}
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                    <button
                      type="button"
                      onClick={() => setDeleting(s)}
                      className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                      title={t("common.delete")}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>
                <div className="mt-2 flex flex-wrap gap-3">
                  {plugins.map((p) => {
                    const enabled =
                      s.apps.find(([pid]) => pid === p.id)?.[1] ?? false;
                    return (
                      <label
                        key={p.id}
                        className="flex items-center gap-1.5 text-xs text-muted-foreground"
                      >
                        <Checkbox
                          checked={enabled}
                          onCheckedChange={(v) =>
                            void handleToggleApp(s.id, p.id, v === true)
                          }
                        />
                        {p.name}
                      </label>
                    );
                  })}
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <ConfirmDialog
        isOpen={deleting !== null}
        title={t("mcp.deleteConfirmTitle")}
        message={
          deleting ? t("mcp.deleteConfirmMessage", { name: deleting.name }) : ""
        }
        confirmText={t("common.delete")}
        cancelText={t("common.cancel")}
        variant="destructive"
        onConfirm={() => deleting && void handleDelete(deleting)}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
}

/** 解析 KEY=VALUE / KEY: VALUE 的行格式。 */
function parseKeyValueLines(
  text: string,
  sep: "=" | ":",
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const idx = line.indexOf(sep);
    if (idx > 0) {
      const k = line.slice(0, idx).trim();
      const v = line.slice(idx + 1).trim();
      if (k) out[k] = v;
    }
  }
  return out;
}
