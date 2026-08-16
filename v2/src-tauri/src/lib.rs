mod commands;
mod db;
mod plugin;
mod registry;
mod services;
#[cfg(test)]
mod test_support;
mod tray;
mod types;

use std::path::PathBuf;

use db::Database;
use registry::PluginRegistry;
use tauri::{Manager, Wry};

/// 全局状态：应用数据目录（数据库、备份、技能卸载备份等落盘根目录；可经设置覆盖）。
pub struct AppPaths {
    pub data_dir: PathBuf,
}

fn init_db(app: &tauri::App<Wry>) -> Result<(), Box<dyn std::error::Error>> {
    // CC Switch 数据目录覆盖（指针文件在 app_config_dir，独立于数据目录）。
    let config_dir = app.path().app_config_dir()?;
    let dir = services::overrides::get_app_data_dir_override(&config_dir)
        .unwrap_or(app.path().app_data_dir()?);
    let db = Database::new(&dir.join("cc-switch-v2.db"))?;
    // 载入工具目录覆盖注册表（native 插件 config_dir 消费）。
    services::overrides::init(&db)?;

    // M1：插件注册表 —— 首次运行写入内置插件，扫描并同步安装记录。
    let registry = PluginRegistry::new(dir.join("plugins"), db.clone());
    registry.seed_builtin()?;
    let plugins = registry.discover()?;
    registry.sync_installs(&plugins)?;
    log::info!("discovered {} plugin(s)", plugins.len());

    // Skills：一次性种子默认技能仓库，并回填存量技能的内容哈希。
    db.init_default_skill_repos()?;
    match services::skills::SkillService::backfill_content_hashes(&db, &dir) {
        Ok(n) if n > 0 => log::info!("已为 {n} 个 Skill 补算内容哈希"),
        _ => {}
    }

    // Prompts：首次启动（表全空）把各插件记忆文件导入为启用项。
    let mut prompt_sources = Vec::new();
    for plugin in registry.list_installed().map_err(|e| e.to_string())? {
        if let Ok(plugin_impl) = registry.resolve_plugin(&plugin.manifest.id) {
            if let Some(file) = plugin_impl.prompt_file_path() {
                prompt_sources.push((plugin.manifest.id.clone(), file));
            }
        }
    }
    services::prompts::PromptService::auto_import_first_launch(&db, &prompt_sources)?;

    app.manage(AppPaths {
        data_dir: dir.clone(),
    });
    app.manage(db);
    app.manage(registry);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .setup(|app| {
            init_db(app)?;
            tray::create(app.handle())?;
            show_main(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::plugins::get_plugins,
            commands::plugins::install_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::plugin_read_live,
            commands::plugins::plugin_import,
            commands::plugins::plugin_sessions,
            commands::plugins::plugin_apply,
            commands::plugins::plugin_remove_provider,
            commands::plugins::plugin_load_messages,
            commands::plugins::plugin_delete_session,
            commands::plugins::plugin_mcp_get,
            commands::plugins::plugin_mcp_set,
            commands::plugins::plugin_mcp_remove,
            commands::plugins::plugin_read_raw_config,
            commands::plugins::plugin_write_raw_config,
            commands::mcp::mcp_list,
            commands::mcp::mcp_upsert,
            commands::mcp::mcp_delete,
            commands::mcp::mcp_toggle_app,
            commands::mcp::import_mcp_servers_from_plugin,
            commands::mcp::import_mcp_from_plugin,
            commands::usage::plugin_sync_usage,
            commands::usage::usage_insert_records,
            commands::usage::usage_list_request_logs,
            commands::usage::usage_daily_summary,
            commands::skills::skills_list,
            commands::skills::skills_install_local_dir,
            commands::skills::skills_install_skill,
            commands::skills::skills_install_from_zip,
            commands::skills::skills_uninstall,
            commands::skills::skills_toggle_plugin,
            commands::skills::skills_discover,
            commands::skills::skills_list_repos,
            commands::skills::skills_add_repo,
            commands::skills::skills_remove_repo,
            commands::skills::skills_search_skillsh,
            commands::skills::skills_check_updates,
            commands::skills::skills_update_skill,
            commands::skills::skills_list_backups,
            commands::skills::skills_delete_backup,
            commands::skills::skills_restore_backup,
            commands::skills::skills_scan_unmanaged,
            commands::skills::skills_import,
            commands::skills::skills_get_sync_settings,
            commands::skills::skills_set_sync_method,
            commands::skills::skills_migrate_storage,
            commands::prompts::prompts_list,
            commands::prompts::prompts_upsert,
            commands::prompts::prompts_delete,
            commands::prompts::prompts_toggle,
            commands::profiles::profiles_list,
            commands::profiles::profiles_current,
            commands::profiles::profiles_upsert,
            commands::profiles::profiles_delete,
            commands::profiles::profiles_apply,
            commands::profiles::profiles_clear_current,
            commands::backup::backup_create,
            commands::backup::backup_list,
            commands::backup::backup_delete,
            commands::backup::export_config_json,
            commands::backup::export_config_to_file,
            commands::backup::parse_export_json,
            commands::backup::import_config,
            commands::backup::import_config_from_file,
            commands::host::plugin_get_script,
            commands::host::host_read_file,
            commands::host::host_write_file,
            commands::host::host_list_files,
            commands::host::host_read_resource,
            commands::host::host_write_resource,
            commands::host::host_list_resource,
            commands::providers::get_providers,
            commands::providers::add_provider,
            commands::providers::update_provider,
            commands::providers::delete_provider,
            commands::providers::get_provider,
            commands::providers::get_current_provider,
            commands::providers::set_current_provider,
            commands::providers::switch_provider,
            commands::providers::remove_provider_from_live_config,
            commands::providers::update_providers_sort_order,
            commands::providers::sync_all_providers_to_live,
            commands::providers::import_providers_from_live,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::settings_get_overrides,
            commands::settings::settings_set_override,
            commands::settings::get_app_data_dir_override,
            commands::settings::set_app_data_dir_override,
            tray::update_tray_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main(app: &tauri::AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
