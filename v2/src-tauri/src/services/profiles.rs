//! Profiles（项目配置方案）服务。
//!
//! profile 是一份命名保存的配置快照（payload 为 JSON，含 provider/mcp 等），
//! 可应用到当前状态。`settings` 表记录当前激活的 profile。

use rusqlite::{params, OptionalExtension, Row};

use crate::db::Database;

/// 当前 profile 的 settings 键。
pub const CURRENT_PROFILE_KEY: &str = "current_profile_id";

/// Profile 记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub payload: serde_json::Value,
    pub sort_order: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

fn row_to_profile(row: &Row<'_>) -> rusqlite::Result<Profile> {
    let payload_raw: String = row.get("payload")?;
    Ok(Profile {
        id: row.get("id")?,
        name: row.get("name")?,
        payload: serde_json::from_str(&payload_raw).unwrap_or(serde_json::Value::Null),
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

impl Database {
    /// 列出全部 profiles。
    pub fn list_profiles(&self) -> rusqlite::Result<Vec<Profile>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, payload, sort_order, created_at, updated_at FROM profiles ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([], row_to_profile)?;
        rows.collect()
    }

    /// 读取单个 profile。
    pub fn get_profile(&self, id: &str) -> rusqlite::Result<Option<Profile>> {
        self.lock()
            .query_row(
                "SELECT id, name, payload, sort_order, created_at, updated_at FROM profiles WHERE id = ?1",
                params![id],
                row_to_profile,
            )
            .optional()
    }

    /// 新增/更新 profile。
    pub fn upsert_profile(&self, profile: &Profile) -> rusqlite::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let payload = serde_json::to_string(&profile.payload).unwrap_or_else(|_| "{}".into());
        self.lock().execute(
            "INSERT INTO profiles (id, name, payload, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               payload = excluded.payload,
               sort_order = excluded.sort_order,
               updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.name,
                payload,
                profile.sort_order.unwrap_or(0),
                now,
            ],
        )?;
        Ok(())
    }

    /// 删除 profile；若它是当前激活的，则同时清除 current。
    pub fn delete_profile(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let changed = conn.execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        if changed > 0 {
            let current: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![CURRENT_PROFILE_KEY],
                    |r| r.get(0),
                )
                .optional()?;
            if current.as_deref() == Some(id) {
                conn.execute("DELETE FROM settings WHERE key = ?1", params![CURRENT_PROFILE_KEY])?;
            }
        }
        Ok(changed > 0)
    }

    /// 读取当前激活的 profile id。
    pub fn current_profile_id(&self) -> rusqlite::Result<Option<String>> {
        self.get_setting(CURRENT_PROFILE_KEY)
    }

    /// 设置当前激活的 profile id。
    pub fn set_current_profile(&self, id: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.lock();
        match id {
            Some(id) => conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![CURRENT_PROFILE_KEY, id],
            )?,
            None => conn.execute("DELETE FROM settings WHERE key = ?1", params![CURRENT_PROFILE_KEY])?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let profile = Profile {
            id: "proj-a".into(),
            name: "Project A".into(),
            payload: serde_json::json!({ "providers": [{ "id": "x" }] }),
            sort_order: Some(1),
            created_at: None,
            updated_at: None,
        };
        db.upsert_profile(&profile).unwrap();

        let stored = db.get_profile("proj-a").unwrap().unwrap();
        assert_eq!(stored.name, "Project A");
        assert_eq!(stored.payload["providers"][0]["id"], "x");

        db.set_current_profile(Some("proj-a")).unwrap();
        assert_eq!(db.current_profile_id().unwrap().as_deref(), Some("proj-a"));

        db.delete_profile("proj-a").unwrap();
        assert!(db.get_profile("proj-a").unwrap().is_none());
        assert_eq!(db.current_profile_id().unwrap(), None);
    }

    #[test]
    fn profile_list_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.upsert_profile(&Profile {
            id: "b".into(),
            name: "B".into(),
            payload: serde_json::json!({}),
            sort_order: Some(2),
            created_at: None,
            updated_at: None,
        })
        .unwrap();
        db.upsert_profile(&Profile {
            id: "a".into(),
            name: "A".into(),
            payload: serde_json::json!({}),
            sort_order: Some(1),
            created_at: None,
            updated_at: None,
        })
        .unwrap();
        let list = db.list_profiles().unwrap();
        assert_eq!(list[0].id, "a");
        assert_eq!(list[1].id, "b");
    }
}
