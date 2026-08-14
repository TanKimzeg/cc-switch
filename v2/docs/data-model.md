# 数据模型（Data Model）

v2 使用 SQLite，单一数据库文件 `{app_data_dir}/cc-switch-v2.db`。Schema 定义在 `v2/src-tauri/src/db.rs` 的 `SCHEMA` 常量（无历史包袱、无迁移史）。

## 表总览

| 表 | 用途 |
|----|------|
| `providers` | Provider（SSOT） |
| `app_state` | 每插件当前 provider / live 快照 / 标志 |
| `settings` | 应用级键值设置 |
| `plugin_installs` | 插件安装记录（来源、版本、sha256） |
| `mcp_servers` | MCP 服务器（统一格式） |
| `mcp_server_apps` | MCP 服务器 × 插件 启用状态 |
| `prompts` | Prompt 记录 |
| `skills` | Skill 记录（SSOT） |
| `skill_apps` | Skill × 插件 启用状态 |
| `skill_repos` | Skill 仓库订阅（预留） |
| `request_logs` | 请求日志（用量明细） |
| `model_pricing` | 模型价格表（预留） |
| `usage_daily_rollups` | 按日用量汇总 |
| `session_log_sync` | 会话日志增量同步游标 |
| `profiles` | 配置方案（Profile） |
| `db_backups` | 数据库备份记录 |

## 1. providers

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | provider id（additive 插件用 live 键；否则 uuid） |
| `plugin_id` | TEXT | 所属插件 |
| `name` | TEXT | 显示名 |
| `category` | TEXT default `custom` | 分类（custom/imported…） |
| `icon` / `website` / `api_key` | TEXT? | 展示与凭据 |
| `settings_config` | TEXT | 写入 live 的配置片段（JSON 字符串） |
| `meta` | TEXT | 附加元数据（JSON） |
| `sort_order` | INTEGER default 0 | 排序 |
| `live_config_managed` | INTEGER default 1 | 是否投影到 live |
| `created_at` / `updated_at` | TEXT | 时间戳 |

索引：`idx_providers_plugin ON providers(plugin_id)`。

## 2. app_state

| 字段 | 类型 | 说明 |
|------|------|------|
| `plugin_id` | TEXT PK | 插件 |
| `current_provider_id` | TEXT | 当前生效 provider |
| `live_config_snapshot` | TEXT | live 配置快照（预留） |
| `flags` | TEXT | 标志位（预留） |

## 3. settings

| 字段 | 类型 | 说明 |
|------|------|------|
| `key` | TEXT PK | 键 |
| `value` | TEXT | 值 |

## 4. plugin_installs

| 字段 | 类型 | 说明 |
|------|------|------|
| `plugin_id` | TEXT PK | 插件 |
| `version` | TEXT | 版本 |
| `source` | TEXT default `local` | `builtin` / `local` |
| `sha256` | TEXT | 安装内容校验（预留） |
| `installed_at` | TEXT | 安装时间 |

## 5. mcp_servers

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | 服务器 id |
| `name` | TEXT | 显示名 |
| `server_config` | TEXT | 统一格式 spec（JSON） |
| `description` / `homepage` / `docs` | TEXT? | 描述信息 |
| `tags` | TEXT default `[]` | 标签（JSON 数组字符串） |
| `created_at` / `updated_at` | TEXT | 时间戳 |

## 6. mcp_server_apps

| 字段 | 类型 | 说明 |
|------|------|------|
| `mcp_server_id` | TEXT FK → mcp_servers.id (CASCADE) | 服务器 |
| `plugin_id` | TEXT | 插件 |
| `enabled` | INTEGER default 0 | 是否启用 |
| PK | (mcp_server_id, plugin_id) | — |

## 7. prompts

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | prompt id |
| `plugin_id` | TEXT | 所属插件 |
| `name` | TEXT | 名称 |
| `content` | TEXT | 内容（Markdown） |
| `description` | TEXT? | 描述 |
| `enabled` | INTEGER default 1 | 启用状态 |
| `created_at` / `updated_at` | TEXT | 时间戳 |

启用时内容写入 `plugin.prompt_file_path()` 对应文件（如 `~/.claude/CLAUDE.md`）。

## 8. skills

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | skill id |
| `name` | TEXT | 名称 |
| `description` | TEXT? | 描述 |
| `directory` | TEXT | SSOT 内相对目录 |
| `source_path` | TEXT | 来源路径 |
| `repo_owner` / `repo_name` / `repo_branch` / `readme_url` | TEXT? | 仓库来源（预留） |
| `installed_at` | INTEGER | 安装时间（epoch 秒） |
| `content_hash` | TEXT | 内容哈希（预留） |
| `updated_at` | INTEGER | 更新时间 |

## 9. skill_apps

| 字段 | 类型 | 说明 |
|------|------|------|
| `skill_id` | TEXT FK → skills.id (CASCADE) | skill |
| `plugin_id` | TEXT | 插件 |
| `enabled` | INTEGER default 0 | 是否启用 |
| PK | (skill_id, plugin_id) | — |

启用时把 SSOT 技能目录复制到 `plugin.skills_dir()`。

## 10. skill_repos

| 字段 | 类型 | 说明 |
|------|------|------|
| `owner` | TEXT | 仓库 owner |
| `name` | TEXT | 仓库名 |
| `branch` | TEXT default `main` | 分支 |
| `enabled` | INTEGER default 1 | 是否启用 |
| PK | (owner, name) | — |

> 预留：v1 支持从 GitHub 仓库安装技能；v2 当前 `skills_install` 仅从本地目录。

## 11. request_logs

| 字段 | 类型 | 说明 |
|------|------|------|
| `request_id` | TEXT PK | 请求唯一 id（去重键） |
| `provider_id` | TEXT | provider |
| `plugin_id` | TEXT | 插件 |
| `model` / `request_model` / `pricing_model` | TEXT | 模型信息 |
| `input_tokens` / `output_tokens` / `cache_read_tokens` / `cache_creation_tokens` | INTEGER default 0 | token 数 |
| `input_cost_usd` / `output_cost_usd` / `total_cost_usd` | TEXT default `0` | 成本（字符串避免浮点） |
| `latency_ms` | INTEGER | 延迟 |
| `status_code` | INTEGER | 状态码 |
| `error_message` | TEXT? | 错误 |
| `session_id` | TEXT? | 会话 |
| `is_streaming` | INTEGER | 是否流式 |
| `created_at` | INTEGER | 时间戳（epoch 秒） |
| `data_source` | TEXT default `session` | 来源（`session` / `plugin`） |

索引：`(provider_id, plugin_id)`、`(created_at)`。

## 12. model_pricing

| 字段 | 类型 | 说明 |
|------|------|------|
| `model_id` | TEXT PK | 模型 |
| `display_name` | TEXT | 显示名 |
| `input_cost_per_million` / `output_cost_per_million` | TEXT | 每百万 token 成本 |
| `cache_read_cost_per_million` / `cache_creation_cost_per_million` | TEXT default `0` | 缓存成本 |

> 预留：v1 用它按 token 算成本；v2 当前 `sync_usage` 由插件返回 `cost`，未接线价格表。

## 13. usage_daily_rollups

| 字段 | 类型 | 说明 |
|------|------|------|
| `date` | TEXT | 日期 |
| `plugin_id` | TEXT | 插件 |
| `provider_id` | TEXT | provider |
| `model` | TEXT | 模型 |
| `request_count` / `success_count` | INTEGER | 请求数 |
| `input_tokens` / `output_tokens` / `cache_read_tokens` / `cache_creation_tokens` | INTEGER | token 汇总 |
| `total_cost_usd` | TEXT | 成本汇总 |
| PK | (date, plugin_id, provider_id, model) | — |

## 14. session_log_sync

| 字段 | 类型 | 说明 |
|------|------|------|
| `file_path` | TEXT PK | 会话日志文件 |
| `last_modified` | INTEGER | 上次同步时的修改时间 |
| `last_line_offset` | INTEGER default 0 | 增量同步行偏移 |
| `last_synced_at` | INTEGER | 上次同步时间 |

> 预留：v1 的会话用量增量同步游标；v2 当前由插件全量 `sync_usage`。

## 15. profiles

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | profile id |
| `name` | TEXT | 名称 |
| `payload` | TEXT | 配置负载（JSON） |
| `sort_order` | INTEGER? | 排序 |
| `created_at` / `updated_at` | INTEGER? | 时间戳 |

当前激活的 profile 存储在 `settings` 表，键为 `current_profile_id`。

> ⚠️ **现状**：v2 的 profiles 目前只是「命名 + JSON payload」的 CRUD，`profiles_apply` 仅记录 current 而不真正恢复到 live。v1 的语义是「项目快照」（把某分组当前 provider/MCP/Skills/prompt 状态存下、一键恢复现场），差距与实现思路见 [v1-gap-analysis.md](v1-gap-analysis.md) §3.10。

## 16. db_backups

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT PK | 备份 id |
| `name` | TEXT | 名称 |
| `file_path` | TEXT | 备份文件路径 |
| `size_bytes` | INTEGER | 大小 |
| `created_at` | INTEGER | 时间戳 |

## 数据流要点

- **SSOT**：`providers` 是权威，live 配置是投影；`import()` 回填 live → DB。
- **用量**：插件 `sync_usage()` → `request_logs`（`INSERT OR IGNORE` 去重）→ 查询时按日汇总（`usage_daily_summary`）。
- **未接线预留表**：`model_pricing`、`skill_repos`、`session_log_sync` 目前有 schema 但业务未使用（见 [v1-gap-analysis.md](v1-gap-analysis.md)）。
