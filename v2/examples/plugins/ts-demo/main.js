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
  },
};
