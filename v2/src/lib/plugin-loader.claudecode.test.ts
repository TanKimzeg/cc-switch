import { describe, expect, it, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { loadTsPlugin } from "./plugin-loader";
import type { TsHost } from "./plugin-loader";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// 内存态文件系统：模拟 manifest `resources` 白名单下的 host 资源命令。
// - config    → 根目录下 settings.json（单文件资源）
// - mcp       → 根目录下 .claude.json（单文件资源）
// - projects  → 根目录下 projects/（目录资源）
function resourceHost(seed: {
  settings?: Record<string, unknown>;
  mcp?: Record<string, unknown>;
  sessions?: Record<string, string>;
}) {
  const settings = new Map<string, string>();
  if (seed.settings)
    settings.set("settings.json", JSON.stringify(seed.settings));
  if (seed.mcp) settings.set(".claude.json", JSON.stringify(seed.mcp));

  const projectFiles = new Map<string, string>(
    Object.entries(seed.sessions ?? {}),
  );

  const dbUsage: unknown[] = [];
  const host: TsHost = {
    readFile: async () => "",
    writeFile: async () => {},
    listFiles: async () => [],
    readResource: async (name, rel) => {
      if (name === "config") return settings.get("settings.json") ?? "";
      if (name === "mcp") return settings.get(".claude.json") ?? "";
      if (name === "projects") {
        const key = rel ?? "";
        const v = projectFiles.get(key);
        if (v === undefined) throw new Error(`not found: projects/${key}`);
        return v;
      }
      throw new Error(`unknown resource: ${name}`);
    },
    writeResource: async (name, content, rel) => {
      if (name === "config") settings.set("settings.json", content);
      else if (name === "mcp") settings.set(".claude.json", content);
      else if (name === "projects") projectFiles.set(rel ?? "", content);
    },
    listResource: async (name, rel) => {
      if (name === "projects") {
        const all = [...projectFiles.keys()];
        const dirs = new Set<string>();
        for (const key of all) {
          const idx = key.indexOf("/");
          if (idx >= 0) dirs.add(key.slice(0, idx));
        }
        const list = rel
          ? all
              .filter((k) => k.startsWith(`${rel}/`))
              .map((k) => k.split("/")[1])
          : [...dirs];
        return [...new Set(list)].sort();
      }
      return [];
    },
    providers: async () => [],
    upsertProvider: async () => ({ id: "x" }) as never,
    deleteProvider: async () => {},
    saveUsageRecords: async (records) => {
      dbUsage.push(...records);
      return records.length;
    },
    usageDailySummary: async () => [],
    invoke: async <T>() => null as T,
  };
  return {
    host,
    getSettings: () => settings.get("settings.json"),
    getUsage: () => dbUsage,
  };
}

const SESSION_1 = [
  '{"sessionId":"session-1","cwd":"/tmp/proj","timestamp":"2026-03-06T10:00:00Z"}',
  '{"type":"user","message":{"role":"user","content":"How do I deploy?"},"sessionId":"session-1","timestamp":"2026-03-06T10:01:00Z"}',
  '{"type":"assistant","message":{"role":"assistant","content":"Here is how..."},"timestamp":"2026-03-06T10:02:00Z"}',
].join("\n");

const SESSION_USAGE = [
  '{"sessionId":"session-u","timestamp":"2026-03-06T10:00:00Z"}',
  '{"type":"assistant","message":{"id":"m1","model":"claude-opus-4","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5}},"timestamp":"2026-03-06T10:01:00Z"}',
].join("\n");

describe("claudecode TS plugin (real main.js, resource host)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  const mainSource = () =>
    readFileSync(
      join(__dirname, "../../examples/plugins/claudecode/main.js"),
      "utf-8",
    );

  it("loads and aligns with the backend trait", async () => {
    const { host } = resourceHost({});
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);
    expect(plugin.id).toBe("claudecode");
    expect(typeof plugin.readLive).toBe("function");
    expect(typeof plugin.apply).toBe("function");
    expect(typeof plugin.removeProvider).toBe("function");
    expect(typeof plugin.import).toBe("function");
    expect(typeof plugin.sessions).toBe("function");
    expect(typeof plugin.loadMessages).toBe("function");
    expect(typeof plugin.deleteSession).toBe("function");
    expect(typeof plugin.getMcpServers).toBe("function");
    expect(typeof plugin.setMcpServer).toBe("function");
    expect(typeof plugin.removeMcpServer).toBe("function");
    expect(typeof plugin.readRawConfig).toBe("function");
    expect(typeof plugin.writeRawConfig).toBe("function");
    expect(typeof plugin.syncUsage).toBe("function");
  });

  it("readLive/import roundtrip through settings.json", async () => {
    const { host } = resourceHost({
      settings: {
        env: { ANTHROPIC_BASE_URL: "https://api.example.com" },
        permissions: { allow: ["Read"] },
      },
    });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    const live = await plugin.readLive?.();
    expect(live?.providers).toHaveLength(1);
    expect(live?.providers[0].id).toBe("default");
    const settings = live?.providers[0].settingsConfig as Record<
      string,
      { ANTHROPIC_BASE_URL: string }
    >;
    expect(settings.env.ANTHROPIC_BASE_URL).toBe("https://api.example.com");

    const candidates = await plugin.import?.();
    expect(candidates?.[0].id).toBe("default");
  });

  it("apply writes settings (sanitized), removeProvider clears fields", async () => {
    const { host, getSettings } = resourceHost({ settings: {} });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    await plugin.apply?.(
      {
        id: "default",
        name: "Claude Code",
        settingsConfig: JSON.stringify({
          env: { ANTHROPIC_API_KEY: "sk-1", ANTHROPIC_BASE_URL: "https://x" },
          api_format: "should-be-stripped",
        }),
      },
      true,
    );

    const written = JSON.parse(getSettings() ?? "{}");
    expect(written.env.ANTHROPIC_API_KEY).toBe("sk-1");
    expect(written.env.ANTHROPIC_BASE_URL).toBe("https://x");
    // 内部字段被 sanitize 掉。
    expect(written.api_format).toBeUndefined();

    await plugin.removeProvider?.("default");
    const after = JSON.parse(getSettings() ?? "{}");
    expect(after.env).toBeUndefined();
  });

  it("sessions scans ~/.claude/projects/**/*.jsonl and picks first user message as title", async () => {
    const { host } = resourceHost({
      sessions: { "proj-a/session-1.jsonl": SESSION_1 },
    });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    const sessions = await plugin.sessions?.();
    expect(sessions).toHaveLength(1);
    expect(sessions?.[0].sessionId).toBe("session-1");
    expect(sessions?.[0].title).toBe("How do I deploy?");
    expect(sessions?.[0].projectDir).toBe("/tmp/proj");
    expect(sessions?.[0].resumeCommand).toBe("claude --resume session-1");
  });

  it("loadMessages parses roles and reclassifies tool_result as tool", async () => {
    const { host } = resourceHost({
      sessions: {
        "proj-a/s.jsonl": [
          '{"message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Write","input":{}}]},"timestamp":"2026-03-06T10:00:00Z"}',
          '{"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]},"timestamp":"2026-03-06T10:00:01Z"}',
        ].join("\n"),
      },
    });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    const msgs = await plugin.loadMessages?.("proj-a/s.jsonl");
    expect(msgs).toHaveLength(2);
    expect(msgs?.[0].role).toBe("assistant");
    expect(msgs?.[0].content).toContain("[Tool: Write]");
    expect(msgs?.[1].role).toBe("tool");
    expect(msgs?.[1].content).toBe("ok");
  });

  it("syncUsage aggregates assistant tokens and persists via saveUsageRecords", async () => {
    const { host, getUsage } = resourceHost({
      sessions: { "proj-a/session-u.jsonl": SESSION_USAGE },
    });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    const records = await plugin.syncUsage?.();
    expect(records).toHaveLength(1);
    expect(records?.[0].model).toBe("claude-opus-4");
    expect(records?.[0].inputTokens).toBe(100);
    expect(records?.[0].outputTokens).toBe(50);
    expect(records?.[0].cacheReadTokens).toBe(10);
    expect(records?.[0].cacheWriteTokens).toBe(5);
    // 已落库。
    expect(getUsage()).toHaveLength(1);
  });

  it("mcp servers live in ~/.claude.json", async () => {
    const { host } = resourceHost({ mcp: {} });
    const plugin = await loadTsPlugin("claudecode", mainSource(), host);

    await plugin.setMcpServer?.({
      id: "filesystem",
      name: "filesystem",
      spec: {
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-filesystem"],
      },
    });
    const servers = await plugin.getMcpServers?.();
    expect(servers).toHaveLength(1);
    expect(servers?.[0].spec.command).toBe("npx");

    await plugin.removeMcpServer?.("filesystem");
    expect(await plugin.getMcpServers?.()).toHaveLength(0);
  });
});
