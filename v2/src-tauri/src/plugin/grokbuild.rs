//! 原生 Grok Build 插件：读写 `~/.grok/config.toml`（非 additive 整文档快照）、
//! TOML `[mcp_servers]`、`~/.grok/sessions` 会话（summary.json + chat_history.jsonl）。
//!
//! 语义对齐 v1（apps/grokbuild.rs + grok_config.rs + mcp/grokbuild.rs + session grokbuild）：
//! - settings_config 形状为 `{"config": "<TOML 文本>"}`；
//! - 非官方供应商必须携带完整自定义模型表（[models] + [model.<name>]）；
//!   `category = "official"` 的条目允许空文档（回落 Grok CLI 官方 OAuth 登录）；
//! - MCP 与 Codex 同布局但无 `type` 字段、`http_headers` 写作 `headers`。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::plugin::error::PluginError;
use crate::plugin::mcp::{McpPlugin, McpServerSpec};
use crate::plugin::session_utils::{
    extract_text, parse_timestamp_to_ms, truncate_summary, TITLE_MAX_CHARS,
};
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities};
use crate::types::Provider;

const PLUGIN_ID: &str = "grokbuild";

fn home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CC_SWITCH_TEST_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn override_dir(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

/// Grok Build 配置目录（`~/.grok`；可经设置 overrideDir.grokbuild 覆盖）。
fn config_dir() -> PathBuf {
    crate::services::overrides::get(PLUGIN_ID)
        .or_else(|| override_dir("CC_SWITCH_GROK_CONFIG_DIR"))
        .unwrap_or_else(|| home_dir().join(".grok"))
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn read_config_text() -> Result<String, PluginError> {
    let path = config_path();
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))
}

/// 语法级校验（空文档合法——官方态）。
fn validate_config_toml_syntax(text: &str) -> Result<(), PluginError> {
    if text.trim().is_empty() {
        return Ok(());
    }
    text.parse::<toml::Value>()
        .map(|_| ())
        .map_err(|e| PluginError::Config(format!("Grok Build config.toml 格式错误: {e}")))
}

fn table_str<'a>(table: &'a toml::value::Table, key: &str) -> Option<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// 完整自定义模型表校验（对齐 v1 grok_config::validate_config_toml）。
fn validate_model_config(config_toml: &str) -> Result<(), PluginError> {
    let document = config_toml
        .parse::<toml::Value>()
        .map_err(|e| PluginError::Config(format!("Grok Build config.toml 格式错误: {e}")))?;
    let root = document
        .as_table()
        .ok_or_else(|| PluginError::Config("Grok Build 配置必须是 TOML 表结构".into()))?;
    let models = root
        .get("models")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少 [models]".into()))?;
    let default_model = table_str(models, "default")
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少 models.default".into()))?;
    let model_entries = root
        .get("model")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少 [model.<name>]".into()))?;
    let selected = model_entries
        .get(default_model)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            PluginError::Config(format!("Grok Build 配置缺少 [model.\"{default_model}\"]"))
        })?;

    table_str(selected, "model")
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少有效的 model 字段".into()))?;
    table_str(selected, "base_url")
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少有效的 base_url 字段".into()))?;
    table_str(selected, "name")
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少有效的 name 字段".into()))?;
    if table_str(selected, "api_key").is_none() && table_str(selected, "env_key").is_none() {
        return Err(PluginError::Config(
            "Grok Build 配置缺少有效的 api_key 或 env_key 字段".into(),
        ));
    }
    table_str(selected, "api_backend")
        .ok_or_else(|| PluginError::Config("Grok Build 配置缺少有效的 api_backend 字段".into()))?;
    selected
        .get("context_window")
        .and_then(toml::Value::as_integer)
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            PluginError::Config("Grok Build context_window 必须是正整数".into())
        })?;
    Ok(())
}

/// 从 live config.toml 提取选中的自定义模型（无自定义表时 None = 官方态）。
#[allow(dead_code)]
fn extract_model_config(config_toml: &str) -> Option<toml::Table> {
    let document = config_toml.parse::<toml::Value>().ok()?;
    let root = document.as_table()?;
    let default_model = root
        .get("models")?
        .as_table()?
        .get("default")?
        .as_str()?
        .trim();
    root.get("model")?
        .as_table()?
        .get(default_model)?
        .as_table()
        .cloned()
}

/// 原生 Grok Build 插件。
#[derive(Debug, Default)]
pub struct GrokBuildPlugin;

impl GrokBuildPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl AgentPlugin for GrokBuildPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn capabilities(&self) -> &PluginCapabilities {
        static CAPS: PluginCapabilities = PluginCapabilities {
            read_live: true,
            apply: true,
            remove: false,
            import: true,
            sessions: true,
            mcp: true,
        };
        &CAPS
    }

    fn read_live(&self) -> Result<LiveConfig, PluginError> {
        let text = read_config_text()?;
        validate_config_toml_syntax(&text)?;
        Ok(LiveConfig {
            providers: vec![LiveProvider {
                id: "default".into(),
                name: "Grok Build".into(),
                settings_config: json!({ "config": text }),
            }],
            current: Some("default".into()),
        })
    }

    fn apply(&self, provider: &Provider, _current: bool) -> Result<(), PluginError> {
        let raw: Value = provider
            .settings_config
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| {
                PluginError::Config(format!(
                    "provider '{}' settings_config 不是合法 JSON: {e}",
                    provider.id
                ))
            })?
            .unwrap_or_else(|| json!({}));
        let config = raw
            .get("config")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PluginError::Config(format!(
                    "provider '{}' 的 Grok Build 配置缺少 config 字段",
                    provider.id
                ))
            })?
            .to_string();

        // 官方条目不注入自定义模型表：按快照原样写回（首次为空文件），
        // Grok CLI 回落到官方内置模型 + 自带 OAuth 登录。
        if provider.category != "official" {
            validate_model_config(&config)?;
        } else {
            validate_config_toml_syntax(&config)?;
        }

        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, config).map_err(|e| PluginError::io(&path, e))
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let text = read_config_text()?;
        if text.trim().is_empty() {
            return Ok(vec![]);
        }
        validate_config_toml_syntax(&text)?;
        // 官方态（无自定义模型表）没有可导入的 provider。
        if extract_model_config(&text).is_none() {
            return Ok(vec![]);
        }
        Ok(vec![ImportCandidate {
            id: "default".into(),
            name: "Grok Build".into(),
            settings_config: json!({ "config": text }),
        }])
    }

    fn sessions(&self) -> Result<Vec<crate::plugin::SessionMeta>, PluginError> {
        scan_sessions()
    }

    fn load_messages(
        &self,
        source: &str,
    ) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
        load_session_messages(Path::new(source))
    }

    fn delete_session(&self, session_id: &str, source: &str) -> Result<bool, PluginError> {
        delete_session_dir(Path::new(source), session_id)
    }

    fn as_mcp(&self) -> Option<&dyn McpPlugin> {
        Some(self)
    }

    fn prompt_file_path(&self) -> Option<PathBuf> {
        Some(config_dir().join("AGENTS.md"))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        Some(config_dir().join("skills"))
    }

    fn read_raw_config(&self) -> Result<String, PluginError> {
        read_config_text()
    }

    fn write_raw_config(&self, content: &str) -> Result<(), PluginError> {
        validate_config_toml_syntax(content)?;
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, content).map_err(|e| PluginError::io(&path, e))
    }
}

// ============================================================================
// MCP（config.toml 内联 [mcp_servers]，Grok 变体：无 type、headers）
// ============================================================================

fn mcp_target_installed() -> bool {
    crate::services::overrides::get(PLUGIN_ID).is_some()
        || override_dir("CC_SWITCH_GROK_CONFIG_DIR").is_some()
        || config_dir().exists()
}

fn json_server_to_grok_toml_table(spec: &Value) -> Result<toml_edit::Table, PluginError> {
    // 复用 Codex 转换器后剥离 Codex 专属字段：Grok 从 command/url 推断传输方式，
    // 请求头字段名是 `headers`。
    let mut table = crate::plugin::codex::json_server_to_toml_table(spec)?;
    table.remove("type");
    if let Some(headers) = table.remove("http_headers") {
        table.insert("headers", headers);
    }
    Ok(table)
}

fn toml_server_to_json(entry: &toml::value::Table) -> Value {
    fn convert(value: &toml::Value) -> Option<Value> {
        match value {
            toml::Value::String(v) => Some(json!(v)),
            toml::Value::Integer(v) => Some(json!(v)),
            toml::Value::Float(v) => Some(json!(v)),
            toml::Value::Boolean(v) => Some(json!(v)),
            toml::Value::Datetime(v) => Some(json!(v.to_string())),
            toml::Value::Array(values) => Some(Value::Array(
                values.iter().filter_map(convert).collect::<Vec<_>>(),
            )),
            toml::Value::Table(values) => Some(Value::Object(
                values
                    .iter()
                    .filter_map(|(k, v)| convert(v).map(|v| (k.clone(), v)))
                    .collect(),
            )),
        }
    }

    let mut spec = serde_json::Map::new();
    for (key, value) in entry {
        let output_key = if key == "http_headers" { "headers" } else { key };
        if let Some(v) = convert(value) {
            spec.insert(output_key.to_string(), v);
        }
    }
    let default_type = if spec.contains_key("url") {
        "http"
    } else {
        "stdio"
    };
    spec.entry("type".to_string())
        .or_insert_with(|| json!(default_type));
    Value::Object(spec)
}

impl McpPlugin for GrokBuildPlugin {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError> {
        let text = read_config_text()?;
        let mut servers = Vec::new();
        if text.trim().is_empty() {
            return Ok(servers);
        }
        let root: toml::Table = text
            .parse()
            .map_err(|e| PluginError::Config(format!("解析 ~/.grok/config.toml 失败: {e}")))?;
        if let Some(entries) = root.get("mcp_servers").and_then(toml::Value::as_table) {
            for (id, entry) in entries {
                let Some(entry) = entry.as_table() else {
                    continue;
                };
                servers.push(McpServerSpec {
                    id: id.clone(),
                    name: id.clone(),
                    spec: toml_server_to_json(entry),
                });
            }
        }
        servers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(servers)
    }

    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError> {
        if !mcp_target_installed() {
            log::info!("Grok Build 未安装，跳过写入 MCP 服务器 '{}'", spec.id);
            return Ok(());
        }
        let path = config_path();
        let text = read_config_text()?;
        let mut doc = if text.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            text.parse::<toml_edit::DocumentMut>()
                .map_err(|e| PluginError::Config(format!("解析 Grok Build config.toml 失败: {e}")))?
        };
        if doc
            .get("mcp_servers")
            .is_some_and(|item| item.as_table_like().is_none())
        {
            log::warn!("Grok Build config.toml 的 mcp_servers 不是表，已重置为空表");
            doc.remove("mcp_servers");
        }
        if doc.get("mcp_servers").is_none() {
            doc["mcp_servers"] = toml_edit::table();
        }
        let servers = doc
            .get_mut("mcp_servers")
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(|| {
                PluginError::Config("Grok Build config.toml 的 mcp_servers 不是表".into())
            })?;
        servers.insert(
            spec.id.as_str(),
            toml_edit::Item::Table(json_server_to_grok_toml_table(&spec.spec)?),
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| PluginError::io(&path, e))
    }

    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError> {
        if !mcp_target_installed() {
            log::info!("Grok Build 未安装，跳过移除 MCP 服务器 '{id}'");
            return Ok(());
        }
        let path = config_path();
        if !path.exists() {
            return Ok(());
        }
        let text = read_config_text()?;
        let mut doc = match text.parse::<toml_edit::DocumentMut>() {
            Ok(doc) => doc,
            Err(e) => {
                log::warn!("解析 Grok Build config.toml 失败: {e}，跳过删除操作");
                return Ok(());
            }
        };
        if let Some(item) = doc.get_mut("mcp_servers") {
            let user_authored = !item.is_none();
            match item.as_table_like_mut() {
                Some(servers) => {
                    servers.remove(id);
                }
                None if user_authored => {
                    log::warn!("Grok Build config.toml 的 mcp_servers 不是表，无法删除 '{id}'");
                }
                None => {}
            }
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| PluginError::io(&path, e))
    }
}

// ============================================================================
// 会话（sessions/archived_sessions 下的 summary.json + chat_history.jsonl）
// ============================================================================

fn session_roots() -> Vec<PathBuf> {
    let dir = config_dir();
    vec![dir.join("sessions"), dir.join("archived_sessions")]
}

fn scan_sessions() -> Result<Vec<crate::plugin::SessionMeta>, PluginError> {
    let mut files = Vec::new();
    for root in session_roots() {
        collect_summary_files(&root, &mut files);
    }
    let mut sessions = Vec::new();
    for path in files {
        if let Some(meta) = parse_summary(&path) {
            sessions.push(meta);
        }
    }
    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(sessions)
}

fn collect_summary_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_summary_files(&path, files);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("summary.json") {
            files.push(path);
        }
    }
}

#[derive(serde::Deserialize)]
struct GrokSessionSummary {
    info: GrokSessionInfo,
    #[serde(default)]
    session_summary: Option<String>,
    #[serde(default)]
    generated_title: Option<String>,
    #[serde(default)]
    created_at: Option<Value>,
    #[serde(default)]
    updated_at: Option<Value>,
    #[serde(default)]
    last_active_at: Option<Value>,
}

#[derive(serde::Deserialize)]
struct GrokSessionInfo {
    id: String,
    #[serde(default)]
    cwd: Option<String>,
}

fn parse_summary(path: &Path) -> Option<crate::plugin::SessionMeta> {
    let text = std::fs::read_to_string(path).ok()?;
    let summary: GrokSessionSummary = serde_json::from_str(&text).ok()?;
    let session_id = summary.info.id;
    let title = summary
        .generated_title
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            summary
                .session_summary
                .as_deref()
                .filter(|v| !v.trim().is_empty())
        })
        .map(|v| truncate_summary(v, TITLE_MAX_CHARS));
    let created_at = summary.created_at.as_ref().and_then(parse_timestamp_to_ms);
    let last_active_at = summary
        .last_active_at
        .as_ref()
        .or(summary.updated_at.as_ref())
        .and_then(parse_timestamp_to_ms);

    Some(crate::plugin::SessionMeta {
        session_id: session_id.clone(),
        title,
        project_dir: summary.info.cwd,
        created_at,
        last_active_at: last_active_at.or(created_at),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("grok --resume {session_id}")),
    })
}

fn load_session_messages(
    summary_path: &Path,
) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
    use std::io::BufRead;

    let session_dir = summary_path
        .parent()
        .ok_or_else(|| PluginError::Config("Invalid Grok Build session path".into()))?;
    let chat_path = session_dir.join("chat_history.jsonl");
    let file = std::fs::File::open(&chat_path).map_err(|e| PluginError::io(&chat_path, e))?;
    let reader = std::io::BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let role = match kind {
            "system" | "user" | "assistant" | "tool" => kind,
            // reasoning 记录可能含加密内部状态，不是对话消息（对齐 v1）。
            _ => continue,
        };
        let content = value.get("content").map(extract_text).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let ts = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .and_then(parse_timestamp_to_ms);
        messages.push(crate::plugin::SessionMessage {
            role: role.to_string(),
            content,
            ts,
        });
    }
    Ok(messages)
}

fn delete_session_dir(summary_path: &Path, session_id: &str) -> Result<bool, PluginError> {
    let summary: GrokSessionSummary = serde_json::from_str(
        &std::fs::read_to_string(summary_path).map_err(|e| PluginError::io(summary_path, e))?,
    )
    .map_err(|e| PluginError::Config(format!("解析 Grok Build 会话 summary 失败: {e}")))?;
    if summary.info.id != session_id {
        return Err(PluginError::Config(format!(
            "Grok Build 会话 id 不匹配: 期望 {session_id}，实际 {}",
            summary.info.id
        )));
    }
    let session_dir = summary_path
        .parent()
        .ok_or_else(|| PluginError::Config("Invalid Grok Build session path".into()))?;
    if session_dir.file_name().and_then(|n| n.to_str()) != Some(session_id) {
        return Err(PluginError::Config(
            "Grok Build 会话目录名与会话 id 不一致".into(),
        ));
    }
    std::fs::remove_dir_all(session_dir).map_err(|e| PluginError::io(session_dir, e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_support::env_lock()
    }

    struct HomeGuard {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl HomeGuard {
        fn set(home: &Path) -> Self {
            let lock = env_lock().lock().unwrap();
            let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", home);
            Self {
                previous,
                _lock: lock,
            }
        }
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(v) => std::env::set_var("CC_SWITCH_TEST_HOME", v),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    fn valid_config() -> &'static str {
        r#"[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "grok-4.5"
base_url = "https://example.com/v1"
name = "Example"
api_key = "secret"
api_backend = "responses"
context_window = 500000
"#
    }

    fn provider(id: &str, category: &str, config_text: &str) -> Provider {
        Provider {
            id: id.to_string(),
            plugin_id: "grokbuild".to_string(),
            name: id.to_string(),
            category: category.to_string(),
            icon: None,
            website: None,
            api_key: None,
            settings_config: Some(
                serde_json::json!({ "config": config_text }).to_string(),
            ),
            meta: None,
            sort_order: 0,
            live_config_managed: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn apply_writes_custom_and_official_configs() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = GrokBuildPlugin::new();

        p.apply(&provider("grok", "custom", valid_config()), true)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(temp.path().join(".grok").join("config.toml")).unwrap(),
            valid_config()
        );

        // 官方条目：空 config 可写（回落官方 OAuth）
        p.apply(&provider("official", "official", ""), true).unwrap();
        assert_eq!(
            std::fs::read_to_string(temp.path().join(".grok").join("config.toml")).unwrap(),
            ""
        );

        // 非官方 + 空文档 → 强校验失败
        assert!(p.apply(&provider("bad", "custom", ""), true).is_err());
        // 非官方 + 缺凭据 → 失败
        let no_key = valid_config().replace("api_key = \"secret\"\n", "");
        assert!(p.apply(&provider("bad2", "custom", &no_key), true).is_err());

        // read_live + import
        p.apply(&provider("grok", "custom", valid_config()), true)
            .unwrap();
        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 1);
        let imported = p.import().unwrap();
        assert_eq!(imported.len(), 1);

        // 官方态（仅 mcp_servers）→ import 为空
        p.apply(&provider("official", "official", "[mcp_servers.echo]\ncommand = \"echo\"\n"), true)
            .unwrap();
        assert!(p.import().unwrap().is_empty());
    }

    #[test]
    fn mcp_roundtrip_grok_variant() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        std::fs::create_dir_all(temp.path().join(".grok")).unwrap();
        let p = GrokBuildPlugin::new();

        p.set_mcp_server(&McpServerSpec {
            id: "remote".into(),
            name: "remote".into(),
            spec: serde_json::json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "headers": { "Authorization": "Bearer t" }
            }),
        })
        .unwrap();

        // Grok 变体：无 type、headers（不是 http_headers）
        let config =
            std::fs::read_to_string(temp.path().join(".grok").join("config.toml")).unwrap();
        assert!(!config.contains("type ="));
        assert!(config.contains("headers"));

        let servers = p.get_mcp_servers().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].spec["type"], "http");
        assert_eq!(servers[0].spec["headers"]["Authorization"], "Bearer t");

        p.remove_mcp_server("remote").unwrap();
        assert!(p.get_mcp_servers().unwrap().is_empty());
    }

    #[test]
    fn mcp_set_skips_uninstalled_target() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = GrokBuildPlugin::new();
        p.set_mcp_server(&McpServerSpec {
            id: "fs".into(),
            name: "fs".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "npx" }),
        })
        .unwrap();
        assert!(!temp.path().join(".grok").exists());
    }

    #[test]
    fn sessions_scan_load_delete() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let session_id = "019f6af2-18b0-7673-958e-d25be650e172";
        let session_dir = temp
            .path()
            .join(".grok")
            .join("sessions")
            .join("encoded-project")
            .join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("summary.json"),
            format!(
                r#"{{"info":{{"id":"{session_id}","cwd":"C:/work"}},"session_summary":"hello grok","generated_title":"Grok session","created_at":"2026-07-16T12:00:00Z","last_active_at":"2026-07-16T12:00:01Z"}}"#
            ),
        )
        .unwrap();
        std::fs::write(
            session_dir.join("chat_history.jsonl"),
            concat!(
                "{\"type\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}\n",
                "{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"private\"}]}\n",
                "{\"type\":\"assistant\",\"content\":\"Hi there\"}\n"
            ),
        )
        .unwrap();

        let p = GrokBuildPlugin::new();
        let sessions = p.sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, session_id);
        assert_eq!(sessions[0].title.as_deref(), Some("Grok session"));
        assert_eq!(
            sessions[0].resume_command.as_deref(),
            Some(format!("grok --resume {session_id}").as_str())
        );

        let msgs = p
            .load_messages(sessions[0].source_path.as_deref().unwrap())
            .unwrap();
        assert_eq!(msgs.len(), 2, "reasoning 记录不是对话消息");
        assert_eq!(msgs[0].content, "hello");

        assert!(p.delete_session("wrong", sessions[0].source_path.as_deref().unwrap()).is_err());
        assert!(p
            .delete_session(session_id, sessions[0].source_path.as_deref().unwrap())
            .unwrap());
        assert!(!session_dir.exists());
    }
}
