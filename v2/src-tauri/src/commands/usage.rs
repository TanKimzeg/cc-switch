//! 用量查询命令。

use tauri::State;

use crate::db::Database;
use crate::services::usage::{DailyUsageRow, RequestLogRow};

/// 从 OpenCode 会话数据库同步用量到 `request_logs`。
#[tauri::command]
pub fn sync_opencode_usage(db: State<'_, Database>) -> Result<usize, String> {
    let result = db.sync_opencode_usage();
    if let Some(e) = result.errors.first() {
        return Err(e.clone());
    }
    Ok(result.imported)
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
