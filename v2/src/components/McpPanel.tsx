import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { getMcpServers, removeMcpServer, setMcpServer } from "@/lib/api";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import JsonEditor from "@/components/JsonEditor";
import type { McpServerSpec } from "@/types";

export default function McpPanel({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [type, setType] = useState<"stdio" | "sse">("stdio");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [url, setUrl] = useState("");
  const [mcpTab, setMcpTab] = useState<"structured" | "raw">("structured");
  const [rawSpec, setRawSpec] = useState("{}");

  const query = useQuery({
    queryKey: ["plugin-mcp", pluginId],
    queryFn: () => getMcpServers(pluginId),
  });

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["plugin-mcp", pluginId] });

  const handleAdd = async () => {
    const id = name.trim() || command.trim();
    if (!id) {
      toast.error(t("common.error"));
      return;
    }
    let specObj: Record<string, unknown>;
    if (mcpTab === "raw") {
      try {
        specObj = JSON.parse(rawSpec);
      } catch {
        toast.error(t("jsonEditor.invalidJson"));
        return;
      }
    } else {
      specObj =
        type === "stdio"
          ? {
              type: "stdio",
              command: command.trim(),
              args: args
                .split(/\s+/)
                .map((a) => a.trim())
                .filter(Boolean),
            }
          : { type: "sse", url: url.trim() };
    }
    const spec: McpServerSpec = { id, name: id, spec: specObj };
    try {
      await setMcpServer(pluginId, spec);
      await invalidate();
      toast.success(t("common.save"));
      setShowForm(false);
      setName("");
      setCommand("");
      setArgs("");
      setUrl("");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleRemove = async (id: string) => {
    try {
      await removeMcpServer(pluginId, id);
      await invalidate();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const servers = query.data ?? [];

  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("features.mcpTitle")}</h3>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={invalidate}
            className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            title={t("common.refresh")}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={() => setShowForm((v) => !v)}
            className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            title={t("features.mcpAdd")}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {showForm && (
        <div className="space-y-2 rounded-lg border border-border p-3">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("features.mcpName")}
            className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
          />
          <Tabs
            value={mcpTab}
            onValueChange={(v) => {
              if (v === "raw") {
                setRawSpec(
                  JSON.stringify(
                    type === "stdio"
                      ? {
                          type: "stdio",
                          command: command.trim(),
                          args: args
                            .split(/\s+/)
                            .map((a) => a.trim())
                            .filter(Boolean),
                        }
                      : { type: "sse", url: url.trim() },
                    null,
                    2,
                  ),
                );
              }
              setMcpTab(v as "structured" | "raw");
            }}
          >
            <TabsList>
              <TabsTrigger value="structured">
                {t("features.formStructured")}
              </TabsTrigger>
              <TabsTrigger value="raw">{t("features.formRawJson")}</TabsTrigger>
            </TabsList>
            <TabsContent value="structured" className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">
                  {t("features.mcpType")}:
                </span>
                <select
                  value={type}
                  onChange={(e) => setType(e.target.value as "stdio" | "sse")}
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs"
                >
                  <option value="stdio">{t("features.mcpTypeStdio")}</option>
                  <option value="sse">{t("features.mcpTypeRemote")}</option>
                </select>
              </div>
              {type === "stdio" ? (
                <>
                  <input
                    value={command}
                    onChange={(e) => setCommand(e.target.value)}
                    placeholder={t("features.mcpCommand")}
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                  <input
                    value={args}
                    onChange={(e) => setArgs(e.target.value)}
                    placeholder="args…"
                    className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                  />
                </>
              ) : (
                <input
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder={t("features.mcpUrl")}
                  className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
                />
              )}
            </TabsContent>
            <TabsContent value="raw">
              <JsonEditor value={rawSpec} onChange={setRawSpec} rows={10} />
            </TabsContent>
          </Tabs>
          <button
            type="button"
            onClick={handleAdd}
            className="w-full rounded-md bg-primary px-2 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
          >
            {t("common.save")}
          </button>
        </div>
      )}

      {query.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : servers.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.mcpEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {servers.map((s) => (
            <li
              key={s.id}
              className="flex items-center justify-between gap-2 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm">{s.name}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {String(s.spec.type ?? "")}
                  {s.spec.command
                    ? ` · ${String(s.spec.command)}`
                    : s.spec.url
                      ? ` · ${String(s.spec.url)}`
                      : ""}
                </div>
              </div>
              <button
                type="button"
                onClick={() => handleRemove(s.id)}
                className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                title={t("common.delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
