//! Skills 管理命令。

use std::path::Path;

use tauri::State;

use crate::db::Database;
use crate::services::skills::{plugin_skills_dir, SkillRecord};
use crate::AppPaths;

fn skills_root(paths: &AppPaths) -> std::path::PathBuf {
    paths.data_dir.join("skills")
}

/// 列出全部技能。
#[tauri::command]
pub fn skills_list(db: State<'_, Database>) -> Result<Vec<SkillRecord>, String> {
    db.list_skills().map_err(|e| e.to_string())
}

/// 从本地目录安装技能。
#[tauri::command]
pub fn skills_install(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    source: String,
) -> Result<SkillRecord, String> {
    let src = Path::new(&source);
    let id = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("skill")
        .to_string();
    db.install_skill(&skills_root(&paths), src, &id)
}

/// 卸载技能。
#[tauri::command]
pub fn skills_uninstall(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    id: String,
) -> Result<(), String> {
    db.uninstall_skill(&skills_root(&paths), &id)
}

/// 启用/停用某技能在指定插件。
#[tauri::command]
pub fn skills_toggle_plugin(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    id: String,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let Some(skill) = db.get_skill(&id).map_err(|e| e.to_string())? else {
        return Err(format!("技能不存在: {id}"));
    };
    db.set_skill_plugin_enabled(&id, &plugin_id, enabled)
        .map_err(|e| e.to_string())?;

    let src = skills_root(&paths).join(&skill.directory);
    let dest = plugin_skills_dir(&plugin_id).join(&skill.directory);
    if enabled {
        std::fs::create_dir_all(dest.parent().unwrap_or(dest.as_path()))
            .map_err(|e| e.to_string())?;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        copy_dir(&src, &dest).map_err(|e| e.to_string())?;
    } else if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
