use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::types::PluginInstall;

/// v2 数据模型（无历史包袱、无 SCHEMA_VERSION 迁移史）。
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS providers (
  id             TEXT PRIMARY KEY,
  plugin_id      TEXT NOT NULL,
  name           TEXT NOT NULL,
  category       TEXT NOT NULL DEFAULT 'custom',
  icon           TEXT,
  website        TEXT,
  api_key        TEXT,
  settings_config TEXT,
  meta           TEXT,
  sort_order     INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_providers_plugin ON providers(plugin_id);

CREATE TABLE IF NOT EXISTS app_state (
  plugin_id             TEXT PRIMARY KEY,
  current_provider_id   TEXT,
  live_config_snapshot  TEXT,
  flags                 TEXT
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT
);

CREATE TABLE IF NOT EXISTS plugin_installs (
  plugin_id    TEXT PRIMARY KEY,
  version      TEXT NOT NULL,
  source       TEXT NOT NULL DEFAULT 'local',
  sha256       TEXT,
  installed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.conn
            .lock()
            .unwrap()
            .execute_batch(SCHEMA)?;
        Ok(db)
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.lock()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// 记录插件安装信息（已存在则更新版本/来源）。
    pub fn upsert_plugin_install(
        &self,
        plugin_id: &str,
        version: &str,
        source: &str,
        sha256: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO plugin_installs (plugin_id, version, source, sha256, installed_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(plugin_id) DO UPDATE SET
               version = excluded.version,
               source = excluded.source,
               sha256 = excluded.sha256,
               installed_at = excluded.installed_at",
            params![plugin_id, version, source, sha256],
        )?;
        Ok(())
    }

    /// 读取单个插件安装记录。
    pub fn get_plugin_install(&self, plugin_id: &str) -> rusqlite::Result<Option<PluginInstall>> {
        self.lock()
            .query_row(
                "SELECT plugin_id, version, source, sha256, installed_at
                 FROM plugin_installs WHERE plugin_id = ?1",
                params![plugin_id],
                row_to_plugin_install,
            )
            .optional()
    }

    /// 读取全部插件安装记录。
    pub fn list_plugin_installs(&self) -> rusqlite::Result<Vec<PluginInstall>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT plugin_id, version, source, sha256, installed_at FROM plugin_installs",
        )?;
        let rows = stmt.query_map([], row_to_plugin_install)?;
        rows.collect()
    }

    /// 删除插件安装记录。
    pub fn delete_plugin_install(&self, plugin_id: &str) -> rusqlite::Result<()> {
        self.lock()
            .execute("DELETE FROM plugin_installs WHERE plugin_id = ?1", params![plugin_id])?;
        Ok(())
    }

    /// 删除某插件名下的全部供应商（卸载插件时清理数据）。
    pub fn delete_providers_by_plugin(&self, plugin_id: &str) -> rusqlite::Result<()> {
        self.lock()
            .execute("DELETE FROM providers WHERE plugin_id = ?1", params![plugin_id])?;
        Ok(())
    }
}

fn row_to_plugin_install(row: &rusqlite::Row<'_>) -> rusqlite::Result<PluginInstall> {
    Ok(PluginInstall {
        plugin_id: row.get(0)?,
        version: row.get(1)?,
        source: row.get(2)?,
        sha256: row.get(3)?,
        installed_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_all_tables_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        let tables: Vec<String> = db
            .lock()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for expected in [
            "app_state",
            "plugin_installs",
            "providers",
            "settings",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        assert_eq!(db.get_setting("lang").unwrap(), None);
        db.set_setting("lang", "zh").unwrap();
        assert_eq!(db.get_setting("lang").unwrap().as_deref(), Some("zh"));
        db.set_setting("lang", "en").unwrap();
        assert_eq!(db.get_setting("lang").unwrap().as_deref(), Some("en"));
    }

    #[test]
    fn plugin_install_upsert() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();

        db.upsert_plugin_install("openclaw", "0.1.0", "local", None)
            .unwrap();
        let (version, source, sha): (String, String, Option<String>) = db
            .lock()
            .query_row(
                "SELECT version, source, sha256 FROM plugin_installs WHERE plugin_id = 'openclaw'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(version, "0.1.0");
        assert_eq!(source, "local");
        assert!(sha.is_none());

        db.upsert_plugin_install("openclaw", "0.2.0", "local", Some("abc"))
            .unwrap();
        let (version, sha): (String, Option<String>) = db
            .lock()
            .query_row(
                "SELECT version, sha256 FROM plugin_installs WHERE plugin_id = 'openclaw'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(version, "0.2.0");
        assert_eq!(sha.as_deref(), Some("abc"));
    }

    #[test]
    fn plugin_install_list_get_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        db.upsert_plugin_install("openclaw", "0.1.0", "builtin", None)
            .unwrap();
        db.upsert_plugin_install("opencode", "0.1.0", "local", Some("abc"))
            .unwrap();

        let list = db.list_plugin_installs().unwrap();
        assert_eq!(list.len(), 2);

        let inst = db.get_plugin_install("opencode").unwrap().unwrap();
        assert_eq!(inst.source, "local");
        assert_eq!(inst.sha256.as_deref(), Some("abc"));
        assert!(!inst.installed_at.is_empty());

        assert!(db.get_plugin_install("nope").unwrap().is_none());

        db.delete_plugin_install("opencode").unwrap();
        assert!(db.get_plugin_install("opencode").unwrap().is_none());
        assert!(db.get_plugin_install("openclaw").unwrap().is_some());
    }

    #[test]
    fn delete_providers_by_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        let conn = db.lock();
        conn.execute(
            "INSERT INTO providers (id, plugin_id, name) VALUES ('a', 'opencode', 'A'), ('b', 'openclaw', 'B')",
            [],
        )
        .unwrap();
        drop(conn);
        db.delete_providers_by_plugin("opencode").unwrap();
        let remaining: Vec<String> = db
            .lock()
            .prepare("SELECT id FROM providers")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(remaining, vec!["b".to_string()]);
    }
}
