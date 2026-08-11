//! Prompts 管理命令。

use tauri::State;

use crate::db::Database;
use crate::services::prompts::{plugin_prompt_file, PromptRecord};

/// 列出 prompts（可按插件过滤）。
#[tauri::command]
pub fn prompts_list(
    db: State<'_, Database>,
    plugin_id: Option<String>,
) -> Result<Vec<PromptRecord>, String> {
    db.list_prompts(plugin_id.as_deref())
        .map_err(|e| e.to_string())
}

/// 新增或更新 prompt。
#[tauri::command]
pub fn prompts_upsert(
    db: State<'_, Database>,
    id: String,
    plugin_id: String,
    name: String,
    content: String,
    description: Option<String>,
) -> Result<(), String> {
    db.upsert_prompt(&id, &plugin_id, &name, &content, description.as_deref())
        .map_err(|e| e.to_string())
}

/// 删除 prompt。
#[tauri::command]
pub fn prompts_delete(db: State<'_, Database>, id: String) -> Result<(), String> {
    db.delete_prompt(&id).map_err(|e| e.to_string())?;
    Ok(())
}

/// 启用/停用 prompt，并写入（或移除）插件的 prompt 文件。
#[tauri::command]
pub fn prompts_toggle(db: State<'_, Database>, id: String, enabled: bool) -> Result<(), String> {
    let prompt = db
        .get_prompt(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt 不存在: {id}"))?;
    let Some(file) = plugin_prompt_file(&prompt.plugin_id) else {
        return Err(format!("插件 '{}' 不支持 prompt 文件", prompt.plugin_id));
    };

    if enabled {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, &prompt.content).map_err(|e| e.to_string())?;
    } else if file.exists() {
        std::fs::remove_file(&file).map_err(|e| e.to_string())?;
    }
    db.set_prompt_enabled(&id, enabled)
        .map_err(|e| e.to_string())
}
