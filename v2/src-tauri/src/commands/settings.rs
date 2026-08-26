use tauri::{Manager, State};

use crate::db::Database;
use crate::services::overrides::{self, OverrideDir};
use crate::services::settings::{AppBehavior, KEY_MINIMIZE_TO_TRAY_ON_CLOSE, KEY_SILENT_STARTUP};
use crate::tray;

#[tauri::command]
pub fn get_setting(db: State<'_, Database>, key: String) -> Result<Option<String>, String> {
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(db: State<'_, Database>, key: String, value: String) -> Result<(), String> {
    db.set_setting(&key, &value).map_err(|e| e.to_string())
}

/// 读取应用行为设置（托盘/关闭行为/自启/静默启动）。
#[tauri::command]
pub fn settings_get_app_behavior(db: State<'_, Database>) -> Result<AppBehavior, String> {
    Ok(db.get_app_behavior())
}

/// 设置「关闭时最小化到托盘」。
#[tauri::command]
pub fn settings_set_minimize_to_tray_on_close(
    db: State<'_, Database>,
    enabled: bool,
) -> Result<(), String> {
    db.set_bool_setting(KEY_MINIMIZE_TO_TRAY_ON_CLOSE, enabled)
        .map_err(|e| e.to_string())
}

/// 设置「静默启动」。
#[tauri::command]
pub fn settings_set_silent_startup(
    db: State<'_, Database>,
    enabled: bool,
) -> Result<(), String> {
    db.set_bool_setting(KEY_SILENT_STARTUP, enabled)
        .map_err(|e| e.to_string())
}

/// 设置「开机自启」（settings 键 + 系统自启注册同步）。
#[tauri::command]
pub fn settings_set_launch_on_startup(
    app: tauri::AppHandle,
    db: State<'_, Database>,
    enabled: bool,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    db.set_bool_setting(crate::services::settings::KEY_LAUNCH_ON_STARTUP, enabled)
        .map_err(|e| e.to_string())?;
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|e| format!("设置开机自启失败: {e}"))
    } else {
        autostart.disable().map_err(|e| format!("取消开机自启失败: {e}"))
    }
}

/// 设置托盘图标显隐（动态创建/移除，无需重启）。
#[tauri::command]
pub fn settings_set_show_in_tray(
    app: tauri::AppHandle,
    db: State<'_, Database>,
    enabled: bool,
) -> Result<(), String> {
    db.set_bool_setting(crate::services::settings::KEY_SHOW_IN_TRAY, enabled)
        .map_err(|e| e.to_string())?;
    if enabled {
        tray::create(&app).map_err(|e| format!("创建托盘失败: {e}"))?;
    } else {
        let _ = app.remove_tray_by_id(tray::TRAY_ID);
    }
    Ok(())
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
