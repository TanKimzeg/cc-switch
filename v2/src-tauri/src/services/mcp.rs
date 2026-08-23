//! MCP 服务器管理服务。
//!
//! 数据源为 `mcp_servers` 表（统一 CC Switch 格式），通过 `mcp_server_apps`
//! 关联表记录每个服务器在哪些插件中启用。写操作会把服务器同步到启用插件的
//! live 配置（利用 [`crate::plugin::McpPlugin`] 的格式转换）。

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::plugin::mcp::McpServerSpec;
use crate::registry::PluginRegistry;

/// MCP 服务器记录（含各插件启用状态）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub spec: serde_json::Value,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub docs: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub apps: Vec<(String, bool)>,
}

fn row_to_server(row: &Row<'_>) -> rusqlite::Result<McpServer> {
    let spec_raw: String = row.get("server_config")?;
    let tags_raw: String = row.get("tags")?;
    Ok(McpServer {
        id: row.get("id")?,
        name: row.get("name")?,
        spec: serde_json::from_str(&spec_raw).unwrap_or(serde_json::Value::Null),
        description: row.get("description")?,
        homepage: row.get("homepage")?,
        docs: row.get("docs")?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        apps: Vec::new(),
    })
}

impl Database {
    /// 列出全部 MCP 服务器（含各插件启用状态）。
    pub fn list_mcp_servers(&self) -> rusqlite::Result<Vec<McpServer>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, server_config, description, homepage, docs, tags FROM mcp_servers ORDER BY name",
        )?;
        let mut servers = stmt
            .query_map([], row_to_server)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut app_stmt = conn.prepare(
            "SELECT mcp_server_id, plugin_id, enabled FROM mcp_server_apps ORDER BY plugin_id",
        )?;
        let rows = app_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for server in &mut servers {
            server.apps = rows
                .iter()
                .filter(|(sid, _, _)| sid == &server.id)
                .map(|(_, pid, enabled)| (pid.clone(), *enabled != 0))
                .collect();
        }
        Ok(servers)
    }

    /// 读取单个 MCP 服务器。
    pub fn get_mcp_server(&self, id: &str) -> rusqlite::Result<Option<McpServer>> {
        let server = self
            .lock()
            .query_row(
                "SELECT id, name, server_config, description, homepage, docs, tags FROM mcp_servers WHERE id = ?1",
                params![id],
                row_to_server,
            )
            .optional()?;
        if let Some(server) = server {
            let apps: Vec<(String, bool)> = self
                .lock()
                .prepare("SELECT plugin_id, enabled FROM mcp_server_apps WHERE mcp_server_id = ?1 ORDER BY plugin_id")?
                .query_map(params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            return Ok(Some(McpServer { apps, ..server }));
        }
        Ok(None)
    }

    /// 新增/更新 MCP 服务器。
    pub fn upsert_mcp_server(&self, server: &McpServer) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO mcp_servers (id, name, server_config, description, homepage, docs, tags, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               server_config = excluded.server_config,
               description = excluded.description,
               homepage = excluded.homepage,
               docs = excluded.docs,
               tags = excluded.tags,
               updated_at = excluded.updated_at",
            params![
                server.id,
                server.name,
                serde_json::to_string(&server.spec).unwrap_or_else(|_| "{}".into()),
                server.description,
                server.homepage,
                server.docs,
                serde_json::to_string(&server.tags).unwrap_or_else(|_| "[]".into()),
            ],
        )?;
        conn.execute(
            "DELETE FROM mcp_server_apps WHERE mcp_server_id = ?1",
            params![server.id],
        )?;
        for (plugin_id, enabled) in &server.apps {
            conn.execute(
                "INSERT INTO mcp_server_apps (mcp_server_id, plugin_id, enabled) VALUES (?1, ?2, ?3)",
                params![server.id, plugin_id, if *enabled { 1 } else { 0 }],
            )?;
        }
        Ok(())
    }

    /// 删除 MCP 服务器。
    pub fn delete_mcp_server(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM mcp_server_apps WHERE mcp_server_id = ?1",
            params![id],
        )?;
        let changed = conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// 更新单个服务器的某个插件启用状态。
    pub fn set_mcp_server_app_enabled(
        &self,
        server_id: &str,
        plugin_id: &str,
        enabled: bool,
    ) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let changed = conn.execute(
            "INSERT INTO mcp_server_apps (mcp_server_id, plugin_id, enabled) VALUES (?1, ?2, ?3)
             ON CONFLICT(mcp_server_id, plugin_id) DO UPDATE SET enabled = excluded.enabled",
            params![server_id, plugin_id, if enabled { 1 } else { 0 }],
        )?;
        Ok(changed > 0)
    }
}

/// MCP 服务：负责把服务器同步到各插件的 live 配置。
pub struct McpService;

impl McpService {
    /// 把服务器同步到所有启用的插件。
    pub fn sync_server_to_enabled(
        db: &Database,
        registry: &PluginRegistry,
        server: &McpServer,
    ) -> Result<(), String> {
        for (plugin_id, enabled) in &server.apps {
            if !enabled {
                continue;
            }
            Self::sync_server_to_plugin(db, registry, server, plugin_id)?;
        }
        Ok(())
    }

    /// 把服务器同步到单个插件（若插件支持 MCP）。
    ///
    /// 跳过无 MCP 能力的插件（如 TS 插件，其 live 同步由前端脚本处理），
    /// 不阻塞全局 MCP 面板的保存/勾选。
    pub fn sync_server_to_plugin(
        _db: &Database,
        registry: &PluginRegistry,
        server: &McpServer,
        plugin_id: &str,
    ) -> Result<(), String> {
        let plugin = registry
            .resolve_plugin(plugin_id)
            .map_err(|e| e.to_string())?;
        let Some(mcp_plugin) = plugin.as_mcp() else {
            return Ok(());
        };
        let spec = McpServerSpec {
            id: server.id.clone(),
            name: server.name.clone(),
            spec: server.spec.clone(),
        };
        mcp_plugin.set_mcp_server(&spec).map_err(|e| e.to_string())
    }

    /// 从指定插件移除服务器。
    ///
    /// 跳过无 MCP 能力的插件（同 [`Self::sync_server_to_plugin`]）。
    pub fn remove_server_from_plugin(
        _db: &Database,
        registry: &PluginRegistry,
        server_id: &str,
        plugin_id: &str,
    ) -> Result<(), String> {
        let plugin = registry
            .resolve_plugin(plugin_id)
            .map_err(|e| e.to_string())?;
        let Some(mcp_plugin) = plugin.as_mcp() else {
            return Ok(());
        };
        mcp_plugin
            .remove_mcp_server(server_id)
            .map_err(|e| e.to_string())
    }

    /// 从所有（此前）启用的插件移除服务器。
    pub fn remove_server_from_enabled(
        _db: &Database,
        registry: &PluginRegistry,
        server: &McpServer,
    ) -> Result<(), String> {
        for (plugin_id, enabled) in &server.apps {
            if !enabled {
                continue;
            }
            Self::remove_server_from_plugin(_db, registry, &server.id, plugin_id)?;
        }
        Ok(())
    }

    /// 落库并把服务器同步到启用插件的 live 配置。
    ///
    /// 对齐 v1：编辑时被取消勾选的插件，需从其 live 配置移除该服务器，
    /// 避免残留脏数据。
    pub fn upsert_server_full(
        db: &Database,
        registry: &PluginRegistry,
        server: &McpServer,
    ) -> Result<(), String> {
        let previous = db.get_mcp_server(&server.id).map_err(|e| e.to_string())?;
        db.upsert_mcp_server(server).map_err(|e| e.to_string())?;
        Self::sync_server_to_enabled(db, registry, server)?;
        if let Some(prev) = previous {
            let disabled: Vec<String> = prev
                .apps
                .iter()
                .filter(|(_, enabled)| *enabled)
                .map(|(pid, _)| pid.clone())
                .filter(|pid| !server.apps.iter().any(|(p, en)| p == pid && *en))
                .collect();
            for plugin_id in disabled {
                Self::remove_server_from_plugin(db, registry, &server.id, &plugin_id)?;
            }
        }
        Ok(())
    }

    /// 从插件 live 配置导入单个服务器到统一表。
    ///
    /// 对齐 v1 合并语义：记录已存在时仅置位当前插件的启用标志，
    /// 不覆盖其它插件的启用状态，也不覆盖已有配置。
    pub fn import_spec_with_merge(
        db: &Database,
        plugin_id: &str,
        spec: &McpServerSpec,
    ) -> Result<(), String> {
        let server = match db.get_mcp_server(&spec.id).map_err(|e| e.to_string())? {
            Some(mut existing) => {
                match existing.apps.iter_mut().find(|(pid, _)| pid == plugin_id) {
                    Some(slot) => slot.1 = true,
                    None => existing.apps.push((plugin_id.to_string(), true)),
                }
                existing
            }
            None => McpServer {
                id: spec.id.clone(),
                name: spec.name.clone(),
                spec: spec.spec.clone(),
                description: None,
                homepage: None,
                docs: None,
                tags: vec![],
                apps: vec![(plugin_id.to_string(), true)],
            },
        };
        db.upsert_mcp_server(&server).map_err(|e| e.to_string())
    }

    /// 把某插件启用的全部 MCP 服务器重新投影到其 live 配置（best-effort）。
    ///
    /// 对齐 v1：切换供应商后重写该应用的 MCP 现场，避免切换丢失 MCP 配置。
    /// 无 `as_mcp` 能力的插件跳过；单条失败不中断，聚合返回错误。
    pub fn project_all_for_plugin(
        db: &Database,
        registry: &PluginRegistry,
        plugin_id: &str,
    ) -> Result<(), String> {
        let servers = db.list_mcp_servers().map_err(|e| e.to_string())?;
        let mut errors = Vec::new();
        for server in servers {
            let enabled = server
                .apps
                .iter()
                .any(|(pid, en)| pid == plugin_id && *en);
            if !enabled {
                continue;
            }
            if let Err(e) = Self::sync_server_to_plugin(db, registry, &server, plugin_id) {
                errors.push(format!("{}: {e}", server.id));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        Database::new(&dir.path().join("test.db")).unwrap()
    }

    fn sample_server() -> McpServer {
        McpServer {
            id: "filesystem".into(),
            name: "Filesystem".into(),
            spec: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem"]
            }),
            description: Some("Local filesystem".into()),
            homepage: None,
            docs: None,
            tags: vec!["local".into()],
            apps: vec![("opencode".into(), true)],
        }
    }

    #[test]
    fn mcp_server_roundtrip() {
        let db = open_db();
        let server = sample_server();
        db.upsert_mcp_server(&server).unwrap();

        let stored = db.get_mcp_server("filesystem").unwrap().unwrap();
        assert_eq!(stored.name, "Filesystem");
        assert_eq!(stored.spec["command"], "npx");
        assert_eq!(stored.tags, vec!["local"]);
        assert_eq!(stored.apps, vec![("opencode".to_string(), true)]);

        let list = db.list_mcp_servers().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].apps, vec![("opencode".to_string(), true)]);
    }

    #[test]
    fn mcp_server_update_preserves_id() {
        let db = open_db();
        let mut server = sample_server();
        db.upsert_mcp_server(&server).unwrap();

        server.spec = serde_json::json!({ "type": "sse", "url": "https://x/mcp" });
        server.apps = vec![("opencode".into(), false)];
        db.upsert_mcp_server(&server).unwrap();

        let stored = db.get_mcp_server("filesystem").unwrap().unwrap();
        assert_eq!(stored.spec["url"], "https://x/mcp");
        assert_eq!(stored.apps, vec![("opencode".to_string(), false)]);
    }

    #[test]
    fn mcp_server_delete() {
        let db = open_db();
        db.upsert_mcp_server(&sample_server()).unwrap();
        assert!(db.delete_mcp_server("filesystem").unwrap());
        assert!(db.get_mcp_server("filesystem").unwrap().is_none());
        assert!(!db.delete_mcp_server("filesystem").unwrap());
    }

    #[test]
    fn sync_skips_plugins_without_mcp_capability() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        let registry = crate::registry::PluginRegistry::new(
            dir.path().join("plugins"),
            db.clone(),
        );

        // TS 插件（TsPluginStub 无 as_mcp）：同步应跳过而非报错。
        let plugins = dir.path().join("plugins/ts-demo");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(
            plugins.join("manifest.json"),
            r#"{
                "id": "ts-demo",
                "name": "TS Demo",
                "version": "0.1.0",
                "apiVersion": "1",
                "entry": { "type": "ts", "main": "main.js" }
            }"#,
        )
        .unwrap();

        let server = sample_server();
        // 断言不报错（TS 插件被跳过，返回 Ok）。
        McpService::sync_server_to_plugin(&db, &registry, &server, "ts-demo").unwrap();
        McpService::remove_server_from_plugin(&db, &registry, &server.id, "ts-demo").unwrap();
    }

    #[test]
    fn upsert_with_empty_apps_succeeds() {
        // 复现「添加 MCP 点击保存报错」：apps 为空时同步循环为空，应成功落库。
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        let registry = crate::registry::PluginRegistry::new(
            dir.path().join("plugins"),
            db.clone(),
        );

        let server = McpServer {
            id: "fs".into(),
            name: "Filesystem".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "npx" }),
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
            apps: vec![],
        };
        db.upsert_mcp_server(&server).unwrap();
        McpService::sync_server_to_enabled(&db, &registry, &server).unwrap();

        let stored = db.get_mcp_server("fs").unwrap().unwrap();
        assert_eq!(stored.name, "Filesystem");
        assert!(stored.apps.is_empty());
    }

    /// 构造带内置插件注册表的隔离环境（CC_SWITCH_TEST_HOME 指向临时目录，
    /// env_lock 串行化环境变量）。
    struct TestEnv {
        temp: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestEnv {
        fn path(&self) -> &std::path::Path {
            self.temp.path()
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(v) => std::env::set_var("CC_SWITCH_TEST_HOME", v),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    fn home_env() -> (
        TestEnv,
        Database,
        crate::registry::PluginRegistry,
    ) {
        let lock = crate::test_support::env_lock().lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());
        let env = TestEnv {
            temp,
            previous,
            _lock: lock,
        };

        let db = Database::new(&env.temp.path().join("cc.db")).unwrap();
        let registry =
            crate::registry::PluginRegistry::new(env.temp.path().join("plugins"), db.clone());
        let _ = registry.seed_builtin();
        (env, db, registry)
    }

    fn opencode_config(home: &std::path::Path) -> serde_json::Value {
        let path = home.join(".config").join("opencode").join("opencode.json");
        let raw = std::fs::read_to_string(path).unwrap_or_default();
        if raw.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&raw).unwrap()
        }
    }

    #[test]
    fn upsert_server_full_removes_from_unchecked_plugin_live() {
        let (env, db, registry) = home_env();

        // 模拟已安装的 OpenCode
        let oc_dir = env.path().join(".config").join("opencode");
        std::fs::create_dir_all(&oc_dir).unwrap();

        let mut server = sample_server();
        server.apps = vec![("opencode".into(), true)];
        McpService::upsert_server_full(&db, &registry, &server).unwrap();
        assert!(opencode_config(env.path())["mcp"]["filesystem"].is_object());

        // 编辑后取消勾选 opencode → 应从其 live 移除，且关联行删除
        let mut updated = sample_server();
        updated.apps = vec![];
        McpService::upsert_server_full(&db, &registry, &updated).unwrap();
        let config = opencode_config(env.path());
        assert!(
            config["mcp"].is_null()
                || config["mcp"]
                    .get("filesystem")
                    .is_none()
        );
        assert!(
            db.get_mcp_server("filesystem")
                .unwrap()
                .unwrap()
                .apps
                .is_empty()
        );
    }

    #[test]
    fn import_spec_with_merge_preserves_existing_state() {
        let (env, db, _registry) = home_env();
        let _keep = env;

        // 预置记录：opencode 启用、claudecode 禁用、已有配置
        let existing = McpServer {
            id: "shared".into(),
            name: "Old Name".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "old" }),
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
            apps: vec![("opencode".into(), true), ("claudecode".into(), false)],
        };
        db.upsert_mcp_server(&existing).unwrap();

        // 从 claudecode 导入同名服务器（新 spec）→ 仅置位 claudecode 标志，
        // 不覆盖 name/spec，也不动 opencode 的启用状态。
        let spec = McpServerSpec {
            id: "shared".into(),
            name: "New Name".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "new" }),
        };
        McpService::import_spec_with_merge(&db, "claudecode", &spec).unwrap();

        let stored = db.get_mcp_server("shared").unwrap().unwrap();
        // get_mcp_server 的 apps 按 plugin_id 排序，做无序比较。
        let apps: std::collections::BTreeMap<String, bool> =
            stored.apps.iter().cloned().collect();
        assert_eq!(stored.name, "Old Name");
        assert_eq!(stored.spec["command"], "old");
        assert_eq!(apps.get("opencode"), Some(&true));
        assert_eq!(apps.get("claudecode"), Some(&true));
    }

    #[test]
    fn project_all_for_plugin_rewrites_enabled_servers_only() {
        let (env, db, registry) = home_env();
        let oc_dir = env.path().join(".config").join("opencode");
        std::fs::create_dir_all(&oc_dir).unwrap();

        let enabled = sample_server(); // filesystem，启用 opencode
        db.upsert_mcp_server(&enabled).unwrap();
        let disabled = McpServer {
            id: "not-enabled".into(),
            name: "NotEnabled".into(),
            spec: serde_json::json!({ "type": "stdio", "command": "x" }),
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
            apps: vec![],
        };
        db.upsert_mcp_server(&disabled).unwrap();

        McpService::project_all_for_plugin(&db, &registry, "opencode").unwrap();

        let config = opencode_config(env.path());
        assert!(config["mcp"]["filesystem"].is_object());
        assert!(config["mcp"].get("not-enabled").is_none());
    }
}
