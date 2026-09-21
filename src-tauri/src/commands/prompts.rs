//! Prompts 管理命令。

use std::path::PathBuf;

use tauri::State;

use crate::db::Database;
use crate::registry::PluginRegistry;
use crate::services::prompts::{PromptRecord, PromptService};

/// 列出 prompts（可按插件过滤）。
#[tauri::command]
pub fn prompts_list(
    db: State<'_, Database>,
    plugin_id: Option<String>,
) -> Result<Vec<PromptRecord>, String> {
    db.list_prompts(plugin_id.as_deref())
        .map_err(|e| e.to_string())
}

/// 解析插件 prompt 文件路径。
fn resolve_prompt_file(registry: &PluginRegistry, plugin_id: &str) -> Result<PathBuf, String> {
    let plugin = registry
        .resolve_plugin(plugin_id)
        .map_err(|e| e.to_string())?;
    plugin
        .prompt_file_path()
        .ok_or_else(|| format!("插件 '{plugin_id}' 不支持 prompt 文件"))
}

/// 新增或更新 prompt（启用项保存后立即重写记忆文件）。
#[tauri::command]
pub fn prompts_upsert(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
    plugin_id: String,
    name: String,
    content: String,
    description: Option<String>,
) -> Result<(), String> {
    let file = resolve_prompt_file(&registry, &plugin_id)?;
    PromptService::save(
        &db,
        &file,
        &id,
        &plugin_id,
        &name,
        &content,
        description.as_deref(),
    )
}

/// 删除 prompt（已启用项拒绝删除）。
#[tauri::command]
pub fn prompts_delete(db: State<'_, Database>, id: String) -> Result<(), String> {
    PromptService::delete(&db, &id)
}

/// 启用/停用 prompt：启用走回填+互斥+写文件；停用清空（仅当无其他启用项）。
#[tauri::command]
pub fn prompts_toggle(
    db: State<'_, Database>,
    registry: State<'_, PluginRegistry>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let prompt = db
        .get_prompt(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt 不存在: {id}"))?;
    let file = resolve_prompt_file(&registry, &prompt.plugin_id)?;
    if enabled {
        PromptService::enable(&db, &file, &prompt.plugin_id, &id)
    } else {
        PromptService::disable(&db, &file, &prompt.plugin_id, &id)
    }
}
