//! 原生 OpenCode 插件：读写 `~/.config/opencode/opencode.json` 与会话。
//!
//! OpenCode 是「加性（additive）」配置模型：live 配置的 `provider` 字段是一个
//! 对象，键为 provider id，值为该 provider 的配置片段（npm/options/models）。
//! 切换 = 把目标 provider 的片段 upsert 进 `provider`，同时（可选）更新
//! 顶层 `provider` 选择器字段为当前 provider 的 npm 包名。

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::plugin::error::PluginError;
use crate::plugin::{
    AgentPlugin, ImportCandidate, LiveConfig, LiveProvider, PluginCapabilities, SessionMeta,
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

/// OpenCode 配置目录（`~/.config/opencode`）。
fn config_dir() -> PathBuf {
    home_dir().join(".config").join("opencode")
}

/// OpenCode 数据目录（会话等；遵循 XDG_DATA_HOME，兜底 `~/.local/share/opencode`）。
fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("opencode");
        }
    }
    home_dir().join(".local").join("share").join("opencode")
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
            import: true,
            sessions: true,
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

        if current {
            // OpenCode 顶层 `provider` 选择器：字符串形式指定默认 provider。
            config["provider"] = json!(provider.id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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
    fn apply_with_current_sets_top_level_selector() {
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

        let config = read_config(&config_path()).unwrap();
        assert_eq!(config["provider"], "my-prov");
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
}
