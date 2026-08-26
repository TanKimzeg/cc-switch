# AGENTS.md — CC Switch v2 开发指引

> 本项目是 **CC Switch v2**：基于**插件协议（Plugin Protocol）**重构的 Agent 配置切换器。
> 原版（v1，仓库根 `src/` + `src-tauri/`）的功能、UI、交互、成熟实践是**事实标准**。
> **v2 唯一允许的架构差异是「插件模式」；除此之外，逻辑、UI、交互、文案都要看齐 v1。**

---

## 0. 项目定位（务必先读）

- **动机**：v1 每个 Agent 的配置读写散落在 `app_config.rs` 的巨型 `match app`、`services/provider/live.rs`、`services/skill.rs` 等处，新增 Agent 要改多处核心代码。v2 把 Agent 收敛为**插件**：统一 `AgentPlugin` trait，核心只依赖抽象，新增 Agent = 写插件 + 注册。
- **插件形态**：native（Rust 随二进制）、ts（前端 JS 脚本）、shell（外部命令）。
- **硬性要求**：除架构外一切向 v1 看齐——同样的功能语义、UI 布局、文案。**v1 已验证的功能直接照抄，不要重新发明**。

**明确不做（用户决定，勿再提议）**：
- **代理（proxy）** 全家桶——见 `v2/docs/v1-gap-analysis.md` §3.1。
- **供应商预设**（v1 的 449 个预设列表本质是带 aff 推广链接的商业化赞助位；MCP 预设无推广性质、已落地，保留）。

---

## 1. 代码结构

```
v2/
├── src-tauri/
│   ├── plugins/<id>/manifest.json   # 内置插件清单（seed 每次启动覆盖写）
│   ├── src/
│   │   ├── commands/     # Tauri 命令（IPC 层）
│   │   ├── plugin/       # AgentPlugin trait + 各 native 实现
│   │   │   ├── opencode.rs / claudecode.rs / codex.rs / grokbuild.rs / hermes.rs
│   │   │   ├── process.rs / ts.rs / mcp.rs / session_utils.rs / error.rs
│   │   ├── registry.rs   # manifest 解析、resolve_plugin、seed/安装/发现
│   │   ├── services/     # 协议无关业务（mcp/skills/prompts/usage/backup/settings/overrides…）
│   │   ├── db.rs         # SQLite schema
│   │   └── lib.rs        # invoke_handler 注册命令；init_db/setup
├── src/                  # 前端 React（lib/api.ts、lib/plugin-loader.ts、components/…）
├── examples/plugins/     # TS 示例（claudecode-ts、ts-demo）
└── docs/                 # v2 文档（v1-gap-analysis.md 是差距清单与路线）
```

**内置 native 插件（6 个）**：`opencode`（additive，SQLite 会话）、`openclaw`（shell）、`claudecode`（`~/.claude`）、`codex`（`~/.codex` TOML+auth.json）、`grokbuild`（`~/.grok` TOML）、`hermes`（`~/.hermes` YAML，Windows 默认 `%LOCALAPPDATA%\hermes`）。TS 示例 id 是 `claudecode-ts`（native 独占 `claudecode`）。

---

## 2. 插件协议（AgentPlugin）速览

```rust
pub trait AgentPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> &PluginCapabilities;
    fn read_live(&self) -> Result<LiveConfig, PluginError>;
    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError>;
    fn remove_provider(&self, id: &str) -> Result<(), PluginError>;   // 默认不支持
    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError>;
    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError>;
    fn load_messages(&self, source: &str) -> Result<...>;             // 默认不支持
    fn delete_session(&self, id: &str, source: &str) -> Result<bool>; // 默认不支持
    fn as_mcp(&self) -> Option<&dyn McpPlugin>;                       // 默认 None
    fn prompt_file_path(&self) -> Option<PathBuf>;                    // 默认 None
    fn skills_dir(&self) -> Option<PathBuf>;                          // 默认 None
    fn read_raw_config(&self) -> Result<String, PluginError>;         // 默认不支持
    fn write_raw_config(&self, content: &str) -> Result<(), PluginError>;
    fn sync_usage(&self) -> Result<Vec<UsageRecord>, PluginError>;    // 默认不支持
}
```

- 命令层经 `registry.resolve_plugin(id)` 拿 `Box<dyn AgentPlugin>`，不 care 形态。TS 插件后端是 `TsPluginStub`；前端 `api.ts` 用 `loadTsPluginIfTs` 拦截并直接调脚本方法（改前端 API 必须同步这层）。
- **统一能力视图**：`registry.list_installed()` 对 native 插件用 trait 实现回填 `capabilities`/`skillsDir`/`promptFile`，并计算 `backendSwitchable`（apply 且非 TS）。**前端面板按插件过滤时一律消费这份视图，不要自行拼 `apply && entryType !== "ts"` 之类条件**。
- 各插件 `settings_config` 形状不同（易错）：opencode = `{npm, options, models}` 片段；claudecode = settings.json 全文；codex = `{"auth":{},"config":"<toml>"}`；grokbuild = `{"config":"<toml>"}`（官方 category 允许空文档）；hermes = custom_provider 条目 JSON。**注意不对称**：`Provider.settings_config` 是 `Option<String>`（DB 文本），`LiveProvider.settings_config` 是 `Value`。
- 详见 `v2/docs/plugin-protocol.md`（内置插件表）、`v2/docs/plugin-dev-guide.md`（写插件先读）。

---

## 3. 铁律：向 v1 看齐（抄，不要发明）

1. **先找 v1 实现**：`src/`（前端）、`src-tauri/src/`（后端）、`docs/user-manual/zh/`（文案与行为基准）。移植语义/文案/行为边界，不从零设计。
2. **已完成对齐**（细节见 gap-analysis 各节）：Skills 全量（§3.9）、Prompt 互斥+回填（§3.11）、MCP 面板+正确性（§3.18）、设置 Tab 化/主题/语言/窗口行为（§3.18）、备份增强（§3.14）、托盘（§3.6）、Claude Code 原生内置（§3.19）、codex/grokbuild/hermes 原生内置（§3.20）。
3. **未对齐差距**（做前先读对应节）：Profile 项目快照（§3.10，apply 仍未恢复现场）、Deep Link（§3.5）、云端同步（§3.4）、余额/订阅（§3.2）、通用供应商（§3.8）、codex/grokbuild/hermes 用量同步（§3.20 差距）、gemini/claude-desktop 插件化、外壳重构 M4（方向已定：v1 64px header + 保留侧栏）。
4. **不要做**：发明 v1 没有的概念（PluginManager/OMO 是反面教材）；核心代码 `match` 具体插件 id（一切走 trait/注册表）。

---

## 4. TS 插件专项（易踩坑）

- **宿主用 `new Function` 执行脚本，不转译 TS**：`main.js` 必须是合法 JS（无 `interface`/类型注解）。
- 宿主能力：文件 `readFile/writeFile/listFiles`（仅插件目录）+ `readResource/writeResource/listResource`（manifest `resources` 白名单，`~` 展开，后端代劳写文件）；DB 方法自动绑定当前插件 id。
- `resources` 指向不存在的文件时允许写入（后端建父目录）。
- `getMcpServers` 要补全缺省 `type`（有 `command`→`stdio`、有 `url`→`sse`）。
- 详见 `v2/docs/ts-plugin.md`。

---

## 5. 测试与验证

- **后端**：`cargo test`（cwd=`v2/src-tauri`）；`cargo clippy --all-targets` 要求零警告。
- **前端**：`pnpm test:unit` + `pnpm typecheck` + `pnpm format:check`（cwd=`v2`）。
- **每次改动后全套跑过再交付**，并同步更新 `v2/docs/` 对应文档。
- **环境变量测试铁律**：凡读 `CC_SWITCH_TEST_HOME`/`HERMES_HOME`/`LOCALAPPDATA` 等的测试，必须用 `crate::test_support::env_lock()` 串行化 + 带 Drop 的 guard 恢复。**一个持锁测试 panic 会毒化全局锁，导致几十个无关测试连锁失败**——遇到大面积失败先找最早的根因测试，不要逐个修。hermes 测试还要中和 `HERMES_HOME`/`LOCALAPPDATA`（Windows 平台默认不是 `~/.hermes`，测试目录一律用插件自身的 `hermes_dir()` 解析，不要手拼 `~/.hermes`）。
- TS 插件单测：内存资源宿主加载真实 `main.js`（参考 `v2/src/lib/plugin-loader.claudecode.test.ts`）。

---

## 6. 当前文档

`v2/docs/`：`architecture.md`、`plugin-protocol.md`（内置插件表）、`plugin-dev-guide.md`（写插件先读）、`ts-plugin.md`、`commands-api.md`、`frontend-api.md`、`data-model.md`、`v1-gap-analysis.md`（**差距清单与实施顺序，规划前必读**）、`README.md`。

改代码时同步更新对应文档。

---

## 7. 常见教训（踩过的坑）

1. "插件模式"不是自由发挥的许可证——架构可以新，功能必须抄 v1；不要给 trait 加 v1 没有、用户没要求的能力。
2. 全局面板与插件级面板职责要分清（MCP 全局统一表 vs 插件 live）；`McpService` 对无 `as_mcp` 的插件**跳过而非报错**（TS 插件 live 同步由前端脚本完成）。
3. **native 插件能力以 trait 实现为准**：manifest 漏声明不会导致前端隐藏（`list_installed` 回填），但内置 manifest 仍必须声明 `skillsDir`/`promptFile`（TS 生态与文档依赖声明本身）。
4. **SQLite 是 WAL 模式**：备份/恢复禁止文件复制（会丢未 checkpoint 页，含建表），必须用 `rusqlite::backup` API（见 `services/backup.rs`）。整库回灌会连 `db_backups` 表一起还原，恢复后要补回安全备份记录。
5. 未安装的 Agent **不得为其创建配置文件**：MCP 写入前有安装守卫（检查配置目录/主配置存在，目录覆盖视为已安装）。
6. 提交信息用**中文**、正文列要点与测试结论（`cargo test`/clippy/pnpm 各项结果），沿用仓库现有风格。
