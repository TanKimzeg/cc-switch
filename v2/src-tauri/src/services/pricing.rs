//! 模型定价服务：全项目唯一的成本计算逻辑。
//!
//! 设计要点（对齐 gap-analysis §3.3，修复 v1 定价逻辑四处重复的问题）：
//! - 插件只上报 tokens，成本一律由本服务按 `model_pricing` 表计算；
//! - 匹配链：供应商限定精确 > 供应商限定前缀 > 通用精确 > 通用前缀（最长前缀优先）；
//! - 峰谷：`off_peak_discount_percent` + UTC `HH:MM` 窗口（可跨午夜），窗口内整档折扣
//!   （DeepSeek 错峰场景）；中转站差价 = 为该 provider 建限定行，直接填实际单价；
//! - 精度：价格以十进制字符串存储，计算走整数微美元（1e-6 USD），无浮点误差。

use chrono::{Timelike, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Database;

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
pub const KEY_MODELS_DEV_LAST_SYNC: &str = "pricing.modelsDevLastSyncAt";

/// 单条模型定价。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricing {
    pub id: String,
    /// 模型匹配键：精确模型名，或作为前缀匹配（如 `deepseek-chat` 匹配带日期后缀的快照）。
    pub model_match: String,
    /// 供应商限定（匹配 request_logs.active_provider_id）；NULL = 通用默认。
    pub provider_scope: Option<String>,
    pub display_name: String,
    pub input_cost_per_million: String,
    pub output_cost_per_million: String,
    pub cache_read_cost_per_million: String,
    pub cache_creation_cost_per_million: String,
    /// 错峰折扣百分比（如 50 = 半价）；NULL = 无峰谷。
    pub off_peak_discount_percent: Option<i64>,
    /// UTC "HH:MM" 窗口起（可跨午夜）。
    pub off_peak_start: Option<String>,
    /// UTC "HH:MM" 窗口止。
    pub off_peak_end: Option<String>,
    /// `user`（手填，同步永不覆盖）| `models_dev`（目录同步）。
    pub source: String,
    pub updated_at: i64,
}

/// 成本明细（微美元整数，避免浮点误差）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CostMicro {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_creation: i64,
}

impl CostMicro {
    pub fn total(&self) -> i64 {
        self.input + self.output + self.cache_read + self.cache_creation
    }
}

/// 十进制价格字符串 → 微美元/百万 token（i64）。最多 6 位小数，超出截断；负数非法。
pub fn parse_price_micro(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (int_part, frac_part) = match t.split_once('.') {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if int_part.starts_with('-') || int_part.starts_with('+') {
        return None;
    }
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let frac_digits: String = frac_part.chars().take(6).collect();
    let mut padded = frac_digits.clone();
    while padded.len() < 6 {
        padded.push('0');
    }
    if !padded.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let int_val: i64 = if int_part.is_empty() {
        0
    } else {
        int_part.parse().ok()?
    };
    let frac_val: i64 = if padded.is_empty() {
        0
    } else {
        padded.parse().ok()?
    };
    int_val.checked_mul(1_000_000)?.checked_add(frac_val)
}

/// 微美元 → 十进制字符串（去尾零）。
pub fn format_micro(micro: i64) -> String {
    let negative = micro < 0;
    let abs = micro.unsigned_abs();
    let int_part = abs / 1_000_000;
    let frac = abs % 1_000_000;
    let mut out = format!("{int_part}.{frac:06}");
    while out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

/// UTC 时刻（秒）是否落在峰谷窗口内。窗口可跨午夜；start==end 视为全时段。
fn is_off_peak(created_at_secs: i64, start: &str, end: &str) -> bool {
    let (Some(s), Some(e)) = (parse_hhmm(start), parse_hhmm(end)) else {
        return false;
    };
    let minute_of_day = match chrono::DateTime::from_timestamp(created_at_secs, 0) {
        Some(dt) => {
            let t = dt.with_timezone(&Utc);
            t.hour() as i64 * 60 + t.minute() as i64
        }
        None => return false,
    };
    if s == e {
        return true;
    }
    if s < e {
        minute_of_day >= s && minute_of_day < e
    } else {
        // 跨午夜（如 16:30 → 00:30）
        minute_of_day >= s || minute_of_day < e
    }
}

fn parse_hhmm(s: &str) -> Option<i64> {
    let (h, m) = s.trim().split_once(':')?;
    let h: i64 = h.trim().parse().ok()?;
    let m: i64 = m.trim().parse().ok()?;
    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    Some(h * 60 + m)
}

/// 匹配链：供应商限定精确 > 供应商限定最长前缀 > 通用精确 > 通用最长前缀。
pub fn resolve_pricing<'a>(
    rows: &'a [ModelPricing],
    model: &str,
    active_provider: Option<&str>,
) -> Option<&'a ModelPricing> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    let tier = |scoped: bool| -> Option<&ModelPricing> {
        rows.iter()
            .filter(|r| {
                if scoped {
                    r.provider_scope.is_some()
                        && r.provider_scope.as_deref() == active_provider
                } else {
                    r.provider_scope.is_none()
                }
            })
            .filter(|r| model == r.model_match || model.starts_with(&r.model_match))
            .min_by_key(|r| {
                (
                    std::cmp::Reverse(r.model_match.len()),
                    r.provider_scope.is_none(),
                )
            })
    };
    tier(true).or_else(|| tier(false))
}

/// 按定价与请求时间计算成本（微美元）。无峰谷配置时折扣不生效。
pub fn compute_cost(
    pricing: &ModelPricing,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    created_at_secs: i64,
) -> CostMicro {
    let discount: i64 = match pricing.off_peak_discount_percent {
        Some(d) if d > 0 && d <= 100 => {
            let in_window = match (&pricing.off_peak_start, &pricing.off_peak_end) {
                (Some(s), Some(e)) => is_off_peak(created_at_secs, s, e),
                _ => false,
            };
            if in_window {
                100 - d
            } else {
                100
            }
        }
        _ => 100,
    };
    let calc = |tokens: i64, price: &str| -> i64 {
        let Some(price_micro) = parse_price_micro(price) else {
            return 0;
        };
        if tokens <= 0 {
            return 0;
        }
        // cost_micro = tokens * price_micro * discount / (100 * 1_000_000)，四舍五入。
        let num = tokens as i128 * price_micro as i128 * discount as i128;
        let den = 100i128 * 1_000_000i128;
        ((num + den / 2) / den) as i64
    };
    CostMicro {
        input: calc(input_tokens, &pricing.input_cost_per_million),
        output: calc(output_tokens, &pricing.output_cost_per_million),
        cache_read: calc(cache_read_tokens, &pricing.cache_read_cost_per_million),
        cache_creation: calc(
            cache_creation_tokens,
            &pricing.cache_creation_cost_per_million,
        ),
    }
}

fn row_to_pricing(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelPricing> {
    Ok(ModelPricing {
        id: row.get("id")?,
        model_match: row.get("model_match")?,
        provider_scope: row.get("provider_scope")?,
        display_name: row.get("display_name")?,
        input_cost_per_million: row.get("input_cost_per_million")?,
        output_cost_per_million: row.get("output_cost_per_million")?,
        cache_read_cost_per_million: row.get("cache_read_cost_per_million")?,
        cache_creation_cost_per_million: row.get("cache_creation_cost_per_million")?,
        off_peak_discount_percent: row.get("off_peak_discount_percent")?,
        off_peak_start: row.get("off_peak_start")?,
        off_peak_end: row.get("off_peak_end")?,
        source: row.get("source")?,
        updated_at: row.get("updated_at")?,
    })
}

/// 待回填行：(request_id, model, active_provider, input, output, cache_read, cache_creation, created_at)。
type PendingCostRow = (String, String, Option<String>, i64, i64, i64, i64, i64);

impl Database {
    pub fn list_model_pricing(&self) -> rusqlite::Result<Vec<ModelPricing>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, model_match, provider_scope, display_name,
                    input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million,
                    off_peak_discount_percent, off_peak_start, off_peak_end, source, updated_at
             FROM model_pricing ORDER BY model_match, provider_scope",
        )?;
        let rows = stmt.query_map([], row_to_pricing)?;
        rows.collect()
    }

    /// 新增/更新一条定价（按 id upsert）。校验价格可解析与窗口合法。
    pub fn upsert_model_pricing(&self, pricing: &ModelPricing) -> Result<(), String> {
        if pricing.model_match.trim().is_empty() {
            return Err("模型匹配键不能为空".into());
        }
        for price in [
            &pricing.input_cost_per_million,
            &pricing.output_cost_per_million,
            &pricing.cache_read_cost_per_million,
            &pricing.cache_creation_cost_per_million,
        ] {
            if parse_price_micro(price).is_none() {
                return Err(format!("价格 '{price}' 不是合法的非负十进制数"));
            }
        }
        if let (Some(s), Some(e)) = (&pricing.off_peak_start, &pricing.off_peak_end) {
            if parse_hhmm(s).is_none() || parse_hhmm(e).is_none() {
                return Err(format!("峰谷窗口 '{s}-{e}' 不是合法的 HH:MM"));
            }
        }
        if let Some(d) = pricing.off_peak_discount_percent {
            if !(1..=100).contains(&d) {
                return Err("峰谷折扣必须是 1-100 之间的整数".into());
            }
        }
        let now = Utc::now().timestamp();
        self.lock()
            .execute(
                "INSERT INTO model_pricing (
                    id, model_match, provider_scope, display_name,
                    input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million,
                    off_peak_discount_percent, off_peak_start, off_peak_end, source,
                    created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                    model_match = excluded.model_match,
                    provider_scope = excluded.provider_scope,
                    display_name = excluded.display_name,
                    input_cost_per_million = excluded.input_cost_per_million,
                    output_cost_per_million = excluded.output_cost_per_million,
                    cache_read_cost_per_million = excluded.cache_read_cost_per_million,
                    cache_creation_cost_per_million = excluded.cache_creation_cost_per_million,
                    off_peak_discount_percent = excluded.off_peak_discount_percent,
                    off_peak_start = excluded.off_peak_start,
                    off_peak_end = excluded.off_peak_end,
                    source = excluded.source,
                    updated_at = excluded.updated_at",
                params![
                    pricing.id,
                    pricing.model_match.trim(),
                    pricing.provider_scope,
                    pricing.display_name,
                    pricing.input_cost_per_million,
                    pricing.output_cost_per_million,
                    pricing.cache_read_cost_per_million,
                    pricing.cache_creation_cost_per_million,
                    pricing.off_peak_discount_percent,
                    pricing.off_peak_start,
                    pricing.off_peak_end,
                    pricing.source,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_model_pricing(&self, id: &str) -> Result<bool, String> {
        let n = self
            .lock()
            .execute("DELETE FROM model_pricing WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// 回填历史零成本行：仅重算 `total_cost_usd = '0'` 且能匹配到定价的行
    /// （插件自带成本的行如 opencode 不覆盖）。返回更新条数。
    pub fn backfill_zero_costs(&self) -> Result<usize, String> {
        let rows = self.list_model_pricing().map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(0);
        }
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT request_id, model, active_provider_id,
                        input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                        created_at
                 FROM request_logs
                 WHERE total_cost_usd = '0' OR total_cost_usd = ''",
            )
            .map_err(|e| e.to_string())?;
        let pending: Vec<PendingCostRow> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut updated = 0;
        for (request_id, model, active_provider, input, output, cache_read, cache_creation, created_at) in
            pending
        {
            let Some(pricing) = resolve_pricing(&rows, &model, active_provider.as_deref()) else {
                continue;
            };
            let cost = compute_cost(
                pricing,
                input,
                output,
                cache_read,
                cache_creation,
                created_at,
            );
            let n = conn
                .execute(
                    "UPDATE request_logs SET
                        input_cost_usd = ?2, output_cost_usd = ?3,
                        total_cost_usd = ?4, pricing_model = ?5
                     WHERE request_id = ?1",
                    params![
                        request_id,
                        format_micro(cost.input),
                        format_micro(cost.output),
                        format_micro(cost.total()),
                        pricing.model_match,
                    ],
                )
                .map_err(|e| e.to_string())?;
            updated += n;
        }
        Ok(updated)
    }
}

/// models.dev 目录条目（解析容错：缺字段跳过）。
#[derive(Debug, Deserialize)]
struct ModelsDevCatalog {
    #[serde(flatten)]
    providers: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevProvider {
    #[serde(default)]
    models: std::collections::HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cost: Option<ModelsDevCost>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevCost {
    #[serde(default)]
    input: Option<f64>,
    #[serde(default)]
    output: Option<f64>,
    #[serde(default)]
    cache_read: Option<f64>,
    #[serde(default)]
    cache_write: Option<f64>,
}

fn price_to_string(v: f64) -> String {
    // 目录价格单位是 USD/百万 token；转微美元整数字符串，规避浮点尾差。
    format_micro((v * 1_000_000.0).round() as i64)
}

/// 从 models.dev 拉取公共模型价格并 upsert（仅 `models_dev` 来源行；
/// 用户手填行同 key 时跳过不覆盖）。返回 (同步数, 跳过的用户行数)。
pub async fn sync_models_dev(db: &Database) -> Result<(usize, usize), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(MODELS_DEV_API_URL)
        .send()
        .await
        .map_err(|e| format!("拉取 models.dev 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("models.dev 返回 HTTP {}", resp.status()));
    }
    let catalog: ModelsDevCatalog = resp.json().await.map_err(|e| format!("解析目录失败: {e}"))?;

    let existing = db.list_model_pricing().map_err(|e| e.to_string())?;
    let user_keys: std::collections::HashSet<(String, Option<String>)> = existing
        .iter()
        .filter(|r| r.source == "user")
        .map(|r| (r.model_match.clone(), r.provider_scope.clone()))
        .collect();

    let now = Utc::now().timestamp();
    let mut synced = 0usize;
    let mut skipped = 0usize;
    for provider in catalog.providers.values() {
        let Ok(provider) =
            serde_json::from_value::<ModelsDevProvider>(provider.clone())
        else {
            continue;
        };
        for (model_id, model) in provider.models {
            let Some(cost) = model.cost else {
                continue;
            };
            let (Some(input), Some(output)) = (cost.input, cost.output) else {
                continue;
            };
            if input < 0.0 || output < 0.0 {
                continue;
            }
            let key = (model_id.clone(), None);
            if user_keys.contains(&key) {
                skipped += 1;
                continue;
            }
            let row = ModelPricing {
                id: format!("models_dev:{model_id}"),
                model_match: model_id.clone(),
                provider_scope: None,
                display_name: model.name.unwrap_or_else(|| model_id.clone()),
                input_cost_per_million: price_to_string(input),
                output_cost_per_million: price_to_string(output),
                cache_read_cost_per_million: price_to_string(cost.cache_read.unwrap_or(0.0)),
                cache_creation_cost_per_million: price_to_string(cost.cache_write.unwrap_or(0.0)),
                off_peak_discount_percent: None,
                off_peak_start: None,
                off_peak_end: None,
                source: "models_dev".into(),
                updated_at: now,
            };
            if db.upsert_model_pricing(&row).is_ok() {
                synced += 1;
            }
        }
    }
    db.set_setting(KEY_MODELS_DEV_LAST_SYNC, &now.to_string())
        .map_err(|e| e.to_string())?;
    Ok((synced, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pricing(model_match: &str, input: &str, output: &str, cache_read: &str) -> ModelPricing {
        ModelPricing {
            id: format!("t:{model_match}"),
            model_match: model_match.into(),
            provider_scope: None,
            display_name: model_match.into(),
            input_cost_per_million: input.into(),
            output_cost_per_million: output.into(),
            cache_read_cost_per_million: cache_read.into(),
            cache_creation_cost_per_million: "0".into(),
            off_peak_discount_percent: None,
            off_peak_start: None,
            off_peak_end: None,
            source: "user".into(),
            updated_at: 0,
        }
    }

    #[test]
    fn parse_price_micro_handles_decimal_strings() {
        assert_eq!(parse_price_micro("0.27"), Some(270_000));
        assert_eq!(parse_price_micro("1.1"), Some(1_100_000));
        assert_eq!(parse_price_micro("27"), Some(27_000_000));
        assert_eq!(parse_price_micro("0.000001"), Some(1));
        assert_eq!(parse_price_micro(" 0.5 "), Some(500_000));
        assert_eq!(parse_price_micro(""), None);
        assert_eq!(parse_price_micro("abc"), None);
        assert_eq!(parse_price_micro("0.x"), None);
    }

    #[test]
    fn format_micro_trims_trailing_zeros() {
        assert_eq!(format_micro(270_000), "0.27");
        assert_eq!(format_micro(1_100_000), "1.1");
        assert_eq!(format_micro(1), "0.000001");
        assert_eq!(format_micro(0), "0");
        assert_eq!(format_micro(27_000_000), "27");
    }

    #[test]
    fn resolve_prefers_provider_scoped_then_exact_then_longest_prefix() {
        let generic_exact = pricing("deepseek-chat", "0.27", "1.1", "0.07");
        let generic_prefix = pricing("deepseek-", "0.99", "0.99", "0");
        let scoped_exact = ModelPricing {
            provider_scope: Some("relay-a".into()),
            ..pricing("deepseek-chat", "0.5", "2", "0.1")
        };
        let rows = vec![generic_exact, generic_prefix, scoped_exact];

        // 供应商限定精确 > 通用精确 > 通用前缀
        let hit = resolve_pricing(&rows, "deepseek-chat", Some("relay-a")).unwrap();
        assert_eq!(hit.input_cost_per_million, "0.5");
        let hit = resolve_pricing(&rows, "deepseek-chat", Some("other")).unwrap();
        assert_eq!(hit.input_cost_per_million, "0.27");
        // 前缀：deepseek-chat-250xxx 命中通用精确行（deepseek-chat 是其前缀）
        let hit = resolve_pricing(&rows, "deepseek-chat-250528", None).unwrap();
        assert_eq!(hit.input_cost_per_million, "0.27");
        // 最长前缀优先
        let hit = resolve_pricing(&rows, "deepseek-reasoner", None).unwrap();
        assert_eq!(hit.input_cost_per_million, "0.99");
        // 无匹配
        assert!(resolve_pricing(&rows, "gpt-x", None).is_none());
        assert!(resolve_pricing(&rows, "", None).is_none());
    }

    #[test]
    fn compute_cost_deepseek_tiers_and_off_peak() {
        use chrono::TimeZone;
        // DeepSeek 官方口径结构：缓存未命中输入 / 缓存命中输入 / 输出 三档。
        let mut deepseek = pricing("deepseek-chat", "0.27", "1.1", "0.07");

        // 高峰：1M 未命中输入 + 1M 命中 + 1M 输出
        let cost = compute_cost(&deepseek, 1_000_000, 1_000_000, 1_000_000, 0, 1_750_000_000);
        assert_eq!(cost.input, 270_000); // $0.27
        assert_eq!(cost.cache_read, 70_000);
        assert_eq!(cost.output, 1_100_000);
        assert_eq!(cost.total(), 1_440_000);

        // 峰谷：50% 折扣，UTC 窗口 16:30-00:30（跨午夜）。
        deepseek.off_peak_discount_percent = Some(50);
        deepseek.off_peak_start = Some("16:30".into());
        deepseek.off_peak_end = Some("00:30".into());
        let off_peak_ts = chrono::Utc
            .with_ymd_and_hms(2026, 8, 24, 18, 0, 0)
            .unwrap()
            .timestamp();
        let cost = compute_cost(&deepseek, 1_000_000, 1_000_000, 1_000_000, 0, off_peak_ts);
        assert_eq!(cost.input, 135_000, "错峰输入半价");
        assert_eq!(cost.output, 550_000, "错峰输出半价");
        // 高峰时刻（同日 10:00 UTC）不折扣
        let peak_ts = chrono::Utc
            .with_ymd_and_hms(2026, 8, 24, 10, 0, 0)
            .unwrap()
            .timestamp();
        let cost = compute_cost(&deepseek, 1_000_000, 0, 0, 0, peak_ts);
        assert_eq!(cost.input, 270_000);
    }

    #[test]
    fn off_peak_window_crosses_midnight() {
        use chrono::TimeZone;
        fn minute_ts(h: i64, m: i64) -> i64 {
            chrono::Utc
                .with_ymd_and_hms(2026, 8, 24, h as u32, m as u32, 0)
                .unwrap()
                .timestamp()
        }
        // 窗口 22:00-02:00：23:59 与 01:00 在窗口内，12:00 不在。
        assert!(is_off_peak(minute_ts(23, 59), "22:00", "02:00"));
        assert!(is_off_peak(minute_ts(1, 0), "22:00", "02:00"));
        assert!(!is_off_peak(minute_ts(12, 0), "22:00", "02:00"));
        // 非跨午夜窗口
        assert!(is_off_peak(minute_ts(17, 0), "16:30", "20:00"));
        assert!(!is_off_peak(minute_ts(21, 0), "16:30", "20:00"));
        // 非法窗口不生效
        assert!(!is_off_peak(0, "bad", "02:00"));
    }

    #[test]
    fn upsert_validates_prices_and_window() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let mut p = pricing("deepseek-chat", "0.27", "1.1", "0.07");
        db.upsert_model_pricing(&p).unwrap();
        assert_eq!(db.list_model_pricing().unwrap().len(), 1);

        p.input_cost_per_million = "-1".into();
        assert!(db.upsert_model_pricing(&p).is_err());
        p.input_cost_per_million = "0.27".into();
        p.off_peak_start = Some("25:00".into());
        p.off_peak_end = Some("02:00".into());
        assert!(db.upsert_model_pricing(&p).is_err());
        p.off_peak_start = Some("16:30".into());
        db.upsert_model_pricing(&p).unwrap();
        assert_eq!(db.list_model_pricing().unwrap().len(), 1);

        assert!(db.delete_model_pricing(&p.id).unwrap());
        assert!(db.list_model_pricing().unwrap().is_empty());
    }

    #[test]
    fn old_model_pricing_schema_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cc.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute(
                "CREATE TABLE model_pricing (
                    model_id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    input_cost_per_million TEXT NOT NULL,
                    output_cost_per_million TEXT NOT NULL,
                    cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
                    cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
                )",
                [],
            )
            .unwrap();
        }
        let db = Database::new(&path).unwrap();
        let cols: Vec<String> = {
            let conn = db.lock();
            let mut stmt = conn.prepare("PRAGMA table_info(model_pricing)").unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert!(cols.contains(&"id".to_string()));
        assert!(cols.contains(&"off_peak_discount_percent".to_string()));
        assert!(db.list_model_pricing().unwrap().is_empty());
    }

    #[test]
    fn backfill_zero_costs_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.upsert_model_pricing(&pricing("deepseek-chat", "0.27", "1.1", "0.07"))
            .unwrap();

        let conn = db.lock();
        // 一行零成本（可回填）、一行插件自带成本（不覆盖）、一行无匹配模型
        conn.execute(
            "INSERT INTO request_logs (request_id, provider_id, plugin_id, model, active_provider_id,
                input_tokens, output_tokens, cache_read_tokens, total_cost_usd, created_at)
             VALUES ('r1', '_p_session', 'claudecode', 'deepseek-chat', 'relay-a',
                1000000, 1000000, 1000000, '0', 1750000000),
                    ('r2', '_p_session', 'opencode', 'deepseek-chat', NULL,
                1000000, 0, 0, '9.99', 1750000000),
                    ('r3', '_p_session', 'claudecode', 'unknown-model', NULL,
                1000000, 0, 0, '0', 1750000000)",
            [],
        )
        .unwrap();
        drop(conn);

        let n = db.backfill_zero_costs().unwrap();
        assert_eq!(n, 1);

        let logs = db.list_request_logs(None, 10).unwrap();
        let r1 = logs.iter().find(|l| l.request_id == "r1").unwrap();
        assert_eq!(r1.total_cost_usd, "1.44"); // 0.27 + 0.07 + 1.1
        let r2 = logs.iter().find(|l| l.request_id == "r2").unwrap();
        assert_eq!(r2.total_cost_usd, "9.99", "插件自带成本不覆盖");
        let r3 = logs.iter().find(|l| l.request_id == "r3").unwrap();
        assert_eq!(r3.total_cost_usd, "0", "无匹配定价保持 0");
    }

    #[test]
    fn models_dev_catalog_parses_defensively() {
        // 目录解析的形状校验（网络同步逻辑的纯函数部分）。
        let payload = json!({
            "anthropic": {
                "models": {
                    "claude-opus-4": {
                        "name": "Claude Opus 4",
                        "cost": { "input": 15.0, "output": 75.0, "cache_read": 1.5, "cache_write": 18.75 }
                    },
                    "no-cost-model": { "name": "Free?" }
                }
            },
            "broken": "not-an-object"
        });
        let catalog: ModelsDevCatalog = serde_json::from_value(payload).unwrap();
        let mut count = 0;
        for provider in catalog.providers.values() {
            let Ok(provider) = serde_json::from_value::<ModelsDevProvider>(provider.clone()) else {
                continue;
            };
            for (id, model) in provider.models {
                if let Some(cost) = model.cost {
                    if let (Some(input), Some(_output)) = (cost.input, cost.output) {
                        assert_eq!(id, "claude-opus-4");
                        assert_eq!(price_to_string(input), "15");
                        assert_eq!(price_to_string(cost.cache_write.unwrap()), "18.75");
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, 1);
    }
}
