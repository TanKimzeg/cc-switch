// TypeScript 插件示例。
// 通过注入的 host API 调用 Tauri 宿主命令读写插件自有配置。
// 运行前由后端 `plugin_get_script` 返回本文件内容，前端受限作用域执行。

declare const host: {
  readFile(path: string): string;
  writeFile(path: string, content: string): void;
  listFiles(dir?: string): string[];
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

const CONFIG_PATH = "state.json";

function readConfig(): { providers?: Array<Record<string, unknown>>; current?: string } {
  try {
    const raw = host.readFile(CONFIG_PATH);
    return raw ? JSON.parse(raw) : { providers: [] };
  } catch {
    return { providers: [] };
  }
}

function writeConfig(config: Record<string, unknown>): void {
  host.writeFile(CONFIG_PATH, JSON.stringify(config, null, 2));
}

const plugin = {
  id: "ts-demo",
  capabilities: {
    readLive: true,
    apply: true,
    sessions: false,
    mcp: false,
  },
  readLive() {
    const config = readConfig();
    return {
      providers: (config.providers || []).map((p) => ({
        id: p.id,
        name: p.name || p.id,
        settingsConfig: p,
      })),
      current: config.current || null,
    };
  },
  apply(provider: { id: string; name?: string; settingsConfig?: string }, current: boolean) {
    const config = readConfig();
    const id = provider.id;
    const existing = config.providers || [];
    const idx = existing.findIndex((p) => p.id === id);
    let parsed: Record<string, unknown> = {};
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
    writeConfig({ ...config, providers: existing, current: id });
  },
};
