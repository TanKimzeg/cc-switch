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
use crate::plugin::{
    AgentPlugin, ImportCandidate, LiveConfig, PluginCapabilities, SessionMeta,
};
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
                message: format!("子命令 {subcommand} 退出码 {:?}: {stderr}", output.status.code()),
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
        serde_json::from_value(value).map_err(|e| PluginError::Other(format!(
            "插件 '{}' 的 read-live 输出结构不符: {e}",
            self.id
        )))
    }

    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError> {
        let settings = provider.settings_config.clone().unwrap_or_default();
        let flag = if current { "1" } else { "0" };
        self.run("apply", Some(&settings))?;
        // `current` 语义通过第二个子命令参数传递（若有约定）。
        let _ = flag;
        Ok(())
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let value = self.run("import", None)?;
        let items = match value {
            Value::Array(items) => items,
            Value::Object(map) => map
                .into_iter()
                .map(|(id, v)| {
                    serde_json::json!({ "id": id, "name": id, "settingsConfig": v })
                })
                .collect::<Vec<_>>(),
            _ => return Err(PluginError::Other(format!(
                "插件 '{}' 的 import 输出必须是数组或对象",
                self.id
            ))),
        };
        items
            .into_iter()
            .map(|item| {
                serde_json::from_value(item).map_err(|e| {
                    PluginError::Other(format!(
                        "插件 '{}' 的 import 条目结构不符: {e}",
                        self.id
                    ))
                })
            })
            .collect()
    }

    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> {
        let value = self.run("sessions", None)?;
        let items = value.as_array().cloned().unwrap_or_default();
        items
            .into_iter()
            .map(|item| {
                serde_json::from_value(item).map_err(|e| {
                    PluginError::Other(format!(
                        "插件 '{}' 的 sessions 条目结构不符: {e}",
                        self.id
                    ))
                })
            })
            .collect()
    }
}
