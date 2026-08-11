//! Prompts 服务。
//!
//! 统一存储在 `prompts` 表；`enabled` 启用时把内容写入指定插件的
//! prompt 文件（如 opencode 的 `AGENTS.md`）。

use std::path::PathBuf;

use rusqlite::{params, OptionalExtension, Row};

use crate::db::Database;

/// Prompt 记录。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRecord {
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_prompt(row: &Row<'_>) -> rusqlite::Result<PromptRecord> {
    Ok(PromptRecord {
        id: row.get(0)?,
        plugin_id: row.get(1)?,
        name: row.get(2)?,
        content: row.get(3)?,
        description: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

impl Database {
    /// 列出全部 prompts。
    pub fn list_prompts(&self, plugin_id: Option<&str>) -> rusqlite::Result<Vec<PromptRecord>> {
        let conn = self.lock();
        let mut stmt = match plugin_id {
            Some(_) => conn.prepare(
                "SELECT id, plugin_id, name, content, description, enabled, created_at, updated_at
                 FROM prompts WHERE plugin_id = ?1 ORDER BY name",
            )?,
            None => conn.prepare(
                "SELECT id, plugin_id, name, content, description, enabled, created_at, updated_at
                 FROM prompts ORDER BY plugin_id, name",
            )?,
        };
        let rows = match plugin_id {
            Some(pid) => stmt.query_map(params![pid], row_to_prompt)?,
            None => stmt.query_map([], row_to_prompt)?,
        };
        rows.collect()
    }

    /// 读取单个 prompt。
    pub fn get_prompt(&self, id: &str) -> rusqlite::Result<Option<PromptRecord>> {
        self.lock()
            .query_row(
                "SELECT id, plugin_id, name, content, description, enabled, created_at, updated_at
                 FROM prompts WHERE id = ?1",
                params![id],
                row_to_prompt,
            )
            .optional()
    }

    /// 新增/更新 prompt。
    pub fn upsert_prompt(
        &self,
        id: &str,
        plugin_id: &str,
        name: &str,
        content: &str,
        description: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO prompts (id, plugin_id, name, content, description, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               plugin_id = excluded.plugin_id,
               name = excluded.name,
               content = excluded.content,
               description = excluded.description,
               updated_at = excluded.updated_at",
            params![id, plugin_id, name, content, description],
        )?;
        Ok(())
    }

    /// 删除 prompt。
    pub fn delete_prompt(&self, id: &str) -> rusqlite::Result<bool> {
        let changed = self
            .lock()
            .execute("DELETE FROM prompts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// 设置启用状态。
    pub fn set_prompt_enabled(&self, id: &str, enabled: bool) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE prompts SET enabled = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }],
        )?;
        Ok(())
    }
}

/// 插件 prompt 文件路径（约定）。
pub fn plugin_prompt_file(plugin_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    match plugin_id {
        "opencode" => Some(home.join(".config").join("opencode").join("AGENTS.md")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        db.upsert_prompt("p1", "opencode", "Rules", "Be concise", Some("desc"))
            .unwrap();
        let prompt = db.get_prompt("p1").unwrap().unwrap();
        assert_eq!(prompt.name, "Rules");
        assert_eq!(prompt.content, "Be concise");
        assert!(prompt.enabled);

        db.set_prompt_enabled("p1", false).unwrap();
        let prompt = db.get_prompt("p1").unwrap().unwrap();
        assert!(!prompt.enabled);

        let list = db.list_prompts(Some("opencode")).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].plugin_id, "opencode");

        assert!(db.delete_prompt("p1").unwrap());
        assert!(db.get_prompt("p1").unwrap().is_none());
    }

    #[test]
    fn prompt_upsert_updates_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.upsert_prompt("p1", "opencode", "A", "old", None)
            .unwrap();
        db.upsert_prompt("p1", "opencode", "A", "new", None)
            .unwrap();
        let prompt = db.get_prompt("p1").unwrap().unwrap();
        assert_eq!(prompt.content, "new");
    }
}
