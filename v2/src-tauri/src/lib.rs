mod commands;
mod db;
mod plugin;
mod registry;
mod tray;
mod types;

use db::Database;
use registry::PluginRegistry;
use tauri::{Manager, Wry};

fn init_db(app: &tauri::App<Wry>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    let db = Database::new(&dir.join("cc-switch-v2.db"))?;

    // M1：插件注册表 —— 首次运行写入内置插件，扫描并同步安装记录。
    let registry = PluginRegistry::new(dir.join("plugins"), db.clone());
    registry.seed_builtin()?;
    let plugins = registry.discover()?;
    registry.sync_installs(&plugins)?;
    log::info!("discovered {} plugin(s)", plugins.len());

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
            commands::providers::get_providers,
            commands::providers::add_provider,
            commands::providers::update_provider,
            commands::providers::delete_provider,
            commands::providers::get_current_provider,
            commands::providers::set_current_provider,
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
