//! Hermes 插件：实现 [`AppDescriptor`]。
//!
//! B5 阶段：从 `apps/mod.rs` 拆分出来的独立文件，行为与拆分前一致（逐字节不变）。

use std::path::{Path, PathBuf};

use super::AppDescriptor;
use crate::app_config::AppType;
use crate::error::AppError;
use crate::proxy::providers::{get_adapter, ProviderAdapter};
use crate::session_manager::{SessionMessage, SessionMeta};
use crate::store::AppState;

pub(super) struct HermesDescriptor;

impl AppDescriptor for HermesDescriptor {
    fn id(&self) -> &'static str {
        "hermes"
    }
    fn display_name(&self) -> &'static str {
        "Hermes"
    }
    fn is_additive(&self) -> bool {
        true
    }
    fn default_visible(&self) -> bool {
        false
    }
    fn prompt_file_path(&self) -> Result<PathBuf, AppError> {
        Ok(crate::hermes_config::get_hermes_dir().join("AGENTS.md"))
    }
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        Ok(crate::hermes_config::get_hermes_dir())
    }
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        Some(get_adapter(&AppType::Hermes))
    }
    fn session_roots(&self) -> Vec<PathBuf> {
        vec![crate::hermes_config::get_hermes_dir().join("sessions")]
    }
    fn scan_sessions(&self) -> Vec<SessionMeta> {
        crate::session_manager::providers::hermes::scan_sessions()
    }
    fn load_messages(&self, path: &Path) -> Result<Vec<SessionMessage>, String> {
        crate::session_manager::providers::hermes::load_messages(path)
    }
    fn delete_session(&self, root: &Path, source: &Path, session_id: &str) -> Result<bool, String> {
        crate::session_manager::providers::hermes::delete_session(root, source, session_id)
    }
    fn load_messages_sqlite(&self, source: &str) -> Option<Result<Vec<SessionMessage>, String>> {
        Some(crate::session_manager::providers::hermes::load_messages_sqlite(source))
    }
    fn delete_session_sqlite(
        &self,
        session_id: &str,
        source: &str,
    ) -> Option<Result<bool, String>> {
        Some(crate::session_manager::providers::hermes::delete_session_sqlite(session_id, source))
    }
    fn import_mcp(&self, state: &AppState) -> Result<usize, AppError> {
        crate::services::mcp::McpService::import_from_hermes(state)
    }
    fn sync_single_mcp_server(
        &self,
        id: &str,
        server_spec: &serde_json::Value,
    ) -> Result<(), AppError> {
        crate::mcp::sync_single_server_to_hermes(&Default::default(), id, server_spec)
    }
    fn remove_mcp_server(&self, id: &str) -> Result<(), AppError> {
        crate::mcp::remove_server_from_hermes(id)
    }
    fn import_from_live(&self, state: &AppState) -> Result<usize, AppError> {
        crate::services::provider::import_hermes_providers_from_live(state)
    }
}
