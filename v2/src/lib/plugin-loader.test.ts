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
});
