use crate::db::Database;
use crate::services::http_client;

#[tauri::command]
pub fn get_global_proxy_url(db: tauri::State<'_, Database>) -> Result<Option<String>, String> {
    let url = db.get_setting(http_client::KEY_GLOBAL_PROXY_URL).map_err(|e| e.to_string())?;
    let trimmed = url.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Ok(trimmed)
}

#[tauri::command]
pub async fn set_global_proxy_url(
    url: String,
    db: tauri::State<'_, Database>,
) -> Result<(), String> {
    let trimmed = url.trim();
    let effective = if trimmed.is_empty() { None } else { Some(trimmed) };

    // 先校验格式。
    http_client::validate_proxy(effective)?;

    // 重建客户端、立即生效。
    http_client::apply(effective)?;

    // 持久化（空串 = 清除）。
    db.set_setting(http_client::KEY_GLOBAL_PROXY_URL, effective.unwrap_or(""))
        .map_err(|e| e.to_string())?;

    log::info!(
        "[GlobalProxy] 出站代理已更新: {}",
        effective.map(http_client::mask_url).unwrap_or_default()
    );
    Ok(())
}

/// 测试代理连通性（向 models.dev 发请求，返回延迟）。
#[tauri::command]
pub async fn test_global_proxy_url(
    url: String,
    _db: tauri::State<'_, Database>,
) -> Result<ProxyTestResult, String> {
    let trimmed = url.trim();
    let effective = if trimmed.is_empty() { None } else { Some(trimmed) };
    http_client::validate_proxy(effective)?;

    let client = http_client::get();
    let test_url = "https://models.dev/api.json";
    let start = std::time::Instant::now();
    let result = client
        .get(test_url)
        .header("User-Agent", "CC-Switch/2.0")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                Ok(ProxyTestResult {
                    ok: true,
                    latency_ms: Some(latency_ms),
                    error: None,
                })
            } else {
                Ok(ProxyTestResult {
                    ok: false,
                    latency_ms: Some(latency_ms),
                    error: Some(format!("HTTP {}", status)),
                })
            }
        }
        Err(e) => Ok(ProxyTestResult {
            ok: false,
            latency_ms: None,
            error: Some(format!("{e}")),
        }),
    }
}

#[derive(serde::Serialize)]
pub struct ProxyTestResult {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}
