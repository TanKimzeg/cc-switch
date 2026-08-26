//! 行为锁测试：锁定现有序列化契约与语义，供重构期间对拍。
//!
//! 原则：本文件只“锁定”当前行为，绝不改变行为。重构任何结构类型时，
//! 若这些测试失败，说明行为被无意改变，必须停下来评估。

use std::collections::HashMap;
use std::str::FromStr;

use cc_switch_lib::{update_settings, AppSettings, AppType, McpApps, McpServer, MultiAppConfig};
use serde_json::json;

// ===== AppType 字符串契约 =====

#[test]
fn locks_app_type_as_str_values() {
    assert_eq!(AppType::Claude.as_str(), "claude");
    assert_eq!(AppType::ClaudeDesktop.as_str(), "claude-desktop");
    assert_eq!(AppType::Codex.as_str(), "codex");
    assert_eq!(AppType::Gemini.as_str(), "gemini");
    assert_eq!(AppType::GrokBuild.as_str(), "grokbuild");
    assert_eq!(AppType::OpenCode.as_str(), "opencode");
    assert_eq!(AppType::OpenClaw.as_str(), "openclaw");
    assert_eq!(AppType::Hermes.as_str(), "hermes");
}

#[test]
fn locks_app_type_from_str_aliases() {
    assert_eq!(
        "claude-desktop".parse::<AppType>().unwrap(),
        AppType::ClaudeDesktop
    );
    assert_eq!(
        "claude_desktop".parse::<AppType>().unwrap(),
        AppType::ClaudeDesktop
    );
    assert_eq!(
        "claudeDesktop".parse::<AppType>().unwrap(),
        AppType::ClaudeDesktop
    );
    assert_eq!("grok-build".parse::<AppType>().unwrap(), AppType::GrokBuild);
    assert_eq!("grok_build".parse::<AppType>().unwrap(), AppType::GrokBuild);
    assert_eq!("grok".parse::<AppType>().unwrap(), AppType::GrokBuild);
    assert!(AppType::from_str("unknown").is_err());
}

#[test]
fn locks_app_type_serde_round_trip() {
    for app in [
        AppType::Claude,
        AppType::ClaudeDesktop,
        AppType::Codex,
        AppType::Gemini,
    ] {
        let json = serde_json::to_string(&app).unwrap();
        let back: AppType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, app, "round-trip failed for {json}");
    }
    // claude-desktop 序列化用连字符，兼容别名反序列化
    let parsed: AppType = serde_json::from_str(r#""claudeDesktop""#).unwrap();
    assert_eq!(parsed, AppType::ClaudeDesktop);
}

// ===== McpApps JSON 契约 =====

#[test]
fn locks_mcp_apps_json_shape() {
    let apps = McpApps {
        claude: true,
        codex: false,
        gemini: true,
        grokbuild: false,
        opencode: true,
        hermes: false,
    };
    let value = serde_json::to_value(&apps).unwrap();
    assert_eq!(
        value,
        json!({
            "claude": true,
            "codex": false,
            "gemini": true,
            "grokbuild": false,
            "opencode": true,
            "hermes": false,
        })
    );
    // 缺省字段反序列化为 false
    let parsed: McpApps = serde_json::from_value(json!({ "claude": true })).unwrap();
    assert!(parsed.claude);
    assert!(!parsed.codex);
    assert!(!parsed.gemini);
    assert!(!parsed.grokbuild);
    assert!(!parsed.opencode);
    assert!(!parsed.hermes);
}

#[test]
fn locks_skill_apps_json_shape() {
    let apps = cc_switch_lib::SkillApps {
        claude: true,
        codex: false,
        gemini: true,
        grokbuild: false,
        opencode: true,
        hermes: false,
    };
    let value = serde_json::to_value(&apps).unwrap();
    assert_eq!(
        value,
        json!({
            "claude": true,
            "codex": false,
            "gemini": true,
            "grokbuild": false,
            "opencode": true,
            "hermes": false,
        })
    );
}

// ===== McpApps / SkillApps 语义 =====

#[test]
fn locks_mcp_apps_is_enabled_semantics() {
    let mut apps = McpApps::default();
    apps.set_enabled_for(&AppType::Claude, true);
    apps.set_enabled_for(&AppType::GrokBuild, true);
    assert!(apps.is_enabled_for(&AppType::Claude));
    assert!(apps.is_enabled_for(&AppType::GrokBuild));
    assert!(!apps.is_enabled_for(&AppType::Codex));
    // OpenClaw / ClaudeDesktop 不支持 MCP，恒为 false 且 set 是空操作
    assert!(!apps.is_enabled_for(&AppType::OpenClaw));
    assert!(!apps.is_enabled_for(&AppType::ClaudeDesktop));
    apps.set_enabled_for(&AppType::OpenClaw, true);
    assert!(!apps.is_enabled_for(&AppType::OpenClaw));
}

#[test]
fn locks_skill_apps_is_enabled_semantics() {
    let mut apps = cc_switch_lib::SkillApps::default();
    apps.set_enabled_for(&AppType::Hermes, true);
    assert!(apps.is_enabled_for(&AppType::Hermes));
    assert!(!apps.is_enabled_for(&AppType::OpenClaw));
    assert!(!apps.is_enabled_for(&AppType::ClaudeDesktop));
    apps.set_enabled_for(&AppType::OpenClaw, true);
    assert!(!apps.is_enabled_for(&AppType::OpenClaw));
}

#[test]
fn locks_skill_apps_only_and_from_labels() {
    let only = cc_switch_lib::SkillApps::only(&AppType::Codex);
    assert!(only.is_enabled_for(&AppType::Codex));
    assert!(!only.is_enabled_for(&AppType::Claude));

    let from_labels = cc_switch_lib::SkillApps::from_labels(&[
        "claude".to_string(),
        "agents".to_string(),
        "gemini".to_string(),
    ]);
    assert!(from_labels.is_enabled_for(&AppType::Claude));
    assert!(from_labels.is_enabled_for(&AppType::Gemini));
    assert!(!from_labels.is_enabled_for(&AppType::Codex));
}

// ===== MultiAppConfig（McpRoot / PromptRoot / CommonConfigSnippets）契约 =====

fn sample_mcp_server() -> McpServer {
    McpServer {
        id: "server-1".to_string(),
        name: "Server One".to_string(),
        server: json!({ "command": "npx", "args": ["foo"] }),
        apps: {
            let mut apps = McpApps::default();
            apps.set_enabled_for(&AppType::Claude, true);
            apps
        },
        description: Some("desc".to_string()),
        homepage: None,
        docs: None,
        tags: vec!["tag".to_string()],
    }
}

#[test]
fn locks_multiapp_config_json_shape() {
    let mut config = MultiAppConfig::default();
    let mut servers = HashMap::new();
    servers.insert("server-1".to_string(), sample_mcp_server());
    config.mcp.servers = Some(servers);

    let value = serde_json::to_value(&config).unwrap();
    let obj = value.as_object().unwrap();
    // 顶层版本与 flatten 的 apps
    assert_eq!(obj.get("version"), Some(&json!(2)));
    assert!(obj.contains_key("claude"));
    assert!(obj.contains_key("codex"));
    // mcp 统一结构
    let mcp = obj.get("mcp").unwrap().as_object().unwrap();
    assert!(mcp.contains_key("servers"));
    let server = mcp
        .get("servers")
        .unwrap()
        .get("server-1")
        .unwrap()
        .as_object()
        .unwrap();
    assert_eq!(server.get("id"), Some(&json!("server-1")));
    let apps = server.get("apps").unwrap();
    assert_eq!(apps.get("claude"), Some(&json!(true)));
    assert_eq!(apps.get("codex"), Some(&json!(false)));

    // 往返
    let parsed: MultiAppConfig = serde_json::from_value(value).unwrap();
    let again = serde_json::to_value(&parsed).unwrap();
    assert_eq!(serde_json::to_value(&config).unwrap(), again);
}

#[test]
fn locks_multiapp_config_deserializes_legacy_aliases() {
    // 旧配置里 claude-desktop 可能写成 claudeDesktop / claude_desktop，每个别名单独出现
    for key in ["claudeDesktop", "claude_desktop"] {
        let mut mcp = serde_json::Map::new();
        mcp.insert("servers".to_string(), serde_json::Value::Null);
        mcp.insert(
            key.to_string(),
            json!({ "servers": { "legacy-1": {
                "name": "Legacy",
                "server": { "command": "npx" },
                "apps": { "claude": true }
            } } }),
        );
        let raw = serde_json::json!({
            "version": 2,
            "mcp": serde_json::Value::Object(mcp),
            "prompts": {},
            "common_config_snippets": {}
        });
        let parsed: MultiAppConfig = serde_json::from_value(raw).unwrap();
        let servers = parsed.mcp.claude_desktop.servers;
        assert!(
            servers.contains_key("legacy-1"),
            "alias {key} should map to claude_desktop"
        );
    }
    // 同一字段出现多个别名会报 duplicate field（当前真实行为，锁定之）
    let raw = json!({
        "version": 2,
        "mcp": { "claudeDesktop": { "servers": {} }, "claude_desktop": { "servers": {} } },
        "prompts": {},
        "common_config_snippets": {}
    });
    assert!(serde_json::from_value::<MultiAppConfig>(raw).is_err());
}

// ===== AppSettings（VisibleApps / 目录覆盖 / current_provider）契约 =====

#[test]
fn locks_visible_apps_json_shape_and_aliases() {
    let settings = AppSettings {
        visible_apps: Some(Default::default()),
        ..Default::default()
    };
    let value = serde_json::to_value(&settings).unwrap();
    let visible = value.get("visibleApps").unwrap();
    // hermes 默认 false，其余默认 true
    assert_eq!(visible.get("claude"), Some(&json!(true)));
    assert_eq!(visible.get("claude-desktop"), Some(&json!(true)));
    assert_eq!(visible.get("codex"), Some(&json!(true)));
    assert_eq!(visible.get("gemini"), Some(&json!(true)));
    assert_eq!(visible.get("grokbuild"), Some(&json!(true)));
    assert_eq!(visible.get("opencode"), Some(&json!(true)));
    assert_eq!(visible.get("openclaw"), Some(&json!(true)));
    assert_eq!(visible.get("hermes"), Some(&json!(false)));

    // 旧格式字段名别名兼容
    let parsed: AppSettings = serde_json::from_value(json!({
        "visibleApps": {
            "claudeDesktop": false,
            "claude": true
        }
    }))
    .unwrap();
    let visible = parsed.visible_apps.unwrap();
    assert!(!visible.is_visible(&AppType::ClaudeDesktop));
    assert!(visible.is_visible(&AppType::Claude));
}

#[test]
fn locks_current_provider_and_config_dir_settings() {
    let settings = AppSettings {
        current_provider_claude: Some("p1".to_string()),
        current_provider_codex: Some("p2".to_string()),
        grok_config_dir: Some("/tmp/grok".to_string()),
        ..Default::default()
    };
    let value = serde_json::to_value(&settings).unwrap();
    assert_eq!(value.get("currentProviderClaude"), Some(&json!("p1")));
    assert_eq!(value.get("currentProviderCodex"), Some(&json!("p2")));
    assert_eq!(value.get("grokConfigDir"), Some(&json!("/tmp/grok")));

    // settings 文件往返（写入后从磁盘解析回来）
    let home = std::env::temp_dir().join("cc-switch-behavior-lock");
    std::fs::create_dir_all(&home).ok();
    std::env::set_var("CC_SWITCH_TEST_HOME", &home);
    update_settings(settings).unwrap();
    let path = home.join(".cc-switch").join("settings.json");
    let content = std::fs::read_to_string(&path).expect("settings.json written");
    let reloaded: AppSettings = serde_json::from_str(&content).unwrap();
    assert_eq!(reloaded.current_provider_claude.as_deref(), Some("p1"));
    assert_eq!(reloaded.current_provider_codex.as_deref(), Some("p2"));
    assert_eq!(reloaded.grok_config_dir.as_deref(), Some("/tmp/grok"));
}
