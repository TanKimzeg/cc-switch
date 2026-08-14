// Claude Code TypeScript 插件。
// 通过注入的 host API 读写 manifest `resources` 白名单内声明的资源：
//   - config    → ~/.claude/settings.json  （provider 配置，非 additive 单 provider）
//   - mcp       → ~/.claude.json           （mcpServers）
//   - projects  → ~/.claude/projects/**/*.jsonl （会话与用量）
//
// 注意：加载器用 `new Function` 执行脚本，宿主不转译 TypeScript，因此本文件
// 必须是合法 JavaScript（用 JSDoc 标注类型，不使用 interface/declare/类型注解）。

const RES_CONFIG = "config";
const RES_MCP = "mcp";
const RES_PROJECTS = "projects";

/** 读取 provider 配置（settings.json），不存在/损坏返回空对象。 */
async function readSettings() {
  try {
    const raw = await host.readResource(RES_CONFIG);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

/** 写入 provider 配置（settings.json）。 */
async function writeSettings(settings) {
  await host.writeResource(RES_CONFIG, JSON.stringify(settings, null, 2));
}

/** 读取 mcp 根对象（~/.claude.json）。 */
async function readMcpRoot() {
  try {
    const raw = await host.readResource(RES_MCP);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

/** 写入 mcp 根对象。 */
async function writeMcpRoot(root) {
  await host.writeResource(RES_MCP, JSON.stringify(root, null, 2));
}

/** 去掉只属于 cc-switch 的内部字段（与 v1 语义一致）。 */
function sanitizeSettings(settings) {
  const out = { ...settings };
  delete out.api_format;
  delete out.apiFormat;
  delete out.openrouter_compat_mode;
  delete out.openrouterCompatMode;
  return out;
}

/** RFC3339 → 毫秒时间戳。 */
function parseTs(ts) {
  if (typeof ts !== "string") return null;
  const ms = Date.parse(ts);
  return Number.isNaN(ms) ? null : ms;
}

/** 提取消息文本（content 可为字符串或数组）。 */
function extractText(content) {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((item) => {
        switch (item.type) {
          case "text":
            return typeof item.text === "string" ? item.text : "";
          case "tool_use":
            return `[Tool: ${item.name || "unknown"}]`;
          case "tool_result":
            return extractText(item.content);
          default:
            return "";
        }
      })
      .filter((t) => t !== "")
      .join("\n");
  }
  return "";
}

function truncate(s, max) {
  return s.length <= max ? s : `${s.slice(0, max)}...`;
}

/** 扫描 ~/.claude/projects 下所有项目目录的会话文件（两层层级）。 */
async function listSessionFiles() {
  const files = [];
  let dirs = [];
  try {
    dirs = await host.listResource(RES_PROJECTS);
  } catch {
    return files;
  }
  for (const dir of dirs) {
    let names = [];
    try {
      names = await host.listResource(RES_PROJECTS, dir);
    } catch {
      continue;
    }
    for (const name of names) {
      if (name.endsWith(".jsonl")) {
        files.push(`${dir}/${name}`);
      }
    }
  }
  return files;
}

/** 解析单个会话文件的元信息。 */
async function parseSessionMeta(relPath) {
  let raw;
  try {
    raw = await host.readResource(RES_PROJECTS, relPath);
  } catch {
    return null;
  }
  let sessionId = null;
  let projectDir = null;
  let createdAt = null;
  let lastActiveAt = null;
  let title = null;
  for (const line of raw.split("\n")) {
    if (!line.trim()) continue;
    let value;
    try {
      value = JSON.parse(line);
    } catch {
      continue;
    }
    if (sessionId == null && value.sessionId) sessionId = value.sessionId;
    if (projectDir == null && value.cwd) projectDir = value.cwd;
    const ts = parseTs(value.timestamp);
    if (ts != null) {
      if (createdAt == null) createdAt = ts;
      lastActiveAt = ts;
    }
    if (title == null) {
      const isUser =
        value.type === "user" ||
        (value.message && value.message.role === "user");
      if (isUser) {
        const text = extractText(value.message && value.message.content).trim();
        if (
          text &&
          !text.includes("<local-command-caveat>") &&
          !text.startsWith("<command-name>")
        ) {
          title = truncate(text, 60);
        }
      }
    }
  }
  const fallbackId = relPath.split("/").pop().replace(/\.jsonl$/, "");
  return {
    sessionId: sessionId || fallbackId,
    title,
    projectDir,
    createdAt,
    lastActiveAt,
  };
}

const plugin = {
  id: "claudecode",
  capabilities: {
    readLive: true,
    apply: true,
    remove: true,
    import: true,
    sessions: true,
    mcp: true,
  },

  async readLive() {
    const settings = await readSettings();
    return {
      providers: [
        {
          id: "default",
          name: "Claude Code",
          settingsConfig: settings,
        },
      ],
      current: "default",
    };
  },

  async apply(provider, current) {
    let settings = {};
    try {
      settings = provider.settingsConfig
        ? JSON.parse(provider.settingsConfig)
        : {};
    } catch {
      /* 非法 JSON 按空配置处理 */
    }
    await writeSettings(sanitizeSettings(settings));
  },

  async removeProvider(id) {
    const settings = await readSettings();
    delete settings.env;
    delete settings.apiProvider;
    delete settings.model;
    await writeSettings(settings);
  },

  async import() {
    const settings = await readSettings();
    return [{ id: "default", name: "Claude Code", settingsConfig: settings }];
  },

  async sessions() {
    const files = await listSessionFiles();
    const sessions = [];
    for (const rel of files) {
      if (rel.split("/").pop().startsWith("agent-")) continue;
      const meta = await parseSessionMeta(rel);
      if (!meta) continue;
      sessions.push({
        sessionId: meta.sessionId,
        title: meta.title,
        projectDir: meta.projectDir,
        createdAt: meta.createdAt,
        lastActiveAt: meta.lastActiveAt,
        sourcePath: rel,
        resumeCommand: `claude --resume ${meta.sessionId}`,
      });
    }
    sessions.sort((a, b) => (b.lastActiveAt || 0) - (a.lastActiveAt || 0));
    return sessions;
  },

  async loadMessages(source) {
    let raw;
    try {
      raw = await host.readResource(RES_PROJECTS, source);
    } catch {
      return [];
    }
    const messages = [];
    for (const line of raw.split("\n")) {
      if (!line.trim()) continue;
      let value;
      try {
        value = JSON.parse(line);
      } catch {
        continue;
      }
      if (value.isMeta === true) continue;
      const message = value.message;
      if (!message) continue;
      let role = message.role || "unknown";
      // tool_result 包裹在 user 消息里 → 重新归类为 tool。
      if (role === "user" && Array.isArray(message.content)) {
        const allToolResults =
          message.content.length > 0 &&
          message.content.every((i) => i.type === "tool_result");
        if (allToolResults) role = "tool";
      }
      const content = extractText(message.content);
      if (!content.trim()) continue;
      messages.push({ role, content, ts: parseTs(value.timestamp) });
    }
    return messages;
  },

  async deleteSession(sessionId, source) {
    const meta = await parseSessionMeta(source);
    if (!meta || meta.sessionId !== sessionId) return false;
    // 宿主无删除命令，用空文件标记删除（sessions 会跳过无内容文件）。
    try {
      await host.writeResource(RES_PROJECTS, "", source);
      return true;
    } catch {
      return false;
    }
  },

  async getMcpServers() {
    const root = await readMcpRoot();
    const map = root.mcpServers || {};
    return Object.keys(map)
      .map((id) => ({ id, name: id, spec: map[id] }))
      .sort((a, b) => a.id.localeCompare(b.id));
  },

  async setMcpServer(server) {
    const root = await readMcpRoot();
    if (!root.mcpServers) root.mcpServers = {};
    root.mcpServers[server.id] = server.spec;
    await writeMcpRoot(root);
  },

  async removeMcpServer(id) {
    const root = await readMcpRoot();
    if (root.mcpServers && root.mcpServers[id]) {
      delete root.mcpServers[id];
      await writeMcpRoot(root);
    }
  },

  async readRawConfig() {
    return JSON.stringify(await readSettings(), null, 2);
  },

  async writeRawConfig(content) {
    const parsed = JSON.parse(content);
    await writeSettings(parsed);
  },

  async syncUsage() {
    const files = await listSessionFiles();
    const records = [];
    for (const rel of files) {
      let raw;
      try {
        raw = await host.readResource(RES_PROJECTS, rel);
      } catch {
        continue;
      }
      let sessionId = null;
      let input = 0;
      let output = 0;
      let cacheRead = 0;
      let cacheWrite = 0;
      let model = "unknown";
      let ts = 0;
      let count = 0;
      for (const line of raw.split("\n")) {
        if (!line.trim()) continue;
        let value;
        try {
          value = JSON.parse(line);
        } catch {
          continue;
        }
        if (sessionId == null && value.sessionId) sessionId = value.sessionId;
        const lineTs = parseTs(value.timestamp);
        if (lineTs != null && ts === 0) ts = lineTs;
        if (value.type !== "assistant") continue;
        const usage = value.message && value.message.usage;
        if (!usage) continue;
        input += usage.input_tokens || 0;
        output += usage.output_tokens || 0;
        cacheRead += usage.cache_read_input_tokens || 0;
        cacheWrite += usage.cache_creation_input_tokens || 0;
        if (value.message.model) model = value.message.model;
        count += 1;
      }
      if (count === 0 || (input === 0 && output === 0 && cacheRead === 0 && cacheWrite === 0)) {
        continue;
      }
      records.push({
        sourceId: `claude_session:${sessionId || rel}`,
        sessionId: sessionId || rel,
        model,
        inputTokens: input,
        outputTokens: output,
        reasoningTokens: 0,
        cacheReadTokens: cacheRead,
        cacheWriteTokens: cacheWrite,
        cost: 0,
        timestampMs: ts,
      });
    }
    // 落库由宿主完成（写入 request_logs，INSERT OR IGNORE 去重）。
    if (records.length > 0) {
      await host.saveUsageRecords(records);
    }
    return records;
  },
};
