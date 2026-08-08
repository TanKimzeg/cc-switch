//! Gemini 插件：实现 [`AppDescriptor`]。
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

pub(super) struct GeminiDescriptor;

impl AppDescriptor for GeminiDescriptor {
    fn id(&self) -> &'static str {
        "gemini"
    }
    fn display_name(&self) -> &'static str {
        "Gemini"
    }
    fn supports_proxy(&self) -> bool {
        true
    }
    fn prompt_file_path(&self) -> Result<PathBuf, AppError> {
        Ok(crate::gemini_config::get_gemini_dir().join("GEMINI.md"))
    }
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        Ok(crate::gemini_config::get_gemini_dir())
    }
    fn official_seed_provider_id(&self) -> Option<&'static str> {
        Some("gemini-official")
    }
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        Some(get_adapter(&AppType::Gemini))
    }
    fn session_roots(&self) -> Vec<PathBuf> {
        vec![crate::gemini_config::get_gemini_dir().join("tmp")]
    }
    fn scan_sessions(&self) -> Vec<SessionMeta> {
        crate::session_manager::providers::gemini::scan_sessions()
    }
    fn load_messages(&self, path: &Path) -> Result<Vec<SessionMessage>, String> {
        crate::session_manager::providers::gemini::load_messages(path)
    }
    fn delete_session(&self, root: &Path, source: &Path, session_id: &str) -> Result<bool, String> {
        crate::session_manager::providers::gemini::delete_session(root, source, session_id)
    }
    fn import_mcp(&self, state: &AppState) -> Result<usize, AppError> {
        crate::services::mcp::McpService::import_from_gemini(state)
    }
    fn sync_single_mcp_server(
        &self,
        id: &str,
        server_spec: &serde_json::Value,
    ) -> Result<(), AppError> {
        crate::mcp::sync_single_server_to_gemini(&Default::default(), id, server_spec)
    }
    fn remove_mcp_server(&self, id: &str) -> Result<(), AppError> {
        crate::mcp::remove_server_from_gemini(id)
    }
    fn import_default_config(&self, state: &AppState) -> Result<bool, AppError> {
        crate::services::provider::import_default_config(state, AppType::Gemini)
    }
    fn sync_current_provider_to_live(
        &self,
        config: &mut MultiAppConfig,
        provider_id: &str,
        provider: &Provider,
    ) -> Result<(), AppError> {
        crate::services::config::ConfigService::sync_gemini_live(config, provider_id, provider)
    }
}
