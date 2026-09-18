import { describe, expect, it } from "vitest";
import { getMcpSearchText, parseSmartMcpJson } from "./mcpUtils";
import type { McpServer } from "@/types";

describe("parseSmartMcpJson", () => {
  it("extracts the entry from an mcpServers wrapper", () => {
    const result = parseSmartMcpJson(
      JSON.stringify({
        mcpServers: { filesystem: { command: "npx", args: ["-y"] } },
      }),
    );
    expect(result.id).toBe("filesystem");
    expect(result.config).toEqual({ command: "npx", args: ["-y"] });
  });

  it("unwraps a single-key object and returns its key as id", () => {
    const result = parseSmartMcpJson(
      JSON.stringify({ fetch: { type: "stdio", command: "uvx" } }),
    );
    expect(result.id).toBe("fetch");
    expect(result.config).toEqual({ type: "stdio", command: "uvx" });
  });

  it("accepts bare key-value snippets", () => {
    const result = parseSmartMcpJson('"srv": {"command": "x"}');
    expect(result.id).toBe("srv");
    expect(result.config).toEqual({ command: "x" });
  });

  it("returns plain objects untouched without id", () => {
    const config = { type: "http", url: "https://x/mcp" };
    const result = parseSmartMcpJson(JSON.stringify(config));
    expect(result.id).toBeUndefined();
    expect(result.config).toEqual(config);
  });

  it("throws on invalid json", () => {
    expect(() => parseSmartMcpJson("{oops")).toThrow();
  });
});

describe("getMcpSearchText", () => {
  it("includes whitelisted fields but not env/header values", () => {
    const server = {
      id: "fs",
      name: "Filesystem",
      description: "local files",
      tags: ["stdio"],
      spec: {
        type: "stdio",
        command: "npx",
        env: { SECRET_TOKEN: "abc" },
        headers: { Authorization: "Bearer xyz" },
      },
      apps: [],
    } as unknown as McpServer;

    const text = getMcpSearchText(server);
    expect(text).toContain("filesystem");
    expect(text).toContain("local files");
    expect(text).toContain("stdio");
    expect(text).toContain("npx");
    // 敏感凭据不进入搜索索引
    expect(text).not.toContain("SECRET_TOKEN");
    expect(text).not.toContain("abc");
    expect(text).not.toContain("Authorization");
  });
});
