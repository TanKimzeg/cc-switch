//! 模型定价管理命令（PricingService 的 IPC 层）。

use tauri::State;

use crate::db::Database;
use crate::services::pricing::{sync_models_dev, ModelPricing};

#[tauri::command]
pub fn pricing_list(db: State<'_, Database>) -> Result<Vec<ModelPricing>, String> {
    db.list_model_pricing().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pricing_upsert(db: State<'_, Database>, pricing: ModelPricing) -> Result<(), String> {
    db.upsert_model_pricing(&pricing)
}

#[tauri::command]
pub fn pricing_delete(db: State<'_, Database>, id: String) -> Result<bool, String> {
    db.delete_model_pricing(&id)
}

/// 拉取 models.dev 公共模型价格并 upsert（不覆盖用户手填行）。返回 (同步数, 跳过数)。
#[tauri::command]
pub async fn pricing_sync_models_dev(db: State<'_, Database>) -> Result<(usize, usize), String> {
    sync_models_dev(&db).await
}

/// 回填历史零成本行（改价/补定价后使用）。返回更新条数。
#[tauri::command]
pub fn usage_recompute_costs(db: State<'_, Database>) -> Result<usize, String> {
    db.backfill_zero_costs()
}
