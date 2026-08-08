//! Grok Build 插件：实现 [`AppDescriptor`]。
//!
//! B5 阶段：从 `apps/mod.rs` 拆分出来的独立文件，行为与拆分前一致（逐字节不变）。

use std::path::{Path, PathBuf};

use super::AppDescriptor;
use crate::app_config::{AppType, MultiAppConfig};
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::providers::{get_adapter, ProviderAdapter};
use crate::session_manager::{SessionMessage, SessionMeta};
use crate::store::AppState;

pub(super) struct GrokBuildDescriptor;

impl AppDescriptor for GrokBuildDescriptor {
    fn id(&self) -> &'static str {
        "grokbuild"
    }
    fn display_name(&self) -> &'static str {
        "Grok Build"
    }
    fn prompt_file_path(&self) -> Result<PathBuf, AppError> {
        Ok(crate::grok_config::get_grok_config_dir().join("AGENTS.md"))
    }
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        Ok(crate::grok_config::get_grok_config_dir())
    }
    fn official_seed_provider_id(&self) -> Option<&'static str> {
        Some(crate::database::GROKBUILD_OFFICIAL_PROVIDER_ID)
    }
    fn supports_proxy(&self) -> bool {
        true
    }
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        Some(get_adapter(&AppType::GrokBuild))
    }
    fn session_roots(&self) -> Vec<PathBuf> {
        crate::session_manager::providers::grokbuild::session_roots()
    }
    fn scan_sessions(&self) -> Vec<SessionMeta> {
        crate::session_manager::providers::grokbuild::scan_sessions()
    }
    fn load_messages(&self, path: &Path) -> Result<Vec<SessionMessage>, String> {
        crate::session_manager::providers::grokbuild::load_messages(path)
    }
    fn delete_session(&self, root: &Path, source: &Path, session_id: &str) -> Result<bool, String> {
        crate::session_manager::providers::grokbuild::delete_session(root, source, session_id)
    }
    fn import_mcp(&self, state: &AppState) -> Result<usize, AppError> {
        crate::services::mcp::McpService::import_from_grokbuild(state)
    }
    fn sync_single_mcp_server(
        &self,
        id: &str,
        server_spec: &serde_json::Value,
    ) -> Result<(), AppError> {
        crate::mcp::sync_single_server_to_grokbuild(&Default::default(), id, server_spec)
    }
    fn remove_mcp_server(&self, id: &str) -> Result<(), AppError> {
        crate::mcp::remove_server_from_grokbuild(id)
    }
    fn import_default_config(&self, state: &AppState) -> Result<bool, AppError> {
        crate::services::provider::import_default_config(state, AppType::GrokBuild)
    }
    fn sync_current_provider_to_live(
        &self,
        _config: &mut MultiAppConfig,
        _provider_id: &str,
        provider: &Provider,
    ) -> Result<(), AppError> {
        crate::grok_config::write_grok_provider_live(provider)
    }
}
