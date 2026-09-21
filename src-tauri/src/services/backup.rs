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
        self.create_db_backup_with_prefix(backups_dir, "bak_")
    }

    fn create_db_backup_with_prefix(
        &self,
        backups_dir: &Path,
        prefix: &str,
    ) -> Result<BackupRecord, String> {
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
        let seq = BACKUP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!(
            "{prefix}{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            seq
        );
        let dest = backups_dir.join(format!("{id}.db"));

        // 用 SQLite backup API 把活动连接的内容完整写入副本文件
        // （db 为 WAL 模式，直接复制主文件会丢失未 checkpoint 的页）。
        {
            let src = self.lock();
            let mut dst = rusqlite::Connection::open(&dest).map_err(|e| e.to_string())?;
            let backup = rusqlite::backup::Backup::new(&src, &mut dst)
                .map_err(|e| format!("无法初始化备份: {e}"))?;
            backup.step(-1).map_err(|e| format!("写备份失败: {e}"))?;
        }

        let size = std::fs::metadata(&dest)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let now = now_secs();

        self.lock()
            .execute(
                "INSERT INTO db_backups (id, name, file_path, size_bytes, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, id.clone(), dest.display().to_string(), size, now],
            )
            .map_err(|e| e.to_string())?;

        self.get_db_backup(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "备份记录写入失败".to_string())
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

    /// 重命名备份记录。
    pub fn rename_db_backup(&self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("备份名称不能为空".to_string());
        }
        let n = self
            .lock()
            .execute(
                "UPDATE db_backups SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("备份不存在: {id}"));
        }
        Ok(())
    }

    /// 恢复备份：先创建安全备份，再把备份文件内容回灌到当前数据库。
    ///
    /// 返回安全备份 id。使用 SQLite backup API 在连接保持打开的情况下逐页覆盖，
    /// 避免文件级替换与活动连接的竞态。
    pub fn restore_db_backup(&self, backups_dir: &Path, id: &str) -> Result<String, String> {
        let record = self
            .get_db_backup(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("备份不存在: {id}"))?;
        let src_path = PathBuf::from(&record.file_path);
        if !src_path.exists() {
            return Err(format!("备份文件缺失: {}", record.file_path));
        }

        // 恢复前先建当前状态的安全备份。
        let safety = self.create_db_backup(backups_dir)?;

        let src = rusqlite::Connection::open(&src_path).map_err(|e| e.to_string())?;
        {
            let mut dst = self.lock();
            let backup = rusqlite::backup::Backup::new(&src, &mut dst)
                .map_err(|e| format!("无法初始化恢复: {e}"))?;
            backup
                .step(-1)
                .map_err(|e| format!("恢复数据库失败: {e}"))?;
        }

        // 整库回灌会把 db_backups 表一并还原为快照时刻的状态，
        // 安全备份记录需补写回恢复后的库，否则前端拿不到安全备份 id。
        self.lock()
            .execute(
                "INSERT OR REPLACE INTO db_backups (id, name, file_path, size_bytes, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    safety.id,
                    safety.name,
                    safety.file_path,
                    safety.size_bytes,
                    safety.created_at
                ],
            )
            .map_err(|e| e.to_string())?;

        Ok(safety.id)
    }
}

/// 自动备份的设置键与轮换逻辑。
pub mod auto {
    use super::*;

    pub const KEY_INTERVAL_HOURS: &str = "backup.intervalHours";
    pub const KEY_RETAIN_COUNT: &str = "backup.retainCount";
    pub const KEY_LAST_AUTO_AT: &str = "backup.lastAutoAt";
    /// 自动备份的名称/id 前缀（手动备份为 `bak_`）。
    pub const AUTO_PREFIX: &str = "auto_";

    impl Database {
        /// 自动备份间隔（小时）。0 = 禁用；缺省 24（对齐 v1）。
        pub fn get_backup_interval_hours(&self) -> u32 {
            self.get_setting(KEY_INTERVAL_HOURS)
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(24)
        }

        /// 自动备份保留数量。0 视为 1；缺省 10（对齐 v1）。
        pub fn get_backup_retain_count(&self) -> u32 {
            self.get_setting(KEY_RETAIN_COUNT)
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10)
                .max(1)
        }

        /// 创建一份自动备份（`auto_` 前缀）并按保留数量轮换旧自动备份。
        pub fn create_auto_backup(&self, backups_dir: &Path) -> Result<BackupRecord, String> {
            let record = self.create_db_backup_with_prefix(backups_dir, AUTO_PREFIX)?;

            let retain = self.get_backup_retain_count() as usize;
            let stale_ids: Vec<String> = {
                let conn = self.lock();
                let mut stmt = conn
                    .prepare(
                        "SELECT id FROM db_backups WHERE id LIKE 'auto\\_%' ESCAPE '\\' ORDER BY created_at DESC, rowid DESC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                rows.filter_map(|r| r.ok()).collect()
            };
            for id in stale_ids.iter().skip(retain) {
                if let Err(e) = self.delete_db_backup(backups_dir, id) {
                    log::warn!("清理过期自动备份 {id} 失败: {e}");
                }
            }

            let now = now_secs();
            self.set_setting(KEY_LAST_AUTO_AT, &now.to_string())
                .map_err(|e| e.to_string())?;

            Ok(record)
        }

        /// 若到达自动备份时间则执行一次；返回是否创建了备份。
        pub fn auto_backup_tick(&self, backups_dir: &Path) -> Result<bool, String> {
            let interval = self.get_backup_interval_hours();
            if interval == 0 {
                return Ok(false);
            }
            let now = now_secs();
            let last: i64 = self
                .get_setting(KEY_LAST_AUTO_AT)
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if last != 0 && now - last < i64::from(interval) * 3600 {
                return Ok(false);
            }
            self.create_auto_backup(backups_dir)?;
            Ok(true)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn defaults_and_parsing_for_backup_settings() {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(&dir.path().join("cc.db")).unwrap();

            assert_eq!(db.get_backup_interval_hours(), 24);
            assert_eq!(db.get_backup_retain_count(), 10);

            db.set_setting(KEY_INTERVAL_HOURS, "6").unwrap();
            db.set_setting(KEY_RETAIN_COUNT, "3").unwrap();
            assert_eq!(db.get_backup_interval_hours(), 6);
            assert_eq!(db.get_backup_retain_count(), 3);

            // 非法值回退默认
            db.set_setting(KEY_INTERVAL_HOURS, "abc").unwrap();
            db.set_setting(KEY_RETAIN_COUNT, "-4").unwrap();
            assert_eq!(db.get_backup_interval_hours(), 24);
            assert_eq!(db.get_backup_retain_count(), 10);
        }

        #[test]
        fn auto_backup_tick_respects_interval() {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(&dir.path().join("cc.db")).unwrap();
            let backups_dir = dir.path().join("backups");

            // 首次 tick：立即创建
            assert!(db.auto_backup_tick(&backups_dir).unwrap());
            let list = db.list_db_backups().unwrap();
            assert_eq!(list.len(), 1);
            assert!(list[0].id.starts_with(AUTO_PREFIX));

            // 未到期：跳过
            assert!(!db.auto_backup_tick(&backups_dir).unwrap());

            // 到期：再创建
            db.set_setting(KEY_LAST_AUTO_AT, "0").unwrap();
            assert!(db.auto_backup_tick(&backups_dir).unwrap());
            assert_eq!(db.list_db_backups().unwrap().len(), 2);

            // 禁用：不创建
            db.set_setting(KEY_INTERVAL_HOURS, "0").unwrap();
            db.set_setting(KEY_LAST_AUTO_AT, "0").unwrap();
            assert!(!db.auto_backup_tick(&backups_dir).unwrap());
        }

        #[test]
        fn auto_backup_rotation_keeps_retain_count() {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(&dir.path().join("cc.db")).unwrap();
            let backups_dir = dir.path().join("backups");
            db.set_setting(KEY_RETAIN_COUNT, "2").unwrap();

            for _ in 0..4 {
                db.set_setting(KEY_LAST_AUTO_AT, "0").unwrap();
                db.auto_backup_tick(&backups_dir).unwrap();
            }
            let autos: Vec<_> = db
                .list_db_backups()
                .unwrap()
                .into_iter()
                .filter(|b| b.id.starts_with(AUTO_PREFIX))
                .collect();
            assert_eq!(autos.len(), 2);
            for b in &autos {
                assert!(PathBuf::from(&b.file_path).exists());
            }
        }

        #[test]
        fn manual_backups_are_not_rotated_by_auto() {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(&dir.path().join("cc.db")).unwrap();
            let backups_dir = dir.path().join("backups");
            db.set_setting(KEY_RETAIN_COUNT, "1").unwrap();

            db.create_db_backup(&backups_dir).unwrap(); // 手动
            db.set_setting(KEY_LAST_AUTO_AT, "0").unwrap();
            db.auto_backup_tick(&backups_dir).unwrap();

            let list = db.list_db_backups().unwrap();
            assert_eq!(list.len(), 2);
        }
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

    /// 导入配置负载：逐表 upsert（INSERT OR REPLACE）。
    pub fn import_config(&self, payload: &ExportPayload) -> Result<usize, String> {
        let conn = self.lock();
        let mut count = 0;
        for row in &payload.providers {
            let cols = row
                .as_object()
                .ok_or_else(|| "providers 行不是对象".to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO providers
                 (id, plugin_id, name, category, icon, website, api_key, settings_config, meta, sort_order, live_config_managed, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,COALESCE(?12, datetime('now')),COALESCE(?13, datetime('now')))",
                rusqlite::params![
                    str_of(cols, "id"),
                    str_of(cols, "plugin_id"),
                    str_of(cols, "name"),
                    opt_str(cols, "category"),
                    opt_str(cols, "icon"),
                    opt_str(cols, "website"),
                    opt_str(cols, "api_key"),
                    opt_str(cols, "settings_config"),
                    opt_str(cols, "meta"),
                    int_of(cols, "sort_order"),
                    int_of(cols, "live_config_managed"),
                    opt_str(cols, "created_at"),
                    opt_str(cols, "updated_at"),
                ],
            )
            .map_err(|e| format!("导入 providers 失败: {e}"))?;
            count += 1;
        }
        for row in &payload.mcp_servers {
            let cols = row
                .as_object()
                .ok_or_else(|| "mcp_servers 行不是对象".to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO mcp_servers (id, name, server_config, description, homepage, docs, tags, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,COALESCE(?8, datetime('now')),COALESCE(?9, datetime('now')))",
                rusqlite::params![
                    str_of(cols, "id"),
                    str_of(cols, "name"),
                    str_of(cols, "server_config"),
                    opt_str(cols, "description"),
                    opt_str(cols, "homepage"),
                    opt_str(cols, "docs"),
                    str_of(cols, "tags"),
                    opt_str(cols, "created_at"),
                    opt_str(cols, "updated_at"),
                ],
            )
            .map_err(|e| format!("导入 mcp_servers 失败: {e}"))?;
            count += 1;
        }
        for row in &payload.skills {
            let cols = row
                .as_object()
                .ok_or_else(|| "skills 行不是对象".to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO skills (id, name, description, directory, source_path, repo_owner, repo_name, repo_branch, readme_url, installed_at, content_hash, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,COALESCE(?10,0),?11,COALESCE(?12,0))",
                rusqlite::params![
                    str_of(cols, "id"),
                    str_of(cols, "name"),
                    opt_str(cols, "description"),
                    str_of(cols, "directory"),
                    opt_str(cols, "source_path"),
                    opt_str(cols, "repo_owner"),
                    opt_str(cols, "repo_name"),
                    opt_str(cols, "repo_branch"),
                    opt_str(cols, "readme_url"),
                    int_of(cols, "installed_at"),
                    opt_str(cols, "content_hash"),
                    int_of(cols, "updated_at"),
                ],
            )
            .map_err(|e| format!("导入 skills 失败: {e}"))?;
            count += 1;
        }
        for row in &payload.prompts {
            let cols = row
                .as_object()
                .ok_or_else(|| "prompts 行不是对象".to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO prompts (id, plugin_id, name, content, description, enabled, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,COALESCE(?6,1),COALESCE(?7, datetime('now')),COALESCE(?8, datetime('now')))",
                rusqlite::params![
                    str_of(cols, "id"),
                    str_of(cols, "plugin_id"),
                    str_of(cols, "name"),
                    str_of(cols, "content"),
                    opt_str(cols, "description"),
                    int_of(cols, "enabled"),
                    opt_str(cols, "created_at"),
                    opt_str(cols, "updated_at"),
                ],
            )
            .map_err(|e| format!("导入 prompts 失败: {e}"))?;
            count += 1;
        }
        for row in &payload.profiles {
            let cols = row
                .as_object()
                .ok_or_else(|| "profiles 行不是对象".to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO profiles (id, name, payload, sort_order, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,COALESCE(?5,0),COALESCE(?6,0))",
                rusqlite::params![
                    str_of(cols, "id"),
                    str_of(cols, "name"),
                    str_of(cols, "payload"),
                    int_of(cols, "sort_order"),
                    int_of(cols, "created_at"),
                    int_of(cols, "updated_at"),
                ],
            )
            .map_err(|e| format!("导入 profiles 失败: {e}"))?;
            count += 1;
        }
        Ok(count)
    }
}

fn str_of(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn opt_str(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn int_of(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> i64 {
    map.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn query_all(conn: &rusqlite::Connection, sql: &str) -> Vec<serde_json::Value> {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let column_count = stmt.column_count();
    let mut column_names = Vec::with_capacity(column_count);
    for i in 0..column_count {
        column_names.push(
            stmt.column_name(i)
                .map(|n| n.to_string())
                .unwrap_or_default(),
        );
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

/// 备份 id 去重序号：同毫秒内连续创建备份时保证 id 唯一。
static BACKUP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    fn rename_db_backup_updates_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let backups_dir = dir.path().join("backups");

        let backup = db.create_db_backup(&backups_dir).unwrap();
        db.rename_db_backup(&backup.id, "我的快照").unwrap();
        let renamed = db.get_db_backup(&backup.id).unwrap().unwrap();
        assert_eq!(renamed.name, "我的快照");

        assert!(db.rename_db_backup(&backup.id, "  ").is_err());
        assert!(db.rename_db_backup("nope", "x").is_err());
    }

    #[test]
    fn restore_creates_safety_and_replaces_data() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cc.db");
        let db = Database::new(&db_path).unwrap();
        let backups_dir = dir.path().join("backups");

        {
            let conn = db.lock();
            conn.execute(
                "INSERT INTO providers (id, plugin_id, name) VALUES ('a', 'opencode', 'A')",
                [],
            )
            .unwrap();
        }
        let snapshot = db.create_db_backup(&backups_dir).unwrap();

        {
            let conn = db.lock();
            conn.execute("DELETE FROM providers WHERE id='a'", []).unwrap();
            conn.execute(
                "INSERT INTO providers (id, plugin_id, name) VALUES ('b', 'opencode', 'B')",
                [],
            )
            .unwrap();
        }

        let safety_id = db.restore_db_backup(&backups_dir, &snapshot.id).unwrap();
        assert_ne!(safety_id, snapshot.id);

        let names: Vec<String> = {
            let conn = db.lock();
            let mut stmt = conn
                .prepare("SELECT name FROM providers ORDER BY name")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(names, vec!["A".to_string()]);

        // 安全备份记录存在且文件在
        let safety = db.get_db_backup(&safety_id).unwrap().unwrap();
        assert!(PathBuf::from(&safety.file_path).exists());

        // 恢复不存在的备份应报错
        assert!(db.restore_db_backup(&backups_dir, "missing").is_err());
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

    #[test]
    fn import_config_roundtrips_tables() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let payload = ExportPayload {
            version: 1,
            providers: vec![serde_json::json!({
                "id": "deepseek", "plugin_id": "opencode", "name": "DeepSeek",
                "category": "custom", "settings_config": "{\"npm\":\"@ai-sdk/openai-compatible\"}",
                "live_config_managed": 1, "sort_order": 0
            })],
            mcp_servers: vec![serde_json::json!({
                "id": "filesystem", "name": "FS", "server_config": "{}", "tags": "[]"
            })],
            skills: vec![],
            prompts: vec![serde_json::json!({
                "id": "rules", "plugin_id": "opencode", "name": "Rules", "content": "be concise", "enabled": 1
            })],
            profiles: vec![],
        };

        let n = db.import_config(&payload).unwrap();
        assert_eq!(n, 3); // providers + mcp_servers + prompts

        let p: String = db
            .lock()
            .query_row("SELECT name FROM providers WHERE id='deepseek'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(p, "DeepSeek");
        let mcp: String = db
            .lock()
            .query_row(
                "SELECT name FROM mcp_servers WHERE id='filesystem'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mcp, "FS");
        let prompt: String = db
            .lock()
            .query_row("SELECT content FROM prompts WHERE id='rules'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(prompt, "be concise");
    }
}
