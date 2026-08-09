//! 插件内插件管理协议（可选能力）。
//!
//! 某些 Agent（如 OpenCode）支持在自身配置中启用第三方插件
//! （如 oh-my-opencode / oh-my-openagent，简称 OMO）。实现
//! [`PluginManagerPlugin`] 的插件可读写其 live 配置的 `plugin` 数组。

use crate::plugin::error::PluginError;

/// 插件内插件管理协议（可选能力）。
pub trait PluginManagerPlugin: Send + Sync {
    /// 读取 live 配置中的插件列表（字符串数组）。
    fn get_plugins(&self) -> Result<Vec<String>, PluginError>;

    /// 添加一个插件（写入 `plugin` 数组，已存在则跳过）。
    fn add_plugin(&self, name: &str) -> Result<(), PluginError>;

    /// 移除一个插件（按名称精确匹配，已移除则视为成功）。
    fn remove_plugin(&self, name: &str) -> Result<(), PluginError>;
}
