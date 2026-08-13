# 后端 Tauri 命令清单（IPC API）

后端 Rust 侧通过 Tauri `invoke` 暴露给前端的全部命令。按域分组。除特别注明外，命令都解析插件后调用 `AgentPlugin` trait 方法（见 [plugin-protocol.md](plugin-protocol.md)）。

> 注：TS 插件的「live 类操作」（Provider/MCP/会话/用量/raw config）在**前端**被 `api.ts` 拦截，直接调脚本方法；只有 native/shell 插件才真正走这些后端命令（见 [frontend-api.md](frontend-api.md)）。

## 1. 插件（plugins）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_plugins` | — | `InstalledPlugin[]` | 列出已安装插件（含 manifest 字段 + 来源） |
| `install_plugin` | `source: string` | `InstalledPlugin` | 从本地目录安装（目录须含 manifest.json） |
| `uninstall_plugin` | `id: string` | `()` | 卸载；内置插件拒绝卸载 |
| `plugin_read_live` | `id: string` | `LiveConfig` | 读取插件 live 配置（需 `read_live`） |
| `plugin_import` | `id: string` | `ImportCandidate[]` | 从 live 导入候选（需 `import`） |
| `plugin_sessions` | `id: string` | `SessionMeta[]` | 列出会话（需 `sessions`） |
| `plugin_load_messages` | `id`, `source` | `SessionMessage[]` | 加载会话消息（需 `sessions`） |
| `plugin_delete_session` | `id`, `session_id`, `source` | `bool` | 删除会话 |
| `plugin_apply` | `id`, `provider_id`, `current?` | `()` | 把 provider 写入 live（需 `apply`） |
| `plugin_remove_provider` | `id`, `provider_id` | `()` | 从 live 移除 provider（需 `remove`） |
| `plugin_mcp_get` | `id` | `McpServerSpec[]` | 读取 MCP 服务器（需 `mcp`） |
| `plugin_mcp_set` | `id`, `server` | `()` | 写入/更新 MCP 服务器 |
| `plugin_mcp_remove` | `id`, `server_id` | `()` | 移除 MCP 服务器 |
| `plugin_read_raw_config` | `id` | `string` | 读取 live 配置原始文本（需 `read_live`） |
| `plugin_write_raw_config` | `id`, `content` | `()` | 写入 live 配置原始文本（需 `apply`） |

## 2. Provider（providers）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_providers` | `plugin_id?` | `Provider[]` | 列出 provider（可按插件过滤） |
| `get_provider` | `id` | `Provider \| null` | 读取单个 provider |
| `add_provider` | `input: ProviderInput`, `add_to_live?` | `Provider` | 新增（默认写入 live，需 `apply`） |
| `update_provider` | `id`, `input`, `apply_live?` | `Provider` | 更新（默认同步 live） |
| `delete_provider` | `id` | `()` | 删除 DB 记录（不碰 live） |
| `get_current_provider` | `plugin_id` | `Provider \| null` | 读取某插件当前生效 provider |
| `set_current_provider` | `plugin_id`, `provider_id?` | `()` | 记录某插件当前 provider（只写 DB，不写 live） |
| `switch_provider` | `provider_id` | `()` | 切换：写 live（current=true）+ 记录当前 |
| `remove_provider_from_live_config` | `provider_id` | `()` | 从 live 移除（不删 DB） |
| `update_providers_sort_order` | `plugin_id`, `ids[]` | `()` | 批量更新排序 |
| `sync_all_providers_to_live` | `plugin_id?` | `usize` | 把 DB 中 `live_config_managed=1` 的 provider 全量投影到 live |
| `import_providers_from_live` | `plugin_id` | `usize` | 从 live 反向导入（更新/新增 provider） |

## 3. MCP（mcp）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `mcp_list` | — | `McpServer[]` | 列出全部 MCP 服务器（含各插件启用状态） |
| `mcp_upsert` | `server: McpServer` | `()` | 新增/更新，并同步到启用的插件 |
| `mcp_delete` | `id` | `()` | 删除，并从所有启用插件移除 |
| `mcp_toggle_app` | `id`, `plugin_id`, `enabled` | `()` | 切换某服务器在某插件的启用状态 |
| `import_mcp_from_plugin` | `id` | `McpServerSpec[]` | 从插件 live 读取 MCP 服务器 |
| `import_mcp_servers_from_plugin` | `id` | `usize` | 从插件导入 MCP 服务器到 DB |

## 4. 用量（usage）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `plugin_sync_usage` | `plugin_id` | `usize` | 从插件解析用量写入 `request_logs`（需 `sessions`） |
| `usage_insert_records` | `plugin_id`, `records: UsageRecord[]` | `usize` | 写入用量记录（TS 插件前端解析后调用；`INSERT OR IGNORE` 去重） |
| `usage_list_request_logs` | `plugin_id?`, `limit?` | `RequestLogRow[]` | 查询请求日志（按时间倒序） |
| `usage_daily_summary` | `plugin_id?` | `DailyUsageRow[]` | 按日汇总用量 |

## 5. Skills（skills）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `skills_list` | — | `SkillRecord[]` | 列出全部技能（含启用插件） |
| `skills_install` | `source: string` | `SkillRecord` | 从本地目录安装技能到 SSOT |
| `skills_uninstall` | `id` | `()` | 卸载技能 |
| `skills_toggle_plugin` | `id`, `plugin_id`, `enabled` | `()` | 启用/停用某技能在指定插件（复制/移除到 `plugin.skills_dir()`） |

## 6. Prompts（prompts）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `prompts_list` | `plugin_id?` | `PromptRecord[]` | 列出 prompts |
| `prompts_upsert` | `id`, `plugin_id`, `name`, `content`, `description?` | `()` | 新增/更新 prompt |
| `prompts_delete` | `id` | `()` | 删除 prompt |
| `prompts_toggle` | `id`, `enabled` | `()` | 启用/停用，写入或移除 `plugin.prompt_file_path()` 文件 |

## 7. Profiles（profiles）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `profiles_list` | — | `Profile[]` | 列出配置方案 |
| `profiles_current` | — | `string \| null` | 当前激活的 profile id |
| `profiles_upsert` | `profile: Profile` | `()` | 新增/更新 |
| `profiles_delete` | `id` | `()` | 删除 |
| `profiles_apply` | `id` | `()` | 激活某 profile |
| `profiles_clear_current` | — | `()` | 清除当前激活 |

## 8. 备份 / 导入导出（backup）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `backup_create` | — | `BackupRecord` | 创建数据库备份（复制 db 文件到 `data_dir/backups/`） |
| `backup_list` | — | `BackupRecord[]` | 列出备份 |
| `backup_delete` | `id` | `()` | 删除备份 |
| `export_config_json` | — | `ExportPayload` | 导出全部配置为 JSON |
| `export_config_to_file` | `path` | `()` | 导出配置 JSON 到文件 |
| `parse_export_json` | `content` | `ExportPayload` | 解析导出 JSON 文本 |
| `import_config` | `payload` | `usize` | 导入配置负载（逐表 upsert） |
| `import_config_from_file` | `path` | `usize` | 从 JSON 文件导入 |

## 9. 宿主（host，仅 TS 插件）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `plugin_get_script` | `id`, `main` | `string` | 读取 TS 插件主脚本内容 |
| `host_read_file` | `id`, `path` | `string` | 读插件目录内文件（沙箱） |
| `host_write_file` | `id`, `path`, `content` | `()` | 写插件目录内文件（沙箱） |
| `host_list_files` | `id`, `dir?` | `string[]` | 列插件目录内容（沙箱） |

## 10. 设置（settings）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_setting` | `key` | `string \| null` | 读取应用级设置 |
| `set_setting` | `key`, `value` | `()` | 写入应用级设置 |
