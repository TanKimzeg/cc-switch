use rusqlite::{params, OptionalExtension, Row};
use tauri::State;
use uuid::Uuid;

use crate::db::Database;
use crate::types::{Provider, ProviderInput};

const PROVIDER_COLUMNS: &str = "id, plugin_id, name, category, icon, website, api_key, settings_config, meta, sort_order, created_at, updated_at";

fn row_to_provider(row: &Row<'_>) -> rusqlite::Result<Provider> {
    let meta: Option<String> = row.get("meta")?;
    let meta = meta
        .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
    Ok(Provider {
        id: row.get("id")?,
        plugin_id: row.get("plugin_id")?,
        name: row.get("name")?,
        category: row.get("category")?,
        icon: row.get("icon")?,
        website: row.get("website")?,
        api_key: row.get("api_key")?,
        settings_config: row.get("settings_config")?,
        meta,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// 返回插件列表；`plugin_id` 为空则返回全部。
#[tauri::command]
pub fn get_providers(
    db: State<'_, Database>,
    plugin_id: Option<String>,
) -> Result<Vec<Provider>, String> {
    let conn = db.lock();
    let mut stmt = match plugin_id {
        Some(_) => conn
            .prepare(&format!(
                "SELECT {PROVIDER_COLUMNS} FROM providers WHERE plugin_id = ?1 ORDER BY sort_order, created_at"
            ))
            .map_err(|e| e.to_string())?,
        None => conn
            .prepare(&format!(
                "SELECT {PROVIDER_COLUMNS} FROM providers ORDER BY sort_order, created_at"
            ))
            .map_err(|e| e.to_string())?,
    };
    let rows = match plugin_id {
        Some(pid) => {
            let iter = stmt
                .query_map(params![pid], row_to_provider)
                .map_err(|e| e.to_string())?;
            iter.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        }
        None => {
            let iter = stmt
                .query_map([], row_to_provider)
                .map_err(|e| e.to_string())?;
            iter.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        }
    };
    Ok(rows)
}

#[tauri::command]
pub fn add_provider(
    db: State<'_, Database>,
    input: ProviderInput,
) -> Result<Provider, String> {
    let input = input.normalize();
    let id = Uuid::new_v4().to_string();
    let meta = input
        .meta
        .map(|m| serde_json::to_string(&m))
        .transpose()
        .map_err(|e| e.to_string())?;
    db.lock()
        .execute(
            "INSERT INTO providers (id, plugin_id, name, category, icon, website, api_key, settings_config, meta, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                input.plugin_id,
                input.name,
                input.category,
                input.icon,
                input.website,
                input.api_key,
                input.settings_config,
                meta,
                input.sort_order.unwrap_or(0),
            ],
        )
        .map_err(|e| e.to_string())?;
    get_provider_by_id(&db, &id)
}

#[tauri::command]
pub fn update_provider(
    db: State<'_, Database>,
    id: String,
    input: ProviderInput,
) -> Result<Provider, String> {
    let input = input.normalize();
    let meta = input
        .meta
        .map(|m| serde_json::to_string(&m))
        .transpose()
        .map_err(|e| e.to_string())?;
    let changed = db
        .lock()
        .execute(
            "UPDATE providers SET name = ?2, category = ?3, icon = ?4, website = ?5, api_key = ?6,
                    settings_config = ?7, meta = ?8, sort_order = ?9, updated_at = datetime('now')
             WHERE id = ?1",
            params![
                id,
                input.name,
                input.category,
                input.icon,
                input.website,
                input.api_key,
                input.settings_config,
                meta,
                input.sort_order.unwrap_or(0),
            ],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err(format!("provider not found: {id}"));
    }
    get_provider_by_id(&db, &id)
}

#[tauri::command]
pub fn delete_provider(db: State<'_, Database>, id: String) -> Result<(), String> {
    let conn = db.lock();
    conn.execute("DELETE FROM providers WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE app_state SET current_provider_id = NULL WHERE current_provider_id = ?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_current_provider(db: State<'_, Database>) -> Result<Option<Provider>, String> {
    let conn = db.lock();
    let pid: Option<String> = conn
        .query_row(
            "SELECT current_provider_id FROM app_state WHERE plugin_id = 'default'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(pid) = pid else {
        return Ok(None);
    };
    get_provider_by_id(&db, &pid).map(Some)
}

#[tauri::command]
pub fn set_current_provider(
    db: State<'_, Database>,
    id: Option<String>,
) -> Result<(), String> {
    db.lock()
        .execute(
            "INSERT INTO app_state (plugin_id, current_provider_id) VALUES ('default', ?1)
             ON CONFLICT(plugin_id) DO UPDATE SET current_provider_id = excluded.current_provider_id",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn get_provider_by_id(db: &Database, id: &str) -> Result<Provider, String> {
    db.lock()
        .query_row(
            &format!("SELECT {PROVIDER_COLUMNS} FROM providers WHERE id = ?1"),
            params![id],
            row_to_provider,
        )
        .map_err(|e| e.to_string())
}
