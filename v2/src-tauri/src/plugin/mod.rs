//! M3 插件协议：定义 AgentPlugin trait 与能力声明。
//!
//! 插件是「切换 Agent 配置」的协议载体。每个已安装插件（见 `registry.rs`）
//! 在运行期可被解析为某个实现了 [`AgentPlugin`] 的实例，由协议方法完成：
//!
//! - `read_live`：读取该 Agent 当前生效的 live 配置
//! - `apply`：把某个 provider 写入 live 配置（切换）
//! - `remove_provider`：从 live 配置移除某个 provider
//! - `import`：从 live 配置反向导入 provider 到数据库
//! - `sessions`：列出该 Agent 的会话
//!
//! 插件形态：
//! - 原生内置插件（如 opencode）在二进制内直接实现 [`AgentPlugin`]；
//! - 第三方插件通过 manifest 的 `entry.shell` 声明一个外部命令，
//!   由 [`super::registry`] 包装成进程插件调用。

pub mod error;
pub mod mcp;
pub mod opencode;
pub mod ops;
pub mod process;
pub mod ts;

pub use error::PluginError;
pub use opencode::OpenCodePlugin;
pub use ops::PluginManagerPlugin;
pub use process::ProcessPlugin;
pub use ts::TsPluginStub;

use serde::{Deserialize, Serialize};

use crate::types::Provider;

pub use mcp::McpPlugin;

/// 插件能力声明：manifest 中的 `capabilities` 字段。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCapabilities {
    /// 支持读取 live 配置。
    #[serde(default)]
    pub read_live: bool,
    /// 支持把 provider 写入 live 配置（切换）。
    #[serde(default)]
    pub apply: bool,
    /// 支持从 live 配置移除 provider。
    #[serde(default)]
    pub remove: bool,
    /// 支持从 live 配置导入 provider。
    #[serde(default)]
    pub import: bool,
    /// 支持列出会话。
    #[serde(default)]
    pub sessions: bool,
    /// 支持 MCP 服务器管理。
    #[serde(default)]
    pub mcp: bool,
    /// 支持插件管理（如 OMO 等 opencode 插件）。
    #[serde(default)]
    pub plugins: bool,
}

/// 从 live 配置中读到的单个 provider 视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProvider {
    /// provider 在 live 配置中的键。
    pub id: String,
    /// 展示名（可能来自配置的 name 字段）。
    pub name: String,
    /// 该 provider 的 settings_config（写入 live 的原始片段）。
    pub settings_config: serde_json::Value,
}

/// `read_live` 的返回值：live 配置中的全部 provider 与当前选中项。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveConfig {
    pub providers: Vec<LiveProvider>,
    /// 当前生效的 provider id（若该 Agent 的配置能表达）。
    pub current: Option<String>,
}

/// `import` 的结果：单个可从 live 导入到数据库的 provider 候选。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub id: String,
    pub name: String,
    pub settings_config: serde_json::Value,
}

/// 会话元信息（`sessions` 返回值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub session_id: String,
    pub title: Option<String>,
    pub project_dir: Option<String>,
    pub created_at: Option<i64>,
    pub last_active_at: Option<i64>,
    pub source_path: Option<String>,
    pub resume_command: Option<String>,
}

/// 会话消息（`load_messages` 返回值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub ts: Option<i64>,
}

/// Agent 插件协议。
///
/// 实现方必须同时满足 `Send + Sync`，以便作为 Tauri 全局状态被并发访问。
pub trait AgentPlugin: Send + Sync {
    /// 插件 id（与 manifest 的 `id` 一致）。
    fn id(&self) -> &str;

    /// 能力声明。
    fn capabilities(&self) -> &PluginCapabilities;

    /// 读取 live 配置。
    fn read_live(&self) -> Result<LiveConfig, PluginError>;

    /// 把一个 provider 写入 live 配置（切换）。
    ///
    /// `provider.settings_config` 是写入 live 的配置片段；`current` 表示
    /// 是否同时把它标记为当前生效的 provider。
    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError>;

    /// 从 live 配置移除某个 provider。
    ///
    /// 默认实现返回能力不支持错误；支持的插件覆盖此方法。
    fn remove_provider(&self, _id: &str) -> Result<(), PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 不支持移除 provider",
            self.id()
        )))
    }

    /// 从 live 配置导入 provider 候选列表。
    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError>;

    /// 列出该 Agent 的会话。
    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError>;

    /// 加载某个会话的消息。
    ///
    /// `source` 是 [`SessionMeta::source_path`] 返回的来源引用。
    fn load_messages(&self, _source: &str) -> Result<Vec<SessionMessage>, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 不支持加载会话消息",
            self.id()
        )))
    }

    /// 删除某个会话。
    ///
    /// `source` 是 [`SessionMeta::source_path`] 返回的来源引用。
    fn delete_session(&self, _session_id: &str, _source: &str) -> Result<bool, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 不支持删除会话",
            self.id()
        )))
    }

    /// 若插件实现 MCP 管理，返回对应的 trait 对象引用。
    fn as_mcp(&self) -> Option<&dyn McpPlugin> {
        None
    }

    /// 若插件支持插件内插件管理（如 OMO），返回对应的 trait 对象引用。
    fn as_plugin_manager(&self) -> Option<&dyn PluginManagerPlugin> {
        None
    }

    /// 读取 live 配置的原始文本（JSON5 等），供用户手动编辑。
    fn read_raw_config(&self) -> Result<String, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 不支持原始配置读取",
            self.id()
        )))
    }

    /// 写入 live 配置的原始文本（须为合法配置格式）。
    fn write_raw_config(&self, _content: &str) -> Result<(), PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 不支持原始配置写入",
            self.id()
        )))
    }
}
