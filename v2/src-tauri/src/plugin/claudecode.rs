//! 原生 Claude Code 插件：读写 `~/.claude/settings.json`（provider 配置）、
//! `~/.claude.json`（MCP）与 `~/.claude/projects/**/*.jsonl`（会话与用量）。
//!
//! Claude Code 是非 additive 模型：live 配置就是 `~/.claude/settings.json`
//! 的完整内容，provider 的 `settings_config` 即该 JSON（含 `env` 等字段）。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::plugin::error::PluginError;
use crate::plugin::mcp::{McpPlugin, McpServerSpec};
use crate::plugin::{
    AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities, SessionMessage,
    SessionMeta, UsageRecord,
};
use crate::types::Provider;

const PLUGIN_ID: &str = "claudecode";

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

/// Claude Code 配置目录（`~/.claude`；可经设置 overrideDir.claudecode 覆盖）。
fn config_dir() -> PathBuf {
    crate::services::overrides::get(PLUGIN_ID)
        .or_else(|| override_dir("CC_SWITCH_CLAUDE_CONFIG_DIR"))
        .unwrap_or_else(|| home_dir().join(".claude"))
}

/// Claude Code 主配置文件（`~/.claude/settings.json`，兼容旧 `claude.json`）。
fn settings_path() -> PathBuf {
    let dir = config_dir();
    let settings = dir.join("settings.json");
    if settings.exists() {
        return settings;
    }
    let legacy = dir.join("claude.json");
    if legacy.exists() {
        return legacy;
    }
    settings
}

/// Claude MCP 配置文件。
///
/// 对齐 v1：自定义配置目录时 `.claude.json` 放在目录内（`<dir>/.claude.json`），
/// 默认 `~/.claude` 目录仍使用 Claude 默认的 `~/.claude.json`。
fn mcp_path() -> PathBuf {
    if let Some(dir) = crate::services::overrides::get(PLUGIN_ID) {
        let default_dir = home_dir().join(".claude");
        if dir != default_dir {
            return dir.join(".claude.json");
        }
    }
    override_dir("CC_SWITCH_CLAUDE_MCP_PATH").unwrap_or_else(|| home_dir().join(".claude.json"))
}

/// 会话目录（`~/.claude/projects`）。
fn projects_dir() -> PathBuf {
    config_dir().join("projects")
}

/// 原生 Claude Code 插件。
#[derive(Debug, Default)]
pub struct ClaudeCodePlugin;

impl ClaudeCodePlugin {
    pub fn new() -> Self {
        Self
    }
}

fn read_json(path: &Path) -> Result<Value, PluginError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(PluginError::io(path, e)),
    };
    serde_json::from_str(&content).map_err(|e| PluginError::json(path, e))
}

fn write_json(path: &Path, value: &Value) -> Result<(), PluginError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| PluginError::json(path, e))?;
    std::fs::write(path, format!("{json}\n")).map_err(|e| PluginError::io(path, e))
}

impl AgentPlugin for ClaudeCodePlugin {
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
        let path = settings_path();
        if !path.exists() {
            return Ok(LiveConfig::default());
        }
        let value = read_json(&path)?;
        Ok(LiveConfig {
            providers: vec![LiveProvider {
                id: "default".into(),
                name: "Claude Code".into(),
                settings_config: value,
            }],
            current: Some("default".into()),
        })
    }

    fn apply(&self, provider: &Provider, _current: bool) -> Result<(), PluginError> {
        let settings: Value = provider
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
        write_json(&settings_path(), &sanitize(&settings))
    }

    fn remove_provider(&self, _id: &str) -> Result<(), PluginError> {
        let path = settings_path();
        if !path.exists() {
            return Ok(());
        }
        let mut value = read_json(&path)?;
        // 移除 provider 相关的 env / apiProvider / model 字段，保留其他用户配置。
        if let Some(obj) = value.as_object_mut() {
            obj.remove("env");
            obj.remove("apiProvider");
            obj.remove("model");
        }
        write_json(&path, &value)
    }

    fn import(&self) -> Result<Vec<ImportCandidate>, PluginError> {
        let path = settings_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let value = read_json(&path)?;
        Ok(vec![ImportCandidate {
            id: "default".into(),
            name: "Claude Code".into(),
            settings_config: value,
        }])
    }

    fn sessions(&self) -> Result<Vec<SessionMeta>, PluginError> {
        let mut files = Vec::new();
        collect_jsonl_files(&projects_dir(), &mut files);

        let mut sessions = Vec::new();
        for path in files {
            if let Some(meta) = parse_session(&path) {
                sessions.push(meta);
            }
        }
        sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
        Ok(sessions)
    }

    fn load_messages(&self, source: &str) -> Result<Vec<SessionMessage>, PluginError> {
        load_messages_jsonl(Path::new(source))
    }

    fn delete_session(&self, session_id: &str, source: &str) -> Result<bool, PluginError> {
        let path = Path::new(source);
        let meta = parse_session(path)
            .ok_or_else(|| PluginError::Other("无法解析 Claude 会话元数据".into()))?;
        if meta.session_id != session_id {
            return Err(PluginError::Other(format!(
                "会话 id 不匹配: 期望 {session_id}，实际 {}",
                meta.session_id
            )));
        }
        // 删除 sidecar 目录（subagents / tool-results）。
        if let Some(stem) = path.file_stem() {
            let sibling = path.parent().unwrap_or_else(|| Path::new("")).join(stem);
            let _ = std::fs::remove_dir_all(&sibling);
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(PluginError::io(path, e)),
        }
    }

    fn as_mcp(&self) -> Option<&dyn McpPlugin> {
        Some(self)
    }

    fn prompt_file_path(&self) -> Option<PathBuf> {
        Some(config_dir().join("CLAUDE.md"))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        Some(config_dir().join("skills"))
    }

    fn read_raw_config(&self) -> Result<String, PluginError> {
        let path = settings_path();
        match std::fs::read_to_string(&path) {
            Ok(c) => Ok(c),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("{\n}\n".into()),
            Err(e) => Err(PluginError::io(&path, e)),
        }
    }

    fn write_raw_config(&self, content: &str) -> Result<(), PluginError> {
        let value: Value = serde_json::from_str(content)
            .map_err(|e| PluginError::Config(format!("JSON 解析失败，拒绝写入: {e}")))?;
        if !value.is_object() {
            return Err(PluginError::Config(
                "live 配置根节点必须是 JSON 对象".into(),
            ));
        }
        write_json(&settings_path(), &value)
    }

    fn sync_usage(&self) -> Result<Vec<UsageRecord>, PluginError> {
        let mut files = Vec::new();
        collect_jsonl_files(&projects_dir(), &mut files);

        let mut records = Vec::new();
        for path in files {
            if let Ok(Some(usage)) = parse_usage(&path) {
                records.push(usage);
            }
        }
        Ok(records)
    }
}

/// 去掉只属于 cc-switch 的内部字段（与 v1 语义一致）。
fn sanitize(settings: &Value) -> Value {
    let mut v = settings.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("api_format");
        obj.remove("apiFormat");
        obj.remove("openrouter_compat_mode");
        obj.remove("openrouterCompatMode");
    }
    v
}

// ---------------------------------------------------------------------------
// 会话扫描
// ---------------------------------------------------------------------------

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

fn parse_timestamp_ms(value: &Value) -> Option<i64> {
    value.as_str().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.timestamp_millis())
    })
}

fn is_agent_session(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("agent-"))
        .unwrap_or(false)
}

/// 提取消息文本（content 可为字符串或数组）。
fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("type").and_then(Value::as_str).and_then(|t| match t {
                    "text" => item.get("text").and_then(Value::as_str).map(str::to_string),
                    "tool_use" => item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(|n| format!("[Tool: {n}]")),
                    "tool_result" => item
                        .get("content")
                        .map(extract_tool_result_text)
                        .filter(|s| !s.is_empty()),
                    _ => None,
                })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn extract_tool_result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("type")
                    .and_then(Value::as_str)
                    .and_then(|t| match t {
                        "text" => item.get("text").and_then(Value::as_str).map(str::to_string),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    if is_agent_session(path) {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;

    let mut session_id: Option<String> = None;
    let mut project_dir: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut last_active_at: Option<i64> = None;
    let mut title: Option<String> = None;

    for line in content.lines() {
        let value: Value = serde_json::from_str(line).ok()?;
        if session_id.is_none() {
            session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if project_dir.is_none() {
            project_dir = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        let ts = value.get("timestamp").and_then(parse_timestamp_ms);
        if ts.is_some() {
            if created_at.is_none() {
                created_at = ts;
            }
            last_active_at = ts;
        }
        if title.is_none() {
            let is_user = value.get("type").and_then(Value::as_str) == Some("user")
                || value
                    .get("message")
                    .and_then(|m| m.get("role"))
                    .and_then(Value::as_str)
                    == Some("user");
            if is_user {
                if let Some(message) = value.get("message") {
                    let text = extract_text(message.get("content").unwrap_or(&Value::Null));
                    let trimmed = text.trim();
                    if !trimmed.is_empty()
                        && !trimmed.contains("<local-command-caveat>")
                        && !trimmed.starts_with("<command-name>")
                    {
                        title = Some(truncate(trimmed, 60));
                    }
                }
            }
        }
    }

    let session_id = session_id
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))?;

    Some(SessionMeta {
        session_id: session_id.clone(),
        title,
        project_dir,
        created_at,
        last_active_at,
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: Some(format!("claude --resume {session_id}")),
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

fn load_messages_jsonl(path: &Path) -> Result<Vec<SessionMessage>, PluginError> {
    let content = std::fs::read_to_string(path).map_err(|e| PluginError::io(path, e))?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let mut role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        // tool_result 包裹在 user 消息里 → 重新归类为 tool。
        if role == "user" {
            if let Some(Value::Array(items)) = message.get("content") {
                let all_tool_results = !items.is_empty()
                    && items
                        .iter()
                        .all(|i| i.get("type").and_then(Value::as_str) == Some("tool_result"));
                if all_tool_results {
                    role = "tool".into();
                }
            }
        }
        let content = extract_text(message.get("content").unwrap_or(&Value::Null));
        if content.trim().is_empty() {
            continue;
        }
        let ts = value.get("timestamp").and_then(parse_timestamp_ms);
        messages.push(SessionMessage {
            role,
            content,
            ts,
        });
    }
    Ok(messages)
}

/// 从 jsonl 解析 assistant 消息用量，聚合为一条记录。
fn parse_usage(path: &Path) -> Result<Option<UsageRecord>, PluginError> {
    let content = std::fs::read_to_string(path).map_err(|e| PluginError::io(path, e))?;
    let mut session_id: Option<String> = None;
    let mut total_input: i64 = 0;
    let mut total_output: i64 = 0;
    let mut total_cache_read: i64 = 0;
    let mut total_cache_write: i64 = 0;
    let mut model = "unknown".to_string();
    let mut timestamp_ms: i64 = 0;
    let mut message_count = 0;

    for line in content.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if session_id.is_none() {
            session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if let Some(ts) = value.get("timestamp").and_then(parse_timestamp_ms) {
            if timestamp_ms == 0 {
                timestamp_ms = ts;
            }
        }
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let Some(usage) = message.get("usage") else {
            continue;
        };
        total_input += usage
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        total_output += usage
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        total_cache_read += usage
            .get("cache_read_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        total_cache_write += usage
            .get("cache_creation_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if let Some(m) = message.get("model").and_then(Value::as_str) {
            model = m.to_string();
        }
        message_count += 1;
    }

    if message_count == 0
        || (total_input == 0
            && total_output == 0
            && total_cache_read == 0
            && total_cache_write == 0)
    {
        return Ok(None);
    }

    let session_id = session_id
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".into());

    Ok(Some(UsageRecord {
        source_id: format!("claude_session:{session_id}"),
        session_id,
        model,
        input_tokens: total_input,
        output_tokens: total_output,
        reasoning_tokens: 0,
        cache_read_tokens: total_cache_read,
        cache_write_tokens: total_cache_write,
        cost: 0.0,
        timestamp_ms,
    }))
}

impl McpPlugin for ClaudeCodePlugin {
    fn get_mcp_servers(&self) -> Result<Vec<McpServerSpec>, PluginError> {
        let root = read_json(&mcp_path())?;
        let mut servers = Vec::new();
        if let Some(map) = root.get("mcpServers").and_then(Value::as_object) {
            for (id, value) in map {
                servers.push(McpServerSpec {
                    id: id.clone(),
                    name: id.clone(),
                    spec: value.clone(),
                });
            }
        }
        servers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(servers)
    }

    fn set_mcp_server(&self, spec: &McpServerSpec) -> Result<(), PluginError> {
        let path = mcp_path();
        let mut root = read_json(&path)?;
        if !root.get("mcpServers").is_some_and(Value::is_object) {
            root["mcpServers"] = json!({});
        }
        if let Some(map) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            map.insert(spec.id.clone(), spec.spec.clone());
        }
        write_json(&path, &root)
    }

    fn remove_mcp_server(&self, id: &str) -> Result<(), PluginError> {
        let path = mcp_path();
        if !path.exists() {
            return Ok(());
        }
        let mut root = read_json(&path)?;
        if let Some(map) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            map.remove(id);
        }
        write_json(&path, &root)
    }
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

    fn write_session_file(home: &Path, name: &str, content: &str) {
        let dir = home.join(".claude").join("projects").join("proj");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn sessions_scans_projects_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_session_file(
            temp.path(),
            "session-1.jsonl",
            concat!(
                "{\"sessionId\":\"session-1\",\"cwd\":\"/tmp/proj\",\"timestamp\":\"2026-03-06T10:00:00Z\"}\n",
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"How do I deploy?\"},\"sessionId\":\"session-1\",\"timestamp\":\"2026-03-06T10:01:00Z\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":\"Here is how...\"},\"timestamp\":\"2026-03-06T10:02:00Z\"}\n",
            ),
        );

        let p = ClaudeCodePlugin::new();
        let sessions = p.sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-1");
        assert_eq!(sessions[0].title.as_deref(), Some("How do I deploy?"));
        assert_eq!(sessions[0].resume_command.as_deref(), Some("claude --resume session-1"));
    }

    #[test]
    fn load_messages_parses_roles() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let dir = temp.path().join(".claude").join("projects").join("p");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Write\",\"input\":{}}]},\"timestamp\":\"2026-03-06T10:00:00Z\"}\n",
                "{\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\",\"content\":\"ok\"}]},\"timestamp\":\"2026-03-06T10:00:01Z\"}\n",
            ),
        )
        .unwrap();

        let p = ClaudeCodePlugin::new();
        let msgs = p.load_messages(path.to_str().unwrap()).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "assistant");
        assert!(msgs[0].content.contains("[Tool: Write]"));
        assert_eq!(msgs[1].role, "tool");
    }

    #[test]
    fn sync_usage_aggregates_assistant_tokens() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        write_session_file(
            temp.path(),
            "session-u.jsonl",
            concat!(
                "{\"sessionId\":\"session-u\",\"timestamp\":\"2026-03-06T10:00:00Z\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-opus-4\",\"usage\":{\"input_tokens\":100,\"output_tokens\":50,\"cache_read_input_tokens\":10,\"cache_creation_input_tokens\":5}},\"timestamp\":\"2026-03-06T10:01:00Z\"}\n",
            ),
        );

        let p = ClaudeCodePlugin::new();
        let records = p.sync_usage().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "session-u");
        assert_eq!(records[0].model, "claude-opus-4");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_write_tokens, 5);
    }

    #[test]
    fn apply_writes_settings_json() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = HomeGuard::set(temp.path());
        let p = ClaudeCodePlugin::new();

        p.apply(
            &crate::types::Provider {
                id: "default".into(),
                plugin_id: "claudecode".into(),
                name: "default".into(),
                category: "custom".into(),
                icon: None,
                website: None,
                api_key: None,
                settings_config: Some(r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.example.com"}}"#.into()),
                meta: None,
                sort_order: 0,
                live_config_managed: true,
                created_at: String::new(),
                updated_at: String::new(),
            },
            false,
        )
        .unwrap();

        let live = p.read_live().unwrap();
        assert_eq!(live.providers.len(), 1);
        assert_eq!(live.providers[0].id, "default");
        assert_eq!(
            live.providers[0].settings_config["env"]["ANTHROPIC_BASE_URL"],
            "https://api.example.com"
        );
    }
}
