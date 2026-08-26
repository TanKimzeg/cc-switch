import type { McpServerSpec } from "@/types";

function isWindows(): boolean {
  if (typeof navigator === "undefined") return false;
  return /win/i.test(navigator.platform || navigator.userAgent || "");
}

export interface McpPreset {
  id: string;
  name: string;
  tags: string[];
  server: McpServerSpec["spec"];
  homepage?: string;
  docs?: string;
}

const createNpxCommand = (
  packageName: string,
  extraArgs: string[] = [],
): { command: string; args: string[] } => {
  if (isWindows()) {
    return {
      command: "cmd",
      args: ["/c", "npx", ...extraArgs, packageName],
    };
  }
  return {
    command: "npx",
    args: [...extraArgs, packageName],
  };
};

// 预设 MCP（对齐 v1 config/mcpPresets.ts）：最常用的 stdio 模式服务器，
// description 使用 i18n key（mcp.presets.<id>.description）。
export const mcpPresets: McpPreset[] = [
  {
    id: "fetch",
    name: "mcp-server-fetch",
    tags: ["stdio", "http", "web"],
    server: {
      type: "stdio",
      command: "uvx",
      args: ["mcp-server-fetch"],
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
  },
  {
    id: "time",
    name: "@modelcontextprotocol/server-time",
    tags: ["stdio", "time", "utility"],
    server: {
      type: "stdio",
      ...createNpxCommand("@modelcontextprotocol/server-time", ["-y"]),
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/time",
  },
  {
    id: "memory",
    name: "@modelcontextprotocol/server-memory",
    tags: ["stdio", "memory", "graph"],
    server: {
      type: "stdio",
      ...createNpxCommand("@modelcontextprotocol/server-memory", ["-y"]),
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
  },
  {
    id: "sequential-thinking",
    name: "@modelcontextprotocol/server-sequential-thinking",
    tags: ["stdio", "thinking", "reasoning"],
    server: {
      type: "stdio",
      ...createNpxCommand("@modelcontextprotocol/server-sequential-thinking", [
        "-y",
      ]),
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
  },
  {
    id: "context7",
    name: "@upstash/context7-mcp",
    tags: ["stdio", "docs", "search"],
    server: {
      type: "stdio",
      ...createNpxCommand("@upstash/context7-mcp", ["-y"]),
    },
    homepage: "https://context7.com",
    docs: "https://github.com/upstash/context7/blob/master/README.md",
  },
];

export default mcpPresets;
