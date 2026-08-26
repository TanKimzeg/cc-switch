//! TypeScript 插件占位实现。
//!
//! TS 插件的实际逻辑由前端（WebView）动态加载脚本执行：脚本通过 Tauri
//! `invoke` 调用宿主命令（文件读写、provider/会话/用量等）。Rust 侧仅提供
//! 一个 [`TsPluginStub`]，让 [`crate::registry`] 的统一分派不因入口类型而
//! 中断；其协议方法返回「由前端宿主执行」的错误。

use std::path::PathBuf;

use crate::plugin::error::PluginError;
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, PluginCapabilities, SessionMeta};
use crate::types::Provider;

/// TS 插件占位实现。
pub struct TsPluginStub {
    id: String,
    capabilities: PluginCapabilities,
    prompt_file: Option<PathBuf>,
    skills_dir: Option<PathBuf>,
}

impl TsPluginStub {
    pub fn new(
        id: String,
        capabilities: PluginCapabilities,
        prompt_file: Option<PathBuf>,
        skills_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            id,
            capabilities,
            prompt_file,
            skills_dir,
        }
    }
}

impl AgentPlugin for TsPluginStub {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> &PluginCapabilities {
        &self.capabilities
    }

    fn prompt_file_path(&self) -> Option<PathBuf> {
        self.prompt_file.clone()
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        self.skills_dir.clone()
    }

    fn read_live(&self) -> Result<LiveConfig, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 是 TypeScript 插件，请通过前端宿主执行",
            self.id
        )))
    }

    fn apply(&self, _provider: &Provider, _current: bool) -> Result<(), PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 是 TypeScript 插件，请通过前端宿主执行",
            self.id
        )))
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 是 TypeScript 插件，请通过前端宿主执行",
            self.id
        )))
    }

    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> {
        Err(PluginError::Capability(format!(
            "插件 '{}' 是 TypeScript 插件，请通过前端宿主执行",
            self.id
        )))
    }
}
