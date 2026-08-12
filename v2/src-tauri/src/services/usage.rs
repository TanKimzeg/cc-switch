//! 用量统计服务。
//!
//! 由插件实现 `AgentPlugin::sync_usage` 解析各自会话存储中的
//! token/cost 数据，软件层通过 [`Database::insert_usage_records`]
//! 写入 `request_logs` 表，并提供查询/汇总命令。

use std::time::SystemTime;

use rusqlite::params;

use crate::db::Database;

impl Database {
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
    fn insert_usage_records_imports_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let records = vec![crate::plugin::UsageRecord {
            source_id: "opencode_session:ses_1:msg_1".into(),
            session_id: "ses_1".into(),
            model: "m1".into(),
            input_tokens: 100,
            output_tokens: 50,
            reasoning_tokens: 5,
            cache_read_tokens: 10,
            cache_write_tokens: 0,
            cost: 0.001,
            timestamp_ms: 1_000_000,
        }];

        assert_eq!(db.insert_usage_records("opencode", &records), 1);
        // 重复导入被去重。
        assert_eq!(db.insert_usage_records("opencode", &records), 0);

        let logs = db.list_request_logs(Some("opencode"), 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].model, "m1");
        assert_eq!(logs[0].input_tokens, 100);
        assert_eq!(logs[0].output_tokens, 55); // output + reasoning
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
