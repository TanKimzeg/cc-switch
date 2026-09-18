// TypeScript 插件示例。
// 通过注入的 host API 调用 Tauri 宿主命令读写插件自有配置。
// 运行前由后端 `plugin_get_script` 返回本文件内容，前端受限作用域执行。
//
// 注意：加载器用 `new Function` 执行脚本，宿主不转译 TypeScript，因此本文件
// 必须是合法 JavaScript（用 JSDoc 标注类型，不使用 interface/declare/类型注解），
// 文件名用 .js。

const CONFIG_PATH = "state.json";

/** @returns {Promise<{ providers?: Array<Record<string, unknown>>, current?: string }>} */
async function readConfig() {
  try {
    const raw = await host.readFile(CONFIG_PATH);
    return raw ? JSON.parse(raw) : { providers: [] };
  } catch {
    return { providers: [] };
  }
}

/** @param {Record<string, unknown>} config */
async function writeConfig(config) {
  await host.writeFile(CONFIG_PATH, JSON.stringify(config, null, 2));
}

// 资源白名单示例（manifest `resources.demo` → ~/.cc-switch-demo）：
// 宿主把文件 I/O 交给后端执行，TS 插件只写读取/解析逻辑。
const RESOURCE_NAME = "demo";
const RESOURCE_FILE = "note.txt";

/** @returns {Promise<string>} 读取白名单资源（不存在返回空串）。 */
async function readNote() {
  try {
    return await host.readResource(RESOURCE_NAME, RESOURCE_FILE);
  } catch {
    return "";
  }
}

/** @param {string} content 写入白名单资源。 */
async function writeNote(content) {
  await host.writeResource(RESOURCE_NAME, content, RESOURCE_FILE);
}

// 用量同步示例：脚本只负责「解析自己的会话存储」，落库交给 host.saveUsageRecords。
const USAGE_FILE = "usage.json";

/** @returns {Promise<Array<Record<string, unknown>>>} 读取插件目录内的用量记录。 */
async function readUsageRecords() {
  try {
    const raw = await host.readFile(USAGE_FILE);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

const plugin = {
  id: "ts-demo",
  capabilities: {
    readLive: true,
    apply: true,
    sessions: false,
    mcp: false,
  },
  async readLive() {
    const config = await readConfig();
    return {
      providers: (config.providers || []).map((p) => ({
        id: p.id,
        name: p.name || p.id,
        settingsConfig: p,
      })),
      current: config.current || null,
    };
  },
  async apply(provider, current) {
    const config = await readConfig();
    const id = provider.id;
    const existing = config.providers || [];
    const idx = existing.findIndex((p) => p.id === id);
    let parsed = {};
    try {
      parsed = JSON.parse(provider.settingsConfig || "{}");
    } catch {
      /* ignore */
    }
    if (idx >= 0) {
      existing[idx] = { ...existing[idx], ...parsed, id };
    } else {
      existing.push({ ...parsed, id, name: provider.name || id });
    }
    await writeConfig({ ...config, providers: existing, current: id });
    // 演示资源写入：把当前 provider id 记到白名单资源里。
    await writeNote(`current provider: ${id}`);
  },
  async readRawConfig() {
    const config = await readConfig();
    const note = await readNote();
    return JSON.stringify({ ...config, demoNote: note }, null, 2);
  },
  async syncUsage() {
    // 解析自己的用量记录（纯逻辑）。
    const records = await readUsageRecords();
    // 落库：由宿主调用 usage_insert_records 写入 request_logs（INSERT OR IGNORE 去重）。
    await host.saveUsageRecords(records);
    return records;
  },
  async sessions() {
    // 演示 DB 读取：返回当前插件的 provider 列表作为示例会话元信息。
    const providers = await host.providers();
    return providers.map((p) => ({
      sessionId: `provider:${p.id}`,
      title: p.name,
      projectDir: null,
      createdAt: null,
      lastActiveAt: null,
      sourcePath: null,
      resumeCommand: null,
    }));
  },
};
