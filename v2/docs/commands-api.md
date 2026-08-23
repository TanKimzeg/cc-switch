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

能力对齐 v1 `services/skill.rs`：仓库/ZIP 安装、skills.sh 搜索、SHA-256 更新检测、卸载自动备份 + 恢复、未管理导入、软链/复制分发、存储位置迁移。SSOT 目录由设置 `skills.storageLocation` 决定：`cc_switch` → **`~/.cc-switch/skills/`**（对齐 v1），`unified` → `~/.agents/skills/`。

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `skills_list` | — | `SkillRecord[]` | 列出全部技能（含启用插件与仓库/哈希字段） |
| `skills_install_local_dir` | `source: string` | `SkillRecord` | 从本地目录安装技能到 SSOT（兼容旧入口） |
| `skills_install_skill` | `skill: DiscoverableSkill`, `current_plugin` | `SkillRecord` | 从仓库下载安装并启用当前插件 |
| `skills_install_from_zip` | `file_path`, `current_plugin` | `SkillRecord[]` | 从本地 ZIP 安装（扫描含 `SKILL.md` 的目录，id=`local:*`） |
| `skills_uninstall` | `id` | `string \| null` | 卸载并自动备份到 `~/.cc-switch/skill-backups/`（对齐 v1），返回备份路径 |
| `skills_toggle_plugin` | `id`, `plugin_id`, `enabled` | `()` | 启用/停用并同步/移除 `plugin.skills_dir()`（按同步方式软链或复制） |
| `skills_discover` | — | `DiscoverableSkill[]` | 并发拉取全部启用仓库，扫描 `SKILL.md` 去重排序 |
| `skills_list_repos` | — | `SkillRepo[]` | 列出技能仓库（启动时种子 4 个默认仓库） |
| `skills_add_repo` | `owner`, `name`, `branch` | `SkillRepo` | 添加仓库（校验坐标，branch 空则默认 main） |
| `skills_remove_repo` | `owner`, `name` | `()` | 删除仓库 |
| `skills_search_skillsh` | `query`, `limit`, `offset` | `SkillsShSearchResult` | 搜索 skills.sh 公共注册表（GET `/api/search`） |
| `skills_check_updates` | — | `SkillUpdateInfo[]` | 按仓库分组比对本地/远端 SHA-256 |
| `skills_update_skill` | `id` | `SkillRecord` | 重新下载、备份旧版、替换 SSOT、重算哈希、重同步 |
| `skills_list_backups` | — | `SkillBackupEntry[]` | 列出技能备份（读 `skill-backups/*/meta.json`） |
| `skills_delete_backup` | `backup_id` | `()` | 删除备份 |
| `skills_restore_backup` | `backup_id`, `current_plugin` | `SkillRecord` | 从备份恢复并启用当前插件 |
| `skills_scan_unmanaged` | — | `UnmanagedSkill[]` | 扫描各插件 skills 目录 + SSOT 中未入库的技能 |
| `skills_import` | `imports: ImportSkillSelection[]` | `SkillRecord[]` | 导入所选技能（honor 用户勾选插件） |
| `skills_get_sync_settings` | — | `SyncSettings` | 读取同步方式 + 存储位置 |
| `skills_set_sync_method` | `method` | `()` | 设置同步方式（auto/symlink/copy） |
| `skills_migrate_storage` | `target` | `MigrationResult` | 迁移存储位置（先移文件后改设置） |

> 安全：仓库下载走 `https://github.com/{owner}/{name}/archive/refs/heads/{branch}.zip`，带坐标白名单校验 + 出口 URL 断言 + 60s 超时 + 128MiB 下载上限 + 解压预算（10k 条目 / 512MiB / 4KiB symlink / 目录计费）+ zip-slip 双层防护。错误为结构化 JSON（`{code, context, suggestion}`），前端 `skillsError.*` 文案渲染。

## 6. Prompts（prompts）

行为对齐 v1 `services/prompt.rs`：单插件单激活（互斥）+ 回填保护。记忆文件 = `plugin.prompt_file_path()`，原子写入。

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `prompts_list` | `plugin_id?` | `PromptRecord[]` | 列出 prompts（可按插件过滤） |
| `prompts_upsert` | `id`, `plugin_id`, `name`, `content`, `description?` | `()` | 新增/更新（新行默认禁用；启用项保存后立即重写记忆文件） |
| `prompts_delete` | `id` | `()` | 删除；已启用项拒绝（`无法删除已启用的提示词`） |
| `prompts_toggle` | `id`, `enabled` | `()` | 启用 = 回填 live 文件 + 互斥禁用同插件其他项 + 写文件；停用 = 仅当无其他启用项时清空文件（写 `""`） |

> 首启自动导入：`prompts` 表全空时，`init_db` 把各插件记忆文件导入为启用项（`auto-imported-*`）。

## 7. Profiles（profiles）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `profiles_list` | — | `Profile[]` | 列出配置方案 |
| `profiles_current` | — | `string \| null` | 当前激活的 profile id |
| `profiles_upsert` | `profile: Profile` | `()` | 新增/更新 |
| `profiles_delete` | `id` | `()` | 删除 |
| `profiles_apply` | `id` | `()` | 激活某 profile |
| `profiles_clear_current` | — | `()` | 清除当前激活 |

> ⚠️ **现状差距**：v2 的 `profiles_apply` 只记录 current_profile_id，**未真正把快照恢复到各插件 live**。v1 是「项目快照」（存/恢复某分组 provider/MCP/Skills/prompt 现场），见 [v1-gap-analysis.md](v1-gap-analysis.md) §3.10。

## 8. 备份 / 导入导出（backup）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `backup_create` | — | `BackupRecord` | 创建数据库备份（SQLite backup API 写入 `data_dir/backups/`，id 前缀 `bak_`） |
| `backup_list` | — | `BackupRecord[]` | 列出备份 |
| `backup_rename` | `id`, `name` | `()` | 重命名备份 |
| `backup_restore` | `id` | `string` | 恢复备份：先自动创建安全备份，再整库回灌；返回安全备份 id |
| `backup_delete` | `id` | `()` | 删除备份 |
| `export_config_json` | — | `ExportPayload` | 导出全部配置为 JSON |
| `export_config_to_file` | `path` | `()` | 导出配置 JSON 到文件 |
| `parse_export_json` | `content` | `ExportPayload` | 解析导出 JSON 文本 |
| `import_config` | `payload` | `usize` | 导入配置负载（逐表 upsert） |
| `import_config_from_file` | `path` | `usize` | 从 JSON 文件导入 |

> 自动备份：settings 键 `backup.intervalHours`（0=禁用，默认 24）、`backup.retainCount`（默认 10）、
> `backup.lastAutoAt`。启动即检查到期，此后每 30 分钟 tick 一次；按保留数轮换仅 `auto_` 前缀备份。

## 9. 宿主（host，仅 TS 插件）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `plugin_get_script` | `id`, `main` | `string` | 读取 TS 插件主脚本内容 |
| `host_read_file` | `id`, `path` | `string` | 读插件目录内文件（沙箱） |
| `host_write_file` | `id`, `path`, `content` | `()` | 写插件目录内文件（沙箱） |
| `host_list_files` | `id`, `dir?` | `string[]` | 列插件目录内容（沙箱） |
| `host_read_resource` | `id`, `name`, `rel?` | `string` | 读 manifest `resources` 白名单资源（可访问用户目录） |
| `host_write_resource` | `id`, `name`, `content`, `rel?` | `()` | 写白名单资源（自动建父目录） |
| `host_list_resource` | `id`, `name`, `rel?` | `string[]` | 列白名单资源目录 |

## 10. 设置（settings）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_setting` | `key` | `string \| null` | 读取应用级设置 |
| `set_setting` | `key`, `value` | `()` | 写入应用级设置 |
| `settings_get_app_behavior` | — | `AppBehavior` | 读取行为设置（showInTray/minimizeToTrayOnClose/silentStartup/launchOnStartup） |
| `settings_set_minimize_to_tray_on_close` | `enabled` | `()` | 关闭时最小化到托盘（主窗 CloseRequested 拦截实时读取） |
| `settings_set_silent_startup` | `enabled` | `()` | 静默启动（启动不显示主窗，仅托盘运行） |
| `settings_set_launch_on_startup` | `enabled` | `()` | 开机自启（tauri-plugin-autostart 同步注册/注销） |
| `settings_set_show_in_tray` | `enabled` | `()` | 托盘图标显隐（动态创建/移除，无需重启） |
| `settings_get_overrides` | — | `OverrideDir[]` | 列出已配置的工具配置目录覆盖 |
| `settings_set_override` | `plugin_id`, `path?` | `()` | 设置/清除工具配置目录覆盖（native 插件 `config_dir` 消费） |
| `get_app_data_dir_override` | — | `string \| null` | 读取 CC Switch 数据目录覆盖（指针文件 `app_paths.json`） |
| `set_app_data_dir_override` | `path?` | `bool` | 设置/清除数据目录覆盖（返回 true = 需重启生效） |
| `update_tray_menu` | — | `()` | 重建系统托盘菜单（provider 变更后调用） |

> 行为设置键：`app.showInTray` / `app.minimizeToTrayOnClose` / `app.silentStartup` / `app.launchOnStartup`
> （"1"/"0"，缺省对齐 v1：托盘显示、关闭最小化默认开；自启/静默默认关）。
