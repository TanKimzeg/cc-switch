//! App 注册表（Plugin / Registry）
//!
//! 每个 Agent 框架（Claude / Codex / Gemini / Grok Build / OpenCode / OpenClaw / Hermes）
//! 实现 [`AppDescriptor`] 并在 [`REGISTRY`] 中注册自身。
//!
//! 核心代码只依赖 [`AppDescriptor`] 抽象，不再 `match` 具体 app，从而让新增 app
//! 只改本目录的插件文件，不动核心文件（开闭原则）。
//!
//! 新增 app：在 `apps/` 下新建 `<id>.rs` 实现 [`AppDescriptor`]，然后在下面的
//! `mod` / `use` 与注册列表里登记即可。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::app_config::{AppType, MultiAppConfig};
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::providers::ProviderAdapter;
use crate::session_manager::{SessionMessage, SessionMeta};
use crate::store::AppState;

/// 单个 Agent 框架的行为描述。
///
/// 未实现的方法使用默认值（不支持该能力）。新增能力时优先用默认实现收敛到
/// “不支持”，避免每个 app 都要补实现。
pub trait AppDescriptor: Send + Sync {
    /// 唯一标识，与 `AppType::as_str()` 一致。
    fn id(&self) -> &'static str;

    /// 显示名称。
    fn display_name(&self) -> &'static str;

    /// 是否 additive 模式：live 配置里保留全部 provider 而非只写当前 provider。
    fn is_additive(&self) -> bool {
        false
    }

    /// 是否支持 MCP 服务器同步。
    fn supports_mcp(&self) -> bool {
        true
    }

    /// 是否支持 Skills 同步。
    fn supports_skills(&self) -> bool {
        true
    }

    /// 是否支持 Prompts。
    fn supports_prompts(&self) -> bool {
        true
    }

    /// 是否支持本地代理（proxy）。
    fn supports_proxy(&self) -> bool {
        false
    }

    /// 主页面默认是否显示该应用（默认 true）。
    fn default_visible(&self) -> bool {
        true
    }

    /// 提示词文件路径（如 `~/.claude/CLAUDE.md`）。
    ///
    /// 默认返回“不支持 Prompts”的本地化错误；支持 Prompts 的 app 需覆盖此方法。
    fn prompt_file_path(&self) -> Result<PathBuf, AppError> {
        Err(AppError::localized(
            "app.prompts_unsupported",
            "当前应用暂不支持 Prompts",
            "This app does not support Prompts",
        ))
    }

    /// 配置目录（如 `~/.claude`）。用于前端展示与打开配置文件夹。
    fn config_dir(&self) -> Result<PathBuf, AppError> {
        Err(AppError::Message(format!(
            "{} does not support config dir",
            self.display_name()
        )))
    }

    /// 官方种子 provider 的固定 id（无则返回 None）。
    fn official_seed_provider_id(&self) -> Option<&'static str> {
        None
    }

    // ===== 代理线协议 =====

    /// 该 app 使用的代理适配器（共享线协议），不支持代理时返回 None。
    fn proxy_adapter(&self) -> Option<Box<dyn ProviderAdapter>> {
        None
    }

    // ===== 会话管理 =====

    /// 会话根目录（删除会话时用于路径校验）。
    fn session_roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// 扫描该 app 的历史会话。
    fn scan_sessions(&self) -> Vec<SessionMeta> {
        Vec::new()
    }

    /// 加载单个会话消息。
    fn load_messages(&self, _path: &Path) -> Result<Vec<SessionMessage>, String> {
        Err(format!(
            "{} does not support session loading",
            self.display_name()
        ))
    }

    /// 删除单个会话。
    fn delete_session(
        &self,
        _root: &Path,
        _source: &Path,
        _session_id: &str,
    ) -> Result<bool, String> {
        Err(format!(
            "{} does not support session deletion",
            self.display_name()
        ))
    }

    /// SQLite 会话：加载消息。不支持 SQLite 会话的 app 返回 None，
    /// 调用方据此回退到文件路径（与历史行为一致）。
    fn load_messages_sqlite(&self, _source: &str) -> Option<Result<Vec<SessionMessage>, String>> {
        None
    }

    /// SQLite 会话：删除会话。不支持 SQLite 会话的 app 返回 None。
    fn delete_session_sqlite(
        &self,
        _session_id: &str,
        _source: &str,
    ) -> Option<Result<bool, String>> {
        None
    }

    // ===== MCP 导入 =====

    /// 从 live 配置导入 MCP 服务器到数据库。
    fn import_mcp(&self, state: &AppState) -> Result<usize, AppError> {
        let _ = state;
        Ok(0)
    }

    /// 把单个 MCP 服务器同步到该 app 的 live 配置。
    /// 不支持 MCP 的 app（OpenClaw / ClaudeDesktop）默认空操作。
    fn sync_single_mcp_server(
        &self,
        _id: &str,
        _server_spec: &serde_json::Value,
    ) -> Result<(), AppError> {
        Ok(())
    }

    /// 从该 app 的 live 配置移除单个 MCP 服务器。
    fn remove_mcp_server(&self, _id: &str) -> Result<(), AppError> {
        Ok(())
    }

    // ===== 启动导入 =====

    /// 非 additive 应用启动时是否需要导入 live 配置（默认：无该 app 的 provider 时导入）。
    fn should_import_default_config_on_startup(&self, state: &AppState) -> Result<bool, AppError> {
        if self.is_additive() {
            return Ok(false);
        }
        Ok(!state.db.has_any_provider_for_app(self.id())?)
    }

    /// 非 additive 应用：把 live 配置导入为默认 provider。
    fn import_default_config(&self, state: &AppState) -> Result<bool, AppError> {
        let _ = state;
        Ok(false)
    }

    /// additive 应用：把 live 中全部 provider 同步进数据库。
    fn import_from_live(&self, state: &AppState) -> Result<usize, AppError> {
        let _ = state;
        Ok(0)
    }

    // ===== live 配置同步 =====

    /// 把当前 provider 同步写入该 app 的 live 配置。
    /// 非 additive app 需覆盖；additive 应用默认空操作（live 配置保留全部 provider）。
    fn sync_current_provider_to_live(
        &self,
        _config: &mut MultiAppConfig,
        _provider_id: &str,
        _provider: &Provider,
    ) -> Result<(), AppError> {
        Ok(())
    }
}

// ===== 各 app 插件模块 =====
//
// B5 阶段：每个 app 一个文件，注册在这里集中登记。
// 新增 app 时：① 新建 `apps/<id>.rs`；② 在本文件加 `mod` 与 `use`；③ 在注册列表登记。

mod claude;
mod claude_desktop;
mod codex;
mod gemini;
mod grokbuild;
mod hermes;
mod openclaw;
mod opencode;

use claude::ClaudeDescriptor;
use claude_desktop::ClaudeDesktopDescriptor;
use codex::CodexDescriptor;
use gemini::GeminiDescriptor;
use grokbuild::GrokBuildDescriptor;
use hermes::HermesDescriptor;
use openclaw::OpenClawDescriptor;
use opencode::OpenCodeDescriptor;

// ===== 注册表 =====

static REGISTRY: LazyLock<HashMap<&'static str, &'static dyn AppDescriptor>> =
    LazyLock::new(|| {
        let mut registry: HashMap<&'static str, &'static dyn AppDescriptor> = HashMap::new();
        for descriptor in [
            &ClaudeDescriptor as &dyn AppDescriptor,
            &ClaudeDesktopDescriptor,
            &CodexDescriptor,
            &GeminiDescriptor,
            &GrokBuildDescriptor,
            &OpenCodeDescriptor,
            &OpenClawDescriptor,
            &HermesDescriptor,
        ] {
            registry.insert(descriptor.id(), descriptor);
        }
        registry
    });

/// 获取指定 app 的 descriptor；未注册时返回 None。
pub fn get(id: &str) -> Option<&'static dyn AppDescriptor> {
    REGISTRY.get(id).copied()
}

/// 遍历所有已注册的 app descriptor。
pub fn all() -> impl Iterator<Item = &'static dyn AppDescriptor> {
    REGISTRY.values().copied()
}

/// 遍历满足 `filter` 的 app descriptor。
pub fn all_where(
    filter: impl Fn(&'static dyn AppDescriptor) -> bool + 'static,
) -> impl Iterator<Item = &'static dyn AppDescriptor> {
    REGISTRY.values().copied().filter(move |d| filter(*d))
}

/// 获取 AppType 对应的 descriptor；AppType 变体必须已注册，否则 panic。
///
/// 由 B7 的注册表完整性测试保证每个变体都已注册。
pub fn for_app_type(app: &AppType) -> &'static dyn AppDescriptor {
    get(app.as_str()).unwrap_or_else(|| {
        panic!(
            "AppType {} has no registered AppDescriptor; add it to apps::REGISTRY",
            app.as_str()
        )
    })
}

/// 注册表里的 app id 列表（供完整性测试与前端枚举对齐）。
pub fn ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = REGISTRY.keys().copied().collect();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_app_types() {
        for app in AppType::all() {
            assert!(
                get(app.as_str()).is_some(),
                "AppType {} missing from registry",
                app.as_str()
            );
        }
    }

    #[test]
    fn registry_ids_are_unique() {
        let ids = ids();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "registry ids must be unique");
    }

    #[test]
    fn every_descriptor_has_non_empty_display_name() {
        for d in all() {
            assert!(
                !d.display_name().is_empty(),
                "{} has empty display_name",
                d.id()
            );
        }
    }
}
