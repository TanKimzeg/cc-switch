//! MCP 服务器管理命令（全局统一服务）。

use tauri::State;

use crate::db::Database;
use crate::registry::PluginRegistry;
use crate::services::mcp::{McpServer, McpService};

/// 读取一个插件的 live 配置中的 MCP 服务器（供导入预览）。
#[tauri::command]
pub fn import_mcp_from_plugin(
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Vec<crate::plugin::mcp::McpServerSpec>, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_mcp(plugin.as_ref(), &id)?;
    plugin.as_mcp().unwrap().get_mcp_servers().map_err(|e| e.to_string())
}

/// 从插件 live 配置导入 MCP 服务器到统一表。
#[tauri::command]
pub fn import_mcp_servers_from_plugin(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<usize, String> {
    let plugin = registry.resolve_plugin(&id).map_err(|e| e.to_string())?;
    require_mcp(plugin.as_ref(), &id)?;
    let servers = plugin
        .as_mcp()
        .unwrap()
        .get_mcp_servers()
        .map_err(|e| e.to_string())?;
    let mut imported = 0;
    for spec in servers {
        let server = McpServer {
            id: spec.id.clone(),
            name: spec.name.clone(),
            spec: spec.spec,
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
            apps: vec![(id.clone(), true)],
        };
        db.upsert_mcp_server(&server).map_err(|e| e.to_string())?;
        imported += 1;
    }
    Ok(imported)
}

fn require_mcp(plugin: &dyn crate::plugin::AgentPlugin, id: &str) -> Result<(), String> {
    if !plugin.capabilities().mcp {
        return Err(format!("插件 '{id}' 不支持 MCP 管理"));
    }
    if plugin.as_mcp().is_none() {
        return Err(format!("插件 '{id}' 不支持 MCP 管理"));
    }
    Ok(())
}

/// 列出全部 MCP 服务器。
#[tauri::command]
pub fn mcp_list(db: State<'_, Database>) -> Result<Vec<McpServer>, String> {
    db.list_mcp_servers().map_err(|e| e.to_string())
}

/// 新增或更新 MCP 服务器，并同步到启用插件的 live 配置。
#[tauri::command]
pub fn mcp_upsert(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    server: McpServer,
) -> Result<(), String> {
    db.upsert_mcp_server(&server).map_err(|e| e.to_string())?;
    McpService::sync_server_to_enabled(&db, &registry, &server)?;
    Ok(())
}

/// 删除 MCP 服务器，并从所有启用插件的 live 配置移除。
#[tauri::command]
pub fn mcp_delete(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<(), String> {
    let server = db.get_mcp_server(&id).map_err(|e| e.to_string())?;
    if let Some(server) = server {
        McpService::remove_server_from_enabled(&db, &registry, &server)?;
    }
    db.delete_mcp_server(&id).map_err(|e| e.to_string())?;
    Ok(())
}

/// 切换某个 MCP 服务器在指定插件的启用状态。
#[tauri::command]
pub fn mcp_toggle_app(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let exists = db
        .get_mcp_server(&id)
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Err(format!("MCP 服务器不存在: {id}"));
    }
    if enabled {
        db.set_mcp_server_app_enabled(&id, &plugin_id, true)
            .map_err(|e| e.to_string())?;
        let updated = db.get_mcp_server(&id).map_err(|e| e.to_string())?.unwrap();
        McpService::sync_server_to_plugin(&db, &registry, &updated, &plugin_id)?;
    } else {
        McpService::remove_server_from_plugin(&db, &registry, &id, &plugin_id)?;
        db.set_mcp_server_app_enabled(&id, &plugin_id, false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
