use std::path::Path;

use tauri::State;

use crate::db::Database;
use crate::plugin::mcp::McpServerSpec;
use crate::plugin::ops::PluginManagerPlugin;
use crate::plugin::{
    AgentPlugin, ImportCandidate, LiveConfig, PluginError, SessionMessage, SessionMeta,
};
use crate::registry::PluginRegistry;
use crate::types::{InstalledPlugin, Provider};

/// 校验插件是否声明了某能力；未声明则返回 [`PluginError::Capability`]。
fn require_capability(
    plugin: &dyn AgentPlugin,
    supported: bool,
    action: &str,
) -> Result<(), String> {
    if supported {
        Ok(())
    } else {
        Err(PluginError::Capability(format!(
            "plugin '{}' 不支持 {action}",
            plugin.id()
        ))
        .to_string())
    }
}

/// 返回插件注册表中发现的全部插件（含安装来源）。
#[tauri::command]
pub fn get_plugins(registry: State<'_, PluginRegistry>) -> Result<Vec<InstalledPlugin>, String> {
    registry.list_installed().map_err(|e| e.to_string())
}

/// 从本地目录安装插件（目录内须有 manifest.json）。
#[tauri::command]
pub fn install_plugin(
    registry: State<'_, PluginRegistry>,
    source: String,
) -> Result<InstalledPlugin, String> {
    registry
        .install_from_dir(Path::new(&source))
        .map_err(|e| e.to_string())
}

/// 卸载插件；内置插件拒绝卸载。
#[tauri::command]
pub fn uninstall_plugin(registry: State<'_, PluginRegistry>, id: String) -> Result<(), String> {
    registry.uninstall(&id).map_err(|e| e.to_string())
}

/// 读取插件对应的 live 配置。
#[tauri::command]
pub fn plugin_read_live(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<LiveConfig, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().read_live, "read_live")?;
    plugin.read_live().map_err(|e| e.to_string())
}

/// 从 live 配置导入 provider 候选列表。
#[tauri::command]
pub fn plugin_import(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Vec<ImportCandidate>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().import, "import")?;
    plugin.import().map_err(|e| e.to_string())
}

/// 列出插件的会话。
#[tauri::command]
pub fn plugin_sessions(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Vec<SessionMeta>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().sessions, "sessions")?;
    plugin.sessions().map_err(|e| e.to_string())
}

/// 从 live 配置移除某个 provider。
#[tauri::command]
pub fn plugin_remove_provider(
    registry: State<'_, PluginRegistry>,
    id: String,
    provider_id: String,
) -> Result<(), String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().remove, "remove_provider")?;
    plugin.remove_provider(&provider_id).map_err(|e| e.to_string())
}

/// 加载某个会话的消息。
#[tauri::command]
pub fn plugin_load_messages(
    registry: State<'_, PluginRegistry>,
    id: String,
    source: String,
) -> Result<Vec<SessionMessage>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().sessions, "load_messages")?;
    plugin.load_messages(&source).map_err(|e| e.to_string())
}

/// 删除某个会话。
#[tauri::command]
pub fn plugin_delete_session(
    registry: State<'_, PluginRegistry>,
    id: String,
    session_id: String,
    source: String,
) -> Result<bool, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().sessions, "delete_session")?;
    plugin
        .delete_session(&session_id, &source)
        .map_err(|e| e.to_string())
}

/// 读取插件的 MCP 服务器列表。
#[tauri::command]
pub fn plugin_mcp_get(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Vec<McpServerSpec>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().mcp, "mcp")?;
    let mcp_plugin = plugin
        .as_mcp()
        .ok_or_else(|| format!("插件 '{id}' 不支持 MCP 管理"))?;
    mcp_plugin.get_mcp_servers().map_err(|e| e.to_string())
}

/// 写入/更新插件的某个 MCP 服务器。
#[tauri::command]
pub fn plugin_mcp_set(
    registry: State<'_, PluginRegistry>,
    id: String,
    server: McpServerSpec,
) -> Result<(), String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().mcp, "mcp")?;
    let mcp_plugin = plugin
        .as_mcp()
        .ok_or_else(|| format!("插件 '{id}' 不支持 MCP 管理"))?;
    mcp_plugin.set_mcp_server(&server).map_err(|e| e.to_string())
}

/// 移除插件的某个 MCP 服务器。
#[tauri::command]
pub fn plugin_mcp_remove(
    registry: State<'_, PluginRegistry>,
    id: String,
    server_id: String,
) -> Result<(), String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().mcp, "mcp")?;
    let mcp_plugin = plugin
        .as_mcp()
        .ok_or_else(|| format!("插件 '{id}' 不支持 MCP 管理"))?;
    mcp_plugin
        .remove_mcp_server(&server_id)
        .map_err(|e| e.to_string())
}

/// 读取插件的插件内插件列表（如 OMO）。
#[tauri::command]
pub fn plugin_get_plugins(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Vec<String>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().plugins, "plugins")?;
    let ops = plugin
        .as_plugin_manager()
        .ok_or_else(|| format!("插件 '{id}' 不支持插件管理"))?;
    ops.get_plugins().map_err(|e| e.to_string())
}

/// 向插件的 live 配置添加一个插件（如 OMO）。
#[tauri::command]
pub fn plugin_add_plugin(
    registry: State<'_, PluginRegistry>,
    id: String,
    name: String,
) -> Result<(), String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().plugins, "plugins")?;
    let ops = plugin
        .as_plugin_manager()
        .ok_or_else(|| format!("插件 '{id}' 不支持插件管理"))?;
    ops.add_plugin(&name).map_err(|e| e.to_string())
}

/// 从插件的 live 配置移除一个插件。
#[tauri::command]
pub fn plugin_remove_plugin(
    registry: State<'_, PluginRegistry>,
    id: String,
    name: String,
) -> Result<(), String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().plugins, "plugins")?;
    let ops = plugin
        .as_plugin_manager()
        .ok_or_else(|| format!("插件 '{id}' 不支持插件管理"))?;
    ops.remove_plugin(&name).map_err(|e| e.to_string())
}

/// 把某个 provider 写入插件对应的 live 配置（切换）。
///
/// `current` 为 true 时同时标记为当前生效的 provider。
#[tauri::command]
pub fn plugin_apply(
    registry: State<'_, PluginRegistry>,
    db: State<'_, Database>,
    id: String,
    provider_id: String,
    current: Option<bool>,
) -> Result<(), String> {
    let provider = get_provider_by_id(&db, &provider_id).map_err(|e| e.to_string())?;
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_capability(plugin.as_ref(), plugin.capabilities().apply, "apply")?;
    plugin
        .apply(&provider, current.unwrap_or(true))
        .map_err(|e| e.to_string())
}

fn get_provider_by_id(db: &Database, id: &str) -> Result<Provider, String> {
    use rusqlite::{params, OptionalExtension, Row};

    fn row_to_provider(row: &Row<'_>) -> rusqlite::Result<Provider> {
        let meta: Option<String> = row.get("meta")?;
        let meta = meta
            .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
        Ok(Provider {
            id: row.get("id")?,
            plugin_id: row.get("plugin_id")?,
            name: row.get("name")?,
            category: row.get("category")?,
            icon: row.get("icon")?,
            website: row.get("website")?,
            api_key: row.get("api_key")?,
            settings_config: row.get("settings_config")?,
            meta,
            sort_order: row.get("sort_order")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    const COLUMNS: &str =
        "id, plugin_id, name, category, icon, website, api_key, settings_config, meta, sort_order, created_at, updated_at";

    db.lock()
        .query_row(
            &format!("SELECT {COLUMNS} FROM providers WHERE id = ?1"),
            params![id],
            row_to_provider,
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("provider not found: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{OpenCodePlugin, PluginCapabilities, ProcessPlugin};

    fn plugin_with(caps: PluginCapabilities) -> ProcessPlugin {
        ProcessPlugin::new("demo", "demo-cli", vec![], caps)
    }

    #[test]
    fn require_capability_allows_declared() {
        let plugin = plugin_with(PluginCapabilities {
            read_live: true,
            ..Default::default()
        });
        require_capability(&plugin, plugin.capabilities().read_live, "read_live").unwrap();
    }

    #[test]
    fn require_capability_rejects_missing() {
        let plugin = plugin_with(PluginCapabilities::default());
        let err = require_capability(&plugin, plugin.capabilities().sessions, "sessions")
            .unwrap_err();
        assert!(err.contains("不支持"));
        assert!(err.contains("demo"));
    }

    #[test]
    fn require_capability_error_carries_id() {
        let plugin = OpenCodePlugin::new();
        let err = require_capability(&plugin, plugin.capabilities().sessions, "sessions");
        assert!(err.is_ok());
        assert_eq!(plugin.id(), "opencode");
    }
}
