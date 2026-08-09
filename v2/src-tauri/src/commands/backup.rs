//! 数据库备份与配置导入导出命令。

use tauri::State;

use crate::AppPaths;
use crate::db::Database;
use crate::services::backup::{BackupRecord, ExportPayload};

fn backups_dir(paths: &AppPaths) -> std::path::PathBuf {
    paths.data_dir.join("backups")
}

/// 创建数据库备份。
#[tauri::command]
pub fn backup_create(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
) -> Result<BackupRecord, String> {
    db.create_db_backup(&backups_dir(&paths))
}

/// 列出备份。
#[tauri::command]
pub fn backup_list(db: State<'_, Database>) -> Result<Vec<BackupRecord>, String> {
    db.list_db_backups().map_err(|e| e.to_string())
}

/// 删除备份。
#[tauri::command]
pub fn backup_delete(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    id: String,
) -> Result<(), String> {
    db.delete_db_backup(&backups_dir(&paths), &id)?;
    Ok(())
}

/// 导出全部配置为 JSON。
#[tauri::command]
pub fn export_config_json(db: State<'_, Database>) -> Result<ExportPayload, String> {
    db.export_config()
}

/// 把 JSON 文本解析为导出负载（供前端下载保存）。
#[tauri::command]
pub fn parse_export_json(content: String) -> Result<ExportPayload, String> {
    serde_json::from_str(&content).map_err(|e| format!("导出 JSON 解析失败: {e}"))
}
