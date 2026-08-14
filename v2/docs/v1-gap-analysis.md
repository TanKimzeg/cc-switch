# 与 v1 的能力差距（Gap Analysis）

本文档对照 v1 的完整功能（见仓库根 `README_ZH.md` 与 `src-tauri/src/apps/*`、`src-tauri/src/services/*`），列出 v2 已具备 / 缺失的能力，并对每项缺失给出实现思路与优先级。

> 优先级说明：**P0** = 用户可直接感知的核心差距；**P1** = 重要增强；**P2** = 锦上添花/预留。

## 1. 总体对照

| 能力域 | v1 | v2 现状 | 差距 |
|--------|----|---------|------|
| Provider 配置与切换 | ✅ 8 工具、50+ 预设、一键切换、托盘 | ✅ 插件化 read_live/apply/import；native 两插件 | 预设/托盘/通用供应商缺失 |
| MCP | ✅ 统一面板、双向同步、Deep Link 导入 | ✅ mcp_servers + 插件同步 | 缺 Deep Link 导入、部分 Agent 原生适配 |
| Skill | ✅ GitHub 仓库 / ZIP 安装、软链/复制 | ✅ 本地目录安装 + 按插件复制 | 缺仓库/ZIP 安装、软链 |
| Prompt | ✅ Markdown 编辑、跨应用同步、回填保护 | ✅ prompts 表 + 写插件文件 | 缺跨应用一键同步、回填保护 |
| 用量 | ✅ 用量仪表盘、趋势、请求日志、自定义定价 | ✅ request_logs + 日汇总 + sync_usage | 缺仪表盘图表、model_pricing 接线 |
| 会话 | ✅ 浏览/搜索/恢复，SQLite 会话 | ✅ sessions/load/delete（claude/opencode） | 缺搜索、更多 Agent 会话源 |
| **代理（proxy）** | ✅ 本地代理热切换、故障转移、熔断器 | ❌ **无** | **P0 大项** |
| 余额/订阅 | ✅ balance、subscription、grok 订阅 | ❌ 无 | P1 |
| 云端同步 | ✅ Dropbox/OneDrive/iCloud/WebDAV/S3 | ❌ 无 | P1 |
| Deep Link | ✅ `ccswitch://` 导入 | ❌ 无 | P1 |
| 托盘快捷切换 | ✅ | ❌（仅托盘创建窗口） | P1 |
| 配置方案 Profile | ✅ | ✅ profiles 表 + CRUD | 基本对齐（缺 apply 到 live 的完整链路） |
| 备份/导入导出 | ✅ 自动备份、导入导出 | ✅ db_backups + export/import | 基本对齐（缺自动备份轮换） |
| 工作区编辑器（OpenClaw） | ✅ AGENTS.md/SOUL.md 编辑 | ❌ 无 | P2 |
| 速度测试 / 健康监控 | ✅ SpeedtestService、供应商健康 | ❌ 无 | P2 |
| 系统设置 | 自定义配置目录、override 目录 | ✅ settings 键值 | 缺 override 目录支持 |

## 2. 已对齐的能力（v2 已具备）

- **Provider 配置与切换**：`read_live/apply/remove_provider/import` 全链路（`opencode` additive、`claudecode` 非 additive），支持 `sync_all_providers_to_live`（全量投影）与 `import_providers_from_live`（回填）。
- **MCP 统一管理**：`mcp_servers` + `mcp_server_apps` 统一面板，写操作经 `McpPlugin` 同步到启用插件；支持从插件导入。
- **Skill 基础**：SSOT 本地目录安装 + 按插件复制/移除（`skills_dir()` 由插件声明）。
- **Prompt 基础**：prompts 表 CRUD + 启用时写入 `prompt_file_path()`。
- **用量基础**：`request_logs`（`INSERT OR IGNORE` 去重）+ 按日汇总；native 插件实现 `sync_usage`。
- **会话基础**：claudecode（`~/.claude/projects/*.jsonl`）、opencode（SQLite + 旧 JSON）的扫描/加载/删除。
- **配置方案 / 备份 / 导入导出**：profiles CRUD + db 备份 + export/import JSON。

## 3. 待补齐的能力（每项附实现思路）

### 3.1 本地代理（proxy）—— P0（最大差距）

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

### 3.3 模型定价接线（model_pricing）—— P1

**v1**：`services/model_pricing.rs` 维护模型价格表，按 token 计算成本。

**v2 现状**：`model_pricing` 表存在但未使用；`sync_usage` 由插件返回 `cost`（claudecode 返回 0.0，opencode 从 db 读）。

**实现思路**：
1. `model_pricing` 表预置常见模型价格（`input/output/cache_read/cache_creation` 每百万）。
2. `usage_daily_summary` / `request_logs` 写入时，若 `total_cost_usd=0` 则按 `model_pricing` 计算。
3. 前端用量面板支持自定义模型价格编辑（v1 有该 UI）。

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
2. 新增 `commands/deeplink.rs`：解析 URL 参数（`action`/`config`/`apiKey` 等），调用现有 `add_provider`/`mcp_upsert`/`prompts_upsert`/`skills_install` 落库。
3. 复用 v1 的 Base64 编码/解析逻辑。

### 3.6 系统托盘快捷切换 —— P1

**v1**：`tray.rs` 托盘菜单可直接切换当前 provider。

**v2 现状**：`tray.rs` 只创建窗口/单实例，无切换菜单。

**实现思路**：在 `tray.rs` 构建插件 → provider 两级菜单，点击调用 `switch_provider`；切换后重建菜单。

### 3.7 供应商预设（50+）—— P1

**v1**：内置 50+ 供应商预设（AWS Bedrock、NVIDIA NIM、社区中转等），复制 key 一键导入。

**实现思路**：
1. 新增 `services/presets.rs`（或 JSON 资源文件）：预设 provider 的 `settings_config` 模板（按 npm 包 + options）。
2. 前端「添加供应商」下拉可选预设，预填 `settings_config`。
> 预设本质是静态数据，工作量小、收益直接。

### 3.8 通用供应商（一份配置同步多 Agent）—— P1

**v1**：一份通用配置同步到 Claude Code / Codex / Gemini CLI。

**实现思路**：
1. `providers` 表新增「通用」概念（或 meta 标记 `universal: true`）。
2. `sync_all_providers_to_live` 遍历时，把该 provider 投影到多个插件的 live 配置（复用各插件的 `apply`）。

### 3.9 Skill：GitHub 仓库 / ZIP 安装、软链 —— P2

**v1**：从 GitHub 仓库或 ZIP 文件一键安装技能；支持软连接与文件复制两种模式。

**v2 现状**：`skills_install` 仅从本地目录；`skill_repos` 表预留。

**实现思路**：
1. `skills_install_from_zip(source)`：解压 → 扫描 `SKILL.md` → 复制到 SSOT。
2. `skills_install_from_repo(owner, name, branch)`：git clone（或下载 tarball）→ 同上；记录到 `skill_repos`。
3. 可选：`skills_toggle_plugin` 支持软链模式（`std::os::unix::fs::symlink`，Windows 用 `junction`）。

### 3.10 Prompt：跨应用一键同步、回填保护 —— P2

**v1**：同一个 prompt 内容跨多应用同步（CLAUDE.md / AGENTS.md / GEMINI.md），回填保护防止覆盖用户手改内容。

**v2 现状**：prompt 单插件存储。

**实现思路**：
1. `prompts` 表加 `apps` 字段（或按 `name` 绑定多插件），`prompts_toggle` 写多个 `prompt_file_path()`。
2. 启用前比对文件当前内容与上次写入内容，若被用户修改则提示（回填保护）。

### 3.11 用量仪表盘（趋势图表、请求日志页）—— P1

**v1**：趋势图表、详细请求日志、自定义模型定价。

**v2 现状**：`usage_daily_summary` 返回表格行；`usage_list_request_logs` 返回明细；无图表。

**实现思路**：前端用图表库（如 recharts）基于 `usage_daily_summary` 画趋势；已有数据接口，纯前端工作量。

### 3.12 会话搜索与更多会话源 —— P2

**v1**：浏览/搜索/恢复；codex 的 SQLite 会话、grok 等。

**v2 现状**：claudecode（jsonl）、opencode（sqlite+json）。

**实现思路**：后续新增 Agent 时实现 `sessions()`；前端加搜索框过滤 `title`/`project_dir`。

### 3.13 自动备份轮换 —— P2

**v1**：自动备份 + 轮换（保留 N 份）。

**实现思路**：`backup_create` 触发时机（定时/启动）+ 按数量/时间清理旧备份（`db_backups` 已有）。

### 3.14 工作区编辑器（OpenClaw）—— P2

**v1**：编辑 OpenClaw 的 AGENTS.md / SOUL.md，Markdown 预览。

**实现思路**：`AgentPlugin` 加 `config_dir()` 返回目录，前端 Markdown 编辑器列出并编辑该目录下的 Agent 文件。

### 3.15 速度测试 / 供应商健康监控 —— P2

**v1**：SpeedtestService 测 API 端点延迟；proxy 的供应商健康监控。

**实现思路**：新增 `speedtest` 命令对 provider 的 baseURL 发 ping 请求测延迟；健康监控可挂在 proxy（3.1）之上。

## 4. TS 插件沙箱演进（架构差距，非 v1 对齐）

- **现状（已实现方案 A）**：manifest `resources` 白名单 + 后端通用资源命令 `host_read/write/list_resource` 已落地，TS 插件可管理声明的用户目录资源；宿主文件 I/O 仍限定插件目录 + 白名单（见 [ts-plugin.md](ts-plugin.md)）。
- **方向（方案 B，未实现）**：声明式插件——manifest 声明 config 路径 + 格式，后端用通用解析器实现常见 Agent，80% 场景免写代码。
- **与 v1 关系**：v1 没有 TS 插件（全是 native descriptor）。此差距是 v2 新增的能力设计问题，不影响 v1 对齐。

## 5. 建议实施顺序

1. **P0**：本地代理（3.1）—— 先跑通 1 个 Agent 闭环。
2. **P1 快速项**：模型定价接线（3.3）、供应商预设（3.7）、用量图表（3.11）、托盘切换（3.6）。
3. **P1 中型项**：余额/订阅（3.2）、Deep Link（3.5）、通用供应商（3.8）、云端同步（3.4）。
4. **P2**：Skill 仓库/ZIP（3.9）、Prompt 跨应用（3.10）、其余。
5. **TS 沙箱**：方案 A → B，作为贯穿性的架构演进。
