//! 数据库备份与配置导入导出服务。

use std::path::{Path, PathBuf};

use rusqlite::{params, Row};

use crate::db::Database;

/// 备份记录。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    pub name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub created_at: i64,
}

impl Database {
    /// 创建一个数据库备份：把当前 db 文件复制到备份目录并记录。
    pub fn create_db_backup(&self, backups_dir: &Path) -> Result<BackupRecord, String> {
        // 获取当前 db 文件路径（从 pragma database_list）。
        let db_path: String = self
            .lock()
            .query_row("PRAGMA database_list", [], |row| {
                let name: String = row.get(1)?;
                let file: String = row.get(2)?;
                Ok((name, file))
            })
            .map_err(|e| e.to_string())?
            .1;
        if db_path.is_empty() || db_path == ":memory:" {
            return Err("无法备份内存数据库".to_string());
        }

        std::fs::create_dir_all(backups_dir).map_err(|e| e.to_string())?;
        let id = format!(
            "bak_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let dest = backups_dir.join(format!("{id}.db"));
        std::fs::copy(&db_path, &dest).map_err(|e| e.to_string())?;
        let size = std::fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);
        let now = now_secs();

        self.lock()
            .execute(
                "INSERT INTO db_backups (id, name, file_path, size_bytes, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, id.clone(), dest.display().to_string(), size, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(self
            .get_db_backup(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "备份记录写入失败".to_string())?)
    }

    /// 列出备份记录。
    pub fn list_db_backups(&self) -> rusqlite::Result<Vec<BackupRecord>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, name, file_path, size_bytes, created_at FROM db_backups ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row: &Row<'_>| {
            Ok(BackupRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                file_path: row.get(2)?,
                size_bytes: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// 读取单个备份记录。
    pub fn get_db_backup(&self, id: &str) -> rusqlite::Result<Option<BackupRecord>> {
        self.lock()
            .query_row(
                "SELECT id, name, file_path, size_bytes, created_at FROM db_backups WHERE id = ?1",
                params![id],
                |row: &Row<'_>| {
                    Ok(BackupRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        file_path: row.get(2)?,
                        size_bytes: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .map(Some)
    }

    /// 删除备份记录（同时删除文件）。
    pub fn delete_db_backup(&self, backups_dir: &Path, id: &str) -> Result<bool, String> {
        let record = self
            .get_db_backup(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("备份不存在: {id}"))?;
        self.lock()
            .execute("DELETE FROM db_backups WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        let file = PathBuf::from(&record.file_path);
        let _ = std::fs::remove_file(&file);
        let _ = backups_dir;
        Ok(true)
    }
}

/// 完整配置导出（providers/mcp/skills/prompts/profiles）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub version: i32,
    pub providers: Vec<serde_json::Value>,
    pub mcp_servers: Vec<serde_json::Value>,
    pub skills: Vec<serde_json::Value>,
    pub prompts: Vec<serde_json::Value>,
    pub profiles: Vec<serde_json::Value>,
}

impl Database {
    /// 导出全部配置为 JSON 负载。
    pub fn export_config(&self) -> Result<ExportPayload, String> {
        let conn = self.lock();
        let providers = query_all(&conn, "SELECT * FROM providers");
        let mcp_servers = query_all(&conn, "SELECT * FROM mcp_servers");
        let skills = query_all(&conn, "SELECT * FROM skills");
        let prompts = query_all(&conn, "SELECT * FROM prompts");
        let profiles = query_all(&conn, "SELECT * FROM profiles");
        Ok(ExportPayload {
            version: 1,
            providers,
            mcp_servers,
            skills,
            prompts,
            profiles,
        })
    }
}

fn query_all(conn: &rusqlite::Connection, sql: &str) -> Vec<serde_json::Value> {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let column_count = stmt.column_count();
    let mut column_names = Vec::with_capacity(column_count);
    for i in 0..column_count {
        column_names.push(stmt.column_name(i).map(|n| n.to_string()).unwrap_or_default());
    }
    let mut rows = Vec::new();
    let mut query = match stmt.query([]) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    while let Ok(Some(row)) = query.next() {
        let mut obj = serde_json::Map::new();
        for (i, name) in column_names.iter().enumerate() {
            let value = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Text(s)) => {
                    serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                }
                Ok(rusqlite::types::ValueRef::Integer(v)) => serde_json::Value::from(v),
                Ok(rusqlite::types::ValueRef::Real(v)) => serde_json::json!(v),
                Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                _ => continue,
            };
            obj.insert(name.clone(), value);
        }
        rows.push(serde_json::Value::Object(obj));
    }
    rows
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_create_list_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let backups_dir = dir.path().join("backups");

        let backup = db.create_db_backup(&backups_dir).unwrap();
        assert!(PathBuf::from(&backup.file_path).exists());
        assert!(backup.size_bytes >= 0);

        let list = db.list_db_backups().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, backup.id);

        db.delete_db_backup(&backups_dir, &backup.id).unwrap();
        assert!(!PathBuf::from(&backup.file_path).exists());
        assert!(db.list_db_backups().unwrap().is_empty());
    }

    #[test]
    fn export_config_serializes_tables() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let conn = db.lock();
        conn.execute(
            "INSERT INTO providers (id, plugin_id, name) VALUES ('p1', 'opencode', 'P')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mcp_servers (id, name, server_config) VALUES ('m1', 'M', '{}')",
            [],
        )
        .unwrap();
        drop(conn);

        let payload = db.export_config().unwrap();
        assert_eq!(payload.providers.len(), 1);
        assert_eq!(payload.providers[0]["id"], "p1");
        assert_eq!(payload.mcp_servers.len(), 1);
        assert_eq!(payload.version, 1);
    }
}
