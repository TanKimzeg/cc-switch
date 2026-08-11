use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub category: String,
    pub icon: Option<String>,
    pub website: Option<String>,
    pub api_key: Option<String>,
    pub settings_config: Option<String>,
    pub meta: Option<serde_json::Value>,
    pub sort_order: i64,
    #[serde(default)]
    pub live_config_managed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    /// 可选：additive 插件（如 opencode）的用户自定义 provider id
    /// （作为 live 配置 `provider.<id>` 的键）。未提供时由后端生成 uuid。
    pub id: Option<String>,
    pub plugin_id: String,
    pub name: String,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub website: Option<String>,
    pub api_key: Option<String>,
    pub settings_config: Option<String>,
    pub meta: Option<serde_json::Value>,
    pub sort_order: Option<i64>,
    /// 是否投影到 live 配置（默认 true）。
    #[serde(default)]
    pub live_config_managed: Option<bool>,
}

impl ProviderInput {
    pub fn normalize(mut self) -> Self {
        if self.name.trim().is_empty() {
            self.name = self.plugin_id.clone();
        }
        if self.category.is_none() {
            self.category = Some("custom".to_string());
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<crate::plugin::PluginCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_schema: Option<serde_json::Value>,
    /// 入口类型：native | shell | ts。
    #[serde(default)]
    pub entry_type: String,
    /// TS 插件主脚本（相对插件目录）；非 TS 插件为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
}

/// 插件安装记录（对应 plugin_installs 表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstall {
    pub plugin_id: String,
    pub version: String,
    pub source: String,
    pub sha256: Option<String>,
    pub installed_at: String,
}

/// 已安装插件：manifest 清单 + 安装来源信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    #[serde(flatten)]
    pub manifest: PluginManifest,
    /// 安装来源：`builtin`（内置分发）或 `local`（本地目录安装）。
    pub source: String,
    pub installed_at: String,
}
