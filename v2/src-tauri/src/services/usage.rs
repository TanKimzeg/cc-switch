//! 用量统计服务。
//!
//! 从 OpenCode 会话数据库（`opencode.db`）提取 assistant 消息的
//! token/cost 数据写入 `request_logs` 表，并提供查询/汇总命令。
//!
//! ## 数据流
//! ```text
//! opencode.db (SQLite)
//!   → session 表
//!   → message 表（assistant 消息，解析 data JSON 的 tokens/cost/modelID）
//!   → request_logs 表
//!   → usage_daily_rollups（查询时按日聚合）
//! ```

use std::path::PathBuf;
use std::time::SystemTime;

use rusqlite::{params, Connection};

use crate::db::Database;

/// 从 opencode 消息中提取的用量数据。
#[derive(Debug, Clone)]
pub struct OpenCodeUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost: f64,
    pub model_id: String,
    pub timestamp_ms: i64,
}

/// 从 assistant 消息 JSON 解析用量（与 v1 语义一致）。
pub fn parse_opencode_message(value: &serde_json::Value) -> Option<OpenCodeUsage> {
    let tokens = value.get("tokens")?;
    let input_tokens = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
    let output_tokens = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
    let reasoning_tokens = tokens
        .get("reasoning")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_obj = tokens.get("cache");
    let cache_read_tokens = cache_obj
        .and_then(|c| c.get("read"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write_tokens = cache_obj
        .and_then(|c| c.get("write"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input_tokens == 0
        && output_tokens == 0
        && reasoning_tokens == 0
        && cache_read_tokens == 0
        && cache_write_tokens == 0
    {
        return None;
    }

    let cost = value.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let model_id = value
        .get("modelID")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let timestamp_ms = value
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Some(OpenCodeUsage {
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_read_tokens,
        cache_write_tokens,
        cost,
        model_id,
        timestamp_ms,
    })
}

/// OpenCode 数据目录（`$XDG_DATA_HOME/opencode`，兜底 `~/.local/share/opencode`）。
fn opencode_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CC_SWITCH_OPENCODE_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("opencode");
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("opencode")
}

fn opencode_db_path() -> PathBuf {
    opencode_data_dir().join("opencode.db")
}

/// 单次会话用量记录（用于同步状态）。
#[derive(Debug, Default)]
pub struct SyncResult {
    pub imported: usize,
    pub skipped: usize,
    pub sessions_scanned: usize,
    pub errors: Vec<String>,
}

impl Database {
    /// 从 OpenCode 数据库同步用量到 `request_logs`。
    pub fn sync_opencode_usage(&self) -> SyncResult {
        let db_path = opencode_db_path();
        if !db_path.exists() {
            return SyncResult::default();
        }

        let conn = match Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                let mut r = SyncResult::default();
                r.errors.push(format!("打开 OpenCode 数据库失败: {e}"));
                return r;
            }
        };

        let mut result = SyncResult::default();

        let sessions: Vec<(String, i64, i64)> = match query_sessions(&conn) {
            Ok(v) => v,
            Err(e) => {
                result.errors.push(e);
                return result;
            }
        };
        result.sessions_scanned = sessions.len();

        for (session_id, _, session_created) in &sessions {
            let messages = match query_assistant_messages(&conn, session_id, *session_created) {
                Ok(v) => v,
                Err(e) => {
                    result.errors.push(e);
                    continue;
                }
            };
            for (message_id, usage) in messages {
                let request_id = format!("opencode_session:{session_id}:{message_id}");
                match self.insert_request_log(&request_id, session_id, &usage) {
                    Ok(true) => result.imported += 1,
                    _ => result.skipped += 1,
                }
            }
        }
        result
    }

    /// 插入一条请求日志（`INSERT OR IGNORE` 去重）。
    fn insert_request_log(
        &self,
        request_id: &str,
        session_id: &str,
        usage: &OpenCodeUsage,
    ) -> rusqlite::Result<bool> {
        let created_at = if usage.timestamp_ms > 0 {
            usage.timestamp_ms / 1000
        } else {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        };
        let output_with_reasoning = usage.output_tokens + usage.reasoning_tokens;

        let conn = self.lock();
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO request_logs (
                request_id, provider_id, plugin_id, model, request_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                total_cost_usd, latency_ms, status_code, session_id, is_streaming,
                created_at, data_source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 200, ?11, 1, ?12, 'opencode_session')",
            params![
                request_id,
                "_opencode_session",
                "opencode",
                usage.model_id,
                usage.model_id,
                usage.input_tokens,
                output_with_reasoning as i64,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.cost.to_string(),
                session_id,
                created_at,
            ],
        )?;
        Ok(inserted > 0)
    }

    /// 写入插件解析出的用量记录（`INSERT OR IGNORE` 去重）。返回导入条数。
    pub fn insert_usage_records(
        &self,
        plugin_id: &str,
        records: &[crate::plugin::UsageRecord],
    ) -> usize {
        let mut imported = 0;
        for r in records {
            let created_at = if r.timestamp_ms > 0 {
                r.timestamp_ms / 1000
            } else {
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0)
            };
            let output_with_reasoning = r.output_tokens + r.reasoning_tokens;
            let conn = self.lock();
            let inserted = conn
                .execute(
                    "INSERT OR IGNORE INTO request_logs (
                        request_id, provider_id, plugin_id, model, request_model,
                        input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                        total_cost_usd, latency_ms, status_code, session_id, is_streaming,
                        created_at, data_source
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 200, ?11, 1, ?12, 'plugin')",
                    params![
                        r.source_id,
                        format!("_{plugin_id}_session"),
                        plugin_id,
                        r.model,
                        r.model,
                        r.input_tokens,
                        output_with_reasoning,
                        r.cache_read_tokens,
                        r.cache_write_tokens,
                        r.cost.to_string(),
                        r.session_id,
                        created_at,
                    ],
                )
                .unwrap_or(0);
            if inserted > 0 {
                imported += 1;
            }
        }
        imported
    }
}

fn query_sessions(conn: &Connection) -> Result<Vec<(String, i64, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id,
                    MAX(s.time_updated, COALESCE(MAX(m.time_updated), s.time_updated)) AS sync_watermark,
                    s.time_created
             FROM session s
             LEFT JOIN message m ON m.session_id = s.id
             GROUP BY s.id
             ORDER BY sync_watermark",
        )
        .map_err(|e| format!("准备会话查询失败: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("查询会话失败: {e}"))?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row.map_err(|e| format!("读取会话行失败: {e}"))?);
    }
    Ok(sessions)
}

fn query_assistant_messages(
    conn: &Connection,
    session_id: &str,
    session_created: i64,
) -> Result<Vec<(String, OpenCodeUsage)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, data FROM message WHERE session_id = ?1 AND time_created > ?2 ORDER BY time_created",
        )
        .map_err(|e| format!("准备消息查询失败: {e}"))?;
    let rows = stmt
        .query_map(params![session_id, session_created], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("查询消息失败: {e}"))?;

    let mut messages = Vec::new();
    for row in rows {
        let (message_id, data_json) = row.map_err(|e| format!("读取消息行失败: {e}"))?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&data_json) else {
            continue;
        };
        if value.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        if value.get("tokens").is_none() {
            continue;
        }
        if value.get("time").and_then(|t| t.get("completed")).is_none() {
            continue;
        }
        if let Some(usage) = parse_opencode_message(&value) {
            messages.push((message_id, usage));
        }
    }
    Ok(messages)
}

/// 请求日志查询结果（用于命令返回）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogRow {
    pub request_id: String,
    pub plugin_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_cost_usd: String,
    pub session_id: Option<String>,
    pub created_at: i64,
}

impl Database {
    /// 查询请求日志（按时间倒序，可限制条数）。
    pub fn list_request_logs(
        &self,
        plugin_id: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<RequestLogRow>> {
        let conn = self.lock();
        let mut stmt = match plugin_id {
            Some(_) => conn.prepare(
                "SELECT request_id, plugin_id, model, input_tokens, output_tokens, cache_read_tokens,
                        cache_creation_tokens, total_cost_usd, session_id, created_at
                 FROM request_logs WHERE plugin_id = ?1 ORDER BY created_at DESC LIMIT ?2",
            )?,
            None => conn.prepare(
                "SELECT request_id, plugin_id, model, input_tokens, output_tokens, cache_read_tokens,
                        cache_creation_tokens, total_cost_usd, session_id, created_at
                 FROM request_logs ORDER BY created_at DESC LIMIT ?1",
            )?,
        };
        let rows = match plugin_id {
            Some(pid) => stmt.query_map(params![pid, limit as i64], row_to_log)?,
            None => stmt.query_map(params![limit as i64], row_to_log)?,
        };
        rows.collect()
    }

    /// 按日汇总用量。
    pub fn usage_daily_summary(
        &self,
        plugin_id: Option<&str>,
    ) -> rusqlite::Result<Vec<DailyUsageRow>> {
        let conn = self.lock();
        let mut stmt = match plugin_id {
            Some(_) => conn.prepare(
                "SELECT date(created_at, 'unixepoch') AS day, plugin_id, model,
                        COUNT(*) AS requests, SUM(input_tokens) AS input_tokens,
                        SUM(output_tokens) AS output_tokens,
                        SUM(cache_read_tokens) AS cache_read,
                        SUM(cache_creation_tokens) AS cache_creation,
                        SUM(CAST(total_cost_usd AS REAL)) AS cost
                 FROM request_logs WHERE plugin_id = ?1
                 GROUP BY day, plugin_id, model ORDER BY day DESC",
            )?,
            None => conn.prepare(
                "SELECT date(created_at, 'unixepoch') AS day, plugin_id, model,
                        COUNT(*) AS requests, SUM(input_tokens) AS input_tokens,
                        SUM(output_tokens) AS output_tokens,
                        SUM(cache_read_tokens) AS cache_read,
                        SUM(cache_creation_tokens) AS cache_creation,
                        SUM(CAST(total_cost_usd AS REAL)) AS cost
                 FROM request_logs
                 GROUP BY day, plugin_id, model ORDER BY day DESC",
            )?,
        };
        let rows = match plugin_id {
            Some(pid) => stmt.query_map(params![pid], row_to_daily)?,
            None => stmt.query_map([], row_to_daily)?,
        };
        rows.collect()
    }
}

fn row_to_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLogRow> {
    Ok(RequestLogRow {
        request_id: row.get(0)?,
        plugin_id: row.get(1)?,
        model: row.get(2)?,
        input_tokens: row.get(3)?,
        output_tokens: row.get(4)?,
        cache_read_tokens: row.get(5)?,
        cache_creation_tokens: row.get(6)?,
        total_cost_usd: row.get(7)?,
        session_id: row.get(8)?,
        created_at: row.get(9)?,
    })
}

/// 日汇总行。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageRow {
    pub day: String,
    pub plugin_id: String,
    pub model: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost_usd: f64,
}

fn row_to_daily(row: &rusqlite::Row<'_>) -> rusqlite::Result<DailyUsageRow> {
    Ok(DailyUsageRow {
        day: row.get(0)?,
        plugin_id: row.get(1)?,
        model: row.get(2)?,
        requests: row.get(3)?,
        input_tokens: row.get(4)?,
        output_tokens: row.get(5)?,
        cache_read_tokens: row.get(6)?,
        cache_creation_tokens: row.get(7)?,
        cost_usd: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message_full() {
        let value = serde_json::json!({
            "role": "assistant",
            "cost": 0.0023113,
            "tokens": {
                "total": 56554,
                "input": 3272,
                "output": 383,
                "reasoning": 419,
                "cache": { "write": 0, "read": 52480 }
            },
            "modelID": "deepseek-v4-pro",
            "time": { "created": 1779755333700i64, "completed": 1779755350639i64 }
        });
        let usage = parse_opencode_message(&value).unwrap();
        assert_eq!(usage.input_tokens, 3272);
        assert_eq!(usage.output_tokens, 383);
        assert_eq!(usage.reasoning_tokens, 419);
        assert_eq!(usage.cache_read_tokens, 52480);
        assert_eq!(usage.model_id, "deepseek-v4-pro");
    }

    #[test]
    fn parse_message_skips_zero_tokens() {
        let value = serde_json::json!({
            "role": "assistant",
            "tokens": { "input": 0, "output": 0 }
        });
        assert!(parse_opencode_message(&value).is_none());
    }

    fn create_opencode_db(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        let msg = serde_json::json!({
            "role": "assistant",
            "tokens": { "input": 100, "output": 50, "cache": { "read": 10, "write": 0 } },
            "cost": 0.001,
            "modelID": "m1",
            "time": { "created": 1000, "completed": 2000 }
        })
        .to_string();
        conn.execute(
            "INSERT INTO session VALUES ('ses_1', 'T', '/p', 100, 500)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message VALUES ('msg_1', 'ses_1', 200, 500, ?1)",
            params![msg],
        )
        .unwrap();
    }

    #[test]
    fn sync_opencode_usage_imports_messages() {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("CC_SWITCH_OPENCODE_DATA_DIR");
        std::env::set_var("CC_SWITCH_OPENCODE_DATA_DIR", dir.path());
        create_opencode_db(&dir.path().join("opencode.db"));

        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let result = db.sync_opencode_usage();
        assert_eq!(result.imported, 1);

        let logs = db.list_request_logs(Some("opencode"), 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].model, "m1");
        assert_eq!(logs[0].input_tokens, 100);
        assert_eq!(logs[0].output_tokens, 50);

        if let Some(v) = original {
            std::env::set_var("CC_SWITCH_OPENCODE_DATA_DIR", v);
        } else {
            std::env::remove_var("CC_SWITCH_OPENCODE_DATA_DIR");
        }
    }

    #[test]
    fn sync_opencode_usage_dedups() {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("CC_SWITCH_OPENCODE_DATA_DIR");
        std::env::set_var("CC_SWITCH_OPENCODE_DATA_DIR", dir.path());
        create_opencode_db(&dir.path().join("opencode.db"));

        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.sync_opencode_usage();
        let second = db.sync_opencode_usage();
        assert_eq!(second.imported, 0);

        if let Some(v) = original {
            std::env::set_var("CC_SWITCH_OPENCODE_DATA_DIR", v);
        } else {
            std::env::remove_var("CC_SWITCH_OPENCODE_DATA_DIR");
        }
    }

    #[test]
    fn usage_daily_summary_groups_by_day() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let conn = db.lock();
        conn.execute(
            "INSERT INTO request_logs (request_id, provider_id, plugin_id, model, input_tokens, output_tokens, total_cost_usd, created_at)
             VALUES ('r1', 'p', 'opencode', 'm1', 100, 50, '0.01', 1700000000),
                    ('r2', 'p', 'opencode', 'm1', 200, 60, '0.02', 1700003600)",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = db.usage_daily_summary(Some("opencode")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requests, 2);
        assert_eq!(rows[0].input_tokens, 300);
        assert_eq!(rows[0].output_tokens, 110);
        assert!((rows[0].cost_usd - 0.03).abs() < 1e-9);
    }
}
