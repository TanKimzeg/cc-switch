use tauri::{Manager, State};

use crate::db::Database;
use crate::services::overrides::{self, OverrideDir};

#[tauri::command]
pub fn get_setting(db: State<'_, Database>, key: String) -> Result<Option<String>, String> {
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(db: State<'_, Database>, key: String, value: String) -> Result<(), String> {
    db.set_setting(&key, &value).map_err(|e| e.to_string())
}

/// 列出已配置的工具配置目录覆盖。
#[tauri::command]
pub fn settings_get_overrides(db: State<'_, Database>) -> Result<Vec<OverrideDir>, String> {
    overrides::list(&db)
}

/// 设置/清除某插件的配置目录覆盖。
#[tauri::command]
pub fn settings_set_override(
    db: State<'_, Database>,
    plugin_id: String,
    path: Option<String>,
) -> Result<(), String> {
    overrides::set(&db, &plugin_id, path.as_deref())
}

/// 读取 CC Switch 数据目录覆盖（指针文件）。
#[tauri::command]
pub fn get_app_data_dir_override(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(overrides::get_app_data_dir_override(&config_dir)
        .map(|p| p.to_string_lossy().to_string()))
}

/// 设置/清除 CC Switch 数据目录覆盖（返回 true = 需要重启生效）。
#[tauri::command]
pub fn set_app_data_dir_override(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<bool, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    overrides::set_app_data_dir_override(&config_dir, path.as_deref())
}
