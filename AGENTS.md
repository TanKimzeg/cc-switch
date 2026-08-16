# AGENTS.md — CC Switch v2 开发指引

> 本项目是 **CC Switch v2**：基于**插件协议（Plugin Protocol）**重构的 Agent 配置切换器。
> 原版（v1，仓库根 `src/` + `src-tauri/`，GitHub 120k+ stars）的功能、UI、交互、成熟实践是**事实标准**。
> **v2 唯一允许的架构差异是「插件模式」；除此之外，逻辑、UI、交互、文案、行为都要看齐 v1。**

---

## 0. 项目定位（务必先读）

- **动机**：v1 功能强大但代码是"屎山"，每个 Agent 的配置读写散落在 `app_config.rs` 的巨型 `match app`、`services/*/live.rs` 的 `match app_type`、`services/skill.rs` 的 `get_app_skills_dir` 等处，新增 Agent 要改多处核心代码，违背开闭原则。
- **v2 设计**：把 Agent 收敛为**插件**——每个 Agent（Claude Code、OpenCode、OpenClaw、未来的 Kimi Code / Pi / Hermes…）是一个插件，通过统一 `AgentPlugin` trait 暴露能力；核心代码只依赖抽象，**新增 Agent = 写插件 + 注册，零改核心**。
- **插件形态**：native（Rust，随二进制）、ts（前端脚本，可免编译分发）、shell（外部命令）。
- **硬性要求**：除架构外，**其余一切向 v1 看齐**——同样的功能语义、同样的 UI 布局与交互、同样的文案。**v1 已实现且验证过的功能，直接照着移植，不要重新发明**。

**能力清单（v1 都有，v2 逐项对齐）**：Provider 配置与切换、MCP、Skills、Prompts、用量/成本追踪、会话管理、**配置方案（Profile，v1 是「项目快照」）**、系统托盘、Deep Link、云端同步、代理（proxy）、余额/订阅、速度测试。详见 `v2/docs/v1-gap-analysis.md`。

---

## 1. 代码结构

```
v2/
├── src-tauri/src/
│   ├── commands/     # Tauri 命令（IPC 层）
│   ├── plugin/       # 插件协议（trait + 实现）
│   │   ├── mod.rs    # AgentPlugin trait、PluginCapabilities、数据类型
│   │   ├── opencode.rs    # native 插件（additive 模型、SQLite 会话）
│   │   ├── claudecode.rs  # native 参考实现
│   │   ├── process.rs     # shell 插件
│   │   ├── mcp.rs         # McpPlugin + 格式转换
│   │   ├── ts.rs          # TS 插件占位（TsPluginStub）
│   │   └── error.rs
│   ├── registry.rs   # manifest 解析、resolve_plugin、安装/发现
│   ├── services/     # 与协议无关的业务（mcp/skills/prompts/usage/backup/profiles…）
│   ├── db.rs         # SQLite schema
│   └── lib.rs
├── src/              # 前端 React（lib/api.ts、lib/plugin-loader.ts、components/…）
├── examples/plugins/ # 示例插件（claudecode=TS、ts-demo=TS）
└── docs/             # v2 文档（含 v1-gap-analysis.md 差距清单）
```

**后端入口**：`v2/src-tauri/src/lib.rs` 的 `invoke_handler` 注册命令；`init_db` 初始化插件注册表。

---

## 2. 插件协议（AgentPlugin）速览

```rust
pub trait AgentPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> &PluginCapabilities;
    fn read_live(&self) -> Result<LiveConfig, PluginError>;
    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError>;
    fn remove_provider(&self, id: &str) -> Result<(), PluginError>;          // 默认不支持
    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError>;
    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError>;
    fn load_messages(&self, source: &str) -> Result<Vec<SessionMessage>, PluginError>; // 默认不支持
    fn delete_session(&self, id: &str, source: &str) -> Result<bool, PluginError>;     // 默认不支持
    fn as_mcp(&self) -> Option<&dyn McpPlugin>;                              // 默认 None
    fn prompt_file_path(&self) -> Option<PathBuf>;                            // 默认 None
    fn skills_dir(&self) -> Option<PathBuf>;                                  // 默认 None
    fn read_raw_config(&self) -> Result<String, PluginError>;                 // 默认不支持
    fn write_raw_config(&self, content: &str) -> Result<(), PluginError>;     // 默认不支持
    fn sync_usage(&self) -> Result<Vec<UsageRecord>, PluginError>;            // 默认不支持
}
```

- 命令层通过 `registry.resolve_plugin(id)` 拿 `Box<dyn AgentPlugin>` 统一调用，不 care 形态。
- TS 插件后端是 `TsPluginStub`（占位）；前端 `api.ts` 用 `loadTsPluginIfTs` 判断 TS 并直接调脚本方法。**这层路由对插件作者透明**，但改前端 API 时必须同步。
- manifest 字段：`id/name/version/apiVersion/capabilities/promptFile/skillsDir/resources/entry`。详见 `v2/docs/plugin-protocol.md` 与 `v2/docs/plugin-dev-guide.md`。

---

## 3. 铁律：向 v1 看齐（抄，不要发明）

### 3.1 开发新功能时
1. **先找 v1 实现**：功能入口在 `src/`（前端组件/hooks）、`src-tauri/src/`（后端 service/command）、`docs/user-manual/zh/`（用户手册）。**v1 已有 99% 的功能，你的工作是把它们移植进插件模式，而不是从零设计。**
2. **保持语义一致**：数据结构、字段名、错误文案、行为边界照搬 v1。例如：
   - Skills：v1 有 SSOT 存储、GitHub 仓库安装、**skills.sh 公共注册表搜索**、SHA-256 更新检测、备份/恢复、软链/复制两种分发方式。
   - MCP：v1 有统一面板 + 各 app live 双向同步 + 结构校验 + wizard 引导 + 预设。
   - Prompts：v1 跨应用同步（CLAUDE.md/AGENTS.md/GEMINI.md）+ 回填保护。
   - 用量：v1 仪表盘（趋势图、请求日志、自定义模型定价）。
   - Profile（配置方案）：**v1 是「项目快照」**——把某应用分组的当前供应商/MCP/Skills/记忆文件快照存为命名 profile，一键切换项目配置。
3. **复制 v1 文案**：用户可见文案从 `docs/user-manual/zh/`、`src/i18n/` 抄。
   - Skills 完整行为参考：`docs/user-manual/zh/3-extensions/3.3-skills.md` + `src/components/skills/` + `src-tauri/src/services/skill.rs`。
   - Profile 项目快照参考：`src/components/profiles/`（ProfileSwitcher/ProfileManageDialog/scope）+ `src-tauri/src/services/profile.rs`。

### 3.2 已知差距（当前未对齐，逐个补齐）
- **Skills**：✅ 已对齐 v1（2026-08-15）——仓库/ZIP 安装、skills.sh 搜索、SHA-256 更新检测、卸载备份+恢复、未管理导入、软链/复制分发、存储位置迁移（`v2/src-tauri/src/services/skills.rs` + `v2/src/components/skills/`）。
- **Profile（配置方案）**：v2 现在只是 `profiles` 表 CRUD 存 JSON payload，`apply` 只记录 current 但不真正应用到 live——**这不是 v1 的项目快照语义**。要对齐：快照=当前 provider/MCP/Skills/prompt 实际状态，应用=恢复现场。
- **代理（proxy）**：v1 的本地代理热切换、故障转移、熔断、格式转换（`services/proxy.rs`，体量最大）**v2 完全没有**。
- Deep Link、云端同步、余额/订阅、速度测试、模型定价接线：见 `v2/docs/v1-gap-analysis.md`。（**托盘快捷切换已实现**；**proxy 暂不考虑实现**，用户决定）

### 3.3 不要做的
- 不要发明 v1 没有的"新概念"（如之前的 PluginManager/OMO 就是反面教材——v1 没有，用户不需要）。
- 不要在核心代码里 `match` 具体插件 id；一切走 trait/注册表。

---

## 4. TS 插件专项（易踩坑）

- **宿主用 `new Function` 执行脚本，不转译 TS**：`main.js` 必须是**合法 JS**（JSDoc 注释，无 `interface`/类型注解），文件名 `.js`。
- 宿主提供两类能力：
  - **文件**：`readFile/writeFile/listFiles`（仅插件目录）；`readResource/writeResource/listResource(name, rel?)`（manifest `resources` 白名单，`~` 展开，后端代劳写文件）。
  - **DB**：`providers/upsertProvider/deleteProvider/saveUsageRecords/usageDailySummary`（**自动绑定当前插件 id**，脚本无法越权）。
- `resources` 指向**尚不存在的文件**时允许写入（后端建父目录）。
- MCP spec **缺省 type** 要在 `getMcpServers` 补全（有 `command`→`stdio`、有 `url`→`sse`），否则展示/格式转换异常（claudecode 示例已做）。
- 详见 `v2/docs/ts-plugin.md`、`v2/docs/plugin-dev-guide.md`。

---

## 5. 测试与验证

- **后端**：`cargo test`（cwd=`v2/src-tauri`）。native 插件路径测试用 `CC_SWITCH_TEST_HOME` 指向临时目录 + `crate::test_support::env_lock()` 串行化环境变量。clippy 要求零警告（`cargo clippy --all-targets`）。
- **前端**：`pnpm test:unit`（vitest）、`pnpm typecheck`、`pnpm format:check`（prettier）。
- **TS 插件单测**：用「内存资源宿主」模拟 `TsHost`，加载真实 `main.js` 断言行为（参考 `v2/src/lib/plugin-loader.claudecode.test.ts`）。
- **每次改动后**：`cargo test` + `cargo clippy --all-targets` + `pnpm test:unit` + `pnpm typecheck` + `pnpm format:check` 全过再交付。

---

## 6. 当前文档

`v2/docs/`：`architecture.md`（设计意图）、`plugin-protocol.md`（协议）、`plugin-dev-guide.md`（**开发插件先读**）、`ts-plugin.md`、`commands-api.md`、`frontend-api.md`、`data-model.md`、`v1-gap-analysis.md`（**差距清单与实现思路**）、`README.md`（索引）。

改代码时**同步更新对应文档**，保持文档与代码一致。

---

## 7. 常见教训（踩过的坑）

1. **"插件模式"不是自由发挥的许可证**——架构可以新，功能必须抄 v1。
2. 不要给 trait 加 v1 没有、用户没要求的能力（PluginManager 教训）。
3. TS 插件沙箱 ≠ 不能管理真实 Agent：`resources` 白名单 + 后端代劳文件/DB 已解决。
4. 全局面板与插件级面板的职责要分清（MCP 全局统一表 vs 插件 live），TS 插件的 live 同步由前端脚本完成，后端 `McpService` 对无 `as_mcp` 的插件要**跳过而非报错**。
5. `manifest.json` 的能力声明要与 trait 实现一致，否则 UI 隐藏/报错。
6. 版本管理：提交用英文，含测试/校验结论。
