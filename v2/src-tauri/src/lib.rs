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

/// 全局状态：应用数据目录（skills 等落盘根目录）。
pub struct AppPaths {
    pub data_dir: PathBuf,
}

fn init_db(app: &tauri::App<Wry>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    let db = Database::new(&dir.join("cc-switch-v2.db"))?;

    // M1：插件注册表 —— 首次运行写入内置插件，扫描并同步安装记录。
    let registry = PluginRegistry::new(dir.join("plugins"), db.clone());
    registry.seed_builtin()?;
    let plugins = registry.discover()?;
    registry.sync_installs(&plugins)?;
    log::info!("discovered {} plugin(s)", plugins.len());

    app.manage(AppPaths { data_dir: dir.clone() });
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
            commands::plugins::plugin_get_plugins,
            commands::plugins::plugin_add_plugin,
            commands::plugins::plugin_remove_plugin,
            commands::mcp::mcp_list,
            commands::mcp::mcp_upsert,
            commands::mcp::mcp_delete,
            commands::mcp::mcp_toggle_app,
            commands::mcp::import_mcp_servers_from_plugin,
            commands::mcp::import_mcp_from_plugin,
            commands::usage::sync_opencode_usage,
            commands::usage::usage_list_request_logs,
            commands::usage::usage_daily_summary,
            commands::skills::skills_list,
            commands::skills::skills_install,
            commands::skills::skills_uninstall,
            commands::skills::skills_toggle_plugin,
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
            commands::settings::get_setting,
            commands::settings::set_setting,
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
