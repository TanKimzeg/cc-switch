import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, MessageSquare, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { deleteSession, listSessions, loadSessionMessages } from "@/lib/api";
import type { SessionMessage, SessionMeta } from "@/types";

function formatTime(ts?: number | null): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString();
}

export default function SessionList({ pluginId }: { pluginId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [messages, setMessages] = useState<SessionMessage[]>([]);

  const sessionsQuery = useQuery({
    queryKey: ["sessions", pluginId],
    queryFn: () => listSessions(pluginId),
  });

  const sessions = sessionsQuery.data ?? [];

  const handleToggleMessages = async (s: SessionMeta) => {
    if (expanded === s.sessionId) {
      setExpanded(null);
      return;
    }
    if (!s.sourcePath) {
      toast.info(t("features.sessionsEmpty"));
      return;
    }
    try {
      const msgs = await loadSessionMessages(pluginId, s.sourcePath);
      setMessages(msgs);
      setExpanded(s.sessionId);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (s: SessionMeta) => {
    if (!s.sourcePath) return;
    try {
      await deleteSession(pluginId, s.sessionId, s.sourcePath);
      await queryClient.invalidateQueries({ queryKey: ["sessions", pluginId] });
      toast.success(t("features.sessionDeleted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">{t("shell.sessions")}</h3>
        <button
          type="button"
          onClick={() =>
            queryClient.invalidateQueries({ queryKey: ["sessions", pluginId] })
          }
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
          title={t("common.refresh")}
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </button>
      </div>
      {sessionsQuery.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading")}</p>
      ) : sessions.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("features.sessionsEmpty")}
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {sessions.map((s: SessionMeta) => (
            <li key={s.sessionId} className="px-3 py-2">
              <div className="flex items-center gap-3">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">
                    {s.title || s.projectDir || s.sessionId}
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                    <span className="truncate">{s.projectDir ?? "—"}</span>
                    <span>{formatTime(s.lastActiveAt)}</span>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => handleToggleMessages(s)}
                  className="shrink-0 rounded-md border border-border px-2 py-1 text-xs transition-colors hover:bg-accent"
                  title={t("features.sessionMessages")}
                >
                  <MessageSquare className="h-3.5 w-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => handleDelete(s)}
                  className="shrink-0 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title={t("features.sessionDelete")}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
                {expanded === s.sessionId && (
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                )}
              </div>
              {expanded === s.sessionId && (
                <div className="mt-2 space-y-1 rounded-md bg-muted/50 p-2">
                  {messages.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {t("features.sessionsEmpty")}
                    </p>
                  ) : (
                    messages.map((m, i) => (
                      <div key={i} className="text-xs">
                        <span className="font-medium text-primary">
                          {m.role}
                        </span>
                        <span className="text-muted-foreground">
                          {" "}
                          · {formatTime(m.ts)}
                        </span>
                        <pre className="mt-0.5 whitespace-pre-wrap break-words text-foreground">
                          {m.content}
                        </pre>
                      </div>
                    ))
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
