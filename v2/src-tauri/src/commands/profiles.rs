//! Profiles 管理命令。

use tauri::State;

use crate::db::Database;
use crate::services::profiles::Profile;

/// 列出全部 profiles。
#[tauri::command]
pub fn profiles_list(db: State<'_, Database>) -> Result<Vec<Profile>, String> {
    db.list_profiles().map_err(|e| e.to_string())
}

/// 读取当前激活的 profile。
#[tauri::command]
pub fn profiles_current(db: State<'_, Database>) -> Result<Option<String>, String> {
    db.current_profile_id().map_err(|e| e.to_string())
}

/// 新增或更新 profile。
#[tauri::command]
pub fn profiles_upsert(db: State<'_, Database>, profile: Profile) -> Result<(), String> {
    db.upsert_profile(&profile).map_err(|e| e.to_string())
}

/// 删除 profile。
#[tauri::command]
pub fn profiles_delete(db: State<'_, Database>, id: String) -> Result<(), String> {
    db.delete_profile(&id).map_err(|e| e.to_string())?;
    Ok(())
}

/// 应用（激活）某个 profile。
#[tauri::command]
pub fn profiles_apply(db: State<'_, Database>, id: String) -> Result<(), String> {
    if db.get_profile(&id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("profile 不存在: {id}"));
    }
    db.set_current_profile(Some(&id)).map_err(|e| e.to_string())
}

/// 清除当前激活的 profile。
#[tauri::command]
pub fn profiles_clear_current(db: State<'_, Database>) -> Result<(), String> {
    db.set_current_profile(None).map_err(|e| e.to_string())
}
