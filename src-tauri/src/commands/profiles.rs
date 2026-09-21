//! Profiles 管理命令（项目快照语义，对齐 v1）。
//!
//! scope = 单个插件：创建/更新/应用均作用于发起插件的槽位，
//! 各插件的 current 指针相互独立。

use tauri::State;

use crate::db::Database;
use crate::registry::PluginRegistry;
use crate::services::profiles::Profile;
use crate::AppPaths;

/// 列出全部 profiles（附当前插件是否激活的标记）。
#[tauri::command]
pub fn profiles_list(
    db: State<'_, Database>,
    plugin_id: String,
) -> Result<Vec<ProfileWithCurrent>, String> {
    let profiles = db.list_profiles().map_err(|e| e.to_string())?;
    let current = db
        .current_profile_for_plugin(&plugin_id)
        .map_err(|e| e.to_string())?;
    Ok(profiles
        .into_iter()
        .map(|p| {
            let is_current = current.as_deref() == Some(p.id.as_str());
            ProfileWithCurrent {
                profile: p,
                is_current,
            }
        })
        .collect())
}

/// 带当前标记的 profile（针对发起插件）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWithCurrent {
    #[serde(flatten)]
    pub profile: Profile,
    pub is_current: bool,
}

/// 读取某插件当前激活的 profile id。
#[tauri::command]
pub fn profiles_current(
    db: State<'_, Database>,
    plugin_id: String,
) -> Result<Option<String>, String> {
    db.current_profile_for_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// 创建新项目：拍取发起插件的当前状态。
#[tauri::command]
pub fn profiles_create(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    name: String,
    plugin_id: String,
) -> Result<Profile, String> {
    crate::services::profiles::ProfileService::create(&db, &registry, &name, &plugin_id)
}

/// 更新项目：重命名和/或以当前状态重拍某插件槽位。
#[tauri::command]
pub fn profiles_update(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
    name: Option<String>,
    resnapshot_plugin_id: Option<String>,
) -> Result<Profile, String> {
    crate::services::profiles::ProfileService::update(
        &db,
        &registry,
        &id,
        name,
        resnapshot_plugin_id.as_deref(),
    )
}

/// 删除 profile；指向它的插件 current 指针一并清除。
#[tauri::command]
pub fn profiles_delete(db: State<'_, Database>, id: String) -> Result<(), String> {
    db.delete_profile(&id).map_err(|e| e.to_string())?;
    Ok(())
}

/// 应用项目快照到指定插件（best-effort）。返回 warnings 供前端提示。
#[tauri::command]
pub fn profiles_apply(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    paths: State<'_, AppPaths>,
    id: String,
    plugin_id: String,
) -> Result<Vec<String>, String> {
    crate::services::profiles::ProfileService::apply(&db, &registry, &paths, &id, &plugin_id)
}
