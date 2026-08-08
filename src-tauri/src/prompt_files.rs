use std::path::PathBuf;

use crate::app_config::AppType;
use crate::error::AppError;

/// 返回指定应用所使用的提示词文件路径。
///
/// 具体路径逻辑（哪个目录、什么文件名、是否支持）由各 app 的 descriptor 提供，
/// 这里只做委托。
pub fn prompt_file_path(app: &AppType) -> Result<PathBuf, AppError> {
    app.descriptor().prompt_file_path()
}

/// 以某个主路径（如 settings.json / auth.json）的父目录为基准，取不到时回退到
/// `~/{fallback_dir}`。Claude / Codex 的提示词目录据此推导。
pub(crate) fn get_base_dir_with_fallback(
    primary_path: PathBuf,
    fallback_dir: &str,
) -> Result<PathBuf, AppError> {
    primary_path
        .parent()
        .map(|p| p.to_path_buf())
        .or_else(|| dirs::home_dir().map(|h| h.join(fallback_dir)))
        .ok_or_else(|| {
            AppError::localized(
                "home_dir_not_found",
                format!("无法确定 {fallback_dir} 配置目录：用户主目录不存在"),
                format!("Cannot determine {fallback_dir} config directory: user home not found"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_path_delegates_to_descriptor() {
        for app in AppType::all().filter(|a| *a != AppType::ClaudeDesktop) {
            assert!(
                prompt_file_path(&app).is_ok(),
                "{} should have a prompt path",
                app.as_str()
            );
        }
        assert!(prompt_file_path(&AppType::ClaudeDesktop).is_err());
    }
}
