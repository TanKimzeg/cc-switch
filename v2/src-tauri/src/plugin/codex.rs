//! 原生 Codex 插件：读写 `~/.codex/config.toml` + `auth.json`（非 additive 整文档快照）、
//! TOML `[mcp_servers]`、`~/.codex/sessions` 会话。
//!
//! 语义对齐 v1（apps/codex.rs + codex_config.rs + mcp/codex.rs + session codex）：
//! - settings_config 形状为 `{"auth": {...}, "config": "<TOML 文本>"}`；
//! - 切换写 auth.json + config.toml（先 auth 后 config，失败回滚 auth）；
//! - MCP 走 config.toml 内联表（toml_edit 保注释），`http_headers` ↔ `headers` 转换；
//! - 未安装（`~/.codex` 不存在）时 MCP 写入静默跳过，不创建文件。

use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::plugin::error::PluginError;
use crate::plugin::mcp::{McpPlugin, McpServerSpec};
use crate::plugin::session_utils::{
    collect_files_with_ext, extract_text, parse_timestamp_to_ms, path_basename, read_head_tail_lines,
    truncate_summary, TITLE_MAX_CHARS,
};
use crate::plugin::{AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities};
use crate::types::Provider;

const PLUGIN_ID: &str = "codex";
const CODEX_REQUEST_MARKER: &str = "my request for codex";
const VSCODE_CONTEXT_PREFIX: &str = "# Context from my IDE setup:";

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

/// Codex 配置目录（`~/.codex`；可经设置 overrideDir.codex 覆盖）。
fn config_dir() -> PathBuf {
    crate::services::overrides::get(PLUGIN_ID)
        .or_else(|| override_dir("CC_SWITCH_CODEX_CONFIG_DIR"))
        .unwrap_or_else(|| home_dir().join(".codex"))
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn auth_path() -> PathBuf {
    config_dir().join("auth.json")
}

fn read_config_text() -> Result<String, PluginError> {
    let path = config_path();
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))
}

fn validate_config_toml(text: &str) -> Result<(), PluginError> {
    if text.trim().is_empty() {
        return Ok(());
    }
    text.parse::<toml::Table>()
        .map(|_| ())
        .map_err(|e| PluginError::Config(format!("config.toml 解析失败: {e}")))
}

fn read_auth_json() -> Result<Value, PluginError> {
    let path = auth_path();
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| PluginError::io(&path, e))?;
    serde_json::from_str(&content).map_err(|e| PluginError::json(&path, e))
}

/// 原子写 auth.json + config.toml（先 auth 后 config，config 失败回滚 auth）。
fn write_live_atomic(auth: &Value, config_text: &str) -> Result<(), PluginError> {
    validate_config_toml(config_text)?;

    let auth_p = auth_path();
    let config_p = config_path();
    if let Some(parent) = auth_p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
    }

    let old_auth = if auth_p.exists() {
        Some(std::fs::read(&auth_p).map_err(|e| PluginError::io(&auth_p, e))?)
    } else {
        None
    };

    let auth_json = serde_json::to_string_pretty(auth).map_err(|e| PluginError::json(&auth_p, e))?;
    std::fs::write(&auth_p, format!("{auth_json}\n")).map_err(|e| PluginError::io(&auth_p, e))?;

    if let Err(e) = std::fs::write(&config_p, config_text) {
        // 回滚 auth.json，避免半切换状态。
        match old_auth {
            Some(bytes) => {
                let _ = std::fs::write(&auth_p, bytes);
            }
            None => {
                let _ = std::fs::remove_file(&auth_p);
            }
        }
        return Err(PluginError::io(&config_p, e));
    }
    Ok(())
}

/// 原生 Codex 插件。
#[derive(Debug, Default)]
pub struct CodexPlugin;

impl CodexPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl AgentPlugin for CodexPlugin {
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
        let settings = json!({
            "auth": read_auth_json()?,
            "config": read_config_text()?,
        });
        Ok(LiveConfig {
            providers: vec![LiveProvider {
                id: "default".into(),
                name: "Codex".into(),
                settings_config: settings,
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
        let settings = raw
            .as_object()
            .ok_or_else(|| {
                PluginError::Config(format!(
                    "provider '{}' 的 settings_config 必须是 JSON 对象",
                    provider.id
                ))
            })?;
        let auth = settings
            .get("auth")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !auth.is_object() {
            return Err(PluginError::Config(format!(
                "provider '{}' 的 Codex auth 配置必须是 JSON 对象",
                provider.id
            )));
        }
        let config_text = settings
            .get("config")
            .and_then(Value::as_str)
            .unwrap_or("");
        write_live_atomic(&auth, config_text)
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let candidate = ImportCandidate {
            id: "default".into(),
            name: "Codex".into(),
            settings_config: json!({
                "auth": read_auth_json()?,
                "config": read_config_text()?,
            }),
        };
        Ok(vec![candidate])
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
        delete_session_file(Path::new(source), session_id)
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
        validate_config_toml(content)?;
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, content).map_err(|e| PluginError::io(&path, e))
    }

    fn sync_usage(&self) -> Result<Vec<crate::plugin::UsageRecord>, PluginError> {
        sync_usage_impl()
    }

    // total_token_usage 是会话累计快照：同一 source_id 需随会话增长刷新。
    fn usage_upsert(&self) -> bool {
        true
    }
}

// ============================================================================
// MCP（config.toml 内联 [mcp_servers] 表）
// ============================================================================

fn mcp_target_installed() -> bool {
    crate::services::overrides::get(PLUGIN_ID).is_some()
        || override_dir("CC_SWITCH_CODEX_CONFIG_DIR").is_some()
        || config_dir().exists()
}

impl McpPlugin for CodexPlugin {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError> {
        let text = read_config_text()?;
        let mut servers = Vec::new();
        if text.trim().is_empty() {
            return Ok(servers);
        }
        let root: toml::Table = text
            .parse()
            .map_err(|e| PluginError::Config(format!("解析 config.toml 失败: {e}")))?;
        // 官方标准 [mcp_servers.*]；容错读取 [mcp.servers.*]（历史错误写入）。
        for section_key in ["mcp_servers", "mcp.servers"] {
            let Some(entries) = root
                .get(section_key)
                .and_then(toml::Value::as_table)
            else {
                continue;
            };
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
            log::info!("Codex 未安装，跳过写入 MCP 服务器 '{}'", spec.id);
            return Ok(());
        }
        let path = config_path();
        let text = read_config_text()?;
        let mut doc = if text.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            text.parse::<toml_edit::DocumentMut>()
                .map_err(|e| PluginError::Config(format!("解析 config.toml 失败: {e}")))?
        };
        // mcp_servers 缺失或非表时归一化为空表（用户手写脏值不可信）。
        if doc
            .get("mcp_servers")
            .is_some_and(|item| item.as_table_like().is_none())
        {
            log::warn!("config.toml 的 mcp_servers 不是表，已重置为空表");
            doc.remove("mcp_servers");
        }
        if doc.get("mcp_servers").is_none() {
            doc["mcp_servers"] = toml_edit::table();
        }
        let servers = doc
            .get_mut("mcp_servers")
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(|| PluginError::Config("config.toml 的 mcp_servers 不是表".into()))?;
        servers.insert(
            spec.id.as_str(),
            toml_edit::Item::Table(json_server_to_toml_table(&spec.spec)?),
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| PluginError::io(&path, e))
    }

    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError> {
        if !mcp_target_installed() {
            log::info!("Codex 未安装，跳过移除 MCP 服务器 '{id}'");
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
                log::warn!("解析 config.toml 失败: {e}，跳过删除操作");
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
                    log::warn!("config.toml 的 mcp_servers 不是表，无法删除服务器 '{id}'");
                }
                None => {}
            }
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| PluginError::io(&path, e))
    }
}

/// 统一 JSON spec → Codex TOML 表（对齐 v1 mcp/codex.rs json_server_to_toml_table）。
pub(crate) fn json_server_to_toml_table(spec: &Value) -> Result<toml_edit::Table, PluginError> {
    let mut t = toml_edit::Table::new();
    let typ = spec.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");
    t["type"] = toml_edit::value(typ);

    let core_fields: &[&str] = match typ {
        "stdio" => &["type", "command", "args", "env", "cwd"],
        "http" | "sse" => &["type", "url", "headers", "http_headers"],
        _ => &["type"],
    };

    match typ {
        "stdio" => {
            let cmd = spec.get("command").and_then(|v| v.as_str()).unwrap_or("");
            t["command"] = toml_edit::value(cmd);
            if let Some(args) = spec.get("args").and_then(|v| v.as_array()) {
                let mut arr = toml_edit::Array::default();
                for a in args.iter().filter_map(|x| x.as_str()) {
                    arr.push(a);
                }
                if !arr.is_empty() {
                    t["args"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
                }
            }
            if let Some(cwd) = spec.get("cwd").and_then(|v| v.as_str()) {
                if !cwd.trim().is_empty() {
                    t["cwd"] = toml_edit::value(cwd);
                }
            }
            if let Some(env) = spec.get("env").and_then(|v| v.as_object()) {
                let mut env_tbl = toml_edit::Table::new();
                for (k, v) in env {
                    if let Some(s) = v.as_str() {
                        env_tbl[&k[..]] = toml_edit::value(s);
                    }
                }
                if !env_tbl.is_empty() {
                    t["env"] = toml_edit::Item::Table(env_tbl);
                }
            }
        }
        "http" | "sse" => {
            let url = spec.get("url").and_then(|v| v.as_str()).unwrap_or("");
            t["url"] = toml_edit::value(url);
            if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
                let mut h_tbl = toml_edit::Table::new();
                for (k, v) in headers {
                    if let Some(s) = v.as_str() {
                        h_tbl[&k[..]] = toml_edit::value(s);
                    }
                }
                if !h_tbl.is_empty() {
                    t["http_headers"] = toml_edit::Item::Table(h_tbl);
                }
            }
        }
        _ => {}
    }

    // 扩展字段与未知字段走通用转换。
    if let Some(obj) = spec.as_object() {
        for (key, value) in obj {
            if core_fields.contains(&key.as_str()) {
                continue;
            }
            if let Some(item) = json_value_to_toml_item(value) {
                t[&key[..]] = item;
            }
        }
    }
    Ok(t)
}

fn json_value_to_toml_item(value: &Value) -> Option<toml_edit::Item> {
    match value {
        Value::String(s) => Some(toml_edit::value(s.clone())),
        Value::Bool(b) => Some(toml_edit::value(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml_edit::value(i))
            } else {
                n.as_f64().map(toml_edit::value)
            }
        }
        Value::Array(items) => {
            let mut arr = toml_edit::Array::default();
            for item in items {
                if let Some(toml_edit::Item::Value(val)) = json_value_to_toml_item(item) {
                    arr.push(val);
                }
            }
            Some(toml_edit::Item::Value(toml_edit::Value::Array(arr)))
        }
        Value::Object(map) => {
            let mut tbl = toml_edit::Table::new();
            for (k, v) in map {
                if let Some(item) = json_value_to_toml_item(v) {
                    tbl[&k[..]] = item;
                }
            }
            Some(toml_edit::Item::Table(tbl))
        }
        Value::Null => None,
    }
}

/// Codex TOML 表 → 统一 JSON spec（http_headers→headers，type 缺省按 url/command 推断）。
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

// ============================================================================
// 会话（~/.codex/sessions + archived_sessions，rollout jsonl）
// ============================================================================

fn session_roots() -> Vec<PathBuf> {
    let dir = config_dir();
    vec![dir.join("sessions"), dir.join("archived_sessions")]
}

fn scan_sessions() -> Result<Vec<crate::plugin::SessionMeta>, PluginError> {
    let titles = load_thread_titles();
    let mut files = Vec::new();
    for root in session_roots() {
        collect_files_with_ext(&root, "jsonl", &mut files);
    }
    let mut sessions = Vec::new();
    for path in files {
        if let Some(meta) = parse_session(&path, &titles) {
            sessions.push(meta);
        }
    }
    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(sessions)
}

/// 线程标题：来自 `~/.codex/session_index.jsonl`（state DB 标题暂不支持，见文档差距）。
fn load_thread_titles() -> std::collections::HashMap<String, String> {
    let index_path = config_dir().join("session_index.jsonl");
    let mut titles = std::collections::HashMap::new();
    let Ok(file) = std::fs::File::open(&index_path) else {
        return titles;
    };
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let id = entry.get("id").and_then(Value::as_str).unwrap_or("").trim();
        let title = entry
            .get("thread_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !id.is_empty() && !title.is_empty() {
            titles.insert(id.to_string(), title.to_string());
        }
    }
    titles
}

fn parse_session(
    path: &Path,
    titles: &std::collections::HashMap<String, String>,
) -> Option<crate::plugin::SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 10, 30).ok()?;

    let mut session_id: Option<String> = None;
    let mut project_dir: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut first_user_message: Option<String> = None;

    for line in &head {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if created_at.is_none() {
            created_at = value.get("timestamp").and_then(parse_timestamp_to_ms);
        }
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            if let Some(payload) = value.get("payload") {
                // 子代理会话不列出（对齐 v1）。
                if payload
                    .get("source")
                    .and_then(Value::as_object)
                    .is_some_and(|s| s.contains_key("subagent"))
                {
                    return None;
                }
                if session_id.is_none() {
                    session_id = payload.get("id").and_then(Value::as_str).map(String::from);
                }
                if project_dir.is_none() {
                    project_dir = payload.get("cwd").and_then(Value::as_str).map(String::from);
                }
                if let Some(ts) = payload.get("timestamp").and_then(parse_timestamp_to_ms) {
                    created_at.get_or_insert(ts);
                }
            }
        }
        if first_user_message.is_none()
            && value.get("type").and_then(Value::as_str) == Some("response_item")
        {
            if let Some(payload) = value.get("payload") {
                if payload.get("type").and_then(Value::as_str) == Some("message")
                    && payload.get("role").and_then(Value::as_str) == Some("user")
                {
                    let text = payload.get("content").map(extract_text).unwrap_or_default();
                    if let Some(title) = title_candidate_from_user_message(&text) {
                        first_user_message = Some(title);
                    }
                }
            }
        }
        if session_id.is_some()
            && project_dir.is_some()
            && created_at.is_some()
            && first_user_message.is_some()
        {
            break;
        }
    }

    let mut last_active_at: Option<i64> = None;
    for line in tail.iter().rev() {
        let value: Value = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if let Some(ts) = value.get("timestamp").and_then(parse_timestamp_to_ms) {
            last_active_at = Some(ts);
            break;
        }
    }

    let session_id = session_id.or_else(|| infer_session_id_from_filename(path))?;
    let title = titles
        .get(&session_id)
        .map(|t| truncate_summary(t, TITLE_MAX_CHARS))
        .or_else(|| first_user_message.map(|t| truncate_summary(&t, TITLE_MAX_CHARS)))
        .or_else(|| project_dir.as_deref().and_then(path_basename));

    Some(crate::plugin::SessionMeta {
        session_id: session_id.clone(),
        title,
        project_dir,
        created_at,
        last_active_at: last_active_at.or(created_at),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("codex resume {session_id}")),
    })
}

/// 标题候选：跳过 AGENTS.md 注入与环境上下文；VS Code IDE 上下文里提取真实请求。
fn title_candidate_from_user_message(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("# AGENTS.md")
        || trimmed.starts_with("<environment_context>")
    {
        return None;
    }
    if trimmed.starts_with(VSCODE_CONTEXT_PREFIX) {
        return extract_prompt_from_ide_context(trimmed);
    }
    Some(trimmed.to_string())
}

fn extract_prompt_from_ide_context(text: &str) -> Option<String> {
    let normalized = text.replace("\r\n", "\n");
    let lines = normalized.lines().collect::<Vec<_>>();
    // VS Code 把真实请求放在最后一个 "## My request for Codex:" 小节。
    let mut prompt: Option<String> = None;
    for (index, line) in lines.iter().enumerate() {
        let Some(inline) = request_heading_payload(line) else {
            continue;
        };
        if !inline.is_empty() {
            prompt = Some(inline.to_string());
            continue;
        }
        let following = lines[index + 1..].join("\n").trim().to_string();
        prompt = (!following.is_empty()).then_some(following);
    }
    prompt
}

fn request_heading_payload(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let heading = trimmed.trim_start_matches('#').trim_start();
    if !heading.to_ascii_lowercase().starts_with(CODEX_REQUEST_MARKER) {
        return None;
    }
    let suffix = heading[CODEX_REQUEST_MARKER.len()..].trim_start();
    if suffix.is_empty() {
        return Some("");
    }
    let separator = suffix.chars().next()?;
    if !matches!(separator, ':' | '：' | '-' | '—') {
        return None;
    }
    Some(
        suffix
            .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '：' | '-' | '—'))
            .trim(),
    )
}

/// 文件名回退：rollout-<ts>-<uuid>.jsonl 中的 UUID。
fn infer_session_id_from_filename(path: &Path) -> Option<String> {
    use regex::Regex;
    static UUID_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = UUID_RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    });
    let file_name = path.file_name()?.to_string_lossy();
    re.find(&file_name).map(|m| m.as_str().to_string())
}

fn load_session_messages(path: &Path) -> Result<Vec<crate::plugin::SessionMessage>, PluginError> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).map_err(|e| PluginError::io(path, e))?;
    let reader = std::io::BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let value: Value = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if value.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let payload_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let (role, content) = match payload_type {
            "message" => {
                let role = payload
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let content = payload.get("content").map(extract_text).unwrap_or_default();
                (role, content)
            }
            "function_call" => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                ("assistant".to_string(), format!("[Tool: {name}]"))
            }
            "function_call_output" => (
                "tool".to_string(),
                payload
                    .get("output")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            _ => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        let ts = value.get("timestamp").and_then(parse_timestamp_to_ms);
        messages.push(crate::plugin::SessionMessage { role, content, ts });
    }
    Ok(messages)
}

/// 会话级用量聚合（简化口径，对齐 v1 数据源但去掉 turn 级增量/ fork 解析的
/// 复杂度）：取每个 rollout 文件最后一次 `token_count` 的 `total_token_usage`
/// 会话累计快照作为整条记录；模型取最后一个 `turn_context` 声明的值；
/// 子代理会话跳过。成本不在此计算（PricingService 统一补算）。
fn sync_usage_impl() -> Result<Vec<crate::plugin::UsageRecord>, PluginError> {
    let mut files = Vec::new();
    for root in session_roots() {
        collect_files_with_ext(&root, "jsonl", &mut files);
    }

    let mut records = Vec::new();
    for path in files {
        // 异常大文件跳过（对齐 v1 的 50MiB 上限）。
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 50 * 1024 * 1024 {
                log::warn!("Codex 会话日志过大，跳过用量: {}", path.display());
                continue;
            }
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };

        let mut session_id: Option<String> = None;
        let mut is_subagent = false;
        let mut model: Option<String> = None;
        let mut total: Option<(i64, i64, i64)> = None; // (input_total, cached_input, output)
        let mut last_ts: Option<i64> = None;

        for line in content.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(ts) = value.get("timestamp").and_then(parse_timestamp_to_ms) {
                last_ts = Some(ts);
            }
            match value.get("type").and_then(Value::as_str).unwrap_or("") {
                "session_meta" => {
                    if let Some(payload) = value.get("payload") {
                        if payload
                            .get("source")
                            .and_then(Value::as_object)
                            .is_some_and(|s| s.contains_key("subagent"))
                        {
                            is_subagent = true;
                            break;
                        }
                        if session_id.is_none() {
                            session_id =
                                payload.get("id").and_then(Value::as_str).map(String::from);
                        }
                    }
                }
                "turn_context" => {
                    if let Some(m) = value
                        .get("payload")
                        .and_then(|p| p.get("model"))
                        .and_then(Value::as_str)
                        .filter(|m| !m.trim().is_empty())
                    {
                        model = Some(m.trim().to_string());
                    }
                }
                "event_msg" => {
                    let Some(payload) = value.get("payload") else {
                        continue;
                    };
                    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                        continue;
                    }
                    let Some(info) = payload.get("info").filter(|i| !i.is_null()) else {
                        continue;
                    };
                    let Some(usage) = info.get("total_token_usage") else {
                        continue;
                    };
                    let input = usage
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let cached = usage
                        .get("cached_input_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let output = usage
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    total = Some((input as i64, cached as i64, output as i64));
                }
                _ => {}
            }
        }

        if is_subagent {
            continue;
        }
        let Some((input_total, cached, output)) = total else {
            continue;
        };
        if input_total == 0 && cached == 0 && output == 0 {
            continue;
        }
        let session_id = session_id.or_else(|| infer_session_id_from_filename(&path));
        let Some(session_id) = session_id else {
            continue;
        };
        // OpenAI 口径 input_tokens 含 cached_input：fresh 输入 = 总输入 - 命中。
        let fresh_input = (input_total - cached).max(0);
        records.push(crate::plugin::UsageRecord {
            source_id: format!("codex_session:{session_id}"),
            session_id: session_id.clone(),
            model: model.unwrap_or_else(|| "unknown".into()),
            input_tokens: fresh_input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cached,
            cache_write_tokens: 0,
            cost: 0.0,
            timestamp_ms: last_ts.unwrap_or(0),
        });
    }
    records.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    Ok(records)
}

fn delete_session_file(path: &Path, session_id: &str) -> Result<bool, PluginError> {
    // 删除前校验文件确属该会话（防误删）。
    if let Some(meta) = parse_session(path, &std::collections::HashMap::new()) {
        if meta.session_id != session_id {
            return Err(PluginError::Config(format!(
                "Codex 会话 id 不匹配: 期望 {session_id}，实际 {}",
                meta.session_id
            )));
        }
    } else {
        // 元数据不可解析时回退文件名推断，仍要求一致。
        let inferred = infer_session_id_from_filename(path)
            .ok_or_else(|| PluginError::Config("无法解析 Codex 会话元数据".into()))?;
        if inferred != session_id {
            return Err(PluginError::Config(format!(
                "Codex 会话 id 不匹配: 期望 {session_id}，实际 {inferred}"
            )));
        }
    }
    std::fs::remove_file(path).map_err(|e| PluginError::io(path, e))?;
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

    fn sample_settings() -> String {
        serde_json::json!({
            "auth": { "OPENAI_API_KEY": "sk-test" },
            "config": "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://example.com/v1\"\n"
        })
        .to_string()
    }

    fn provider(id: &str, settings: &str) -> Provider {
        Provider {
            id: id.to_string(),
            plugin_id: "codex".to_string(),
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
    fn provider_apply_read_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = CodexPlugin::new();

        p.apply(&provider("default", &sample_settings()), true).unwrap();

        assert_eq!(
            std::fs::read_to_string(temp.path().join(".codex").join("config.toml")).unwrap(),
            "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://example.com/v1\"\n"
        );
        let auth: Value = serde_json::from_str(
            &std::fs::read_to_string(temp.path().join(".codex").join("auth.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(auth["OPENAI_API_KEY"], "sk-test");

        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 1);
        assert_eq!(live.current.as_deref(), Some("default"));
        let settings = live.providers[0].settings_config.clone();
        assert_eq!(settings["auth"]["OPENAI_API_KEY"], "sk-test");
        assert_eq!(settings["config"], "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://example.com/v1\"\n");

        let imported = p.import().unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].id, "default");
    }

    #[test]
    fn apply_rejects_invalid_toml_and_keeps_auth_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = CodexPlugin::new();

        let bad = serde_json::json!({
            "auth": { "OPENAI_API_KEY": "sk-new" },
            "config": "not = [valid"
        })
        .to_string();
        assert!(p.apply(&provider("default", &bad), true).is_err());
        // config 写入失败 → auth.json 不得残留半切换状态
        assert!(!temp.path().join(".codex").join("auth.json").exists());
    }

    #[test]
    fn mcp_roundtrip_with_format_conversion() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        // 模拟已安装
        std::fs::create_dir_all(temp.path().join(".codex")).unwrap();
        let p = CodexPlugin::new();

        p.set_mcp_server(&McpServerSpec {
            id: "fs".into(),
            name: "fs".into(),
            spec: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "server"],
                "env": { "KEY": "value" }
            }),
        })
        .unwrap();
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

        let servers = p.get_mcp_servers().unwrap();
        assert_eq!(servers.len(), 2);
        let remote = servers.iter().find(|s| s.id == "remote").unwrap();
        assert_eq!(remote.spec["type"], "http");
        assert_eq!(remote.spec["headers"]["Authorization"], "Bearer t");
        let fs_server = servers.iter().find(|s| s.id == "fs").unwrap();
        assert_eq!(fs_server.spec["env"]["KEY"], "value");

        // 写入保注释：手工注释行不得丢失
        let config = std::fs::read_to_string(temp.path().join(".codex").join("config.toml")).unwrap();
        assert!(config.contains("[mcp_servers.fs]"));
        assert!(config.contains("http_headers"));

        p.remove_mcp_server("fs").unwrap();
        let servers = p.get_mcp_servers().unwrap();
        assert!(servers.iter().all(|s| s.id != "fs"));
    }

    #[test]
    fn mcp_set_skips_uninstalled_target() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = CodexPlugin::new();
        p.set_mcp_server(&McpServerSpec {
            id: "fs".into(),
            name: "fs".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "npx" }),
        })
        .unwrap();
        assert!(!temp.path().join(".codex").exists());
    }

    #[test]
    fn sessions_scan_load_delete() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let session_dir = temp
            .path()
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("08")
            .join("24");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("rollout-2026-08-24T10-00-00-019f6af2-18b0-7673-958e-d25be650e172.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-08-24T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"019f6af2-18b0-7673-958e-d25be650e172\",\"cwd\":\"/tmp/proj\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"How do I deploy?\"}]}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:03Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"ok\"}}\n"
            ),
        )
        .unwrap();

        let p = CodexPlugin::new();
        let sessions = p.sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_id,
            "019f6af2-18b0-7673-958e-d25be650e172"
        );
        assert_eq!(sessions[0].title.as_deref(), Some("How do I deploy?"));
        assert_eq!(sessions[0].project_dir.as_deref(), Some("/tmp/proj"));
        assert_eq!(
            sessions[0].resume_command.as_deref(),
            Some("codex resume 019f6af2-18b0-7673-958e-d25be650e172")
        );

        let msgs = p
            .load_messages(sessions[0].source_path.as_deref().unwrap())
            .unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "How do I deploy?");
        assert_eq!(msgs[1].content, "[Tool: shell]");
        assert_eq!(msgs[2].role, "tool");

        // id 不匹配拒绝删除
        assert!(p
            .delete_session("wrong-id", sessions[0].source_path.as_deref().unwrap())
            .is_err());
        assert!(p
            .delete_session(
                "019f6af2-18b0-7673-958e-d25be650e172",
                sessions[0].source_path.as_deref().unwrap()
            )
            .unwrap());
        assert!(!session_dir
            .join("rollout-2026-08-24T10-00-00-019f6af2-18b0-7673-958e-d25be650e172.jsonl")
            .exists());
    }

    #[test]
    fn usage_sync_takes_last_cumulative_snapshot() {
        use chrono::TimeZone;
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let dir = temp.path().join(".codex").join("sessions").join("d");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rollout-2026-08-24T10-00-00-019f6af2-18b0-7673-958e-d25be650e172.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-08-24T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"019f6af2-18b0-7673-958e-d25be650e172\",\"cwd\":\"/w\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:01Z\",\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.5\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":1000,\"cached_input_tokens\":400,\"output_tokens\":50}}}}\n",
                "{\"timestamp\":\"2026-08-24T10:01:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":2000,\"cached_input_tokens\":900,\"output_tokens\":120}}}}\n",
                "{\"timestamp\":\"2026-08-24T10:01:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"other\",\"info\":{}}}\n"
            ),
        )
        .unwrap();

        let p = CodexPlugin::new();
        let records = p.sync_usage().unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.source_id, "codex_session:019f6af2-18b0-7673-958e-d25be650e172");
        assert_eq!(r.model, "gpt-5.5");
        // 最后一次累计快照；OpenAI 口径 input 含 cached → fresh = 2000 - 900
        assert_eq!(r.input_tokens, 1100);
        assert_eq!(r.cache_read_tokens, 900);
        assert_eq!(r.output_tokens, 120);
        let expected_ts = chrono::Utc
            .with_ymd_and_hms(2026, 8, 24, 10, 1, 1)
            .unwrap()
            .timestamp_millis();
        assert_eq!(r.timestamp_ms, expected_ts);
        assert_eq!(r.cost, 0.0, "成本交由 PricingService 计算");
    }

    #[test]
    fn usage_sync_skips_subagent_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let dir = temp.path().join(".codex").join("sessions").join("sub");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rollout-sub.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-08-24T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"sub-1\",\"source\":{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"p\"}}}}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":500,\"cached_input_tokens\":0,\"output_tokens\":10}}}}\n"
            ),
        )
        .unwrap();

        let p = CodexPlugin::new();
        assert!(p.sync_usage().unwrap().is_empty());
    }

    #[test]
    fn session_title_skips_agents_md_and_extracts_vscode_request() {        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let dir = temp.path().join(".codex").join("sessions").join("d");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rollout-x.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-08-24T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"sid-1\",\"cwd\":\"/w\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"# AGENTS.md\\ninstructions\"}}\n",
                "{\"timestamp\":\"2026-08-24T10:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"# Context from my IDE setup:\\n\\n## My request for Codex:\\nfix the bug\"}}\n"
            ),
        )
        .unwrap();

        let p = CodexPlugin::new();
        let sessions = p.sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title.as_deref(), Some("fix the bug"));
    }
}
