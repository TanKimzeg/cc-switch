//! 数据库备份与配置导入导出命令。

use tauri::State;

use crate::db::Database;
use crate::services::backup::{BackupRecord, ExportPayload};
use crate::AppPaths;

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

/// 重命名备份。
#[tauri::command]
pub fn backup_rename(db: State<'_, Database>, id: String, name: String) -> Result<(), String> {
    db.rename_db_backup(&id, &name)
}

/// 恢复备份（恢复前自动创建安全备份），返回安全备份 id。
#[tauri::command]
pub fn backup_restore(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    id: String,
) -> Result<String, String> {
    db.restore_db_backup(&backups_dir(&paths), &id)
}

/// 导出全部配置为 JSON。
#[tauri::command]
pub fn export_config_json(db: State<'_, Database>) -> Result<ExportPayload, String> {
    db.export_config()
}

/// 把导出的配置 JSON 写入指定路径。
#[tauri::command]
pub fn export_config_to_file(db: State<'_, Database>, path: String) -> Result<(), String> {
    let payload = db.export_config()?;
    let json =
        serde_json::to_string_pretty(&payload).map_err(|e| format!("序列化导出失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))
}

/// 把 JSON 文本解析为导出负载（供前端下载保存）。
#[tauri::command]
pub fn parse_export_json(content: String) -> Result<ExportPayload, String> {
    serde_json::from_str(&content).map_err(|e| format!("导出 JSON 解析失败: {e}"))
}

/// 导入配置负载到数据库（逐表 upsert）。
#[tauri::command]
pub fn import_config(db: State<'_, Database>, payload: ExportPayload) -> Result<usize, String> {
    db.import_config(&payload)
}

/// 从 JSON 文件导入配置（读取文件 → 解析 → 落库）。
#[tauri::command]
pub fn import_config_from_file(db: State<'_, Database>, path: String) -> Result<usize, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    let payload: ExportPayload =
        serde_json::from_str(&content).map_err(|e| format!("解析配置 JSON 失败: {e}"))?;
    db.import_config(&payload)
}
