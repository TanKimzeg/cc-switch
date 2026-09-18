import type { McpServer } from "@/types";

/**
 * 智能解析粘贴的 MCP JSON（对齐 v1 parseSmartMcpJson）：
 * 1. mcpServers 包装：{ "mcpServers": { "id": {...} } } → 提取首个条目
 * 2. 单键包装："server-name": {...} 或 { "server-name": {...} } → 提取键与配置
 *
 * 返回提取出的 id（可能为空）与配置对象。
 */
export function parseSmartMcpJson(jsonText: string): {
  id?: string;
  config: Record<string, unknown>;
} {
  let trimmed = jsonText.trim();
  if (!trimmed) return { config: {} };

  // 键值对片段（"key": {...}）补全成完整对象
  if (trimmed.startsWith('"') && !trimmed.startsWith("{")) {
    trimmed = `{${trimmed}}`;
  }

  const parsed = JSON.parse(trimmed) as Record<string, unknown>;

  // mcpServers 包装：取第一个条目
  const wrapper = parsed["mcpServers"];
  if (
    wrapper &&
    typeof wrapper === "object" &&
    !Array.isArray(wrapper) &&
    Object.keys(parsed).length === 1
  ) {
    const map = wrapper as Record<string, unknown>;
    const firstKey = Object.keys(map)[0];
    if (firstKey && map[firstKey] && typeof map[firstKey] === "object") {
      return {
        id: firstKey,
        config: map[firstKey] as Record<string, unknown>,
      };
    }
  }

  const keys = Object.keys(parsed);
  if (
    keys.length === 1 &&
    parsed[keys[0]] &&
    typeof parsed[keys[0]] === "object" &&
    !Array.isArray(parsed[keys[0]])
  ) {
    return {
      id: keys[0],
      config: parsed[keys[0]] as Record<string, unknown>,
    };
  }

  return { config: parsed };
}

/** 构造面板搜索文本（白名单字段，排除 env/headers 等敏感值，对齐 v1）。 */
export function getMcpSearchText(server: McpServer): string {
  const parts: string[] = [
    server.name,
    server.id,
    server.description ?? "",
    (server.tags ?? []).join(" "),
  ];
  const spec = server.spec as Record<string, unknown>;
  if (spec && typeof spec === "object") {
    parts.push(String(spec["type"] ?? ""));
    parts.push(String(spec["command"] ?? ""));
    parts.push(String(spec["url"] ?? ""));
  }
  return parts.join(" ").toLowerCase();
}
