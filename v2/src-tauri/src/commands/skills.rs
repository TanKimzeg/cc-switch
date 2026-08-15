//! Skills 管理命令。

use std::path::{Path, PathBuf};

use tauri::State;

use crate::db::Database;
use crate::registry::PluginRegistry;
use crate::services::skills::{
    remove_skill_from_dir, search_skills_sh, ssot_dir, sync_skill_to_dir, validate_repo_ref,
    DiscoverableSkill, ImportSkillSelection, MigrationResult, SkillBackupEntry, SkillRecord,
    SkillRepo, SkillService, SkillsShSearchResult, SkillStorageLocation, SkillUpdateInfo,
    SyncMethod, SyncSettings, UnmanagedSkill,
};
use crate::AppPaths;

/// 列出全部技能。
#[tauri::command]
pub fn skills_list(db: State<'_, Database>) -> Result<Vec<SkillRecord>, String> {
    db.list_skills().map_err(|e| e.to_string())
}

/// 从本地目录安装技能（兼容旧入口，不启用任何插件）。
#[tauri::command]
pub fn skills_install_local_dir(
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
    SkillService::install_local_dir(&db, &paths.data_dir, src, &id)
}

/// 从仓库安装技能，并启用当前插件。
#[tauri::command]
pub async fn skills_install_skill(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    skill: DiscoverableSkill,
    current_plugin: String,
) -> Result<SkillRecord, String> {
    SkillService::install_from_repo(&db, &paths.data_dir, &skill, &current_plugin).await
}

/// 从本地 ZIP 安装技能，并启用当前插件。
#[tauri::command]
pub fn skills_install_from_zip(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    file_path: String,
    current_plugin: String,
) -> Result<Vec<SkillRecord>, String> {
    SkillService::install_from_zip(&db, &paths.data_dir, Path::new(&file_path), &current_plugin)
}

/// 卸载技能（自动备份），返回备份路径。
#[tauri::command]
pub fn skills_uninstall(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    registry: State<'_, PluginRegistry>,
    id: String,
) -> Result<Option<String>, String> {
    let dirs = registry_skills_dirs(&registry);
    let dir_refs: Vec<&Path> = dirs.iter().map(|(_, d)| d.as_path()).collect();
    SkillService::uninstall(&db, &paths.data_dir, &dir_refs, &id)
}

/// 启用/停用某技能在指定插件，并同步文件。
#[tauri::command]
pub fn skills_toggle_plugin(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    registry: State<'_, PluginRegistry>,
    id: String,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let Some(skill) = db.get_skill(&id).map_err(|e| e.to_string())? else {
        return Err(format!("技能不存在: {id}"));
    };
    let plugin = registry
        .resolve_plugin(&plugin_id)
        .map_err(|e| e.to_string())?;
    let dest_dir = plugin
        .skills_dir()
        .ok_or_else(|| format!("插件 '{plugin_id}' 不支持 skills 同步"))?;
    db.set_skill_plugin_enabled(&id, &plugin_id, enabled)
        .map_err(|e| e.to_string())?;

    let settings = SkillService::get_sync_settings(&db)?;
    let ssot = ssot_dir(&paths.data_dir, settings.storage_location);
    if enabled {
        sync_skill_to_dir(&ssot, &skill.directory, &dest_dir, settings.sync_method)
    } else {
        remove_skill_from_dir(&skill.directory, &dest_dir)
    }
}

/// 发现全部启用仓库中的技能。
#[tauri::command]
pub async fn skills_discover(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
) -> Result<Vec<DiscoverableSkill>, String> {
    let repos = db.list_skill_repos().map_err(|e| e.to_string())?;
    SkillService::discover(&db, &paths.data_dir, repos).await
}

/// 列出技能仓库。
#[tauri::command]
pub fn skills_list_repos(db: State<'_, Database>) -> Result<Vec<SkillRepo>, String> {
    db.list_skill_repos().map_err(|e| e.to_string())
}

/// 添加技能仓库。
#[tauri::command]
pub fn skills_add_repo(
    db: State<'_, Database>,
    owner: String,
    name: String,
    branch: String,
) -> Result<SkillRepo, String> {
    let branch = if branch.trim().is_empty() {
        "main".to_string()
    } else {
        branch
    };
    validate_repo_ref(&owner, &name, &branch)?;
    let repo = SkillRepo {
        owner,
        name,
        branch,
        enabled: true,
    };
    db.save_skill_repo(&repo).map_err(|e| e.to_string())?;
    Ok(repo)
}

/// 删除技能仓库。
#[tauri::command]
pub fn skills_remove_repo(
    db: State<'_, Database>,
    owner: String,
    name: String,
) -> Result<(), String> {
    db.delete_skill_repo(&owner, &name).map_err(|e| e.to_string())
}

/// 搜索 skills.sh 公共注册表。
#[tauri::command]
pub async fn skills_search_skillsh(
    query: String,
    limit: usize,
    offset: usize,
) -> Result<SkillsShSearchResult, String> {
    search_skills_sh(&query, limit, offset).await
}

/// 检查全部已安装技能的更新。
#[tauri::command]
pub async fn skills_check_updates(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
) -> Result<Vec<SkillUpdateInfo>, String> {
    SkillService::check_updates(&db, &paths.data_dir).await
}

/// 更新单个技能。
#[tauri::command]
pub async fn skills_update_skill(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    id: String,
) -> Result<SkillRecord, String> {
    SkillService::update_skill(&db, &paths.data_dir, &id).await
}

/// 列出技能备份。
#[tauri::command]
pub fn skills_list_backups(paths: State<'_, AppPaths>) -> Result<Vec<SkillBackupEntry>, String> {
    SkillService::list_backups(&paths.data_dir)
}

/// 删除技能备份。
#[tauri::command]
pub fn skills_delete_backup(
    paths: State<'_, AppPaths>,
    backup_id: String,
) -> Result<(), String> {
    SkillService::delete_backup(&paths.data_dir, &backup_id)
}

/// 从备份恢复技能，并启用当前插件。
#[tauri::command]
pub fn skills_restore_backup(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    backup_id: String,
    current_plugin: String,
) -> Result<SkillRecord, String> {
    SkillService::restore_backup(&db, &paths.data_dir, &backup_id, &current_plugin)
}

/// 扫描未管理的技能。
#[tauri::command]
pub fn skills_scan_unmanaged(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    registry: State<'_, PluginRegistry>,
) -> Result<Vec<UnmanagedSkill>, String> {
    let sources = scan_sources(&db, &paths, &registry);
    SkillService::scan_unmanaged(&db, &paths.data_dir, &sources)
}

/// 从应用/SSOT 目录导入技能。
#[tauri::command]
pub fn skills_import(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    registry: State<'_, PluginRegistry>,
    imports: Vec<ImportSkillSelection>,
) -> Result<Vec<SkillRecord>, String> {
    let sources = scan_sources(&db, &paths, &registry);
    SkillService::import_from_dirs(&db, &paths.data_dir, &sources, imports)
}

/// 读取同步设置。
#[tauri::command]
pub fn skills_get_sync_settings(db: State<'_, Database>) -> Result<SyncSettings, String> {
    SkillService::get_sync_settings(&db)
}

/// 设置同步方式。
#[tauri::command]
pub fn skills_set_sync_method(
    db: State<'_, Database>,
    method: SyncMethod,
) -> Result<(), String> {
    SkillService::set_sync_method(&db, method)
}

/// 迁移技能存储位置。
#[tauri::command]
pub fn skills_migrate_storage(
    db: State<'_, Database>,
    paths: State<'_, AppPaths>,
    target: SkillStorageLocation,
) -> Result<MigrationResult, String> {
    SkillService::migrate_storage(&db, &paths.data_dir, target)
}

/// 收集全部插件的 skills 目录（插件 id + 路径）。
fn registry_skills_dirs(registry: &PluginRegistry) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(plugins) = registry.list_installed() {
        for p in plugins {
            if let Ok(plugin) = registry.resolve_plugin(&p.manifest.id) {
                if let Some(dir) = plugin.skills_dir() {
                    out.push((p.manifest.id, dir));
                }
            }
        }
    }
    out
}

/// 收集全部扫描来源：各插件 skills 目录 + SSOT。
fn scan_sources(
    db: &Database,
    paths: &AppPaths,
    registry: &PluginRegistry,
) -> Vec<(String, PathBuf)> {
    let mut sources = registry_skills_dirs(registry);
    if let Ok(settings) = SkillService::get_sync_settings(db) {
        sources.push(("cc-switch".to_string(), ssot_dir(&paths.data_dir, settings.storage_location)));
    }
    sources
}
