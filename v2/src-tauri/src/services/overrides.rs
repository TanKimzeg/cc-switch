//! 设备级配置目录覆盖（对齐 v1 `settings.rs` 的 `*_config_dir` + `app_store.rs`）。
//!
//! 两类覆盖：
//! - **各工具（插件）配置目录**：settings 表键 `overrideDir.<plugin_id>` 存原始路径
//!   （`~` 读取时展开）。native 插件在 `config_dir()` 中优先读取本注册表。
//! - **CC Switch 自身数据目录**：指针文件 `{app_config_dir}/app_paths.json`
//!   存 `appDataDirOverride`（须在打开数据库前读取；目录不存在时回退默认）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use crate::db::Database;

static OVERRIDES: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, String>> {
    OVERRIDES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 测试/真实用户主目录（`CC_SWITCH_TEST_HOME` 优先，对齐 native 插件测试约定）。
fn home_dir() -> PathBuf {
    std::env::var("CC_SWITCH_TEST_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
}

/// 读取时展开 `~`（对齐 v1 `resolve_override_path`；相对路径原样返回）。
fn resolve(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir();
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        return home_dir().join(stripped);
    }
    if let Some(stripped) = path.strip_prefix("~\\") {
        return home_dir().join(stripped);
    }
    PathBuf::from(path)
}

/// 从 settings 表加载全部 `overrideDir.*` 到静态注册表。
pub fn init(db: &Database) -> Result<(), String> {
    let conn = db.lock();
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings WHERE key LIKE 'overrideDir.%'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for row in rows.flatten() {
        let (key, value) = row;
        if let Some(plugin_id) = key.strip_prefix("overrideDir.") {
            if !value.trim().is_empty() {
                map.insert(plugin_id.to_string(), value);
            }
        }
    }
    drop(stmt);
    drop(conn);
    match registry().write() {
        Ok(mut guard) => {
            *guard = map;
            Ok(())
        }
        Err(e) => Err(format!("覆盖注册表锁损坏: {e}")),
    }
}

/// 读取某插件的配置目录覆盖（`~` 已展开）；未设置返回 None。
pub fn get(plugin_id: &str) -> Option<PathBuf> {
    registry()
        .read()
        .ok()?
        .get(plugin_id)
        .map(|raw| resolve(raw))
}

/// 设置/清除某插件的配置目录覆盖，并刷新注册表。
pub fn set(db: &Database, plugin_id: &str, path: Option<&str>) -> Result<(), String> {
    match path {
        Some(p) if !p.trim().is_empty() => {
            db.set_setting(&format!("overrideDir.{plugin_id}"), p.trim())
                .map_err(|e| e.to_string())?;
        }
        _ => {
            db.lock()
                .execute(
                    "DELETE FROM settings WHERE key = ?1",
                    rusqlite::params![format!("overrideDir.{plugin_id}")],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    init(db)
}

/// 已配置的工具目录覆盖。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideDir {
    pub plugin_id: String,
    pub path: String,
}

/// 列出全部已配置的工具目录覆盖。
pub fn list(db: &Database) -> Result<Vec<OverrideDir>, String> {
    init(db)?;
    let guard = registry().read().map_err(|e| e.to_string())?;
    let mut items: Vec<OverrideDir> = guard
        .iter()
        .map(|(k, v)| OverrideDir {
            plugin_id: k.clone(),
            path: v.clone(),
        })
        .collect();
    items.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    Ok(items)
}

// ========== CC Switch 数据目录覆盖 ==========

const APP_PATHS_FILE: &str = "app_paths.json";
const APP_DATA_DIR_KEY: &str = "appDataDirOverride";

fn read_app_paths(config_dir: &Path) -> serde_json::Value {
    let file = config_dir.join(APP_PATHS_FILE);
    match std::fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

/// 读取 CC Switch 数据目录覆盖：设置且目录存在时返回，否则 None（回退默认）。
///
/// 须在打开数据库/初始化路径**之前**调用。
pub fn get_app_data_dir_override(config_dir: &Path) -> Option<PathBuf> {
    let value = read_app_paths(config_dir);
    let raw = value.get(APP_DATA_DIR_KEY)?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    let path = resolve(raw);
    if !path.is_dir() {
        log::warn!("数据目录覆盖不存在，回退默认路径: {}", path.display());
        return None;
    }
    Some(path)
}

/// 设置/清除 CC Switch 数据目录覆盖（写指针文件，不移动任何数据）。
pub fn set_app_data_dir_override(config_dir: &Path, path: Option<&str>) -> Result<bool, String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    let mut value = read_app_paths(config_dir);
    match path {
        Some(p) if !p.trim().is_empty() => {
            value[APP_DATA_DIR_KEY] = serde_json::json!(p.trim());
        }
        _ => {
            if let Some(obj) = value.as_object_mut() {
                obj.remove(APP_DATA_DIR_KEY);
            }
        }
    }
    let file = config_dir.join(APP_PATHS_FILE);
    let json = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&file, json).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db(dir: &tempfile::TempDir) -> Database {
        Database::new(&dir.path().join("cc.db")).unwrap()
    }

    #[test]
    fn set_get_clear_roundtrip() {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir);
        init(&db).unwrap();
        assert!(get("opencode").is_none());

        set(&db, "opencode", Some("~/custom/opencode")).unwrap();
        // `~` 展开到测试主目录
        let resolved = get("opencode").unwrap();
        assert_eq!(resolved, resolve("~/custom/opencode"));

        // 清除
        set(&db, "opencode", None).unwrap();
        assert!(get("opencode").is_none());
    }

    #[test]
    fn list_returns_configured_overrides() {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir);
        set(&db, "opencode", Some("~/oc")).unwrap();
        set(&db, "claudecode", Some("/abs/path")).unwrap();
        let items = list(&db).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .any(|d| d.plugin_id == "opencode" && d.path == "~/oc"));
    }

    #[test]
    fn app_data_dir_override_requires_existing_dir() {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");

        // 未设置 → None
        assert!(get_app_data_dir_override(&config_dir).is_none());

        // 设置为存在的目录 → Some
        let existing = dir.path().join("sync-data");
        std::fs::create_dir_all(&existing).unwrap();
        set_app_data_dir_override(&config_dir, Some(existing.to_str().unwrap())).unwrap();
        let got = get_app_data_dir_override(&config_dir).unwrap();
        assert_eq!(got, existing);

        // 设置为不存在的目录 → None（回退默认）
        set_app_data_dir_override(&config_dir, Some("~/nonexistent-override-dir-xyz")).unwrap();
        assert!(get_app_data_dir_override(&config_dir).is_none());

        // 清除 → None
        set_app_data_dir_override(&config_dir, None).unwrap();
        assert!(get_app_data_dir_override(&config_dir).is_none());
    }
}
