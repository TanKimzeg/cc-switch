//! Prompts 服务。
//!
//! 统一存储在 `prompts` 表；`enabled` 启用时把内容写入指定插件的
//! prompt 文件（路径由插件协议 `AgentPlugin::prompt_file_path` 提供）。
//!
//! 行为对齐 v1 `src-tauri/src/services/prompt.rs`：
//! - **单插件单激活（互斥）**：启用一个 prompt 会先禁用该插件的其他 prompt。
//! - **回填保护**：启用前读取当前 live 记忆文件，非空时回填到已启用项，
//!   或创建禁用的备份 prompt（`原始提示词 …`），防止用户手改内容丢失。
//! - **停用最后一个启用项**时清空记忆文件（写 `""`，不删除文件）。
//! - **已启用项不可删除**。
//! - 首次启动全表为空时，自动把各插件记忆文件导入为启用项。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Prompt 业务逻辑（路径参数化以便测试；`file` 为插件记忆文件路径）。
pub struct PromptService;

impl PromptService {
    /// 原子写文本文件（临时文件 + rename，对齐 v1 `write_text_file`）。
    pub fn write_text_file(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let nonce = now_ts();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "prompt".to_string());
        let tmp = path.with_file_name(format!(
            ".{file_name}.tmp-{}-{nonce}",
            std::process::id()
        ));
        if let Err(e) = std::fs::write(&tmp, content) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("写入 prompt 文件失败: {}: {e}", path.display())
        })
    }

    /// 启用 prompt：回填 live 文件 → 互斥禁用其他 → 启用目标 → 写文件。
    pub fn enable(
        db: &Database,
        file: &Path,
        plugin_id: &str,
        id: &str,
    ) -> Result<(), String> {
        // 回填：把当前 live 文件内容保存到已启用项，或创建备份。
        if let Ok(live_content) = std::fs::read_to_string(file) {
            if !live_content.trim().is_empty() {
                let prompts = db.list_prompts(Some(plugin_id)).map_err(|e| e.to_string())?;
                if let Some(mut enabled) = prompts
                    .iter()
                    .find(|p| p.enabled)
                    .cloned()
                {
                    enabled.content = live_content.clone();
                    db.save_prompt(&enabled).map_err(|e| e.to_string())?;
                    log::info!("回填 live 提示词内容到已启用项: {}", enabled.id);
                } else {
                    let content_exists = prompts
                        .iter()
                        .any(|p| p.content.trim() == live_content.trim());
                    if !content_exists {
                        let timestamp = now_ts();
                        let backup = PromptRecord {
                            id: format!("backup-{timestamp}"),
                            plugin_id: plugin_id.to_string(),
                            name: format!(
                                "原始提示词 {}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M")
                            ),
                            content: live_content,
                            description: Some("自动备份的原始提示词".to_string()),
                            enabled: false,
                            created_at: String::new(),
                            updated_at: String::new(),
                        };
                        db.save_prompt(&backup).map_err(|e| e.to_string())?;
                        log::info!("回填 live 提示词内容，创建备份: {}", backup.id);
                    }
                }
            }
        }

        // 互斥禁用同插件其他 prompt。
        let prompts = db.list_prompts(Some(plugin_id)).map_err(|e| e.to_string())?;
        for prompt in &prompts {
            if prompt.id != id {
                db.set_prompt_enabled(&prompt.id, false)
                    .map_err(|e| e.to_string())?;
            }
        }
        db.set_prompt_enabled(id, true).map_err(|e| e.to_string())?;

        let prompt = db
            .get_prompt(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("提示词 {id} 不存在"))?;
        Self::write_text_file(file, &prompt.content)?;
        Ok(())
    }

    /// 停用 prompt：若该插件不再有启用项，则清空记忆文件。
    pub fn disable(
        db: &Database,
        file: &Path,
        plugin_id: &str,
        id: &str,
    ) -> Result<(), String> {
        db.set_prompt_enabled(id, false).map_err(|e| e.to_string())?;
        let remaining = db.list_prompts(Some(plugin_id)).map_err(|e| e.to_string())?;
        if !remaining.iter().any(|p| p.enabled) && file.exists() {
            Self::write_text_file(file, "")?;
        }
        Ok(())
    }

    /// 保存（upsert 语义）：启用项保存后立即重写记忆文件。
    pub fn save(
        db: &Database,
        file: &Path,
        id: &str,
        plugin_id: &str,
        name: &str,
        content: &str,
        description: Option<&str>,
    ) -> Result<(), String> {
        let was_enabled = db
            .get_prompt(id)
            .map_err(|e| e.to_string())?
            .map(|p| p.enabled)
            .unwrap_or(false);
        db.upsert_prompt(id, plugin_id, name, content, description)
            .map_err(|e| e.to_string())?;
        if was_enabled {
            Self::write_text_file(file, content)?;
        }
        Ok(())
    }

    /// 删除：已启用项拒绝删除（避免文件孤儿）。
    pub fn delete(db: &Database, id: &str) -> Result<(), String> {
        let prompt = db
            .get_prompt(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("提示词 {id} 不存在"))?;
        if prompt.enabled {
            return Err("无法删除已启用的提示词".to_string());
        }
        db.delete_prompt(id).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 首次启动：`prompts` 表全空时，把各插件记忆文件导入为启用项。
    ///
    /// `sources` 为 `(插件 id, 记忆文件路径)` 列表。
    pub fn auto_import_first_launch(
        db: &Database,
        sources: &[(String, PathBuf)],
    ) -> Result<usize, String> {
        let count: i64 = db
            .lock()
            .query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Ok(0);
        }
        let mut imported = 0;
        for (plugin_id, file) in sources {
            let Ok(content) = std::fs::read_to_string(file) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }
            let record = PromptRecord {
                id: format!("auto-imported-{}-{imported}", now_ts()),
                plugin_id: plugin_id.clone(),
                name: format!(
                    "Auto-imported Prompt {}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M")
                ),
                content,
                description: Some("Automatically imported on first launch".to_string()),
                enabled: true,
                created_at: String::new(),
                updated_at: String::new(),
            };
            db.save_prompt(&record).map_err(|e| e.to_string())?;
            imported += 1;
        }
        if imported > 0 {
            log::info!("首次启动自动导入 {imported} 个 prompt");
        }
        Ok(imported)
    }
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

    /// 新增/更新 prompt（不触碰 enabled；新行默认禁用，对齐 v1）。
    pub fn upsert_prompt(
        &self,
        id: &str,
        plugin_id: &str,
        name: &str,
        content: &str,
        description: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO prompts (id, plugin_id, name, content, description, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, datetime('now'))
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

    /// 全字段保存 prompt（含 enabled，用于回填备份/自动导入）。
    pub fn save_prompt(&self, prompt: &PromptRecord) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO prompts (id, plugin_id, name, content, description, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               plugin_id = excluded.plugin_id,
               name = excluded.name,
               content = excluded.content,
               description = excluded.description,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at",
            params![
                prompt.id,
                prompt.plugin_id,
                prompt.name,
                prompt.content,
                prompt.description,
                prompt.enabled,
            ],
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn prompt_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        db.upsert_prompt("p1", "opencode", "Rules", "Be concise", Some("desc"))
            .unwrap();
        let prompt = db.get_prompt("p1").unwrap().unwrap();
        assert_eq!(prompt.name, "Rules");
        assert_eq!(prompt.content, "Be concise");
        assert!(!prompt.enabled); // 新 prompt 默认禁用

        db.set_prompt_enabled("p1", true).unwrap();
        let prompt = db.get_prompt("p1").unwrap().unwrap();
        assert!(prompt.enabled);

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

    #[test]
    fn enable_is_mutually_exclusive_and_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let file = write_skill(dir.path(), "AGENTS.md", "old live");

        db.upsert_prompt("a", "opencode", "A", "content A", None).unwrap();
        db.upsert_prompt("b", "opencode", "B", "content B", None).unwrap();

        PromptService::enable(&db, &file, "opencode", "a").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "content A");
        assert!(db.get_prompt("a").unwrap().unwrap().enabled);
        assert!(!db.get_prompt("b").unwrap().unwrap().enabled);

        // 切换：回填 live 到 a，再写 b
        PromptService::enable(&db, &file, "opencode", "b").unwrap();
        let a = db.get_prompt("a").unwrap().unwrap();
        assert!(!a.enabled);
        assert_eq!(a.content, "content A"); // a 未被回填（live 是我们刚写的 content A）
        assert!(db.get_prompt("b").unwrap().unwrap().enabled);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "content B");
    }

    #[test]
    fn enable_backfills_manual_live_edits() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let file = write_skill(dir.path(), "AGENTS.md", "user edited");

        // 无启用项时启用 → 创建禁用的备份 prompt
        db.upsert_prompt("a", "opencode", "A", "content A", None).unwrap();
        PromptService::enable(&db, &file, "opencode", "a").unwrap();
        let backup = db
            .list_prompts(Some("opencode"))
            .unwrap()
            .into_iter()
            .find(|p| p.id.starts_with("backup-"))
            .expect("应创建备份 prompt");
        assert!(!backup.enabled);
        assert_eq!(backup.content, "user edited");
        assert!(backup.name.contains("原始提示词"));
        // 文件被覆盖为启用项内容
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "content A");
    }

    #[test]
    fn disable_last_clears_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let file = write_skill(dir.path(), "AGENTS.md", "x");

        db.upsert_prompt("a", "opencode", "A", "content A", None).unwrap();
        PromptService::enable(&db, &file, "opencode", "a").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "content A");

        PromptService::disable(&db, &file, "opencode", "a").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
        assert!(file.exists());
    }

    #[test]
    fn save_enabled_prompt_rewrites_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let file = write_skill(dir.path(), "AGENTS.md", "x");

        db.upsert_prompt("a", "opencode", "A", "old", None).unwrap();
        PromptService::enable(&db, &file, "opencode", "a").unwrap();
        PromptService::save(&db, &file, "a", "opencode", "A", "new content", None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
    }

    #[test]
    fn delete_refuses_enabled_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let file = write_skill(dir.path(), "AGENTS.md", "x");

        db.upsert_prompt("a", "opencode", "A", "content", None).unwrap();
        PromptService::enable(&db, &file, "opencode", "a").unwrap();
        assert!(PromptService::delete(&db, "a").is_err());

        PromptService::disable(&db, &file, "opencode", "a").unwrap();
        assert!(PromptService::delete(&db, "a").is_ok());
        assert!(db.get_prompt("a").unwrap().is_none());
    }
}
