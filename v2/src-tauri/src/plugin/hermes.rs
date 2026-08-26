//! 原生 Hermes 插件：读写 `~/.hermes/config.yaml`（additive `custom_providers` +
//! `model:` 切换默认项）、YAML `mcp_servers:`、`state.db` + `sessions/*.jsonl` 会话。
//!
//! 语义对齐 v1（apps/hermes.rs + hermes_config.rs + mcp/hermes.rs + session hermes）：
//! - 目录解析顺序：overrideDir.hermes → `HERMES_HOME` → 平台默认
//!   （Windows `%LOCALAPPDATA%\hermes`，其余 `~/.hermes`）；
//! - provider 写入 `custom_providers:` 序列（按 name upsert，保留盘上未知字段，
//!   models 数组 ↔ 字典归一化），并同步顶层 `model.provider/default`；
//! - `providers:` 字典（v12+）条目只读（`_cc_source` 标记，写/删报错）；
//! - YAML 采用 section 级替换以保留注释与无关段落（含 CRLF/重复键修复）。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::plugin::error::PluginError;
use crate::plugin::mcp::{McpPlugin, McpServerSpec};
use crate::plugin::session_utils::{
    extract_text, parse_timestamp_to_ms, read_head_tail_lines, truncate_summary, TITLE_MAX_CHARS,
};
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities};
use crate::types::Provider;

const PLUGIN_ID: &str = "hermes";

/// v12+ `providers:` 字典条目的只读标记（对齐 v1 PROVIDER_SOURCE_FIELD）。
const PROVIDER_SOURCE_FIELD: &str = "_cc_source";
const PROVIDER_SOURCE_CUSTOM_LIST: &str = "custom_providers";
const PROVIDER_SOURCE_DICT: &str = "providers_dict";

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

/// Hermes 目录（对齐 v1 get_hermes_dir 的解析顺序）。
fn hermes_dir() -> PathBuf {
    if let Some(dir) = crate::services::overrides::get(PLUGIN_ID) {
        return dir;
    }
    if let Some(dir) = override_dir("HERMES_HOME") {
        return dir;
    }
    default_hermes_dir()
}

#[cfg(target_os = "windows")]
fn default_hermes_dir() -> PathBuf {
    // %LOCALAPPDATA%\hermes，缺失回退 <home>\AppData\Local\hermes（对齐 Hermes 自身）。
    std::env::var_os("LOCALAPPDATA")
        .map(|v| v.to_string_lossy().trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
        .join("hermes")
}

#[cfg(not(target_os = "windows"))]
fn default_hermes_dir() -> PathBuf {
    home_dir().join(".hermes")
}

fn config_path() -> PathBuf {
    hermes_dir().join("config.yaml")
}

// ============================================================================
// YAML 读写（section 级替换，保留注释；移植 v1 hermes_config.rs 核心算法）
// ============================================================================

fn read_config_yaml() -> Result<serde_yaml::Value, PluginError> {
    let path = config_path();
    if !path.exists() {
        return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))?;
    if content.trim().is_empty() {
        return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let deduped = deduplicate_top_level_keys(&content);
    serde_yaml::from_str(&deduped)
        .map_err(|e| PluginError::Config(format!("解析 Hermes config.yaml 失败: {e}")))
}

/// 顶层键行判定：列 0 起始、非空/注释/序列项、含 `:` 后跟空白或行尾（兼容 CRLF）。
fn is_top_level_key_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let first = line.as_bytes()[0];
    if first == b' ' || first == b'\t' || first == b'#' || first == b'-' {
        return false;
    }
    if let Some(pos) = line.find(':') {
        let after = &line[pos + 1..];
        after.is_empty() || after.starts_with([' ', '\t', '\r', '\n'])
    } else {
        false
    }
}

/// 定位顶层 section 的字节区间 [start, end)。
fn find_yaml_section_range(raw: &str, section_key: &str) -> Option<(usize, usize)> {
    let target = format!("{section_key}:");
    let mut section_start = None;
    let mut offset = 0;

    for line in raw.split('\n') {
        if section_start.is_none() && is_top_level_key_line(line) && line.starts_with(&target) {
            let after_target = &line[target.len()..];
            if after_target.is_empty()
                || after_target.starts_with(' ')
                || after_target.starts_with('\t')
                || after_target.starts_with('\r')
            {
                section_start = Some(offset);
            }
        } else if section_start.is_some() && is_top_level_key_line(line) {
            return Some((section_start.unwrap(), offset));
        }
        offset += line.len() + 1;
    }

    section_start.map(|start| (start, raw.len()))
}

/// 移除 raw 中所有该键的 section（替换后清理历史重复副本）。
fn remove_all_sections(raw: &str, section_key: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some((start, end)) = find_yaml_section_range(rest, section_key) {
        result.push_str(&rest[..start]);
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

/// 替换顶层 section，不存在则追加。
fn replace_yaml_section(
    raw: &str,
    section_key: &str,
    value: &serde_yaml::Value,
) -> Result<String, PluginError> {
    let mut section = serde_yaml::Mapping::new();
    section.insert(
        serde_yaml::Value::String(section_key.to_string()),
        value.clone(),
    );
    let serialized = serde_yaml::to_string(&serde_yaml::Value::Mapping(section))
        .map_err(|e| PluginError::Config(format!("序列化 YAML section '{section_key}' 失败: {e}")))?;

    if let Some((start, end)) = find_yaml_section_range(raw, section_key) {
        let mut result = String::with_capacity(raw.len());
        result.push_str(&raw[..start]);
        result.push_str(&serialized);
        let remainder = remove_all_sections(&raw[end..], section_key);
        if !serialized.ends_with('\n') && !remainder.is_empty() && !remainder.starts_with('\n') {
            result.push('\n');
        }
        result.push_str(&remainder);
        Ok(result)
    } else {
        let mut result = raw.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&serialized);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }
}

/// 修复历史「替换退化成追加」留下的顶层重复键：保留最后一次出现（PyYAML last-wins）。
fn deduplicate_top_level_keys(raw: &str) -> String {
    let mut sections: Vec<(&str, usize)> = Vec::new();
    let mut offset = 0;
    for line in raw.split('\n') {
        if is_top_level_key_line(line) {
            if let Some(pos) = line.find(':') {
                sections.push((&line[..pos], offset));
            }
        }
        offset += line.len() + 1;
    }

    let mut remaining: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (key, _) in &sections {
        *remaining.entry(key).or_insert(0) += 1;
    }
    if remaining.values().all(|&count| count <= 1) {
        return raw.to_string();
    }

    let mut result = String::with_capacity(raw.len());
    let head_end = sections
        .first()
        .map(|&(_, start)| start)
        .unwrap_or(raw.len());
    result.push_str(&raw[..head_end]);

    for (i, &(key, start)) in sections.iter().enumerate() {
        let end = sections
            .get(i + 1)
            .map(|&(_, next)| next)
            .unwrap_or(raw.len());
        let count = remaining.get_mut(key).expect("key collected");
        *count -= 1;
        if *count > 0 {
            log::warn!("Hermes config: 丢弃重复顶层 section '{key}'（保留最后一次）");
            continue;
        }
        result.push_str(&raw[start..end]);
    }
    result
}

/// section 级替换写回 config.yaml。
fn write_yaml_section(section_key: &str, value: &serde_yaml::Value) -> Result<(), PluginError> {
    let path = config_path();
    let raw = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))?
    } else {
        String::new()
    };
    let new_raw = replace_yaml_section(&raw, section_key, value)?;
    if new_raw == raw {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
    }
    std::fs::write(&path, new_raw).map_err(|e| PluginError::io(&path, e))
}

fn yaml_to_json(yaml: &serde_yaml::Value) -> Result<Value, PluginError> {
    let yaml_str = serde_yaml::to_string(yaml)
        .map_err(|e| PluginError::Config(format!("YAML 序列化失败: {e}")))?;
    serde_yaml::from_str::<Value>(&yaml_str)
        .map_err(|e| PluginError::Config(format!("YAML → JSON 转换失败: {e}")))
}

fn json_to_yaml(json: &Value) -> Result<serde_yaml::Value, PluginError> {
    let json_str =
        serde_json::to_string(json).map_err(|e| PluginError::Config(format!("JSON 序列化失败: {e}")))?;
    serde_yaml::from_str(&json_str)
        .map_err(|e| PluginError::Config(format!("JSON → YAML 转换失败: {e}")))
}

// ============================================================================
// Provider（additive：custom_providers 序列 + providers 字典只读）
// ============================================================================

/// 历史 camelCase 键改写为 Hermes snake_case（对齐 v1 sanitize）。
fn sanitize_provider_keys(config: &mut Value) {
    const KEY_ALIASES: &[(&str, &str)] = &[
        ("baseUrl", "base_url"),
        ("apiKey", "api_key"),
        ("apiMode", "api_mode"),
        ("maxTokens", "max_tokens"),
        ("contextLength", "context_length"),
    ];
    const LEGACY_DROP: &[&str] = &["api", PROVIDER_SOURCE_FIELD, "provider_key"];

    let Some(obj) = config.as_object_mut() else {
        return;
    };
    for (from, to) in KEY_ALIASES {
        if let Some(val) = obj.remove(*from) {
            obj.entry((*to).to_string()).or_insert(val);
        }
    }
    for field in LEGACY_DROP {
        obj.remove(*field);
    }
}

/// models UI 数组 → YAML 字典（id 提升为键）。
fn models_array_to_dict(array: Vec<Value>) -> Value {
    let mut map = serde_json::Map::new();
    for item in array {
        let Value::Object(mut obj) = item else {
            continue;
        };
        let Some(id) = obj
            .remove("id")
            .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        map.insert(id, Value::Object(obj));
    }
    Value::Object(map)
}

/// models YAML 字典 → UI 数组（id 注回字段）。
fn models_dict_to_array(dict: serde_json::Map<String, Value>) -> Value {
    let mut out = Vec::with_capacity(dict.len());
    for (id, value) in dict {
        let mut obj = match value {
            Value::Object(obj) => obj,
            Value::Null => serde_json::Map::new(),
            other => {
                log::warn!("Hermes model 条目 '{id}' 形状异常: {other:?}，跳过");
                continue;
            }
        };
        obj.insert("id".to_string(), Value::String(id));
        out.push(Value::Object(obj));
    }
    Value::Array(out)
}

fn normalize_models_for_write(config: &mut Value) {
    let Some(obj) = config.as_object_mut() else {
        return;
    };
    if let Some(models) = obj.get_mut("models") {
        if models.is_array() {
            let taken = std::mem::take(models);
            if let Value::Array(arr) = taken {
                *models = models_array_to_dict(arr);
            }
        }
    }
}

fn denormalize_models_for_read(config: &mut Value) {
    let Some(obj) = config.as_object_mut() else {
        return;
    };
    if let Some(models) = obj.get_mut("models") {
        if models.is_object() {
            let taken = std::mem::take(models);
            if let Value::Object(map) = taken {
                *models = models_dict_to_array(map);
            }
        }
    }
}

/// 全部 provider：custom_providers 列表（可写）∪ providers 字典（只读，同名时列表优先）。
fn get_providers() -> Result<serde_json::Map<String, Value>, PluginError> {
    let config = read_config_yaml()?;
    let mut map = serde_json::Map::new();

    if let Some(seq) = config.get("custom_providers").and_then(|v| v.as_sequence()) {
        for item in seq {
            let Some(name) = item.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            match yaml_to_json(item) {
                Ok(mut json_val) => {
                    sanitize_provider_keys(&mut json_val);
                    denormalize_models_for_read(&mut json_val);
                    if let Some(obj) = json_val.as_object_mut() {
                        obj.insert(
                            PROVIDER_SOURCE_FIELD.to_string(),
                            json!(PROVIDER_SOURCE_CUSTOM_LIST),
                        );
                    }
                    map.insert(name.to_string(), json_val);
                }
                Err(e) => log::warn!("Hermes provider '{name}' 转 JSON 失败: {e}"),
            }
        }
    }

    if let Some(dict) = config.get("providers").and_then(|v| v.as_mapping()) {
        for (k, v) in dict {
            let Some(key) = k.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };
            if !v.is_mapping() {
                continue;
            }
            let mut json_val = yaml_to_json(v)?;
            let Some(obj) = json_val.as_object_mut() else {
                continue;
            };
            let resolved_name = obj
                .get("name")
                .and_then(|n| n.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(key)
                .to_string();
            if resolved_name.is_empty() || map.contains_key(&resolved_name) {
                continue;
            }
            obj.insert("name".to_string(), json!(resolved_name));
            obj.insert(PROVIDER_SOURCE_FIELD.to_string(), json!(PROVIDER_SOURCE_DICT));
            denormalize_models_for_read(&mut json_val);
            map.insert(resolved_name, json_val);
        }
    }

    Ok(map)
}

/// dict-only 条目不可经 CC Switch 写/删（需走 Hermes Web UI）。
fn ensure_provider_writable(config: &serde_yaml::Value, name: &str, verb: &str) -> Result<(), PluginError> {
    let list_has = config
        .get("custom_providers")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .any(|item| item.get("name").and_then(|n| n.as_str()) == Some(name))
        })
        .unwrap_or(false);
    if list_has {
        return Ok(());
    }
    let dict_has = config
        .get("providers")
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.iter().any(|(k, v)| {
                let key_matches = k.as_str() == Some(name);
                let name_matches = v
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s == name)
                    .unwrap_or(false);
                (key_matches || name_matches) && v.is_mapping()
            })
        })
        .unwrap_or(false);
    if dict_has {
        return Err(PluginError::Config(format!(
            "Provider '{name}' 由 Hermes 'providers:' 字典管理 —— 请经 Hermes Web UI {verb}"
        )));
    }
    Ok(())
}

/// 原生 Hermes 插件。
#[derive(Debug, Default)]
pub struct HermesPlugin;

impl HermesPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl AgentPlugin for HermesPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn capabilities(&self) -> &PluginCapabilities {
        static CAPS: PluginCapabilities = PluginCapabilities {
            read_live: true,
            apply: true,
            remove: true,
            import: true,
            sessions: true,
            mcp: true,
        };
        &CAPS
    }

    fn read_live(&self) -> Result<LiveConfig, PluginError> {
        let providers_map = get_providers()?;
        let mut providers: Vec<LiveProvider> = providers_map
            .into_iter()
            .map(|(id, settings)| LiveProvider {
                name: id.clone(),
                id,
                settings_config: settings,
            })
            .collect();
        providers.sort_by(|a, b| a.id.cmp(&b.id));

        let current = read_config_yaml()?
            .get("model")
            .and_then(|m| m.get("provider"))
            .and_then(|p| p.as_str())
            .map(String::from);

        Ok(LiveConfig {
            providers,
            current,
        })
    }

    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError> {
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
        if raw.get(PROVIDER_SOURCE_FIELD).and_then(Value::as_str) == Some(PROVIDER_SOURCE_DICT) {
            return Err(PluginError::Config(format!(
                "Provider '{}' 由 Hermes 'providers:' 字典管理 —— 请经 Hermes Web UI 编辑",
                provider.id
            )));
        }

        // upsert custom_providers 条目
        let config = read_config_yaml()?;
        ensure_provider_writable(&config, &provider.id, "编辑")?;
        let mut providers_seq: Vec<serde_yaml::Value> = config
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .cloned()
            .unwrap_or_default();

        let mut normalized = raw.clone();
        sanitize_provider_keys(&mut normalized);
        normalize_models_for_write(&mut normalized);

        let first_model_id = normalized
            .get("models")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.keys().next())
            .cloned();

        let mut yaml_val = json_to_yaml(&normalized)?;
        if let serde_yaml::Value::Mapping(ref mut m) = yaml_val {
            m.insert(
                serde_yaml::Value::String("name".to_string()),
                serde_yaml::Value::String(provider.id.clone()),
            );
            if let Some(model_id) = first_model_id.clone() {
                m.insert(
                    serde_yaml::Value::String("model".to_string()),
                    serde_yaml::Value::String(model_id),
                );
            } else {
                m.remove(serde_yaml::Value::String("model".to_string()));
            }
        }

        if let Some(existing) = providers_seq
            .iter_mut()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(provider.id.as_str()))
        {
            // 前向兼容：保留 UI 负载未包含的盘上字段（如 request_timeout_seconds）。
            if let (Some(existing_map), serde_yaml::Value::Mapping(new_map)) =
                (existing.as_mapping(), &mut yaml_val)
            {
                for (k, v) in existing_map {
                    new_map.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            *existing = yaml_val;
        } else {
            providers_seq.push(yaml_val);
        }
        write_yaml_section(
            "custom_providers",
            &serde_yaml::Value::Sequence(providers_seq),
        )?;

        // 切换语义：同步顶层 model.provider / model.default
        if current {
            let first_model = first_model_id.or_else(|| {
                normalized
                    .get("models")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|m| m.get("id"))
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            });
            let mut model_section = match read_config_yaml()?.get("model") {
                Some(serde_yaml::Value::Mapping(m)) => m.clone(),
                _ => serde_yaml::Mapping::new(),
            };
            model_section.insert(
                serde_yaml::Value::String("provider".to_string()),
                serde_yaml::Value::String(provider.id.clone()),
            );
            if let Some(default) = first_model {
                model_section.insert(
                    serde_yaml::Value::String("default".to_string()),
                    serde_yaml::Value::String(default),
                );
            }
            write_yaml_section("model", &serde_yaml::Value::Mapping(model_section))?;
        }
        Ok(())
    }

    fn remove_provider(&self, id: &str) -> Result<(), PluginError> {
        let config = read_config_yaml()?;
        ensure_provider_writable(&config, id, "移除")?;

        let mut providers_seq: Vec<serde_yaml::Value> = config
            .get("custom_providers")
            .and_then(|v| v.as_sequence())
            .cloned()
            .unwrap_or_default();
        let original_len = providers_seq.len();
        providers_seq.retain(|p| p.get("name").and_then(|n| n.as_str()) != Some(id));
        if providers_seq.len() == original_len {
            return Ok(());
        }
        write_yaml_section(
            "custom_providers",
            &serde_yaml::Value::Sequence(providers_seq),
        )
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let map = get_providers()?;
        let mut candidates: Vec<ImportCandidate> = map
            .into_iter()
            .map(|(id, settings)| ImportCandidate {
                name: id.clone(),
                id,
                settings_config: settings,
            })
            .collect();
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(candidates)
    }

    fn sessions(&self) -> Result<Vec<crate::plugin::SessionMeta>, PluginError> {
        Ok(scan_sessions())
    }

    fn load_messages(
        &self,
        source: &str,
    ) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
        if let Some(rest) = source.strip_prefix("sqlite:") {
            return load_messages_sqlite(rest);
        }
        load_messages_jsonl(Path::new(source))
    }

    fn delete_session(&self, session_id: &str, source: &str) -> Result<bool, PluginError> {
        if let Some(rest) = source.strip_prefix("sqlite:") {
            return delete_session_sqlite(session_id, rest);
        }
        std::fs::remove_file(Path::new(source)).map_err(|e| PluginError::io(Path::new(source), e))?;
        Ok(true)
    }

    fn as_mcp(&self) -> Option<&dyn McpPlugin> {
        Some(self)
    }

    fn prompt_file_path(&self) -> Option<PathBuf> {
        Some(hermes_dir().join("AGENTS.md"))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        Some(hermes_dir().join("skills"))
    }

    fn read_raw_config(&self) -> Result<String, PluginError> {
        let path = config_path();
        if !path.exists() {
            return Ok(String::new());
        }
        std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))
    }

    fn write_raw_config(&self, content: &str) -> Result<(), PluginError> {
        // 全文件写回前必须可解析（防手滑写坏）。
        if !content.trim().is_empty() {
            serde_yaml::from_str::<serde_yaml::Value>(content)
                .map_err(|e| PluginError::Config(format!("config.yaml 解析失败: {e}")))?;
        }
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, content).map_err(|e| PluginError::io(&path, e))
    }
}

// ============================================================================
// MCP（config.yaml `mcp_servers:` YAML mapping）
// ============================================================================

fn mcp_target_installed() -> bool {
    crate::services::overrides::get(PLUGIN_ID).is_some()
        || override_dir("HERMES_HOME").is_some()
        || hermes_dir().exists()
}

/// 统一 spec → Hermes 条目（strip type，加 enabled: true）。
fn convert_to_hermes_format(spec: &Value) -> Result<Value, PluginError> {
    let obj = spec
        .as_object()
        .ok_or_else(|| PluginError::Config("MCP spec 必须是 JSON 对象".into()))?;
    let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");

    let mut result = serde_json::Map::new();
    match typ {
        "stdio" => {
            if let Some(command) = obj.get("command") {
                result.insert("command".into(), command.clone());
            }
            if let Some(args) = obj.get("args") {
                if args.as_array().is_some_and(|a| !a.is_empty()) {
                    result.insert("args".into(), args.clone());
                }
            }
            if let Some(env) = obj.get("env") {
                if env.as_object().is_some_and(|o| !o.is_empty()) {
                    result.insert("env".into(), env.clone());
                }
            }
        }
        "sse" | "http" => {
            if let Some(url) = obj.get("url") {
                result.insert("url".into(), url.clone());
            }
            if let Some(headers) = obj.get("headers") {
                if headers.as_object().is_some_and(|o| !o.is_empty()) {
                    result.insert("headers".into(), headers.clone());
                }
            }
        }
        _ => return Err(PluginError::Config(format!("未知 MCP 类型: {typ}"))),
    }
    result.insert("enabled".into(), json!(true));
    Ok(Value::Object(result))
}

/// Hermes 条目 → 统一 spec（command→stdio / url→sse，剥离 enabled 等特有字段）。
fn convert_from_hermes_format(id: &str, spec: &Value) -> Result<Value, PluginError> {
    let obj = spec
        .as_object()
        .ok_or_else(|| PluginError::Config("Hermes MCP spec 必须是 JSON 对象".into()))?;
    let mut result = serde_json::Map::new();
    if obj.contains_key("command") {
        result.insert("type".into(), json!("stdio"));
        for key in ["command", "args", "env"] {
            if let Some(v) = obj.get(key) {
                result.insert(key.into(), v.clone());
            }
        }
    } else if obj.contains_key("url") {
        result.insert("type".into(), json!("sse"));
        for key in ["url", "headers"] {
            if let Some(v) = obj.get(key) {
                result.insert(key.into(), v.clone());
            }
        }
    } else {
        return Err(PluginError::Config(format!(
            "Hermes MCP 服务器 '{id}' 缺少 command/url 字段"
        )));
    }
    Ok(Value::Object(result))
}

impl McpPlugin for HermesPlugin {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError> {
        let config = read_config_yaml()?;
        let Some(servers) = config.get("mcp_servers").and_then(|v| v.as_mapping()) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (k, v) in servers {
            let Some(id) = k.as_str() else {
                continue;
            };
            let Ok(json_val) = yaml_to_json(v) else {
                continue;
            };
            let spec = match convert_from_hermes_format(id, &json_val) {
                Ok(spec) => spec,
                Err(e) => {
                    log::warn!("跳过无效 Hermes MCP 项 '{id}': {e}");
                    continue;
                }
            };
            out.push(McpServerSpec {
                id: id.to_string(),
                name: id.to_string(),
                spec,
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError> {
        if !mcp_target_installed() {
            log::info!("Hermes 未安装，跳过写入 MCP 服务器 '{}'", spec.id);
            return Ok(());
        }
        let hermes_spec = convert_to_hermes_format(&spec.spec)?;
        let config = read_config_yaml()?;
        let mut servers = match config.get("mcp_servers") {
            Some(serde_yaml::Value::Mapping(m)) => m.clone(),
            _ => serde_yaml::Mapping::new(),
        };
        let id_yaml = serde_yaml::Value::String(spec.id.clone());
        // merge-on-write：保留 Hermes 特有字段（enabled/tools/sampling 等）。
        let merged = if let Some(existing) = servers.get(&id_yaml) {
            let existing_json = yaml_to_json(existing)?;
            let mut merged = convert_to_hermes_format(&spec.spec)?;
            if let (Some(existing_obj), Some(new_obj)) =
                (existing_json.as_object(), merged.as_object_mut())
            {
                for (k, v) in existing_obj {
                    new_obj.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            merged
        } else {
            hermes_spec
        };
        servers.insert(id_yaml, json_to_yaml(&merged)?);
        write_yaml_section("mcp_servers", &serde_yaml::Value::Mapping(servers))
    }

    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError> {
        if !mcp_target_installed() {
            log::info!("Hermes 未安装，跳过移除 MCP 服务器 '{id}'");
            return Ok(());
        }
        let config = read_config_yaml()?;
        let mut servers = match config.get("mcp_servers") {
            Some(serde_yaml::Value::Mapping(m)) => m.clone(),
            _ => return Ok(()),
        };
        if servers
            .remove(serde_yaml::Value::String(id.to_string()))
            .is_none()
        {
            return Ok(());
        }
        write_yaml_section("mcp_servers", &serde_yaml::Value::Mapping(servers))
    }
}

// ============================================================================
// 会话（state.db sqlite 优先 + sessions/*.jsonl，ID 冲突时 sqlite 优先）
// ============================================================================

fn db_path() -> PathBuf {
    hermes_dir().join("state.db")
}

fn sessions_dir() -> PathBuf {
    hermes_dir().join("sessions")
}

fn scan_sessions() -> Vec<crate::plugin::SessionMeta> {
    let sqlite = scan_sessions_sqlite();
    let jsonl = scan_sessions_jsonl();
    if sqlite.is_empty() {
        return jsonl;
    }
    if jsonl.is_empty() {
        return sqlite;
    }
    let ids: std::collections::HashSet<String> =
        sqlite.iter().map(|s| s.session_id.clone()).collect();
    let mut merged = sqlite;
    for s in jsonl {
        if !ids.contains(&s.session_id) {
            merged.push(s);
        }
    }
    merged
}

fn scan_sessions_sqlite() -> Vec<crate::plugin::SessionMeta> {
    let path = db_path();
    if !path.exists() {
        return Vec::new();
    }
    let conn = match Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let has_sessions: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='sessions'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !has_sessions {
        return Vec::new();
    }

    let columns = table_columns(&conn, "sessions");
    let Ok(mut stmt) = conn.prepare("SELECT * FROM sessions ORDER BY rowid DESC LIMIT 500") else {
        return Vec::new();
    };
    let db_source = format!("sqlite:{}", path.display());
    let Ok(rows) = stmt.query_map([], |row| {
        let mut map = serde_json::Map::new();
        for (i, col) in columns.iter().enumerate() {
            let value = if let Ok(v) = row.get::<_, String>(i) {
                Value::String(v)
            } else if let Ok(v) = row.get::<_, i64>(i) {
                Value::Number(v.into())
            } else if let Ok(v) = row.get::<_, f64>(i) {
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            map.insert(col.clone(), value);
        }
        Ok(Value::Object(map))
    }) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    for row in rows.flatten() {
        let Some(obj) = row.as_object() else {
            continue;
        };
        let Some(session_id) = obj.get("id").and_then(Value::as_str).map(String::from) else {
            continue;
        };
        let title = obj
            .get("title")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| truncate_summary(s, TITLE_MAX_CHARS));
        let project_dir = obj
            .get("cwd")
            .or_else(|| obj.get("directory"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let started_at = obj
            .get("started_at")
            .or_else(|| obj.get("created_at"))
            .and_then(parse_timestamp_to_ms);
        let ended_at = obj
            .get("ended_at")
            .or_else(|| obj.get("updated_at"))
            .and_then(parse_timestamp_to_ms);
        sessions.push(crate::plugin::SessionMeta {
            session_id,
            title,
            project_dir,
            created_at: started_at,
            last_active_at: ended_at.or(started_at),
            source_path: Some(format!("{db_source}#{}", obj.get("id").and_then(Value::as_str).unwrap_or(""))),
            resume_command: None,
        });
    }
    sessions
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn scan_sessions_jsonl() -> Vec<crate::plugin::SessionMeta> {
    let dir = sessions_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("jsonl") && ext != Some("json") {
            continue;
        }
        if let Some(meta) = parse_jsonl_session(&path) {
            sessions.push(meta);
        }
    }
    sessions
}

fn parse_jsonl_session(path: &Path) -> Option<crate::plugin::SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 30, 10).ok()?;

    let mut first_user_msg: Option<String> = None;
    let mut first_ts: Option<i64> = None;
    let mut last_ts: Option<i64> = None;
    let mut session_id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut cwd: Option<String> = None;

    for line in &head {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .and_then(parse_timestamp_to_ms);
        if first_ts.is_none() {
            first_ts = ts;
        }
        last_ts = ts.or(last_ts);

        let line_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        if line_type == "session" || line_type == "init" {
            if session_id.is_none() {
                session_id = value
                    .get("id")
                    .or_else(|| value.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(String::from);
            }
            if title.is_none() {
                title = value
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(String::from);
            }
            if cwd.is_none() {
                cwd = value
                    .get("cwd")
                    .or_else(|| value.get("directory"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(String::from);
            }
        }

        if first_user_msg.is_none() {
            let role = value
                .get("role")
                .or_else(|| value.get("message").and_then(|m| m.get("role")))
                .and_then(Value::as_str);
            if role == Some("user") {
                let content = value
                    .get("content")
                    .or_else(|| value.get("message").and_then(|m| m.get("content")));
                if let Some(c) = content {
                    let text = extract_text(c);
                    if !text.trim().is_empty() {
                        first_user_msg =
                            Some(truncate_summary(&text, TITLE_MAX_CHARS));
                    }
                }
            }
        }
    }

    for line in tail.iter().rev() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(ts) = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .and_then(parse_timestamp_to_ms)
        {
            last_ts = Some(ts);
            break;
        }
    }

    let session_id = session_id.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    Some(crate::plugin::SessionMeta {
        session_id,
        title: title.or_else(|| first_user_msg.clone()),
        project_dir: cwd,
        created_at: first_ts,
        last_active_at: last_ts.or(first_ts),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: None,
    })
}

fn load_messages_jsonl(path: &Path) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).map_err(|e| PluginError::io(path, e))?;
    let reader = std::io::BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let (role_val, content_val, ts_val) =
            if value.get("type").and_then(Value::as_str) == Some("message") {
                let Some(msg) = value.get("message") else {
                    continue;
                };
                (
                    msg.get("role"),
                    msg.get("content"),
                    value.get("timestamp").or_else(|| msg.get("ts")),
                )
            } else {
                (
                    value.get("role"),
                    value.get("content"),
                    value.get("timestamp").or_else(|| value.get("ts")),
                )
            };
        let Some(role) = role_val.and_then(Value::as_str) else {
            continue;
        };
        let content = content_val.map(extract_text).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let ts = ts_val.and_then(parse_timestamp_to_ms);
        messages.push(crate::plugin::SessionMessage {
            role: role.to_string(),
            content,
            ts,
        });
    }
    Ok(messages)
}

fn load_messages_sqlite(rest: &str) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
    let Some((db, session_id)) = parse_sqlite_source(rest) else {
        return Err(PluginError::Config(format!(
            "Invalid SQLite source reference: {rest}"
        )));
    };
    let conn = Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| PluginError::Config(format!("打开 Hermes 数据库失败: {e}")))?;
    let mut stmt = conn
        .prepare("SELECT role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY created_at ASC")
        .map_err(|e| PluginError::Config(format!("查询 Hermes 消息失败: {e}")))?;
    let rows = stmt
        .query_map([session_id.as_str()], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let ts: Option<i64> = row.get(2).ok();
            Ok((role, content, ts))
        })
        .map_err(|e| PluginError::Config(format!("查询 Hermes 消息失败: {e}")))?;
    let mut messages = Vec::new();
    for row in rows.flatten() {
        let (role, content, ts) = row;
        if content.trim().is_empty() {
            continue;
        }
        let ts_ms = ts.and_then(|v| parse_timestamp_to_ms(&Value::Number(v.into())));
        messages.push(crate::plugin::SessionMessage {
            role,
            content,
            ts: ts_ms,
        });
    }
    Ok(messages)
}

fn delete_session_sqlite(session_id: &str, rest: &str) -> Result<bool, PluginError> {
    let Some((db, ref_id)) = parse_sqlite_source(rest) else {
        return Err(PluginError::Config(format!(
            "Invalid SQLite source reference: {rest}"
        )));
    };
    if ref_id != session_id {
        return Err(PluginError::Config(format!(
            "Hermes 会话 id 不匹配: 期望 {session_id}，实际 {ref_id}"
        )));
    }
    // 防误删：sqlite: 源必须指向当前 Hermes 目录的 state.db。
    let db = db.canonicalize().map_err(|e| PluginError::io(&db, e))?;
    let expected = db_path()
        .canonicalize()
        .map_err(|e| PluginError::io(db_path(), e))?;
    if db != expected {
        return Err(PluginError::Config(
            "SQLite 路径与当前 Hermes 数据库不一致".into(),
        ));
    }

    let conn = Connection::open(&db)
        .map_err(|e| PluginError::Config(format!("打开 Hermes 数据库失败: {e}")))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| PluginError::Other(format!("开启事务失败: {e}")))?;
    let _ = tx.execute("DELETE FROM messages WHERE session_id = ?1", [session_id]);
    let deleted = tx
        .execute("DELETE FROM sessions WHERE id = ?1", [session_id])
        .map_err(|e| PluginError::Other(format!("删除 Hermes 会话失败: {e}")))?;
    tx.commit()
        .map_err(|e| PluginError::Other(format!("提交删除失败: {e}")))?;
    Ok(deleted > 0)
}

/// 解析 `sqlite:<db 路径>#<session id>` 中的路径与 id。
fn parse_sqlite_source(rest: &str) -> Option<(PathBuf, String)> {
    let hash_pos = rest.rfind('#')?;
    let db_path = PathBuf::from(&rest[..hash_pos]);
    let session_id = rest[hash_pos + 1..].to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((db_path, session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_support::env_lock()
    }

    struct HermesGuard {
        previous: (Option<std::ffi::OsString>, Option<std::ffi::OsString>, Option<std::ffi::OsString>),
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl HermesGuard {
        fn set(home: &Path) -> Self {
            let lock = env_lock().lock().unwrap();
            let previous = (
                std::env::var_os("CC_SWITCH_TEST_HOME"),
                std::env::var_os("HERMES_HOME"),
                std::env::var_os("LOCALAPPDATA"),
            );
            std::env::set_var("CC_SWITCH_TEST_HOME", home);
            // 中和环境变量，避免测试逃逸到真实 Hermes 安装目录。
            std::env::remove_var("HERMES_HOME");
            std::env::remove_var("LOCALAPPDATA");
            Self {
                previous,
                _lock: lock,
            }
        }
    }
    impl Drop for HermesGuard {
        fn drop(&mut self) {
            match self.previous.0.take() {
                Some(v) => std::env::set_var("CC_SWITCH_TEST_HOME", v),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
            match self.previous.1.take() {
                Some(v) => std::env::set_var("HERMES_HOME", v),
                None => std::env::remove_var("HERMES_HOME"),
            }
            match self.previous.2.take() {
                Some(v) => std::env::set_var("LOCALAPPDATA", v),
                None => std::env::remove_var("LOCALAPPDATA"),
            }
        }
    }

    fn provider(id: &str, settings: Value) -> Provider {
        Provider {
            id: id.to_string(),
            plugin_id: "hermes".to_string(),
            name: id.to_string(),
            category: "custom".to_string(),
            icon: None,
            website: None,
            api_key: None,
            settings_config: Some(settings.to_string()),
            meta: None,
            sort_order: 0,
            live_config_managed: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn provider_apply_read_remove_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HermesGuard::set(temp.path());
        let p = HermesPlugin::new();

        let settings = serde_json::json!({
            "base_url": "https://openrouter.ai/api/v1",
            "api_key": "sk-or",
            "models": [{ "id": "anthropic/claude-opus-4-8", "context_length": 200000 }]
        });
        p.apply(&provider("openrouter", settings), true).unwrap();

        let config = std::fs::read_to_string(hermes_dir().join("config.yaml")).unwrap();
        assert!(config.contains("custom_providers:"));
        assert!(config.contains("base_url: https://openrouter.ai/api/v1"));
        // models 数组归一化为字典 + 单数 model 字段
        assert!(config.contains("model: anthropic/claude-opus-4-8"));
        // 切换语义：model.provider 同步
        assert!(config.contains("provider: openrouter"));

        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 1);
        assert_eq!(live.providers[0].id, "openrouter");
        assert_eq!(live.current.as_deref(), Some("openrouter"));
        assert_eq!(live.providers[0].settings_config["api_key"], "sk-or");

        // import：每条 custom_provider 一个候选
        let imported = p.import().unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].id, "openrouter");

        // remove
        p.remove_provider("openrouter").unwrap();
        assert!(p.read_live().unwrap().providers.is_empty());
    }

    #[test]
    fn yaml_section_replacement_preserves_other_sections_and_comments() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HermesGuard::set(temp.path());
        let dir = hermes_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.yaml"),
            "# top comment\nagent:\n  max_turns: 50\nmcp_servers:\n  fs:\n    command: npx\n",
        )
        .unwrap();

        let p = HermesPlugin::new();
        p.apply(
            &provider(
                "prov",
                serde_json::json!({ "base_url": "https://x/v1", "api_key": "k" }),
            ),
            true,
        )
        .unwrap();

        let config = std::fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(config.contains("# top comment"));
        assert!(config.contains("max_turns: 50"));
        assert!(config.contains("command: npx"));
        assert!(config.contains("custom_providers:"));
    }

    #[test]
    fn dict_only_provider_rejects_write_and_remove() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HermesGuard::set(temp.path());
        let dir = hermes_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.yaml"),
            "providers:\n  builtin:\n    base_url: https://x/v1\n",
        )
        .unwrap();

        let p = HermesPlugin::new();
        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 1);
        // 只读标记
        assert_eq!(
            live.providers[0].settings_config[PROVIDER_SOURCE_FIELD],
            PROVIDER_SOURCE_DICT
        );

        assert!(p
            .apply(
                &provider("builtin", serde_json::json!({ "base_url": "https://y/v1" })),
                true
            )
            .is_err());
        assert!(p.remove_provider("builtin").is_err());
    }

    #[test]
    fn mcp_roundtrip_merges_hermes_fields() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HermesGuard::set(temp.path());
        let dir = hermes_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.yaml"),
            "mcp_servers:\n  fs:\n    command: old\n    timeout: 30\n",
        )
        .unwrap();

        let p = HermesPlugin::new();
        p.set_mcp_server(&McpServerSpec {
            id: "fs".into(),
            name: "fs".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "npx" }),
        })
        .unwrap();

        // merge：核心字段更新、Hermes 特有字段保留
        let config = std::fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(config.contains("command: npx"));
        assert!(config.contains("timeout: 30"));

        let servers = p.get_mcp_servers().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].spec["command"], "npx");
        assert!(
            servers[0].spec.get("enabled").is_none(),
            "统一 spec 剥离 enabled"
        );

        p.remove_mcp_server("fs").unwrap();
        assert!(p.get_mcp_servers().unwrap().is_empty());
    }

    #[test]
    fn sessions_jsonl_scan_and_load() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HermesGuard::set(temp.path());
        let dir = hermes_dir().join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            concat!(
                "{\"type\":\"session\",\"id\":\"s1\",\"title\":\"My Session\",\"cwd\":\"/home/user/project\"}\n",
                "{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"Hello world\"},\"timestamp\":\"2026-01-01T00:00:00Z\"}\n",
                "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":\"Hi there\"},\"timestamp\":\"2026-01-01T00:01:00Z\"}\n"
            ),
        )
        .unwrap();

        let p = HermesPlugin::new();
        let sessions = p.sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
        assert_eq!(sessions[0].title.as_deref(), Some("My Session"));
        assert_eq!(sessions[0].project_dir.as_deref(), Some("/home/user/project"));

        let msgs = p
            .load_messages(sessions[0].source_path.as_deref().unwrap())
            .unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "Hello world");

        // 删除 jsonl
        assert!(p
            .delete_session("s1", sessions[0].source_path.as_deref().unwrap())
            .unwrap());
        assert!(!dir.join("s1.jsonl").exists());
    }
}
