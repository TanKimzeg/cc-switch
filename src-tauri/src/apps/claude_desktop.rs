//! Claude Desktop 插件：实现 [`AppDescriptor`]。
//!
//! B5 阶段：从 `apps/mod.rs` 拆分出来的独立文件，行为与拆分前一致（逐字节不变）。

use std::path::PathBuf;

use super::AppDescriptor;
use crate::app_config::AppType;
use crate::error::AppError;
use crate::proxy::providers::{get_adapter, ProviderAdapter};
use crate::store::AppState;

pub(super) struct ClaudeDesktopDescriptor;

impl AppDescriptor for ClaudeDesktopDescriptor {
    fn id(&self) -> &'static str {
        "claude-desktop"
    }
    fn display_name(&self) -> &'static str {
        "Claude Desktop"
    }
    fn supports_mcp(&self) -> bool {
        false
    }
    fn supports_skills(&self) -> bool {
        false
    }
    fn supports_prompts(&self) -> bool {
        false
    }
    fn supports_proxy(&self) -> bool {
        true
    }
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        crate::claude_desktop_config::get_config_library_path()
    }
    fn official_seed_provider_id(&self) -> Option<&'static str> {
        Some(crate::database::CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID)
    }
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        Some(get_adapter(&AppType::ClaudeDesktop))
    }
    fn import_default_config(&self, state: &AppState) -> Result<bool, AppError> {
        crate::services::provider::import_default_config(state, AppType::ClaudeDesktop)
    }
}
