# 与 v1 的能力差距（Gap Analysis）

本文档对照 v1 的完整功能（见仓库根 `README_ZH.md` 与 `src-tauri/src/apps/*`、`src-tauri/src/services/*`），列出 v2 已具备 / 缺失的能力，并对每项缺失给出实现思路与优先级。

> 优先级说明：**P0** = 用户可直接感知的核心差距；**P1** = 重要增强；**P2** = 锦上添花/预留。

## 1. 总体对照

| 能力域 | v1 | v2 现状 | 差距 |
|--------|----|---------|------|
| Provider 配置与切换 | ✅ 8 工具、50+ 预设、一键切换、托盘 | ✅ 插件化 read_live/apply/import；native 六插件（opencode/openclaw/claudecode/codex/grokbuild/hermes）；托盘/重投影全后端 | 通用供应商缺失；预设**不做**（§3.7，用户决定）；gemini/claude-desktop 未插件化 |
| MCP | ✅ 统一面板、双向同步、Deep Link 导入 | ✅ mcp_servers + 插件同步 + 编辑/校验/预设/wizard(http)/智能粘贴/批量开关/搜索/删除确认；导入合并语义 + 取消勾选清理 + 安装守卫 + 切换后重投影 | 缺 Deep Link 导入（随 §3.5 一并做）|
| Skill | ✅ GitHub 仓库 / ZIP 安装、skills.sh 公共注册表、SHA-256 更新检测、备份恢复、软链/复制 | ✅ 已对齐（仓库/ZIP 安装、skills.sh 搜索、更新检测、卸载备份+恢复、未管理导入、软链/复制、存储迁移；SSOT 默认 `~/.cc-switch/skills`） | 已补齐（见 §3.9） |
| Prompt | ✅ Markdown 编辑、单应用互斥启用、回填保护 | ✅ 已对齐（互斥启用 + 回填保护 + 清空/删除保护 + 首启自动导入） | 已补齐（见 §3.11） |
| 用量 | ✅ 用量仪表盘、趋势、请求日志、自定义定价 | ✅ 趋势图 + 请求日志 + 日汇总 + **PricingService 定价核心（§3.3）** + codex/grokbuild 用量同步（§3.20） | hermes 用量（v1 也没有） |
| 会话 | ✅ 浏览/搜索/恢复，SQLite 会话 | ✅ sessions/load/delete（claude/opencode） | 缺搜索、更多 Agent 会话源 |
| **代理（proxy）** | ✅ 本地代理热切换、故障转移、熔断器 | ❌ **无** | **暂不考虑实现** |
| 余额/订阅 | ✅ balance、subscription、grok 订阅 | ❌ 无 | P1 |
| 云端同步 | ✅ Dropbox/OneDrive/iCloud/WebDAV/S3 | ❌ 无 | P1 |
| Deep Link | ✅ `ccswitch://` 导入 | ❌ 无 | P1 |
| 托盘快捷切换 | ✅ | ✅ 插件→provider 两级菜单 + 勾选当前 + 切换即重建；托盘显隐可设置 | 已补齐（见 §3.6） |
| 配置方案 Profile | ✅ 项目级配置快照（供应商/MCP/Skills/记忆文件），一键应用 | ⚠️ 仅 profiles 表 CRUD 存 JSON，apply 未真正恢复现场 | **缺快照语义 + 应用到 live** |
| 备份/导入导出 | ✅ 自动备份、导入导出 | ✅ db_backups + export/import + 恢复（安全备份）/重命名/自动备份 interval+retain 轮换 | 已补齐（见 §3.14） |
| 工作区编辑器（OpenClaw） | ✅ AGENTS.md/SOUL.md 编辑 | ❌ 无 | P2 |
| 速度测试 / 健康监控 | ✅ SpeedtestService、供应商健康 | ❌ 无 | P2 |
| 系统设置 | 自定义配置目录、override 目录、主题/语言/窗口行为等 | ✅ Tab 化（通用/高级）：主题三态、语言切换(zh/en)、Skills 存储/同步方式、目录覆盖；窗口行为（自启/静默启动/关闭最小化托盘/托盘显隐） | 已补齐（见 §3.17、§3.18） |
| UI 外壳/设计风格 | 64px header、全屏面板、动画、快捷键 | 设计 token/ui 组件同源；外壳范式差异保留（v2 顶栏 nav + 左侧插件栏） | 外壳重构另轮处理（方向：v1 header + 保留侧栏） |

## 2. 已对齐的能力（v2 已具备）

- **Provider 配置与切换**：`read_live/apply/remove_provider/import` 全链路（`opencode` additive、`claudecode` 非 additive），支持 `sync_all_providers_to_live`（全量投影）与 `import_providers_from_live`（回填）。
- **MCP 统一管理**：`mcp_servers` + `mcp_server_apps` 统一面板，写操作经 `McpPlugin` 同步到启用插件；支持从插件导入。
- **Skill**：SSOT（`~/.cc-switch/skills` 或 `~/.agents/skills`）+ 仓库/ZIP 安装 + skills.sh 搜索 + SHA-256 更新检测 + 卸载备份/恢复 + 未管理导入 + 软链/复制分发 + 存储位置迁移（`skills_dir()` 由插件声明）。
- **Prompt 基础**：prompts 表 CRUD + 启用时写入 `prompt_file_path()`。
- **用量基础**：`request_logs`（`INSERT OR IGNORE` 去重）+ 按日汇总；native 插件实现 `sync_usage`。
- **会话基础**：claudecode（`~/.claude/projects/*.jsonl`）、opencode（SQLite + 旧 JSON）的扫描/加载/删除。
- **配置方案 / 备份 / 导入导出**：profiles CRUD + db 备份 + export/import JSON。

## 3. 待补齐的能力（每项附实现思路）

### 3.1 本地代理（proxy）—— ~~P0~~ **暂不考虑实现（用户决定，2026-08-16）**

> v2 通过「直连 provider + 插件协议」覆盖切换需求；代理的格式转换/故障转移/熔断后续按需评估。本节保留 v1 描述作参考，不再排期。

**v1**：`services/proxy.rs`（28 万字节！）+ `proxy/providers/*`。核心是本地代理服务器：把 Agent 的 API 请求转发到真实 provider，过程中做**格式转换、故障转移、熔断、供应商健康监控**。Claude / Codex / Gemini / Grok Build 各有一个 `ProviderAdapter`。

**v2 现状**：完全缺失。`AgentPlugin` 无 `proxy_adapter`。

**实现思路**：
1. 在 `AgentPlugin` trait 增加 `fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>>`（默认 None），native 插件按需实现；`ProviderAdapter` 用 v1 的 `proxy/providers` 那套线协议抽象。
2. 新增 `services/proxy.rs`：本地 HTTP 服务器（Tauri 侧可复用 tokio/hyper），接收 Agent 的请求 → 按当前 provider 路由 → 记录用量到 `request_logs` → 返回。
3. 故障转移/熔断可先用简单策略（按状态码/超时切到备用 provider），后续再补健康监控与整流器。
4. 前端加「代理开关 / 端口配置」面板。
> 体量最大，建议作为独立里程碑；先支持 1 个 Agent（如 claude）跑通闭环。

### 3.2 余额 / 订阅查询（balance / subscription）—— P1

**v1**：`services/balance.rs`、`subscription.rs`、`subscription_grok.rs`、`usage_script.rs`——配置用量查询脚本，查询 API 余额/订阅状态。

**实现思路**：
1. `settings` 表存「用量查询脚本 + base URL + key」。
2. 后端命令 `balance_query(plugin_id)` 执行脚本（或按 provider 约定请求余额接口）。
3. 前端「用量」面板展示余额卡片。

### 3.3 模型定价接线（PricingService）—— ✅ **已实现（2026-08-24，超越 v1）**

**v1**：`services/model_pricing.rs` 四档平价表 + models.dev 同步；定价计算在四个 `session_usage_*` 模块各抄一遍（改动需同步四处，是 v1 bug 温床）。

**v2 现状**：✅ 已对齐并**超越 v1**（`services/pricing.rs`，全项目唯一成本计算方）：
- **schema**：`model_pricing` 重建为 `model_match`（精确+前缀匹配）/`provider_scope`（供应商限定覆盖行）/四档单价（十进制字符串）/`off_peak_discount_percent` + UTC 窗口（可跨午夜）/`source`。
- **匹配链**：供应商限定精确 > 供应商限定最长前缀 > 通用精确 > 通用最长前缀。
- **峰谷**（v1 没有）：DeepSeek 错峰场景——折扣百分比 + UTC HH:MM 窗口，按请求时间判断。
- **中转站差价**（v1 没有）：为该 provider 建限定行直接填实际单价。
- **精度**：整数微美元运算，无浮点误差（沿用 v1 十进制字符串存储的做法）。
- **接线**：用量写入时零成本记录自动补算；`usage_recompute_costs` 回填历史（不覆盖插件自带成本如 opencode）。
- **UI**：用量面板「模型定价」管理（列表/编辑含峰谷/删除/models.dev 同步/回填）。
- **测试**：DeepSeek 全场景钉死（分档/跨午夜/覆盖行/前缀回退/迁移）。

### 3.4 云端同步（WebDAV / S3 / 目录）—— P1

**v1**：`services/webdav.rs`、`webdav_sync.rs`、`webdav_auto_sync.rs`、`s3.rs`、`s3_sync.rs`、`s3_auto_sync.rs`，以及「自定义配置目录」（Dropbox/OneDrive/iCloud）。

**实现思路**：
1. 复用 v1 的 WebDAV/S3 客户端逻辑（`webdav.rs`/`s3.rs` 是纯 Rust 依赖，可直接迁移）。
2. 新增 `services/sync.rs`：把 SQLite 数据库文件（或导出的 JSON）上传/下载到远端，支持定时/手动同步。
3. `settings` 表存 WebDAV URL/凭据、S3 配置。
> 若目标只是「同步数据库」，`export_config_json` + 远端写入即可先跑通。

### 3.5 Deep Link（`ccswitch://`）—— P1

**v1**：`deeplink` 模块 + `deplink.html`——通过 URL 导入供应商、MCP 服务器、提示词、技能（Base64 JSON/TOML）。

**实现思路**：
1. 注册 `ccswitch://` 协议（Tauri `deep-link` 插件或 OS 注册表）。
2. 新增 `commands/deeplink.rs`：解析 URL 参数（`action`/`config`/`apiKey` 等），调用现有 `add_provider`/`mcp_upsert`/`prompts_upsert`/`skills_install_local_dir` 落库。
3. 复用 v1 的 Base64 编码/解析逻辑。

### 3.6 系统托盘快捷切换 —— ✅ **已实现（2026-08-16）**

**v1**：`tray.rs` 托盘菜单可直接切换当前 provider。

**v2 现状**：✅ `v2/src-tauri/src/tray.rs` 插件 → provider 两级菜单（对齐 v1）：
- 每个「可后端切换」的插件（`capabilities.apply` 且入口非 TS）一个子菜单，标题 = `插件名 · 当前provider`。
- 子菜单内每个 provider 一个 `CheckMenuItem`（勾选 = 当前）；无 provider 的插件显示禁用项 `插件名 (无供应商)`。
- 点击 `switch_{provider_id}` → 复用 `switch_provider_core`（apply current=true + 记录 app_state）→ 就地重建菜单 → 广播 `provider-switched`。
- `update_tray_menu` 命令：前端在 add/update/delete/switch/sync/import provider 后调用（`api.ts` `refreshTray`）。
- 菜单数据由纯函数 `build_menu_spec` 生成（可单测）；`show`/`quit`/左键显示窗口/静态 tooltip 保留。
- TS 插件（如 claudecode 示例）因后端无法执行脚本，托盘切换不含（`build_menu_spec` 排除）。

### 3.7 供应商预设（50+）—— ~~P1~~ **不做（用户决定，2026-08-23）**

**v1**：内置 50+ 供应商预设（AWS Bedrock、NVIDIA NIM、社区中转等），复制 key 一键导入。

> **不做的理由**：v1 的预设列表本质是推广位（置顶伙伴带 aff 推广链接），是商业化赞助机制的产物；本项目无赞助、未来可能转 GPL，不复制这套数据结构。用户经 JSON 编辑器添加 provider 即可。
> 注：**MCP 预设**（无推广性质的常用服务器模板）已随 MCP 面板对齐落地（`src/config/mcpPresets.ts`），保留。

### 3.19 Claude Code 原生内置 + 统一能力视图 —— ✅ **已实现（2026-08-24）**

**背景**：`plugin/claudecode.rs` 早已实现完整 native 能力（Provider/MCP/Sessions/Usage/Prompt/Skills），但内置清单只有 openclaw/opencode，用户管理 Claude Code 只能手装 TS 示例 → 托盘排除 TS、后端 MCP 守卫/重投影/导入合并全部绕过（走前端 shim）。

**落地**：
1. **第三个内置插件**：`plugins/claudecode/manifest.json`（native，capabilities 全 true，声明 promptFile/skillsDir）。seed 每次启动覆盖 manifest → 已安装的 TS 版自动升级为 native（providers/prompts/skills 按 plugin_id 无缝继承，残留 main.js 无害），seed 时 log 提示。
2. **TS 示例改 id**：`examples/plugins/claudecode` → `claudecode-ts`（native 版独占 `claudecode` id，一等公民；两者可共存）。
3. **安装来源升级**：`sync_installs` 把内置 id 的既有 local 记录改标 builtin。
4. **统一能力视图**：`list_installed` 对 native 回填 capabilities（与路径能力同源，防声明漂移）+ 新增 `backendSwitchable`（apply 且非 TS）计算字段；前端 SettingsPanel 目录覆盖列表改用该字段（此前各面板自行拼 `apply && entryType !== "ts"`）。
5. **ProviderForm 兜底**：非 opencode 插件默认 raw JSON 编辑器（结构化字段是 OpenCode additive 形状专用）。

**自动收益**：托盘出现 Claude Code 切换子菜单；MCP 安装守卫/切换重投影/导入合并对 Claude 生效；用量同步与会话走后端。

### 3.20 Codex / Grok Build / Hermes 原生内置 —— ✅ **已实现（2026-08-24，Usage 暂缺）**

**v1**：三工具均为一等应用（provider 切换 + MCP + 会话 + Skills/Prompt；codex/grokbuild 另有 proxy 适配器——v2 proxy 不做）。

**v2 现状**：✅ 三个新 native 内置插件（`plugin/codex.rs` / `grokbuild.rs` / `hermes.rs`，语义照抄 v1 对应模块）：

| 插件 | Provider 语义 | MCP | 会话 |
|---|---|---|---|
| `codex` | 非 additive 整文档：`{"auth":{},"config":"<toml>"}` → `~/.codex/config.toml` + `auth.json`（先 auth 后 config，失败回滚 auth）；remove 不支持（对齐 v1） | `[mcp_servers.*]`（toml_edit 保注释；`http_headers`↔`headers`；扩展字段白名单+通用转换；容错读 `mcp.servers`） | `sessions/`+`archived_sessions/` rollout jsonl（session_meta/首条用户消息标题含 VS Code 上下文提取/subagent 过滤/session_index.jsonl 标题/resume 命令） |
| `grokbuild` | 非 additive：`{"config":"<toml>"}` → `~/.grok/config.toml`；强校验 `[models]+[model.<name>]`（官方 category 允许空文档回落 OAuth）；remove 不支持 | 同布局但无 `type`、`headers`（复用 codex 转换器后剥离） | `summary.json`（info.id/cwd/title）+ `chat_history.jsonl`（reasoning 不算消息）；删除=删目录（id+目录名校验） |
| `hermes` | additive：`custom_providers:` 序列 upsert（models 数组↔字典、camelCase 治理、保留盘上未知字段）+ `model.provider/default` 切换；`providers:` 字典只读（`_cc_source` 标记，写删报错）；支持 remove | `mcp_servers:` YAML mapping（section 级替换保注释、CRLF/重复键修复；merge 保留 Hermes 特有字段；enabled 自动加/剥离） | `state.db`（sessions/messages 表，sqlite: 源）∪ `sessions/*.jsonl`（id 冲突 sqlite 优先）；删除双路（sqlite 校验路径归属） |

**共享设施**：`plugin/session_utils.rs`（v1 会话工具移植：head/tail 读取、时间戳解析、文本提取、截断）；Cargo 新增 toml/toml_edit/serde_yaml/regex。

**目录覆盖**：三插件 `config_dir()` 接入 `overrideDir.<id>`（优先）→ 环境变量（`CC_SWITCH_CODEX_CONFIG_DIR` / `CC_SWITCH_GROK_CONFIG_DIR` / `HERMES_HOME`）→ 平台默认（hermes Windows 为 `%LOCALAPPDATA%\hermes`，对齐 Hermes 自身）。

**已知差距（后续补）**：
- ~~**Usage 同步**~~：✅ codex/grokbuild 已实现（2026-08-24，简化口径：codex 取最后一次 `total_token_usage` 会话累计快照 + `usage_upsert` 刷新语义；grokbuild 解析 `updates.jsonl` 的 `turn_completed` 逐轮事件，`usage_snapshot` 剔除防双算；成本统一由 PricingService 补算，input 口径 = 总输入 − 缓存命中）。**hermes 无独立用量源（v1 也没有），明确不做**。v1 的 turn 级增量/fork 解析（session_usage_codex 3086 行）未照搬——如遇口径偏差再按需补。
- codex 会话标题的 state DB 增强源（`~/.codex` 下 SQLite threads 表）未接，仅 session_index.jsonl。
- codex 切换的 model catalog 生成与 OAuth 模型目录（v1 为 proxy 服务，随 proxy 一并评估）。

### 3.8 通用供应商（一份配置同步多 Agent）—— P1

**v1**：一份通用配置同步到 Claude Code / Codex / Gemini CLI。

**实现思路**：
1. `providers` 表新增「通用」概念（或 meta 标记 `universal: true`）。
2. `sync_all_providers_to_live` 遍历时，把该 provider 投影到多个插件的 live 配置（复用各插件的 `apply`）。

### 3.9 Skill：仓库安装 / skills.sh / 更新检测 / 备份恢复 / 软链 —— ✅ **已实现（2026-08-15）**

**v1（完整能力，见 `docs/user-manual/zh/3-extensions/3.3-skills.md`）**：
- **SSOT 存储**：技能源存在 `~/.cc-switch/skills/`，分发到各应用 `~/.claude/skills/`、`~/.codex/skills/`、`~/.gemini/skills/`、`~/.config/opencode/skills/`、`~/.hermes/skills/`。
- **预配置仓库**：Anthropic 官方、ComposioHQ、社区精选等 GitHub 仓库。
- **仓库管理**：添加/删除自定义 GitHub 仓库（owner/name/branch/subdirectory）。
- **skills.sh 公共注册表搜索**：输入关键词搜索社区 skill，点击安装。
- **SHA-256 更新检测**：比对本地与远端内容哈希，自动标记「有新版本」，支持单项/全部更新。
- **卸载自动备份 + 从备份恢复**：卸载前备份到 `~/.cc-switch/skill-backups/`，可恢复。
- **软链 / 复制两种分发方式**（个性化设置）。

**v2 现状**：✅ 已对齐（`src-tauri/src/services/skills.rs` + `src/components/skills/`）。插件化差异：v1 的「应用」→ v2 的「插件」（`skill_apps` 表按插件 id），分发目标 = `AgentPlugin::skills_dir()`。

**实现要点（已落地）**：
1. `skills_install_from_zip(file_path, current_plugin)`：解压 → 扫描 `SKILL.md` → 复制到 SSOT，id=`local:*`。
2. `skills_install_skill(skill, current_plugin)`：下载 GitHub zip（`archive/refs/heads/{branch}.zip`，分支回退）→ 解压预算 + zip-slip 防护 → 解析源目录 → 复制 SSOT。
3. `skills_list_repos` / `skills_add_repo` / `skills_remove_repo`：仓库 CRUD；启动种子 4 个默认仓库。
4. `skills_search_skillsh(query, limit, offset)`：skills.sh `/api/search`。
5. 更新检测：`skills_check_updates`（按仓库分组一次下载比对）+ `skills_update_skill`（备份→替换→重算→重同步）。
6. 卸载自动备份到 `~/.cc-switch/skill-backups/` + `skills_list_backups` / `skills_restore_backup` / `skills_delete_backup`（保留 20 份）。
7. 分发：`skills_set_sync_method`（auto 优先软链回退复制 / symlink / copy）。
8. 存储位置：`skills_migrate_storage`（`~/.cc-switch/skills` ↔ `~/.agents/skills`，先移文件后改设置）。
9. 未管理导入：`skills_scan_unmanaged` + `skills_import`（honor 用户勾选插件）。
10. 安全：`validate_repo_ref` + 出口 URL 断言 + 60s 超时 + 128MiB 下载上限 + 解压预算（10k 条目/512MiB/4KiB symlink/目录计费）+ `require_valid_directory` 脏值拦截 + 结构化错误。

> 说明：SSOT 默认路径为 `~/.cc-switch/skills/`（与 v1 完全一致），UI 文案沿用「CC Switch」；`~/.agents/skills` 为可切换存储位置。`~` 解析走 `CC_SWITCH_TEST_HOME`（测试可隔离）。

### 3.10 Profile（配置方案）：对齐 v1「项目快照」语义 —— **P1（用户点名）**

**v1（`src/components/profiles/`）**：Profile 是**项目级配置快照**——把某应用分组（Claude 组 / Codex 组）当前的供应商、MCP、Skills、记忆文件（prompt）快照存为命名 profile，可一键切换回某项目配置。UI 是 header 的 ProfileSwitcher（"从当前创建"、下拉切换、管理对话框）。**切换会真正把快照恢复到各应用的 live 配置**。

**v2 现状**：`profiles` 表 CRUD 存 JSON payload；`profiles_apply` 只把 id 写进 `settings.current_profile_id`，**未真正应用到 live**——本质是占位，不是 v1 的项目快照。

**实现思路**：
1. `profiles_upsert` 生成快照：遍历某插件分组的 provider/MCP/skills/prompt 实际状态，序列化为 payload（对齐 v1 的 snapshot 结构）。
2. `profiles_apply`：解析 payload → 恢复 provider（`apply`/`set_current_provider`）、MCP（`mcp_upsert` + 同步）、skills（`skills_toggle_plugin`）、prompt（`prompts_toggle`）到 live。
3. 前端：header 加 ProfileSwitcher（对齐 v1：当前分组显示当前 profile、从当前创建、下拉切换、管理对话框）；`GlobalPanels` 的 ProfilesPanel 改为承载该交互。
4. 快照按插件分组（Claude 组 vs Codex 组各自独立 current），对齐 v1 `APP_PROFILE_SCOPE`。

### 3.11 Prompt：互斥启用 + 回填保护 —— ✅ **已实现（2026-08-16）**

> **语义澄清**：v1 并没有真正的「跨应用同步」——prompts 按应用隔离，同一个提示词要跨应用需分别创建（见 v1 `docs/user-manual/zh/3-extensions/3.2-prompts.md`）。v1 的机制是**单应用单激活（互斥）** + **回填保护**。

**v1 能力**：
- Markdown 编辑（CodeMirror）+ 全屏表单（名称/描述/内容）。
- 启用 prompt = 回填当前 live 记忆文件 → 互斥禁用同应用其他 prompt → 写目标内容到文件（原子写）。
- 回填保护：live 文件非空时，回填到已启用项，或创建禁用备份「原始提示词 …」。
- 停用最后一个启用项时清空文件；已启用项不可删除。
- 首次启动全表为空时自动导入各应用记忆文件为启用项。

**v2 现状**：✅ 已对齐（`src-tauri/src/services/prompts.rs` `PromptService` + `src/components/prompts/`）。插件化差异：v1 的「应用」→ v2 的「插件」；记忆文件路径 = `AgentPlugin::prompt_file_path()`。

**实现要点（已落地）**：
1. `PromptService::enable`：回填（已启用项 or 备份 `backup-*`）→ 互斥禁用 → 启用 → 原子写文件（temp+rename）。
2. `PromptService::disable`：唯一启用时清空文件（写 `""`）。
3. `PromptService::save`：启用项保存后立即重写记忆文件。
4. `PromptService::delete`：已启用项拒绝删除（`无法删除已启用的提示词`）。
5. 首启自动导入：`init_db` 时 `prompts` 表全空 → 遍历各插件 `prompt_file_path()` → 导入为启用项（`auto-imported-*`）。
6. 新 prompt 默认禁用（`upsert_prompt` INSERT 置 `enabled=0`，对齐 v1）。
7. 前端：switch 开关列表 + 计数/已启用 header + 搜索 + 全屏编辑对话框（MarkdownEditor）+ 删除确认 + toasts（`prompts.*` 文案）。

### 3.12 用量仪表盘（趋势图表、请求日志页）—— ✅ **已实现（趋势图部分）**

**v1**：趋势图表、详细请求日志、自定义模型定价。

**v2 现状**：✅ `UsagePanel`（日汇总 + 请求日志）内置 `UsageTrendChart`（多系列 token 面积图 + 成本虚线 + 悬停 tooltip + 刻度）；**自定义模型定价（model_pricing 接线）仍缺**，见 §3.3。

### 3.13 会话搜索与更多会话源 —— P2

**v1**：浏览/搜索/恢复；codex 的 SQLite 会话、grok 等。

**v2 现状**：claudecode（jsonl）、opencode（sqlite+json）。

**实现思路**：后续新增 Agent 时实现 `sessions()`；前端加搜索框过滤 `title`/`project_dir`。

### 3.14 自动备份轮换 —— ✅ **已实现（2026-08-23）**

**v1**：自动备份 + 轮换（保留 N 份）。

**v2 现状**：✅ 已对齐（`services/backup.rs` `auto` 模块）：
- settings 键 `backup.intervalHours`（0=禁用，默认 24）/ `backup.retainCount`（默认 10）/ `backup.lastAutoAt`。
- 启动即检查到期，此后每 30 分钟 tick；`auto_` 前缀备份按保留数轮换（手动备份不受影响）。
- 备份创建改用 SQLite backup API（原文件复制在 WAL 模式下会丢未 checkpoint 页，正确性修复）。
- 新命令 `backup_restore`（恢复前自动建安全备份，回灌后补回安全备份记录）、`backup_rename`。
- 前端 BackupPanel：间隔/保留数量 Select、恢复/重命名/删除确认。

### 3.18 系统设置：Tab 化 + 行为设置 —— ✅ **已实现（2026-08-23）**

**v1**：SettingsPage 六 Tab（general/proxy/auth/advanced/usage/about）；通用 Tab 含语言/主题/Skills/窗口行为等分区。

**v2 现状**：✅ 已对齐（本轮范围，proxy/auth/usage/about 随对应能力落地再补 Tab）：
- **Tab 骨架**：通用（语言/主题/Skills 存储/Skills 同步方式/窗口行为）+ 高级（配置目录 Accordion，对齐 v1 advanced.configDir 分组）。
- **主题切换**：ThemeSettings 三态按钮（theme-provider 与 v1 同源，原生标题栏同步）。
- **语言切换**：LanguageSettings（zh/en），i18n 初始化对齐 v1（localStorage → navigator 探测 → 回退 zh）；zh-TW/ja 待 locale 补齐后放开。
- **窗口行为**：开机自启（tauri-plugin-autostart）+ 条件显示静默启动、关闭时最小化到托盘（CloseRequested 拦截实时读设置）、托盘图标显隐（动态创建/移除）。键见 commands-api.md §10。
- **MCP 面板功能对齐**：编辑已有服务器、预设 chips（mcpPresets 移植含 Windows cmd /c npx 包装）、http 类型向导、智能粘贴（mcpServers 包装识别）、后端结构校验 validate_server_spec、搜索白名单（排除 env/headers 凭据）、按 app 批量开关、元数据字段+行内展示、删除确认+写锁。

### 3.15 工作区编辑器（OpenClaw）—— P2

**v1**：编辑 OpenClaw 的 AGENTS.md / SOUL.md，Markdown 预览。

**实现思路**：`AgentPlugin` 加 `config_dir()` 返回目录，前端 Markdown 编辑器列出并编辑该目录下的 Agent 文件。

### 3.16 速度测试 / 供应商健康监控 —— P2

**v1**：SpeedtestService 测 API 端点延迟；proxy 的供应商健康监控。

**实现思路**：新增 `speedtest` 命令对 provider 的 baseURL 发 ping 请求测延迟；健康监控可挂在 proxy（3.1）之上。

### 3.17 系统设置：配置目录覆盖 —— ✅ **已实现（2026-08-16）**

**v1**：`settings.rs` 的 `*_config_dir` 字段（claude/codex/gemini/grok/opencode/openclaw/hermes）+ `app_store.rs` 的 CC Switch 数据目录覆盖（`app_paths.json`）。

**v2 现状**：✅ 已对齐（`src-tauri/src/services/overrides.rs` + `SettingsPanel`）：

**工具配置目录覆盖**：
1. settings 表键 `overrideDir.<plugin_id>` 存原始路径（`~` 读取时展开），静态注册表 `overrides::get(id)` 供 native 插件 `config_dir()` 读取（优先于 env，回退默认）。
2. opencode/claudecode/codex/grokbuild/hermes 的 `config_dir()` 已接入（codex/grokbuild 另支持 `CC_SWITCH_*_CONFIG_DIR` 环境变量、hermes 支持 `HERMES_HOME`，对齐 v1 解析顺序）；`config_path/skills_dir/prompt_file_path` 自动跟随；claudecode `mcp_path` 特殊处理（自定义目录 → `<dir>/.claude.json`，对齐 v1）。
3. 命令 `settings_get_overrides` / `settings_set_override`；设置后前端调用 `syncAllProvidersToLive` 重写当前 provider 到新 live。
4. TS 插件（manifest 声明路径）暂不支持 override，文档注明。

**CC Switch 数据目录覆盖**：
1. 指针文件 `{app_config_dir}/app_paths.json` 存 `appDataDirOverride`（`app_config_dir` 独立于数据目录，避免鸡生蛋）；`init_db` 在打开数据库前读取，目录不存在时回退默认。
2. 命令 `get/set_app_data_dir_override`（set 返回需重启）；前端显示重启对话框（`@tauri-apps/plugin-process` `relaunch`）。
3. DB 与数据库备份走 `AppPaths.data_dir`（自动跟随数据目录覆盖）；**skills SSOT 固定 `~/.cc-switch/skills`、备份固定 `~/.cc-switch/skill-backups`，不随数据目录覆盖移动**（对齐 v1）。

## 4. TS 插件沙箱演进（架构差距，非 v1 对齐）

- **现状（已实现方案 A）**：manifest `resources` 白名单 + 后端通用资源命令 `host_read/write/list_resource` 已落地，TS 插件可管理声明的用户目录资源；宿主文件 I/O 仍限定插件目录 + 白名单（见 [ts-plugin.md](ts-plugin.md)）。
- **方向（方案 B，未实现）**：声明式插件——manifest 声明 config 路径 + 格式，后端用通用解析器实现常见 Agent，80% 场景免写代码。
- **与 v1 关系**：v1 没有 TS 插件（全是 native descriptor）。此差距是 v2 新增的能力设计问题，不影响 v1 对齐。

## 5. 建议实施顺序

1. ~~**P0**：本地代理（3.1）~~ —— **暂不考虑实现**（用户决定）。
2. ~~**P1 快速项**：模型定价接线（3.3）~~ —— ✅ 已完成（§3.3，超越 v1）；~~**供应商预设（3.7）~~ —— **不做**（用户决定）。**用量图表（3.12）、托盘切换（3.6）、设置 Tab 化/主题语言/窗口行为/MCP 对齐/备份增强（§3.14、§3.18）、codex/grokbuild 用量同步（§3.20）已完成**。
3. **P1 中型项**：余额/订阅（3.2）、Deep Link（3.5，含 MCP Deep Link 导入，**到时重新设计解析层不照抄 v1**）、通用供应商（3.8）、云端同步（3.4）、**Profile 项目快照（3.10）**、**外壳重构（v1 header + 保留侧栏）**、gemini/claude-desktop 插件化。（**Skill 仓库/skills.sh（3.9）、Prompt 互斥+回填（3.11）、Claude Code 原生内置 + 统一能力视图（§3.19）、Codex/Grok Build/Hermes 原生内置（§3.20）已完成**）
4. **P2**：会话搜索（3.13）、工作区（3.15）、速度测试（3.16）。
5. **TS 沙箱**：方案 A → B，作为贯穿性的架构演进。
