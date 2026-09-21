//! 插件协议错误类型。

use std::path::PathBuf;

/// 插件协议执行过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("IO 错误（{path}）: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("解析 JSON 失败（{path}）: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("配置不合法: {0}")]
    Config(String),
    #[error("插件不支持该能力: {0}")]
    Capability(String),
    #[error("外部命令执行失败（{command}）: {message}")]
    Process { command: String, message: String },
    #[error("其他错误: {0}")]
    Other(String),
}

impl PluginError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        PluginError::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        PluginError::Json {
            path: path.into(),
            source,
        }
    }
}

impl From<std::io::Error> for PluginError {
    fn from(err: std::io::Error) -> Self {
        PluginError::Other(format!("IO 错误: {err}"))
    }
}
