//! 用量查询命令。

use tauri::State;

use crate::db::Database;
use crate::plugin::AgentPlugin;
use crate::registry::PluginRegistry;
use crate::services::usage::{DailyUsageRow, RequestLogRow};

/// 从插件自己的会话存储同步用量到 `request_logs`（由插件实现解析）。
#[tauri::command]
pub fn plugin_sync_usage(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    plugin_id: String,
) -> Result<usize, String> {
    let plugin = registry
        .resolve_plugin(&plugin_id)
        .map_err(|e| e.to_string())?;
    require_usage(plugin.as_ref(), &plugin_id)?;
    let records = plugin.sync_usage().map_err(|e| e.to_string())?;
    Ok(db.insert_usage_records(&plugin_id, &records))
}

fn require_usage(plugin: &dyn AgentPlugin, id: &str) -> Result<(), String> {
    if !plugin.capabilities().sessions {
        return Err(format!("插件 '{id}' 不支持用量同步"));
    }
    Ok(())
}

/// 查询请求日志。
#[tauri::command]
pub fn usage_list_request_logs(
    db: State<'_, Database>,
    plugin_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<RequestLogRow>, String> {
    db.list_request_logs(plugin_id.as_deref(), limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

/// 查询每日用量汇总。
#[tauri::command]
pub fn usage_daily_summary(
    db: State<'_, Database>,
    plugin_id: Option<String>,
) -> Result<Vec<DailyUsageRow>, String> {
    db.usage_daily_summary(plugin_id.as_deref())
        .map_err(|e| e.to_string())
}
