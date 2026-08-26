//! 进程插件：通过 manifest 的 `entry.shell` 声明的外部命令实现协议。
//!
//! 第三方插件不内嵌 Rust 代码，而是声明一个命令（`command` + `args`）。
//! cc-switch 按约定调用该命令的子命令来读写配置：
//!
//! - `read-live`：stdout 输出 JSON `{ "providers": [...], "current": "id" }`
//! - `apply <provider-id>`：stdin 输入 provider 的 settings_config JSON
//! - `import`：stdout 输出 JSON 数组 `[{ "id", "name", "settingsConfig" }]`
//! - `sessions`：stdout 输出 JSON 数组会话元信息

use std::process::{Command, Stdio};

use serde_json::Value;

use crate::plugin::error::PluginError;
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, PluginCapabilities, SessionMeta};
use crate::types::Provider;

/// 进程插件：包装 manifest 的 shell entry。
#[derive(Debug, Clone)]
pub struct ProcessPlugin {
    id: String,
    command: String,
    args: Vec<String>,
    capabilities: PluginCapabilities,
}

impl ProcessPlugin {
    pub fn new(
        id: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        capabilities: PluginCapabilities,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            args,
            capabilities,
        }
    }

    fn run(&self, subcommand: &str, stdin_data: Option<&str>) -> Result<Value, PluginError> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .arg(subcommand)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| PluginError::Process {
            command: self.command.clone(),
            message: format!("无法启动: {e}"),
        })?;

        if let Some(data) = stdin_data {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(data.as_bytes());
            }
        }

        let output = child.wait_with_output().map_err(|e| PluginError::Process {
            command: self.command.clone(),
            message: format!("等待退出失败: {e}"),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(PluginError::Process {
                command: self.command.clone(),
                message: format!(
                    "子命令 {subcommand} 退出码 {:?}: {stderr}",
                    output.status.code()
                ),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&stdout).map_err(|e| PluginError::Process {
            command: self.command.clone(),
            message: format!("子命令 {subcommand} 输出不是合法 JSON: {e}"),
        })
    }
}

impl AgentPlugin for ProcessPlugin {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> &PluginCapabilities {
        &self.capabilities
    }

    fn read_live(&self) -> Result<LiveConfig, PluginError> {
        let value = self.run("read-live", None)?;
        parse_live_config(&self.id, value)
    }

    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError> {
        let settings = provider.settings_config.clone().unwrap_or_default();
        // `current` 语义通过 `apply` 子命令的参数传递（约定的第二个参数）。
        let current_flag = if current { "current" } else { "no-current" };
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .arg("apply")
            .arg(current_flag)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| PluginError::Process {
            command: self.command.clone(),
            message: format!("无法启动: {e}"),
        })?;

        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(settings.as_bytes());
        }
        let output = child.wait_with_output().map_err(|e| PluginError::Process {
            command: self.command.clone(),
            message: format!("等待退出失败: {e}"),
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(PluginError::Process {
                command: self.command.clone(),
                message: format!("apply 退出码 {:?}: {stderr}", output.status.code()),
            });
        }
        Ok(())
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let value = self.run("import", None)?;
        parse_import(&self.id, value)
    }

    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> {
        let value = self.run("sessions", None)?;
        parse_sessions(&self.id, value)
    }
}

/// 解析 `read-live` 子命令的输出为 [`LiveConfig`]。
fn parse_live_config(plugin_id: &str, value: Value) -> Result<LiveConfig, PluginError> {
    serde_json::from_value(value).map_err(|e| {
        PluginError::Other(format!("插件 '{plugin_id}' 的 read-live 输出结构不符: {e}"))
    })
}

/// 解析 `import` 子命令的输出；兼容 JSON 数组或 `{ id: 配置 }` 对象两种形态。
fn parse_import(plugin_id: &str, value: Value) -> Result<Vec<ImportCandidate>, PluginError> {
    let items = match value {
        Value::Array(items) => items,
        Value::Object(map) => map
            .into_iter()
            .map(|(id, v)| serde_json::json!({ "id": id, "name": id, "settingsConfig": v }))
            .collect::<Vec<_>>(),
        _ => {
            return Err(PluginError::Other(format!(
                "插件 '{plugin_id}' 的 import 输出必须是数组或对象"
            )))
        }
    };
    items
        .into_iter()
        .map(|item| {
            serde_json::from_value(item).map_err(|e| {
                PluginError::Other(format!("插件 '{plugin_id}' 的 import 条目结构不符: {e}"))
            })
        })
        .collect()
}

/// 解析 `sessions` 子命令的输出为 [`SessionMeta`] 列表。
fn parse_sessions(plugin_id: &str, value: Value) -> Result<Vec<SessionMeta>, PluginError> {
    let items = value.as_array().cloned().unwrap_or_default();
    items
        .into_iter()
        .map(|item| {
            serde_json::from_value(item).map_err(|e| {
                PluginError::Other(format!("插件 '{plugin_id}' 的 sessions 条目结构不符: {e}"))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> PluginCapabilities {
        PluginCapabilities {
            read_live: true,
            apply: true,
            remove: true,
            import: true,
            sessions: true,
            mcp: true,
        }
    }

    #[test]
    fn plugin_exposes_id_and_capabilities() {
        let p = ProcessPlugin::new("demo", "demo-cli", vec![], caps());
        assert_eq!(p.id(), "demo");
        assert!(p.capabilities().sessions);
    }

    #[test]
    fn parse_live_config_accepts_full_shape() {
        let value = serde_json::json!({
            "providers": [
                { "id": "a", "name": "A", "settingsConfig": { "npm": "x" } }
            ],
            "current": "a"
        });
        let live = parse_live_config("demo", value).unwrap();
        assert_eq!(live.providers.len(), 1);
        assert_eq!(live.providers[0].name, "A");
        assert_eq!(live.current.as_deref(), Some("a"));
    }

    #[test]
    fn parse_live_config_rejects_invalid_shape() {
        let value = serde_json::json!({ "providers": "not-an-array" });
        assert!(parse_live_config("demo", value).is_err());
    }

    #[test]
    fn parse_import_accepts_array() {
        let value = serde_json::json!([
            { "id": "a", "name": "A", "settingsConfig": { "npm": "x" } }
        ]);
        let items = parse_import("demo", value).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "a");
    }

    #[test]
    fn parse_import_accepts_object_map() {
        let value = serde_json::json!({ "b": { "npm": "y" } });
        let items = parse_import("demo", value).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "b");
        assert_eq!(items[0].name, "b");
        assert!(items[0].settings_config["npm"].is_string());
    }

    #[test]
    fn parse_import_rejects_scalar() {
        assert!(parse_import("demo", serde_json::json!(42)).is_err());
    }

    #[test]
    fn parse_sessions_parses_entries() {
        let value = serde_json::json!([
            { "sessionId": "s1", "title": "T", "resumeCommand": "opencode -s s1" }
        ]);
        let sessions = parse_sessions("demo", value).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
        assert_eq!(
            sessions[0].resume_command.as_deref(),
            Some("opencode -s s1")
        );
    }

    #[test]
    fn parse_sessions_rejects_invalid_entry() {
        let value = serde_json::json!([{ "id": 42 }]);
        assert!(parse_sessions("demo", value).is_err());
    }

    #[test]
    fn command_failure_reports_process_error() {
        let p = ProcessPlugin::new("demo", "definitely-not-a-real-command-xyz", vec![], caps());
        assert!(matches!(
            p.run("read-live", None),
            Err(PluginError::Process { .. })
        ));
    }
}
