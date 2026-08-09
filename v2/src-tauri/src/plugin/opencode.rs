//! 原生 OpenCode 插件：读写 `~/.config/opencode/opencode.json` 与会话。
//!
//! OpenCode 是「加性（additive）」配置模型：live 配置的 `provider` 字段是一个
//! 对象，键为 provider id，值为该 provider 的配置片段（npm/options/models）。
//! 切换 = 把目标 provider 的片段 upsert 进 `provider`，同时（可选）更新
//! 顶层 `provider` 选择器字段为当前 provider 的 npm 包名。

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::plugin::error::PluginError;
use crate::plugin::mcp::{self, McpPlugin, McpServerSpec};
use crate::plugin::ops::PluginManagerPlugin;
use crate::plugin::{
    AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities, SessionMessage,
    SessionMeta,
};
use crate::types::Provider;

const PLUGIN_ID: &str = "opencode";
const SCHEMA_URL: &str = "https://opencode.ai/config.json";

fn home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CC_SWITCH_TEST_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 读取测试/配置目录 override（环境变量，非空才生效）。
fn override_dir(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

/// OpenCode 配置目录（`~/.config/opencode`）。
fn config_dir() -> PathBuf {
    override_dir("CC_SWITCH_OPENCODE_CONFIG_DIR").unwrap_or_else(|| {
        home_dir().join(".config").join("opencode")
    })
}

/// OpenCode 数据目录（会话等；遵循 XDG_DATA_HOME，兜底 `~/.local/share/opencode`）。
fn data_dir() -> PathBuf {
    override_dir("CC_SWITCH_OPENCODE_DATA_DIR").unwrap_or_else(|| {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("opencode");
            }
        }
        home_dir().join(".local").join("share").join("opencode")
    })
}

fn config_path() -> PathBuf {
    config_dir().join("opencode.json")
}

/// 原生 OpenCode 插件。
#[derive(Debug, Default)]
pub struct OpenCodePlugin;

impl OpenCodePlugin {
    pub fn new() -> Self {
        Self
    }
}

impl AgentPlugin for OpenCodePlugin {
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
            plugins: true,
        };
        &CAPS
    }

    fn read_live(&self) -> Result<LiveConfig, PluginError> {
    let config = read_config(&config_path())?;
    let mut providers = Vec::new();

    if let Some(provider_map) = config.get("provider").and_then(Value::as_object) {
        for (id, value) in provider_map {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| id.clone());
            providers.push(LiveProvider {
                id: id.clone(),
                name,
                settings_config: value.clone(),
            });
        }
    }
    providers.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(LiveConfig {
        providers,
        current: None,
    })
    }

    fn apply(&self, provider: &Provider, current: bool) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;

        let settings = parse_provider_settings(provider)?;

        // provider 段必须是对象；非对象则重置（并记录告警）。
        let provider_section = config
            .get_mut("provider")
            .and_then(Value::as_object_mut);
        match provider_section {
            Some(map) => {
                map.insert(provider.id.clone(), settings);
            }
            None => {
                let mut map = Map::new();
                map.insert(provider.id.clone(), settings);
                config["provider"] = Value::Object(map);
            }
        }

        // OpenCode 是 additive 模式，无单一 current 选择器：
        // provider 全部共存于 `provider` 对象，`current` 仅表示"写入即可"。
        let _ = current;

        write_config(&path, &config)
    }

    fn remove_provider(&self, id: &str) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;

        if let Some(providers) = config.get_mut("provider").and_then(Value::as_object_mut) {
            providers.remove(id);
        }

        write_config(&path, &config)
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let config = read_config(&config_path())?;
        let mut candidates = Vec::new();
        if let Some(provider_map) = config.get("provider").and_then(Value::as_object) {
            for (id, value) in provider_map {
                let name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| id.clone());
                candidates.push(ImportCandidate {
                    id: id.clone(),
                    name,
                    settings_config: value.clone(),
                });
            }
        }
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(candidates)
    }

    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> {
        scan_sessions()
    }

    fn load_messages(&self, source: &str) -> Result<Vec<SessionMessage>, PluginError> {
        if let Some((db_path, session_id)) = parse_sqlite_source(source) {
            return load_messages_sqlite(&db_path, &session_id);
        }
        let path = Path::new(source);
        if !path.is_dir() {
            return Err(PluginError::Other(format!(
                "消息目录不存在: {}",
                path.display()
            )));
        }
        load_messages_json(path)
    }

    fn delete_session(&self, session_id: &str, source: &str) -> Result<bool, PluginError> {
        if let Some((db_path, ref_id)) = parse_sqlite_source(source) {
            if ref_id != session_id {
                return Err(PluginError::Other(format!(
                    "会话 id 不匹配: 期望 {session_id}，实际 {ref_id}"
                )));
            }
            return delete_session_sqlite(&db_path, session_id);
        }
        delete_session_json(source, session_id)
    }

    fn as_mcp(&self) -> Option<&dyn McpPlugin> {
        Some(self)
    }

    fn as_plugin_manager(&self) -> Option<&dyn PluginManagerPlugin> {
        Some(self)
    }
}

impl McpPlugin for OpenCodePlugin {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError> {
        let config = read_config(&config_path())?;
        let mut servers = Vec::new();
        if let Some(map) = config.get("mcp").and_then(Value::as_object) {
            for (id, value) in map {
                let spec = mcp::convert_from_opencode_format(value)
                    .map_err(|e| PluginError::Other(format!("解析 MCP 服务器 '{id}' 失败: {e}")))?;
                servers.push(McpServerSpec {
                    id: id.clone(),
                    name: id.clone(),
                    spec,
                });
            }
        }
        servers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(servers)
    }

    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;
        let opencode_spec = mcp::convert_to_opencode_format(&spec.spec)?;

        if !config.get("mcp").is_some_and(Value::is_object) {
            config["mcp"] = json!({});
        }
        if let Some(map) = config.get_mut("mcp").and_then(Value::as_object_mut) {
            map.insert(spec.id.clone(), opencode_spec);
        }
        write_config(&path, &config)
    }

    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;
        if let Some(map) = config.get_mut("mcp").and_then(Value::as_object_mut) {
            map.remove(id);
        }
        write_config(&path, &config)
    }
}

impl PluginManagerPlugin for OpenCodePlugin {
    fn get_plugins(&self) -> Result<Vec<String>, PluginError> {
        let config = read_config(&config_path())?;
        Ok(config
            .get("plugin")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default())
    }

    fn add_plugin(&self, name: &str) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;
        let normalized = canonicalize_plugin_name(name);
        let target_is_omo = is_omo_plugin(&normalized);
        let mut changed = false;

        let plugins = config.get_mut("plugin").and_then(Value::as_array_mut);
        match plugins {
            Some(arr) => {
                let mut found_target = false;
                arr.retain(|value| {
                    let Some(existing) = value.as_str() else {
                        return true;
                    };
                    if existing == normalized {
                        if found_target {
                            changed = true;
                            return false;
                        }
                        found_target = true;
                        return true;
                    }
                    // Standard OMO 与 OMO Slim 互斥。
                    if target_is_omo && is_omo_plugin(existing) {
                        changed = true;
                        return false;
                    }
                    true
                });
                if !found_target {
                    arr.push(Value::String(normalized.clone()));
                    changed = true;
                }
            }
            None => {
                config["plugin"] = json!([normalized.clone()]);
                changed = true;
            }
        }

        if changed {
            write_config(&path, &config)?;
        }
        Ok(())
    }

    fn remove_plugin(&self, name: &str) -> Result<(), PluginError> {
        let path = config_path();
        let mut config = read_config(&path)?;
        let mut changed = false;
        if let Some(arr) = config.get_mut("plugin").and_then(Value::as_array_mut) {
            let before = arr.len();
            arr.retain(|v| v.as_str() != Some(name));
            changed = arr.len() != before;
            if arr.is_empty() {
                config.as_object_mut().map(|o| o.remove("plugin"));
            }
        }
        if changed {
            write_config(&path, &config)?;
        }
        Ok(())
    }
}

const STANDARD_OMO_PREFIXES: [&str; 2] = ["oh-my-openagent", "oh-my-opencode"];
const SLIM_OMO_PREFIXES: [&str; 1] = ["oh-my-opencode-slim"];

/// 判断插件名是否为 OMO/OMO-Slim 系列（含 `@scope` 变体）。
fn is_omo_plugin(name: &str) -> bool {
    matches_any_prefix(name, &STANDARD_OMO_PREFIXES) || matches_any_prefix(name, &SLIM_OMO_PREFIXES)
}

fn matches_prefix(plugin_name: &str, prefix: &str) -> bool {
    plugin_name == prefix
        || plugin_name
            .strip_prefix(prefix)
            .map(|suffix| suffix.starts_with('@'))
            .unwrap_or(false)
}

fn matches_any_prefix(plugin_name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| matches_prefix(plugin_name, p))
}

/// 规范化插件名：`oh-my-opencode` → `oh-my-openagent`。
fn canonicalize_plugin_name(plugin_name: &str) -> String {
    if let Some(suffix) = plugin_name.strip_prefix("oh-my-opencode") {
        if suffix.is_empty() || suffix.starts_with('@') {
            return format!("oh-my-openagent{suffix}");
        }
    }
    plugin_name.to_string()
}

/// 解析 provider 的 `settings_config` 为写入 live 的 JSON 片段。
///
/// 兼容两种形态：
/// - 直接是 provider 片段（含 `npm` 或 `options`）；
/// - 误存了完整配置结构（含 `$schema` 或顶层 `provider`），则尝试提取
///   `provider.<id>` 片段。
fn parse_provider_settings(provider: &Provider) -> Result<Value, PluginError> {
    let raw = provider.settings_config.clone().unwrap_or_default();
    let value: Value = serde_json::from_str(&raw).map_err(|e| {
        PluginError::Config(format!(
            "provider '{}' settings_config 不是合法 JSON: {e}",
            provider.id
        ))
    })?;

    let is_fragment = value.get("npm").is_some() || value.get("options").is_some();
    if is_fragment {
        return Ok(value);
    }

    // 完整配置结构：尝试提取 provider.<id>
    if let Some(map) = value.as_object() {
        if map.contains_key("$schema") || map.contains_key("provider") {
            if let Some(fragment) = map
                .get("provider")
                .and_then(Value::as_object)
                .and_then(|m| m.get(&provider.id))
            {
                return Ok(fragment.clone());
            }
        }
    }

    Err(PluginError::Config(format!(
        "provider '{}' 的 settings_config 结构无效（必须含 'npm' 或 'options'）",
        provider.id
    )))
}

/// 读取 opencode.json（JSON5 语法，文件不存在时返回带 $schema 的空对象）。
fn read_config(path: &Path) -> Result<Value, PluginError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({ "$schema": SCHEMA_URL }));
        }
        Err(e) => return Err(PluginError::io(path, e)),
    };

    let value: Value = json5::from_str(&content).map_err(|e| {
        PluginError::Config(format!("解析 {} 失败: {e}", path.display()))
    })?;
    if !value.is_object() {
        return Err(PluginError::Config(format!(
            "根节点必须是 JSON 对象: {}",
            path.display()
        )));
    }
    Ok(value)
}

/// 写回 opencode.json（格式化 JSON）。
fn write_config(path: &Path, config: &Value) -> Result<(), PluginError> {
    let json = serde_json::to_string_pretty(config).map_err(|e| PluginError::json(path, e))?;
    atomic_write(path, json.as_bytes())?;
    Ok(())
}

/// 原子写入：先写临时文件再替换，避免半写状态。
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), PluginError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = parent.join(format!("{file_name}.tmp.{ts}.{}.json", std::process::id()));

    std::fs::write(&tmp, data).map_err(|e| PluginError::io(&tmp, e))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(PluginError::io(path, e));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 会话扫描
// ---------------------------------------------------------------------------

/// 扫描 OpenCode 会话：优先 SQLite（新存储），其次旧版 JSON 文件。
fn scan_sessions() -> Result<Vec<SessionMeta>, PluginError> {
    let mut sessions = scan_sessions_sqlite()?;
    let legacy = scan_sessions_json()?;

    let sqlite_ids: std::collections::HashSet<String> =
        sessions.iter().map(|s| s.session_id.clone()).collect();
    for s in legacy {
        if !sqlite_ids.contains(&s.session_id) {
            sessions.push(s);
        }
    }
    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(sessions)
}

/// 从 SQLite 数据库扫描会话（`$XDG_DATA_HOME/opencode/opencode.db`）。
fn scan_sessions_sqlite() -> Result<Vec<SessionMeta>, PluginError> {
    let db_path = data_dir().join("opencode.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("打开 OpenCode 数据库失败: {e}");
            return Ok(Vec::new());
        }
    };

    let mut stmt = match conn.prepare(
        "SELECT id, title, directory, time_created, time_updated FROM session ORDER BY time_updated DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("查询 OpenCode session 表失败: {e}");
            return Ok(Vec::new());
        }
    };

    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let directory: String = row.get(2)?;
            let created: i64 = row.get(3)?;
            let updated: i64 = row.get(4)?;
            Ok((id, title, directory, created, updated))
        })
        .map_err(|e| {
            PluginError::Other(format!("查询 OpenCode session 表失败: {e}"))
        })?;

    let db_display = db_path.display().to_string();
    let mut sessions = Vec::new();
    for row in rows.flatten() {
        let (session_id, title, directory, created, updated) = row;
        let display_title = if title.is_empty() {
            directory
                .rsplit('/')
                .next()
                .unwrap_or(&directory)
                .to_string()
        } else {
            title
        };
        sessions.push(SessionMeta {
            session_id: session_id.clone(),
            title: if display_title.is_empty() {
                None
            } else {
                Some(display_title)
            },
            project_dir: if directory.is_empty() {
                None
            } else {
                Some(directory)
            },
            created_at: Some(created),
            last_active_at: Some(updated),
            source_path: Some(format!("sqlite:{db_display}:{session_id}")),
            resume_command: Some(format!("opencode -s {session_id}")),
        });
    }
    Ok(sessions)
}

/// 从旧版 JSON 文件扫描会话（`storage/session/**/*.json`）。
fn scan_sessions_json() -> Result<Vec<SessionMeta>, PluginError> {
    let storage = data_dir().join("storage");
    let session_dir = storage.join("session");
    if !session_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_json_files(&session_dir, &mut files);

    let mut sessions = Vec::new();
    for path in files {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let Some(session_id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let title = value.get("title").and_then(Value::as_str).map(str::to_string);
        let directory = value
            .get("directory")
            .and_then(Value::as_str)
            .map(str::to_string);
        let created_at = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(Value::as_i64);
        let updated_at = value
            .get("time")
            .and_then(|t| t.get("updated"))
            .and_then(Value::as_i64);

        let display_title = title
            .filter(|t| !t.is_empty())
            .or_else(|| {
                directory
                    .as_deref()
                    .and_then(|d| d.rsplit('/').next())
                    .map(|s| s.to_string())
            });

        sessions.push(SessionMeta {
            session_id: session_id.to_string(),
            title: display_title,
            project_dir: directory,
            created_at,
            last_active_at: updated_at.or(created_at),
            source_path: Some(path.display().to_string()),
            resume_command: Some(format!("opencode -s {session_id}")),
        });
    }
    Ok(sessions)
}

fn collect_json_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            files.push(path);
        }
    }
}

/// 解析 `sqlite:<db_path>:<session_id>` 来源引用。
///
/// 用 `rfind(":ses_")` 切分，因为 db 路径本身可能含冒号（如 Windows 盘符）。
fn parse_sqlite_source(source: &str) -> Option<(PathBuf, String)> {
    let rest = source.strip_prefix("sqlite:")?;
    let sep = rest.rfind(":ses_")?;
    let db_path = PathBuf::from(&rest[..sep]);
    let session_id = rest[sep + 1..].to_string();
    Some((db_path, session_id))
}

/// 从 JSON 会话目录加载消息（`storage/message/{sessionID}/`）。
fn load_messages_json(path: &Path) -> Result<Vec<SessionMessage>, PluginError> {
    let storage = path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| PluginError::Other("无法确定 storage 根目录".into()))?;

    let mut msg_files = Vec::new();
    collect_json_files(path, &mut msg_files);

    let mut entries: Vec<(i64, String)> = Vec::new();
    for msg_path in &msg_files {
        let Ok(content) = std::fs::read_to_string(msg_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let msg_id = match value.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let role = value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let created_ts = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(Value::as_i64)
            .unwrap_or(0);

        let part_dir = storage.join("part").join(&msg_id);
        let text = collect_parts_text(&part_dir);
        if text.trim().is_empty() {
            continue;
        }
        entries.push((created_ts, format!("{role}\u{0001}{text}")));
    }

    entries.sort_by_key(|(ts, _)| *ts);
    let messages = entries
        .into_iter()
        .map(|(ts, packed)| {
            let (role, content) = packed.split_once('\u{0001}').unwrap_or(("unknown", ""));
            SessionMessage {
                role: role.to_string(),
                content: content.to_string(),
                ts: if ts > 0 { Some(ts) } else { None },
            }
        })
        .collect();
    Ok(messages)
}

/// 从 SQLite 数据库加载会话消息。
fn load_messages_sqlite(
    db_path: &Path,
    session_id: &str,
) -> Result<Vec<SessionMessage>, PluginError> {
    let conn = match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => return Err(PluginError::Other(format!("打开会话数据库失败: {e}"))),
    };

    let mut msg_stmt = match conn.prepare(
        "SELECT id, time_created, data FROM message WHERE session_id = ?1 ORDER BY time_created ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };
    let msg_rows = msg_stmt.query_map([session_id], |row| {
        let id: String = row.get(0)?;
        let ts: i64 = row.get(1)?;
        let data: String = row.get(2)?;
        Ok((id, ts, data))
    });

    let mut part_stmt = match conn.prepare(
        "SELECT message_id, data FROM part WHERE session_id = ?1 ORDER BY time_created ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };
    let part_rows = part_stmt.query_map([session_id], |row| {
        let message_id: String = row.get(0)?;
        let data: String = row.get(1)?;
        Ok((message_id, data))
    });

    let mut parts_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    if let Ok(rows) = part_rows {
        for part in rows.flatten() {
            parts_map.entry(part.0).or_default().push(part.1);
        }
    }

    let mut messages = Vec::new();
    if let Ok(rows) = msg_rows {
        for row in rows.flatten() {
            let (msg_id, ts, data) = row;
            let Ok(msg_value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let role = msg_value
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            let mut texts = Vec::new();
            if let Some(parts) = parts_map.get(&msg_id) {
                for part_data in parts {
                    let Ok(part_value) = serde_json::from_str::<Value>(part_data) else {
                        continue;
                    };
                    if let Some(text) = extract_part_text(&part_value) {
                        texts.push(text);
                    }
                }
            }

            let content = texts.join("\n");
            if content.trim().is_empty() {
                continue;
            }
            messages.push(SessionMessage {
                role,
                content,
                ts: Some(ts),
            });
        }
    }
    Ok(messages)
}

/// 提取单个 part 的文本。
fn extract_part_text(part_value: &Value) -> Option<String> {
    match part_value.get("type").and_then(Value::as_str) {
        Some("text") => part_value
            .get("text")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(|t| t.to_string()),
        Some("tool") => {
            let tool = part_value
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Some(format!("[Tool: {tool}]"))
        }
        _ => None,
    }
}

/// 收集 part 目录下全部文本。
fn collect_parts_text(part_dir: &Path) -> String {
    if !part_dir.is_dir() {
        return String::new();
    }
    let mut parts = Vec::new();
    collect_json_files(part_dir, &mut parts);

    let mut texts = Vec::new();
    for part_path in &parts {
        let Ok(content) = std::fs::read_to_string(part_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if let Some(text) = extract_part_text(&value) {
            texts.push(text);
        }
    }
    texts.join("\n")
}

/// 删除 JSON 存储的会话（消息目录 + session_diff + session 文件）。
fn delete_session_json(source: &str, session_id: &str) -> Result<bool, PluginError> {
    let path = PathBuf::from(source);
    let storage = match path.parent().and_then(|p| p.parent()) {
        Some(s) => s.to_path_buf(),
        None => return Err(PluginError::Other("无法确定 storage 根目录".into())),
    };

    let mut msg_files = Vec::new();
    collect_json_files(&path, &mut msg_files);

    let mut message_ids = Vec::new();
    for message_path in &msg_files {
        let Ok(content) = std::fs::read_to_string(message_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if let Some(message_id) = value.get("id").and_then(Value::as_str) {
            message_ids.push(message_id.to_string());
        }
    }

    for message_id in &message_ids {
        let part_dir = storage.join("part").join(message_id);
        remove_dir_all_if_exists(&part_dir).map_err(|e| PluginError::io(&part_dir, e))?;
    }

    let session_diff = storage.join("session_diff").join(format!("{session_id}.json"));
    remove_file_if_exists(&session_diff).map_err(|e| PluginError::io(&session_diff, e))?;

    remove_dir_all_if_exists(&path).map_err(|e| PluginError::io(&path, e))?;

    if let Some(session_file) = find_session_file(&storage, session_id) {
        remove_file_if_exists(&session_file).map_err(|e| PluginError::io(&session_file, e))?;
    }

    Ok(true)
}

/// 从 SQLite 数据库删除会话（含消息与 parts）。
fn delete_session_sqlite(db_path: &Path, session_id: &str) -> Result<bool, PluginError> {
    let expected_db = data_dir().join("opencode.db");
    let canonical_db = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    let canonical_expected = expected_db
        .canonicalize()
        .unwrap_or_else(|_| expected_db.clone());
    if canonical_db != canonical_expected {
        return Err(PluginError::Other(format!(
            "数据库路径与预期不一致（拒绝操作）：{}",
            db_path.display()
        )));
    }

    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => return Err(PluginError::Other(format!("打开会话数据库失败: {e}"))),
    };
    let tx = match conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => return Err(PluginError::Other(format!("开启事务失败: {e}"))),
    };

    tx.execute("DELETE FROM part WHERE session_id = ?1", [session_id])
        .map_err(|e| PluginError::Other(format!("删除会话 parts 失败: {e}")))?;
    tx.execute("DELETE FROM message WHERE session_id = ?1", [session_id])
        .map_err(|e| PluginError::Other(format!("删除会话消息失败: {e}")))?;
    let deleted = tx
        .execute("DELETE FROM session WHERE id = ?1", [session_id])
        .map_err(|e| PluginError::Other(format!("删除会话失败: {e}")))?;
    tx.commit()
        .map_err(|e| PluginError::Other(format!("提交事务失败: {e}")))?;

    Ok(deleted > 0)
}

fn find_session_file(storage: &Path, session_id: &str) -> Option<PathBuf> {
    let session_root = storage.join("session");
    let mut files = Vec::new();
    collect_json_files(&session_root, &mut files);
    let expected = format!("{session_id}.json");
    files
        .into_iter()
        .find(|path| path.file_name().and_then(|n| n.to_str()) == Some(expected.as_str()))
}

fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn remove_dir_all_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

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

    fn write_config_file(home: &Path, content: &str) {
        let dir = home.join(".config").join("opencode");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opencode.json"), content).unwrap();
    }

    fn provider(id: &str, settings: &str) -> Provider {
        Provider {
            id: id.to_string(),
            plugin_id: "opencode".to_string(),
            name: id.to_string(),
            category: "custom".to_string(),
            icon: None,
            website: None,
            api_key: None,
            settings_config: Some(settings.to_string()),
            meta: None,
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn capabilities_are_opt_in() {
        let p = OpenCodePlugin::new();
        assert!(p.capabilities().read_live);
        assert!(p.capabilities().apply);
        assert!(p.capabilities().import);
        assert!(p.capabilities().sessions);
    }

    #[test]
    fn read_live_returns_empty_for_missing_config() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();
        let live = p.read_live().unwrap();
        assert!(live.providers.is_empty());
        assert!(live.current.is_none());
    }

    #[test]
    fn read_live_parses_provider_section() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(
            temp.path(),
            r#"{
                // 注释应当被 json5 解析器接受
                "provider": {
                    "openai": { "npm": "@ai-sdk/openai", "options": { "apiKey": "sk-1" }, "name": "OpenAI" },
                    "local": { "npm": "@ai-sdk/openai-compatible", "options": { "baseURL": "http://localhost:1234/v1" } }
                },
                "theme": "dark"
            }"#,
        );
        let p = OpenCodePlugin::new();
        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 2);
        let openai = live
            .providers
            .iter()
            .find(|x| x.id == "openai")
            .unwrap();
        assert_eq!(openai.name, "OpenAI");
        assert_eq!(openai.settings_config["options"]["apiKey"], "sk-1");
        assert!(live.current.is_none());
    }

    #[test]
    fn read_live_rejects_non_object_root() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(temp.path(), "[1,2,3]");
        let p = OpenCodePlugin::new();
        assert!(p.read_live().is_err());
    }

    #[test]
    fn apply_writes_provider_preserving_other_fields() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(
            temp.path(),
            r#"{"provider": {"existing": {"npm": "@ai-sdk/openai"}}, "theme": "dark"}"#,
        );

        let p = OpenCodePlugin::new();
        p.apply(
            &provider(
                "newp",
                r#"{"npm":"@ai-sdk/openai-compatible","options":{"baseURL":"http://x/v1","apiKey":"k"}}"#,
            ),
            false,
        )
        .unwrap();

        let config = read_config(&config_path()).unwrap();
        assert_eq!(config["theme"], "dark");
        assert!(config["provider"]["newp"]["options"]["baseURL"].is_string());
        // 已有 provider 不受影响
        assert_eq!(config["provider"]["existing"]["npm"], "@ai-sdk/openai");
    }

    #[test]
    fn apply_current_is_additive_no_selector_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();
        p.apply(
            &provider(
                "my-prov",
                r#"{"npm":"@ai-sdk/openai-compatible"}"#,
            ),
            true,
        )
        .unwrap();

        // additive 模式：current 仅表示写入，provider 段保持为对象。
        let config = read_config(&config_path()).unwrap();
        assert!(config["provider"]["my-prov"]["npm"].is_string());
    }

    #[test]
    fn remove_provider_deletes_from_live() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(
            temp.path(),
            r#"{"provider": {"a": {"npm": "x"}, "b": {"npm": "y"}}, "theme": "dark"}"#,
        );
        let p = OpenCodePlugin::new();
        p.remove_provider("a").unwrap();
        let config = read_config(&config_path()).unwrap();
        assert!(config["provider"].get("a").is_none());
        assert!(config["provider"]["b"]["npm"].is_string());
        assert_eq!(config["theme"], "dark");
    }

    #[test]
    fn mcp_servers_roundtrip_with_format_conversion() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();

        p.set_mcp_server(&McpServerSpec {
            id: "filesystem".into(),
            name: "Filesystem".into(),
            spec: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem"]
            }),
        })
        .unwrap();

        let servers = p.get_mcp_servers().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "filesystem");
        assert_eq!(servers[0].spec["command"], "npx");
        assert_eq!(servers[0].spec["args"][1], "@modelcontextprotocol/server-filesystem");

        p.remove_mcp_server("filesystem").unwrap();
        assert!(p.get_mcp_servers().unwrap().is_empty());
    }

    #[test]
    fn apply_rejects_invalid_settings_config() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();
        let bad = provider("x", "not-json");
        assert!(p.apply(&bad, false).is_err());
        let no_fragment = provider("y", r#"{"foo":"bar"}"#);
        assert!(p.apply(&no_fragment, false).is_err());
    }

    #[test]
    fn apply_extracts_fragment_from_full_config_structure() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();
        let full = format!(
            r#"{{"$schema":"{SCHEMA_URL}","provider":{{"deep":{{"npm":"@ai-sdk/deep","options":{{"apiKey":"sk-deep"}}}}}}}}"#
        );
        p.apply(&provider("deep", &full), false).unwrap();
        let config = read_config(&config_path()).unwrap();
        assert_eq!(config["provider"]["deep"]["npm"], "@ai-sdk/deep");
    }

    #[test]
    fn import_lists_all_providers() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(
            temp.path(),
            r#"{"provider": {"a": {"npm": "x", "name": "Alpha"}, "b": {"npm": "y"}}}"#,
        );
        let p = OpenCodePlugin::new();
        let candidates = p.import().unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].id, "a");
        assert_eq!(candidates[0].name, "Alpha");
        assert_eq!(candidates[1].name, "b");
    }

    #[test]
    fn sessions_scans_sqlite_and_legacy_json() {
        let temp = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(temp.path());
        let original_xdg = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", temp.path());

        // SQLite session
        let base = temp.path().join("opencode");
        std::fs::create_dir_all(&base).unwrap();
        let conn = rusqlite::Connection::open(base.join("opencode.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, directory, time_created, time_updated) VALUES ('ses_1', '', '/proj/a', 1000, 3000)",
            [],
        )
        .unwrap();
        drop(conn);

        // Legacy JSON session
        let storage = base.join("storage").join("session").join("p1");
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::write(
            storage.join("ses_legacy.json"),
            r#"{"id":"ses_legacy","directory":"/proj/b","time":{"created":500,"updated":500}}"#,
        )
        .unwrap();

        let result = std::panic::catch_unwind(|| {
            let p = OpenCodePlugin::new();
            p.sessions().unwrap()
        });

        if let Some(v) = original_xdg {
            std::env::set_var("XDG_DATA_HOME", v);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }

        let sessions = result.unwrap();
        assert_eq!(sessions.len(), 2);
        // SQLite 优先，updated 3000 排前面
        assert_eq!(sessions[0].session_id, "ses_1");
        assert_eq!(sessions[0].title.as_deref(), Some("a"));
        assert_eq!(sessions[1].session_id, "ses_legacy");
    }

    #[test]
    fn load_messages_reads_json_storage() {
        let temp = tempfile::tempdir().unwrap();
        let storage = temp.path();
        let session_id = "ses_test";
        let msg_id = "msg_1";

        let msg_dir = storage.join("message").join(session_id);
        let part_dir = storage.join("part").join(msg_id);
        std::fs::create_dir_all(&msg_dir).unwrap();
        std::fs::create_dir_all(&part_dir).unwrap();

        std::fs::write(
            msg_dir.join(format!("{msg_id}.json")),
            r#"{"id":"msg_1","role":"assistant","time":{"created":1000}}"#,
        )
        .unwrap();
        std::fs::write(
            part_dir.join("prt_1.json"),
            r#"{"id":"prt_1","type":"tool","tool":"bash"}"#,
        )
        .unwrap();
        std::fs::write(
            part_dir.join("prt_2.json"),
            r#"{"id":"prt_2","type":"text","text":"Done"}"#,
        )
        .unwrap();

        let p = OpenCodePlugin::new();
        let msgs = p
            .load_messages(&msg_dir.display().to_string())
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "assistant");
        assert!(msgs[0].content.contains("[Tool: bash]"));
        assert!(msgs[0].content.contains("Done"));
        assert_eq!(msgs[0].ts, Some(1000));
    }

    #[test]
    fn load_messages_sqlite_reads_messages_and_parts() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE part (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, message_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session VALUES ('ses_1', 'T', '/p', 1000, 3000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message VALUES ('msg_1', 'ses_1', 1000, '{\"role\":\"user\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part VALUES ('prt_1', 'ses_1', 'msg_1', 1000, '{\"type\":\"text\",\"text\":\"Hello\"}')",
            [],
        )
        .unwrap();
        drop(conn);

        let source = format!("sqlite:{}:ses_1", db_path.display());
        let p = OpenCodePlugin::new();
        let msgs = p.load_messages(&source).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
    }

    #[test]
    fn delete_session_sqlite_removes_session_and_parts() {
        let temp = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(temp.path());
        let original_xdg = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", temp.path());

        let base = temp.path().join("opencode");
        std::fs::create_dir_all(&base).unwrap();
        let db_path = base.join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE part (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, message_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute("INSERT INTO session VALUES ('ses_1', 'T', '/p', 1000, 3000)", []).unwrap();
        conn.execute("INSERT INTO message VALUES ('msg_1', 'ses_1', 1000, '{\"role\":\"user\"}')", []).unwrap();
        conn.execute("INSERT INTO part VALUES ('prt_1', 'ses_1', 'msg_1', 1000, '{\"type\":\"text\",\"text\":\"Hi\"}')", []).unwrap();
        drop(conn);

        let source = format!("sqlite:{}:ses_1", db_path.display());
        let p = OpenCodePlugin::new();
        let deleted = p.delete_session("ses_1", &source).unwrap();
        assert!(deleted);

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM session WHERE id = 'ses_1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        drop(conn);

        if let Some(v) = original_xdg {
            std::env::set_var("XDG_DATA_HOME", v);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
    }

    #[test]
    fn add_plugin_appends_and_dedups() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = OpenCodePlugin::new();

        p.add_plugin("some-plugin").unwrap();
        p.add_plugin("some-plugin").unwrap();
        let plugins = p.get_plugins().unwrap();
        assert_eq!(plugins, vec!["some-plugin".to_string()]);
    }

    #[test]
    fn add_omo_normalizes_and_removes_conflicting() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(
            temp.path(),
            r#"{"plugin": ["oh-my-opencode@latest", "unrelated"]}"#,
        );
        let p = OpenCodePlugin::new();

        p.add_plugin("oh-my-opencode-slim@latest").unwrap();
        let plugins = p.get_plugins().unwrap();
        assert!(!plugins.iter().any(|s| s.starts_with("oh-my-opencode@")));
        assert!(plugins.iter().any(|s| s.starts_with("oh-my-opencode-slim@")));
        assert!(plugins.contains(&"unrelated".to_string()));
    }

    #[test]
    fn remove_plugin_clears_empty_array() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_config_file(temp.path(), r#"{"plugin": ["only-one"]}"#);
        let p = OpenCodePlugin::new();

        p.remove_plugin("only-one").unwrap();
        let config = read_config(&config_path()).unwrap();
        assert!(config.get("plugin").is_none());
        assert!(p.get_plugins().unwrap().is_empty());
    }

    #[test]
    fn canonicalize_omo_names() {
        assert_eq!(canonicalize_plugin_name("oh-my-opencode"), "oh-my-openagent");
        assert_eq!(
            canonicalize_plugin_name("oh-my-opencode@latest"),
            "oh-my-openagent@latest"
        );
        assert_eq!(
            canonicalize_plugin_name("oh-my-opencode-slim"),
            "oh-my-opencode-slim"
        );
        assert_eq!(canonicalize_plugin_name("other"), "other");
    }
}
