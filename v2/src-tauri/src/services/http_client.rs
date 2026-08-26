//! 全局出站 HTTP 客户端（对齐 v1 `proxy/http_client.rs` 的核心能力）。
//!
//! 解决 GFW 场景：models.dev / GitHub 等出站请求可经用户配置的代理
//! （`http(s)://` 或 `socks5(h)://`，如 Clash 的 `http://127.0.0.1:7890`）。
//! 代理 URL 存 settings 键 `global_proxy_url`，启动时加载、修改即时生效；
//! 未配置时跟随系统/环境变量代理。

use std::sync::Mutex;

use reqwest::Client;

use crate::db::Database;

pub const KEY_GLOBAL_PROXY_URL: &str = "global_proxy_url";

static CLIENT: Mutex<Option<Client>> = Mutex::new(None);
static CURRENT_PROXY_URL: Mutex<Option<String>> = Mutex::new(None);

/// 支持的代理 scheme。
const SUPPORTED_SCHEMES: &[&str] = &["http", "https", "socks5", "socks5h"];

/// 启动时从 settings 加载代理并初始化全局客户端。
pub fn init_from_db(db: &Database) {
    let url = db.get_setting(KEY_GLOBAL_PROXY_URL).ok().flatten();
    match apply(url.as_deref()) {
        Ok(()) => {
            if url.as_deref().is_some_and(|u| !u.trim().is_empty()) {
                log::info!("[GlobalProxy] 出站代理已启用: {}", mask_url(&url.unwrap()));
            }
        }
        Err(e) => {
            // 代理配置损坏时回退直连，不阻塞启动。
            log::error!("[GlobalProxy] 代理配置无效，回退直连: {e}");
            let _ = apply(None);
            let _ = db.set_setting(KEY_GLOBAL_PROXY_URL, "");
        }
    }
}

/// 应用代理配置（重建客户端，立即生效）。空/None = 直连。
pub fn apply(proxy_url: Option<&str>) -> Result<(), String> {
    let effective = proxy_url.map(str::trim).filter(|s| !s.is_empty());
    let client = build_client(effective)?;
    *CLIENT.lock().unwrap() = Some(client);
    *CURRENT_PROXY_URL.lock().unwrap() = effective.map(str::to_string);
    Ok(())
}

/// 获取全局客户端（未初始化时构建直连兜底）。
pub fn get() -> Client {
    if let Some(client) = CLIENT.lock().unwrap().as_ref() {
        return client.clone();
    }
    build_client(None).unwrap_or_else(|_| Client::new())
}

/// 当前代理 URL（None = 直连）。
#[allow(dead_code)]
pub fn current_proxy_url() -> Option<String> {
    CURRENT_PROXY_URL.lock().unwrap().clone()
}

/// 校验代理 URL 格式与 scheme。
pub fn validate_proxy(proxy_url: Option<&str>) -> Result<(), String> {
    let effective = proxy_url.map(str::trim).filter(|s| !s.is_empty());
    let Some(url) = effective else {
        return Ok(());
    };
    let parsed =
        url::Url::parse(url).map_err(|e| format!("代理 URL '{}' 不合法: {e}", mask_url(url)))?;
    if !SUPPORTED_SCHEMES.contains(&parsed.scheme()) {
        return Err(format!(
            "不支持的代理 scheme '{}'（支持: {}）: {}",
            parsed.scheme(),
            SUPPORTED_SCHEMES.join("/"),
            mask_url(url)
        ));
    }
    Ok(())
}

fn build_client(proxy_url: Option<&str>) -> Result<Client, String> {
    let mut builder = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .tcp_keepalive(std::time::Duration::from_secs(60));

    if let Some(url) = proxy_url {
        validate_proxy(Some(url))?;
        let proxy =
            reqwest::Proxy::all(url).map_err(|e| format!("代理 URL '{}': {e}", mask_url(url)))?;
        builder = builder.proxy(proxy);
        log::debug!("[GlobalProxy] 代理已配置: {}", mask_url(url));
    } else {
        // 未配置全局代理：跟随系统/环境变量代理（reqwest 默认行为）。
        log::debug!("[GlobalProxy] 未配置代理，跟随系统代理");
    }

    builder.build().map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

/// 隐藏 URL 中的用户名密码（用于日志与错误信息）。
pub fn mask_url(url: &str) -> String {
    if let Ok(mut parsed) = url::Url::parse(url) {
        if !parsed.username().is_empty() {
            let _ = parsed.set_username("***");
        }
        if parsed.password().is_some() {
            let _ = parsed.set_password(Some("***"));
        }
        parsed.to_string()
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_bad_scheme_and_url() {
        assert!(validate_proxy(Some("http://127.0.0.1:7890")).is_ok());
        assert!(validate_proxy(Some("socks5://127.0.0.1:1080")).is_ok());
        assert!(validate_proxy(Some("socks5h://user:pass@host:1080")).is_ok());
        assert!(validate_proxy(Some("ftp://x")).is_err());
        assert!(validate_proxy(Some("not a url")).is_err());
        assert!(validate_proxy(None).is_ok());
        assert!(validate_proxy(Some("  ")).is_ok());
    }

    #[test]
    fn apply_builds_client_and_tracks_url() {
        // 全局静态状态：env_lock 串行化，测试后恢复直连。
        let _lock = crate::test_support::env_lock().lock().unwrap();
        apply(Some("http://127.0.0.1:7890")).unwrap();
        assert_eq!(current_proxy_url().as_deref(), Some("http://127.0.0.1:7890"));
        let _ = get(); // 客户端可用
        apply(None).unwrap();
        assert_eq!(current_proxy_url(), None);
    }

    #[test]
    fn mask_url_hides_credentials() {
        assert_eq!(
            mask_url("http://user:secret@host:8080"),
            "http://***:***@host:8080/"
        );
        assert_eq!(mask_url("http://host:8080"), "http://host:8080/");
    }
}
