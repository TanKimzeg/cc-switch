import { describe, expect, it, vi, beforeEach } from "vitest";
import { loadTsPlugin, makeHost } from "./plugin-loader";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("plugin-loader", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("loads a TS plugin that exports via plugin object", async () => {
    const source = `
      const plugin = {
        id: "demo",
        capabilities: { readLive: true },
        readLive: () => host.invoke("x", {}),
      };
    `;
    const plugin = await loadTsPlugin("demo", source);
    expect(plugin.id).toBe("demo");
    expect(plugin.capabilities.readLive).toBe(true);
    expect(typeof plugin.readLive).toBe("function");
  });

  it("rejects plugins without an id", async () => {
    const source = `const plugin = { capabilities: {} };`;
    await expect(loadTsPlugin("bad", source)).rejects.toThrow();
  });

  it("rejects invalid script output", async () => {
    const source = `const plugin = 42;`;
    await expect(loadTsPlugin("bad", source)).rejects.toThrow();
  });

  it("host routes file ops to plugin-scoped commands", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "host_read_file") return "{}";
      if (cmd === "host_write_file") return null;
      return null;
    });
    const host = makeHost("my-plugin");
    const content = await host.readFile("state.json");
    expect(content).toBe("{}");
    await host.writeFile("state.json", "{}");
    expect(invoke).toHaveBeenCalledWith("host_read_file", {
      id: "my-plugin",
      path: "state.json",
    });
    expect(invoke).toHaveBeenCalledWith("host_write_file", {
      id: "my-plugin",
      path: "state.json",
      content: "{}",
    });
  });

  it("host routes resource ops to allowlisted commands", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "host_read_resource") return "hello";
      if (cmd === "host_write_resource") return null;
      if (cmd === "host_list_resource") return ["a", "b"];
      return null;
    });
    const host = makeHost("my-plugin");
    const content = await host.readResource("config", "settings.json");
    expect(content).toBe("hello");
    await host.writeResource("config", "{}", "settings.json");
    const files = await host.listResource("projects", "sub");
    expect(files).toEqual(["a", "b"]);

    expect(invoke).toHaveBeenCalledWith("host_read_resource", {
      id: "my-plugin",
      name: "config",
      rel: "settings.json",
    });
    expect(invoke).toHaveBeenCalledWith("host_write_resource", {
      id: "my-plugin",
      name: "config",
      content: "{}",
      rel: "settings.json",
    });
    expect(invoke).toHaveBeenCalledWith("host_list_resource", {
      id: "my-plugin",
      name: "projects",
      rel: "sub",
    });
  });

  it("host DB methods bind plugin id and route to backend commands", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_providers") return [{ id: "p1", pluginId: "my-plugin" }];
      if (cmd === "add_provider") return { id: "p2", pluginId: "my-plugin" };
      if (cmd === "delete_provider") return null;
      if (cmd === "usage_insert_records") return 2;
      if (cmd === "usage_daily_summary") return [{ day: "2026-01-01" }];
      return null;
    });
    const host = makeHost("my-plugin");

    const providers = await host.providers();
    expect(providers).toEqual([{ id: "p1", pluginId: "my-plugin" }]);
    expect(invoke).toHaveBeenCalledWith("get_providers", {
      pluginId: "my-plugin",
    });

    const p = await host.upsertProvider({ pluginId: "ignored", name: "P" });
    expect(p.id).toBe("p2");
    // 强制绑定当前插件 id，且不投影 live。
    expect(invoke).toHaveBeenCalledWith("add_provider", {
      input: { pluginId: "my-plugin", name: "P" },
      addToLive: false,
    });

    await host.deleteProvider("p1");
    expect(invoke).toHaveBeenCalledWith("delete_provider", { id: "p1" });

    const n = await host.saveUsageRecords([{ sourceId: "x" }] as never);
    expect(n).toBe(2);
    expect(invoke).toHaveBeenCalledWith("usage_insert_records", {
      pluginId: "my-plugin",
      records: [{ sourceId: "x" }],
    });

    await host.usageDailySummary();
    expect(invoke).toHaveBeenCalledWith("usage_daily_summary", {
      pluginId: "my-plugin",
    });
  });
});
