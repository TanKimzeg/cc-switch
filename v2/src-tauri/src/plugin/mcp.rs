//! MCP 服务器管理协议（可选能力）。
//!
//! 支持 MCP 的插件（如 opencode）实现 [`McpPlugin`]，通过统一
//! CC Switch MCP 格式与各 Agent 原生格式之间的转换来同步服务器：
//!
//! | CC Switch 统一格式 | OpenCode 格式   |
//! |--------------------|-----------------|
//! | `type: "stdio"`    | `type: "local"` |
//! | `command` + `args` | `command: [cmd, ...]` |
//! | `env`              | `environment`   |
//! | `type: "sse"/"http"` | `type: "remote"` |
//! | `url`              | `url`           |

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plugin::error::PluginError;

/// MCP 服务器描述（CC Switch 统一格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSpec {
    /// 服务器 id。
    pub id: String,
    /// 展示名（默认同 id）。
    pub name: String,
    /// 服务器配置（统一格式：type/command/args/env/url/headers）。
    pub spec: Value,
}

/// MCP 服务器管理协议（可选能力）。
///
/// 实现方必须同时满足 `Send + Sync`。
pub trait McpPlugin: Send + Sync {
    /// 读取 live 配置中的全部 MCP 服务器（统一格式）。
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError>;

    /// 把一个 MCP 服务器写入 live 配置。
    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError>;

    /// 从 live 配置移除某个 MCP 服务器。
    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError>;
}

/// 把统一格式转换为 OpenCode 格式。
///
/// - `stdio` → `local`：command+args 合并为 command 数组、env → environment
/// - `sse`/`http` → `remote`：保留 url/headers
pub fn convert_to_opencode_format(spec: &Value) -> Result<Value, PluginError> {
    let obj = spec
        .as_object()
        .ok_or_else(|| PluginError::Config("MCP spec 必须是 JSON 对象".into()))?;

    let typ = obj.get("type").and_then(Value::as_str).unwrap_or("stdio");
    let mut result = serde_json::Map::new();

    match typ {
        "stdio" => {
            result.insert("type".into(), Value::String("local".into()));
            let cmd = obj.get("command").and_then(Value::as_str).unwrap_or("");
            let mut command_arr = vec![Value::String(cmd.into())];
            if let Some(args) = obj.get("args").and_then(Value::as_array) {
                command_arr.extend(args.iter().cloned());
            }
            result.insert("command".into(), Value::Array(command_arr));
            if let Some(env) = obj.get("env") {
                if env.is_object() && !env.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    result.insert("environment".into(), env.clone());
                }
            }
            result.insert("enabled".into(), Value::Bool(true));
        }
        "sse" | "http" => {
            result.insert("type".into(), Value::String("remote".into()));
            if let Some(url) = obj.get("url") {
                result.insert("url".into(), url.clone());
            }
            if let Some(headers) = obj.get("headers") {
                if headers.is_object()
                    && !headers.as_object().map(|o| o.is_empty()).unwrap_or(true)
                {
                    result.insert("headers".into(), headers.clone());
                }
            }
            result.insert("enabled".into(), Value::Bool(true));
        }
        other => {
            return Err(PluginError::Config(format!("未知 MCP 类型: {other}")));
        }
    }

    Ok(Value::Object(result))
}

/// 把 OpenCode 格式转换为统一格式。
///
/// - `local` → `stdio`：command 数组拆分为 command+args、environment → env
/// - `remote` → `sse`：保留 url/headers
pub fn convert_from_opencode_format(spec: &Value) -> Result<Value, PluginError> {
    let obj = spec
        .as_object()
        .ok_or_else(|| PluginError::Config("OpenCode MCP spec 必须是 JSON 对象".into()))?;

    let typ = obj.get("type").and_then(Value::as_str).unwrap_or("local");
    let mut result = serde_json::Map::new();

    match typ {
        "local" => {
            result.insert("type".into(), Value::String("stdio".into()));
            if let Some(cmd_arr) = obj.get("command").and_then(Value::as_array) {
                if let Some(cmd) = cmd_arr.first().and_then(Value::as_str) {
                    result.insert("command".into(), Value::String(cmd.into()));
                }
                if cmd_arr.len() > 1 {
                    result.insert(
                        "args".into(),
                        Value::Array(cmd_arr[1..].to_vec()),
                    );
                }
            }
            if let Some(env) = obj.get("environment") {
                if env.is_object() && !env.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    result.insert("env".into(), env.clone());
                }
            }
        }
        "remote" => {
            result.insert("type".into(), Value::String("sse".into()));
            if let Some(url) = obj.get("url") {
                result.insert("url".into(), url.clone());
            }
            if let Some(headers) = obj.get("headers") {
                if headers.is_object()
                    && !headers.as_object().map(|o| o.is_empty()).unwrap_or(true)
                {
                    result.insert("headers".into(), headers.clone());
                }
            }
        }
        other => {
            return Err(PluginError::Config(format!(
                "未知 OpenCode MCP 类型: {other}"
            )));
        }
    }

    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn convert_stdio_to_local() {
        let spec = json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem"],
            "env": { "HOME": "/Users/test" }
        });
        let result = convert_to_opencode_format(&spec).unwrap();
        assert_eq!(result["type"], "local");
        assert_eq!(result["command"][0], "npx");
        assert_eq!(result["command"][2], "@modelcontextprotocol/server-filesystem");
        assert_eq!(result["environment"]["HOME"], "/Users/test");
        assert_eq!(result["enabled"], true);
    }

    #[test]
    fn convert_sse_to_remote() {
        let spec = json!({
            "type": "sse",
            "url": "https://example.com/mcp",
            "headers": { "Authorization": "Bearer xxx" }
        });
        let result = convert_to_opencode_format(&spec).unwrap();
        assert_eq!(result["type"], "remote");
        assert_eq!(result["url"], "https://example.com/mcp");
        assert_eq!(result["headers"]["Authorization"], "Bearer xxx");
        assert_eq!(result["enabled"], true);
    }

    #[test]
    fn convert_local_to_stdio() {
        let spec = json!({
            "type": "local",
            "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"],
            "environment": { "HOME": "/Users/test" }
        });
        let result = convert_from_opencode_format(&spec).unwrap();
        assert_eq!(result["type"], "stdio");
        assert_eq!(result["command"], "npx");
        assert_eq!(result["args"][1], "@modelcontextprotocol/server-filesystem");
        assert_eq!(result["env"]["HOME"], "/Users/test");
    }

    #[test]
    fn convert_remote_to_sse() {
        let spec = json!({
            "type": "remote",
            "url": "https://example.com/mcp",
            "headers": { "Authorization": "Bearer xxx" }
        });
        let result = convert_from_opencode_format(&spec).unwrap();
        assert_eq!(result["type"], "sse");
        assert_eq!(result["url"], "https://example.com/mcp");
        assert_eq!(result["headers"]["Authorization"], "Bearer xxx");
    }

    #[test]
    fn rejects_unknown_type() {
        assert!(convert_to_opencode_format(&json!({"type": "weird"})).is_err());
        assert!(convert_from_opencode_format(&json!({"type": "weird"})).is_err());
    }
}
