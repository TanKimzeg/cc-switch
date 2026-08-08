//! OpenClaw 插件：实现 [`AppDescriptor`]。
//!
//! B5 阶段：从 `apps/mod.rs` 拆分出来的独立文件，行为与拆分前一致（逐字节不变）。

use std::path::{Path, PathBuf};

use super::AppDescriptor;
use crate::app_config::AppType;
use crate::error::AppError;
use crate::proxy::providers::{get_adapter, ProviderAdapter};
use crate::session_manager::{SessionMessage, SessionMeta};
use crate::store::AppState;

pub(super) struct OpenClawDescriptor;

impl AppDescriptor for OpenClawDescriptor {
    fn id(&self) -> &'static str {
        "openclaw"
    }
    fn display_name(&self) -> &'static str {
        "OpenClaw"
    }
    fn is_additive(&self) -> bool {
        true
    }
    fn supports_mcp(&self) -> bool {
        false
    }
    fn supports_skills(&self) -> bool {
        false
    }
    fn prompt_file_path(&self) -> Result<PathBuf, AppError> {
        Ok(crate::openclaw_config::get_openclaw_dir().join("AGENTS.md"))
    }
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        Ok(crate::openclaw_config::get_openclaw_dir())
    }
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        Some(get_adapter(&AppType::OpenClaw))
    }
    fn session_roots(&self) -> Vec<PathBuf> {
        vec![crate::openclaw_config::get_openclaw_dir().join("agents")]
    }
    fn scan_sessions(&self) -> Vec<SessionMeta> {
        crate::session_manager::providers::openclaw::scan_sessions()
    }
    fn load_messages(&self, path: &Path) -> Result<Vec<SessionMessage>, String> {
        crate::session_manager::providers::openclaw::load_messages(path)
    }
    fn delete_session(&self, root: &Path, source: &Path, session_id: &str) -> Result<bool, String> {
        crate::session_manager::providers::openclaw::delete_session(root, source, session_id)
    }
    fn import_from_live(&self, state: &AppState) -> Result<usize, AppError> {
        crate::services::provider::import_openclaw_providers_from_live(state)
    }
}
