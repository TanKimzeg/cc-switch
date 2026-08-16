//! 系统托盘：插件 → provider 两级菜单，支持不打开主窗口直接切换。

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};

use crate::commands::providers::switch_provider_core;
use crate::db::Database;
use crate::registry::PluginRegistry;

pub const TRAY_ID: &str = "main-tray";
/// 切换菜单项 id 前缀：`switch_{provider_id}`。
const SWITCH_PREFIX: &str = "switch_";
/// 空插件菜单项 id 前缀：`empty_{plugin_id}`。
const EMPTY_PREFIX: &str = "empty_";

/// 托盘菜单数据（纯结构，供 `build_menu_spec` 单测）。
#[derive(Debug, Clone)]
pub struct TraySection {
    pub plugin_id: String,
    pub plugin_name: String,
    pub providers: Vec<TrayProvider>,
    pub current_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrayProvider {
    pub id: String,
    pub name: String,
}

/// 收集托盘菜单数据：仅包含「可后端切换」的插件（`apply` 能力且非 TS）。
///
/// TS 插件需前端宿主执行脚本，后端 `apply` 无法完成切换，托盘不列出。
pub fn build_menu_spec(db: &Database, registry: &PluginRegistry) -> Vec<TraySection> {
    let Ok(plugins) = registry.list_installed() else {
        return Vec::new();
    };
    let mut sections = Vec::new();
    for plugin in plugins {
        let caps = plugin.manifest.capabilities.clone().unwrap_or_default();
        if !caps.apply {
            continue;
        }
        if plugin.manifest.entry_type == "ts" {
            continue;
        }

        let conn = db.lock();
        let mut providers = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, name FROM providers WHERE plugin_id = ?1 ORDER BY sort_order, name",
        ) {
            if let Ok(rows) = stmt.query_map(rusqlite::params![plugin.manifest.id], |row| {
                Ok(TrayProvider {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            }) {
                providers = rows.filter_map(|r| r.ok()).collect();
            }
        }
        let current_id = conn
            .query_row(
                "SELECT current_provider_id FROM app_state WHERE plugin_id = ?1",
                rusqlite::params![plugin.manifest.id],
                |row| row.get::<_, String>(0),
            )
            .ok();

        sections.push(TraySection {
            plugin_id: plugin.manifest.id.clone(),
            plugin_name: plugin.manifest.name,
            providers,
            current_id,
        });
    }
    sections
}

/// 构建托盘菜单。
fn create_menu<R: Runtime>(
    app: &AppHandle<R>,
    db: &Database,
    registry: &PluginRegistry,
) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", "Show CC Switch", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let mut owned: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = vec![Box::new(show)];

    for section in build_menu_spec(db, registry) {
        if section.providers.is_empty() {
            let empty = MenuItem::with_id(
                app,
                format!("{EMPTY_PREFIX}{}", section.plugin_id),
                format!("{} (无供应商)", section.plugin_name),
                false,
                None::<&str>,
            )?;
            owned.push(Box::new(empty));
            continue;
        }
        let current_name = section
            .current_id
            .as_deref()
            .and_then(|id| section.providers.iter().find(|p| p.id == id))
            .map(|p| p.name.as_str())
            .unwrap_or("");
        let submenu_title = if current_name.is_empty() {
            section.plugin_name.clone()
        } else {
            format!("{} · {current_name}", section.plugin_name)
        };
        let mut sub_items: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = Vec::new();
        for provider in &section.providers {
            let is_current = section.current_id.as_deref() == Some(&provider.id);
            let item = CheckMenuItem::with_id(
                app,
                format!("{SWITCH_PREFIX}{}", provider.id),
                &provider.name,
                true,
                is_current,
                None::<&str>,
            )?;
            sub_items.push(Box::new(item));
        }
        let sub_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
            sub_items.iter().map(|b| b.as_ref()).collect();
        let submenu = Submenu::with_items(
            app,
            format!("submenu_{}", section.plugin_id),
            true,
            &sub_refs,
        )?;
        submenu.set_text(&submenu_title)?;
        owned.push(Box::new(submenu));
    }

    owned.push(Box::new(quit));
    let items: Vec<&dyn tauri::menu::IsMenuItem<R>> = owned.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &items)
}

/// 创建托盘图标并绑定事件。
pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let db = app.state::<Database>();
    let registry = app.state::<PluginRegistry>();
    let menu = create_menu(app, &db, &registry)?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("icon".to_string()))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("CC Switch v2")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "show" => show_main(app),
        "quit" => app.exit(0),
        _ => {
            if let Some(provider_id) = id.strip_prefix(SWITCH_PREFIX) {
                switch_from_tray(app, provider_id);
            }
        }
    }
}

/// 托盘点击切换：执行切换 → 重建菜单 → 广播事件。
fn switch_from_tray<R: Runtime>(app: &AppHandle<R>, provider_id: &str) {
    let db = app.state::<Database>();
    let registry = app.state::<PluginRegistry>();
    if let Err(e) = switch_provider_core(&registry, &db, provider_id) {
        log::error!("托盘切换 provider 失败: {e}");
        return;
    }
    let _ = rebuild_menu(app);
    let _ = app.emit("provider-switched", provider_id);
}

/// 就地重建托盘菜单（切换/数据变更后调用）。
pub fn rebuild_menu<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let db = app.state::<Database>();
    let registry = app.state::<PluginRegistry>();
    let menu = create_menu(app, &db, &registry).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 供前端在 provider 增删改/切换后调用，保持托盘菜单同步。
#[tauri::command]
pub fn update_tray_menu(app: tauri::AppHandle) -> Result<(), String> {
    rebuild_menu(&app)
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        (dir, db)
    }

    fn fake_registry(db: &Database) -> (tempfile::TempDir, PluginRegistry) {
        // 用真实插件目录构造，注入 opencode + 一个 ts 插件。
        let dir = tempfile::tempdir().unwrap();
        let registry = PluginRegistry::new(dir.path(), db.clone());
        let _ = registry.seed_builtin();
        // 手工安装一个 TS 插件（claudecode 示例 manifest）。
        let plugin_dir = dir.path().join("claudecode");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let manifest = serde_json::json!({
            "id": "claudecode",
            "name": "Claude Code",
            "version": "0.1.0",
            "apiVersion": "1",
            "capabilities": { "apply": true },
            "entry": { "type": "ts", "main": "main.js" }
        });
        std::fs::write(plugin_dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        (dir, registry)
    }

    #[test]
    fn menu_spec_excludes_ts_plugins() {
        let (_dir, db) = tmp_db();
        let (_guard, registry) = fake_registry(&db);
        let sections = build_menu_spec(&db, &registry);
        // opencode（native, apply）+ openclaw（shell, apply）
        let ids: Vec<&str> = sections.iter().map(|s| s.plugin_id.as_str()).collect();
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"openclaw"));
        // claudecode 是 TS，应被排除
        assert!(!ids.contains(&"claudecode"));
    }

    #[test]
    fn menu_spec_marks_current_and_empty() {
        let (_dir, db) = tmp_db();
        // 给 opencode 加两个 provider，其中一个设为当前
        db.lock()
            .execute(
                "INSERT INTO providers (id, plugin_id, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["p1", "opencode", "Provider One", 0],
            )
            .unwrap();
        db.lock()
            .execute(
                "INSERT INTO providers (id, plugin_id, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["p2", "opencode", "Provider Two", 1],
            )
            .unwrap();
        db.lock()
            .execute(
                "INSERT INTO app_state (plugin_id, current_provider_id) VALUES ('opencode', 'p1')",
                [],
            )
            .unwrap();

        let (_guard, registry) = fake_registry(&db);
        let sections = build_menu_spec(&db, &registry);
        let opencode = sections
            .iter()
            .find(|s| s.plugin_id == "opencode")
            .expect("opencode 应列出");
        assert_eq!(opencode.providers.len(), 2);
        assert_eq!(opencode.current_id.as_deref(), Some("p1"));

        // 无 provider 的插件（openclaw）应列为空 section
        let openclaw = sections
            .iter()
            .find(|s| s.plugin_id == "openclaw")
            .expect("openclaw 应列出");
        assert!(openclaw.providers.is_empty());
    }
}
