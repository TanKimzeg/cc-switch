//! Profiles（项目配置方案）服务——对齐 v1 的「项目快照」语义。
//!
//! Profile 是全应用共享的命名项目实体；payload 按**插件**分槽存选择快照
//! （当前供应商 / 启用的 MCP / 启用的 Skills / 激活的 Prompt）。快照与应用
//! 均按**单个插件**（scope = plugin_id）操作：各插件可独立指向自己的项目，
//! 互不牵连。应用（apply）复用现有切换原语批量落地，best-effort：
//! 单项失败收集为 warning 继续，不整体回滚。
//!
//! None 与空集严格区分（对齐 v1）：插件未出现在 payload 中 = 从未拍过
//! （应用时不动该插件）；拍到的空集 = 应用时清空启用。

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Database;
use crate::registry::PluginRegistry;
use crate::services::skills::{
    remove_skill_from_dir, ssot_dir, sync_skill_to_dir, SkillService,
};
use crate::AppPaths;

/// 当前 profile 的 settings 键前缀（`profile.current.<plugin_id>`）。
pub const CURRENT_PROFILE_KEY_PREFIX: &str = "profile.current.";

fn current_profile_key(plugin_id: &str) -> String {
    format!("{CURRENT_PROFILE_KEY_PREFIX}{plugin_id}")
}

/// Profile 记录（payload 为 [`PluginProfileSnapshot`] 按插件 id 的映射）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub payload: serde_json::Value,
    pub sort_order: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

/// 单个插件的选择快照。槽位 Option 语义见模块注释。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginProfileSnapshot {
    /// 当前供应商 id。
    pub provider: Option<String>,
    /// 启用的 MCP server id 集合。
    pub mcp_enabled_ids: Option<Vec<String>>,
    /// 启用的 Skill id 集合。
    pub skill_enabled_ids: Option<Vec<String>>,
    /// 激活的 prompt id。
    pub active_prompt_id: Option<String>,
}

/// payload 形状：插件 id → 快照（缺失键 = 未拍摄）。
pub type ProfilePayloadMap = BTreeMap<String, PluginProfileSnapshot>;

fn row_to_profile(row: &Row<'_>) -> rusqlite::Result<Profile> {
    let payload_raw: String = row.get("payload")?;
    Ok(Profile {
        id: row.get("id")?,
        name: row.get("name")?,
        payload: serde_json::from_str(&payload_raw).unwrap_or(serde_json::Value::Null),
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

impl Database {
    /// 列出全部 profiles。
    pub fn list_profiles(&self) -> rusqlite::Result<Vec<Profile>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, payload, sort_order, created_at, updated_at FROM profiles ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([], row_to_profile)?;
        rows.collect()
    }

    /// 读取单个 profile。
    pub fn get_profile(&self, id: &str) -> rusqlite::Result<Option<Profile>> {
        self.lock()
            .query_row(
                "SELECT id, name, payload, sort_order, created_at, updated_at FROM profiles WHERE id = ?1",
                params![id],
                row_to_profile,
            )
            .optional()
    }

    /// 新增/更新 profile。
    pub fn upsert_profile(&self, profile: &Profile) -> rusqlite::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let payload = serde_json::to_string(&profile.payload).unwrap_or_else(|_| "{}".into());
        self.lock().execute(
            "INSERT INTO profiles (id, name, payload, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               payload = excluded.payload,
               sort_order = excluded.sort_order,
               updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.name,
                payload,
                profile.sort_order.unwrap_or(0),
                now,
            ],
        )?;
        Ok(())
    }

    /// 删除 profile；同时清除所有指向它的插件 current 指针。
    pub fn delete_profile(&self, id: &str) -> rusqlite::Result<bool> {
        let changed = self.lock().execute(
            "DELETE FROM profiles WHERE id = ?1",
            params![id],
        )?;
        if changed > 0 {
            self.clear_current_profile_if_matches(id)?;
        }
        Ok(changed > 0)
    }

    /// 读取某插件当前激活的 profile id。
    pub fn current_profile_for_plugin(&self, plugin_id: &str) -> rusqlite::Result<Option<String>> {
        self.get_setting(&current_profile_key(plugin_id))
    }

    /// 设置某插件当前激活的 profile id。
    pub fn set_current_profile_for_plugin(
        &self,
        plugin_id: &str,
        id: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.set_setting(&current_profile_key(plugin_id), id.unwrap_or(""))
    }

    /// 清除所有等于给定 profile id 的插件 current 指针。
    fn clear_current_profile_if_matches(&self, profile_id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT key, value FROM settings WHERE key LIKE 'profile.current.%' AND value = ?1",
        )?;
        let keys: Vec<String> = stmt
            .query_map(params![profile_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for key in keys {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        }
        Ok(())
    }
}

// ============================================================================
// ProfileService：快照 / 应用编排
// ============================================================================

pub struct ProfileService;

/// 计算从当前启用状态到目标集合的最小 toggle 集（对齐 v1 plan_toggles）。
/// 返回 (需要执行的 (id, enabled) 列表, 目标中已不存在于 DB 的悬空 id 列表)。
fn plan_toggles(
    current: &[(String, bool)],
    target_ids: &[String],
) -> (Vec<(String, bool)>, Vec<String>) {
    let existing: std::collections::HashSet<&str> =
        current.iter().map(|(id, _)| id.as_str()).collect();
    let target: std::collections::HashSet<&str> =
        target_ids.iter().map(|s| s.as_str()).collect();

    let toggles = current
        .iter()
        .filter(|(id, enabled)| target.contains(id.as_str()) != *enabled)
        .map(|(id, enabled)| (id.clone(), !enabled))
        .collect();

    let dangling = target_ids
        .iter()
        .filter(|id| !existing.contains(id.as_str()))
        .cloned()
        .collect();

    (toggles, dangling)
}

impl ProfileService {
    fn parse_payload(profile: &Profile) -> Result<ProfilePayloadMap, String> {
        if profile.payload.is_null() {
            return Ok(BTreeMap::new());
        }
        serde_json::from_value(profile.payload.clone())
            .map_err(|e| format!("解析 profile payload 失败: {e}"))
    }

    /// 抓取单插件的当前配置状态生成快照。
    pub fn snapshot_plugin(
        db: &Database,
        registry: &PluginRegistry,
        plugin_id: &str,
    ) -> Result<PluginProfileSnapshot, String> {
        let _plugin = registry
            .resolve_plugin(plugin_id)
            .map_err(|e| e.to_string())?;

        let provider: Option<String> = db
            .lock()
            .query_row(
                "SELECT current_provider_id FROM app_state WHERE plugin_id = ?1",
                params![plugin_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap_or(None);

        let mcp_enabled_ids: Option<Vec<String>> = Some(
            db.list_mcp_servers()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|s| s.apps.iter().any(|(pid, en)| pid == plugin_id && *en))
                .map(|s| s.id)
                .collect(),
        );

        let skill_enabled_ids: Option<Vec<String>> = Some(
            db.list_skills()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|s| s.enabled_plugins.iter().any(|pid| pid == plugin_id))
                .map(|s| s.id)
                .collect(),
        );

        let active_prompt_id: Option<String> = {
            let prompts = db
                .list_prompts(Some(plugin_id))
                .map_err(|e| e.to_string())?;
            prompts.into_iter().find(|p| p.enabled).map(|p| p.id)
        };

        Ok(PluginProfileSnapshot {
            provider,
            mcp_enabled_ids,
            skill_enabled_ids,
            active_prompt_id,
        })
    }

    /// 创建新项目：只拍发起插件的当前状态。
    pub fn create(
        db: &Database,
        registry: &PluginRegistry,
        name: &str,
        plugin_id: &str,
    ) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("项目名称不能为空".into());
        }
        let snapshot = Self::snapshot_plugin(db, registry, plugin_id)?;
        let mut payload: ProfilePayloadMap = BTreeMap::new();
        payload.insert(plugin_id.to_string(), snapshot);
        let now = chrono::Utc::now().timestamp();
        let profile = Profile {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            payload: serde_json::to_value(payload).map_err(|e| e.to_string())?,
            sort_order: Some(0),
            created_at: Some(now),
            updated_at: Some(now),
        };
        db.upsert_profile(&profile).map_err(|e| e.to_string())?;
        Ok(profile)
    }

    /// 更新项目：重命名（作用于共享实体）和/或以当前状态重拍某插件槽位。
    pub fn update(
        db: &Database,
        registry: &PluginRegistry,
        id: &str,
        name: Option<String>,
        resnapshot_plugin: Option<&str>,
    ) -> Result<Profile, String> {
        let mut profile = db
            .get_profile(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("profile 不存在: {id}"))?;

        if let Some(name) = name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err("项目名称不能为空".into());
            }
            profile.name = name;
        }
        if let Some(plugin_id) = resnapshot_plugin {
            let mut payload = Self::parse_payload(&profile)?;
            payload.insert(
                plugin_id.to_string(),
                Self::snapshot_plugin(db, registry, plugin_id)?,
            );
            profile.payload = serde_json::to_value(payload).map_err(|e| e.to_string())?;
        }
        db.upsert_profile(&profile).map_err(|e| e.to_string())?;
        Ok(profile)
    }

    /// 应用项目快照到指定插件（best-effort，返回 warnings）。
    ///
    /// - 切换前自动保存旧项目（仅该插件槽位），失败不阻塞切换；
    /// - 该插件从未拍过快照时不改动任何配置，仅标记 current 并提示；
    /// - Provider 走 `switch_provider_core`（apply live + 记录 app_state）；
    /// - MCP/Skills 最小 toggle 差异；Prompt 已激活则幂等跳过。
    pub fn apply(
        db: &Database,
        registry: &PluginRegistry,
        paths: &AppPaths,
        profile_id: &str,
        plugin_id: &str,
    ) -> Result<Vec<String>, String> {
        let mut warnings = Vec::new();

        // 自动保存旧项目当前状态（仅本插件槽位），失败不阻塞切换。
        if let Some(current_id) = db.current_profile_for_plugin(plugin_id).map_err(|e| e.to_string())? {
            if current_id != profile_id {
                if let Err(e) = Self::update(db, registry, &current_id, None, Some(plugin_id)) {
                    warnings.push(format!("自动保存旧项目 '{current_id}' 失败: {e}"));
                }
            }
        }

        let profile = db
            .get_profile(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("profile 不存在: {profile_id}"))?;
        let payload = Self::parse_payload(&profile)?;

        let Some(snapshot) = payload.get(plugin_id) else {
            // 未拍过快照：仅标记 current，切走时会自动补拍。
            db.set_current_profile_for_plugin(plugin_id, Some(profile_id))
                .map_err(|e| e.to_string())?;
            warnings.push(format!(
                "插件 '{plugin_id}' 尚未在该项目中保存过配置，已标记为当前项目（切走时会自动保存）"
            ));
            return Ok(warnings);
        };

        // 1. 供应商
        if let Some(target_pid) = &snapshot.provider {
            let exists = {
                let conn = db.lock();
                conn.query_row(
                    "SELECT COUNT(*) FROM providers WHERE id = ?1",
                    params![target_pid],
                    |row| row.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false)
            };
            if !exists {
                warnings.push(format!("[{plugin_id}] 供应商 '{target_pid}' 已不存在，跳过"));
            } else {
                let current: Option<String> = db
                    .lock()
                    .query_row(
                        "SELECT current_provider_id FROM app_state WHERE plugin_id = ?1",
                        params![plugin_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .unwrap_or(None);
                if current.as_deref() != Some(target_pid.as_str()) {
                    if let Err(e) =
                        crate::commands::providers::switch_provider_core(registry, db, target_pid)
                    {
                        warnings.push(format!(
                            "[{plugin_id}] 切换供应商 '{target_pid}' 失败: {e}"
                        ));
                    }
                }
            }
        }

        // 2. MCP 最小 toggle 差异
        if let Some(target_ids) = &snapshot.mcp_enabled_ids {
            let servers = db.list_mcp_servers().map_err(|e| e.to_string())?;
            let current: Vec<(String, bool)> = servers
                .iter()
                .map(|s| {
                    (
                        s.id.clone(),
                        s.apps.iter().any(|(pid, en)| pid == plugin_id && *en),
                    )
                })
                .collect();
            let (toggles, dangling) = plan_toggles(&current, target_ids);
            for id in dangling {
                warnings.push(format!("[{plugin_id}] MCP '{id}' 已不存在，跳过"));
            }
            for (id, enabled) in toggles {
                if let Err(e) =
                    toggle_mcp_server(db, registry, &id, plugin_id, enabled)
                {
                    warnings.push(format!(
                        "[{plugin_id}] 切换 MCP '{id}' → {enabled} 失败: {e}"
                    ));
                }
            }
        }

        // 3. Skills 最小 toggle 差异
        if let Some(target_ids) = &snapshot.skill_enabled_ids {
            let skills = db.list_skills().map_err(|e| e.to_string())?;
            let current: Vec<(String, bool)> = skills
                .iter()
                .map(|s| {
                    (
                        s.id.clone(),
                        s.enabled_plugins.iter().any(|pid| pid == plugin_id),
                    )
                })
                .collect();
            let (toggles, dangling) = plan_toggles(&current, target_ids);
            for id in dangling {
                warnings.push(format!("[{plugin_id}] 技能 '{id}' 已不存在，跳过"));
            }
            if !toggles.is_empty() {
                let settings = SkillService::get_sync_settings(db)?;
                let ssot = ssot_dir(&paths.data_dir, settings.storage_location);
                for (id, enabled) in toggles {
                    if let Err(e) = toggle_skill(
                        db,
                        registry,
                        &ssot,
                        settings.sync_method,
                        &id,
                        plugin_id,
                        enabled,
                    ) {
                        warnings.push(format!(
                            "[{plugin_id}] 切换技能 '{id}' → {enabled} 失败: {e}"
                        ));
                    }
                }
            }
        }

        // 4. Prompt（已激活则幂等跳过）
        if let Some(target_prompt) = &snapshot.active_prompt_id {
            let enabled_now = db
                .list_prompts(Some(plugin_id))
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|p| p.id == *target_prompt)
                .map(|p| p.enabled);
            match enabled_now {
                None => warnings.push(format!(
                    "[{plugin_id}] prompt '{target_prompt}' 已不存在，跳过"
                )),
                Some(true) => {}
                Some(false) => {
                    if let Err(e) = enable_prompt(db, registry, target_prompt) {
                        warnings.push(format!(
                            "[{plugin_id}] 启用 prompt '{target_prompt}' 失败: {e}"
                        ));
                    }
                }
            }
        }

        db.set_current_profile_for_plugin(plugin_id, Some(profile_id))
            .map_err(|e| e.to_string())?;
        Ok(warnings)
    }
}

// ============================================================================
// toggle/enable 辅助（与对应命令层同语义；供 apply 编排复用）
// ============================================================================

/// MCP 开关（与 commands/mcp.rs 的 mcp_toggle_app 同语义）。
fn toggle_mcp_server(
    db: &Database,
    registry: &PluginRegistry,
    id: &str,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    use crate::services::mcp::McpService;
    if enabled {
        db.set_mcp_server_app_enabled(id, plugin_id, true)
            .map_err(|e| e.to_string())?;
        let server = db.get_mcp_server(id).map_err(|e| e.to_string())?;
        if let Some(server) = server {
            McpService::sync_server_to_plugin(db, registry, &server, plugin_id)?;
        }
    } else {
        McpService::remove_server_from_plugin(db, registry, id, plugin_id)?;
        db.set_mcp_server_app_enabled(id, plugin_id, false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 技能开关（与 commands/skills.rs 的 skills_toggle_plugin 同语义）。
#[allow(clippy::too_many_arguments)]
fn toggle_skill(
    db: &Database,
    registry: &PluginRegistry,
    ssot: &std::path::Path,
    sync_method: crate::services::skills::SyncMethod,
    id: &str,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let Some(skill) = db.get_skill(id).map_err(|e| e.to_string())? else {
        return Err(format!("技能不存在: {id}"));
    };
    let plugin = registry
        .resolve_plugin(plugin_id)
        .map_err(|e| e.to_string())?;
    let dest_dir = plugin
        .skills_dir()
        .ok_or_else(|| format!("插件 '{plugin_id}' 不支持 skills 同步"))?;
    db.set_skill_plugin_enabled(id, plugin_id, enabled)
        .map_err(|e| e.to_string())?;
    if enabled {
        sync_skill_to_dir(ssot, &skill.directory, &dest_dir, sync_method)
    } else {
        remove_skill_from_dir(&skill.directory, &dest_dir)
    }
}

/// 启用 prompt（互斥 + 写记忆文件，与 commands/prompts.rs 同语义）。
fn enable_prompt(
    db: &Database,
    registry: &PluginRegistry,
    id: &str,
) -> Result<(), String> {
    let prompt = db
        .get_prompt(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt 不存在: {id}"))?;
    let plugin = registry
        .resolve_plugin(&prompt.plugin_id)
        .map_err(|e| e.to_string())?;
    let file = plugin.prompt_file_path().ok_or_else(|| {
        format!("插件 '{}' 不支持 prompt 文件", prompt.plugin_id)
    })?;
    crate::services::prompts::PromptService::enable(db, &file, &prompt.plugin_id, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(v) => std::env::set_var("AGENT_SWITCH_TEST_HOME", v),
                None => std::env::remove_var("AGENT_SWITCH_TEST_HOME"),
            }
        }
    }

    fn setup_env() -> (
        tempfile::TempDir,
        Database,
        PluginRegistry,
        AppPaths,
        EnvGuard,
    ) {
        let lock = crate::test_support::env_lock().lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("AGENT_SWITCH_TEST_HOME");
        std::env::set_var("AGENT_SWITCH_TEST_HOME", temp.path());
        let guard = EnvGuard {
            previous,
            _lock: lock,
        };

        let db = Database::new(&temp.path().join("cc.db")).unwrap();
        let registry =
            PluginRegistry::new(temp.path().join("plugins"), db.clone());
        let _ = registry.seed_builtin();
        let paths = AppPaths {
            data_dir: temp.path().to_path_buf(),
        };
        (temp, db, registry, paths, guard)
    }

    fn seed_provider(db: &Database, id: &str, plugin_id: &str) {
        db.lock()
            .execute(
                "INSERT INTO providers (id, plugin_id, name, settings_config) VALUES (?1, ?2, ?1, ?3)",
                params![id, plugin_id, r#"{"npm":"@ai-sdk/openai-compatible"}"#],
            )
            .unwrap();
    }

    fn set_current(db: &Database, plugin_id: &str, provider_id: Option<&str>) {
        db.lock()
            .execute(
                "INSERT INTO app_state (plugin_id, current_provider_id) VALUES (?1, ?2)
                 ON CONFLICT(plugin_id) DO UPDATE SET current_provider_id = excluded.current_provider_id",
                params![plugin_id, provider_id],
            )
            .unwrap();
    }

    fn current(db: &Database, plugin_id: &str) -> Option<String> {
        db.lock()
            .query_row(
                "SELECT current_provider_id FROM app_state WHERE plugin_id = ?1",
                params![plugin_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap_or(None)
    }

    fn seed_mcp(db: &Database, id: &str) {
        db.lock()
            .execute(
                "INSERT INTO mcp_servers (id, name, server_config) VALUES (?1, ?1, '{}')",
                params![id],
            )
            .unwrap();
    }

    #[test]
    fn create_snapshots_plugin_state() {
        let (_temp, db, registry, _paths, _guard) = setup_env();
        seed_provider(&db, "p1", "opencode");
        seed_provider(&db, "p2", "opencode");
        set_current(&db, "opencode", Some("p1"));
        seed_mcp(&db, "fs");
        db.set_mcp_server_app_enabled("fs", "opencode", true).unwrap();

        let profile = ProfileService::create(&db, &registry, "项目A", "opencode").unwrap();

        let payload: ProfilePayloadMap =
            serde_json::from_value(profile.payload).unwrap();
        let snap = payload.get("opencode").unwrap();
        assert_eq!(snap.provider.as_deref(), Some("p1"));
        assert_eq!(snap.mcp_enabled_ids.as_deref(), Some(["fs".to_string()].as_slice()));
        assert_eq!(snap.skill_enabled_ids.as_deref(), Some([].as_slice()));
        // 其它插件未拍快照
        assert!(!payload.contains_key("claudecode"));

        // per-plugin current 指针
        assert_eq!(
            db.current_profile_for_plugin("opencode").unwrap().as_deref(),
            None,
            "create 不改变 current 指针（apply 才标记）"
        );
    }

    #[test]
    fn create_rejects_empty_name() {
        let (_temp, db, registry, _paths, _guard) = setup_env();
        assert!(ProfileService::create(&db, &registry, "  ", "opencode").is_err());
    }

    #[test]
    fn apply_restores_provider_and_autosaves_old_project() {
        let (_temp, db, registry, paths, _guard) = setup_env();
        seed_provider(&db, "p1", "opencode");
        seed_provider(&db, "p2", "opencode");

        // 状态 A：p1
        set_current(&db, "opencode", Some("p1"));
        let proj_a = ProfileService::create(&db, &registry, "项目A", "opencode").unwrap();

        // 状态 B：p2 → 创建项目B
        set_current(&db, "opencode", Some("p2"));
        let proj_b = ProfileService::create(&db, &registry, "项目B", "opencode").unwrap();

        // 应用项目A → 供应商切回 p1，A 标记 current
        let warnings = ProfileService::apply(&db, &registry, &paths, &proj_a.id, "opencode")
            .unwrap();
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(current(&db, "opencode").as_deref(), Some("p1"));
        assert_eq!(
            db.current_profile_for_plugin("opencode").unwrap().as_deref(),
            Some(proj_a.id.as_str())
        );

        // 应用项目B：自动保存旧项目 A（重拍为离开时的 p1 状态），再切到 p2
        let warnings = ProfileService::apply(&db, &registry, &paths, &proj_b.id, "opencode")
            .unwrap();
        assert!(warnings.is_empty());
        assert_eq!(current(&db, "opencode").as_deref(), Some("p2"));
        assert_eq!(
            db.current_profile_for_plugin("opencode").unwrap().as_deref(),
            Some(proj_b.id.as_str())
        );

        // 项目 A 的快照已被自动保存为 p1（离开时状态）
        let a = db.get_profile(&proj_a.id).unwrap().unwrap();
        let payload: ProfilePayloadMap = serde_json::from_value(a.payload).unwrap();
        assert_eq!(payload.get("opencode").unwrap().provider.as_deref(), Some("p1"));
    }

    #[test]
    fn apply_uncaptured_plugin_only_marks_current() {
        let (_temp, db, registry, paths, _guard) = setup_env();
        seed_provider(&db, "p1", "opencode");

        // 项目只在 opencode 上拍过快照
        set_current(&db, "opencode", Some("p1"));
        let profile = ProfileService::create(&db, &registry, "仅OC", "opencode").unwrap();

        // 应用到 claudecode（未拍摄）→ 不改配置，仅标记 current + warning
        let warnings =
            ProfileService::apply(&db, &registry, &paths, &profile.id, "claudecode").unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("尚未在该项目中保存过配置"));
        assert_eq!(
            db.current_profile_for_plugin("claudecode").unwrap().as_deref(),
            Some(profile.id.as_str())
        );
        assert_eq!(
            db.current_profile_for_plugin("opencode").unwrap(),
            None,
            "不牵连其它插件的 current 指针"
        );
    }

    #[test]
    fn apply_toggles_mcp_with_minimal_diff() {
        let (_temp, db, registry, paths, _guard) = setup_env();
        seed_provider(&db, "p1", "opencode");
        set_current(&db, "opencode", Some("p1"));
        seed_mcp(&db, "fs");
        seed_mcp(&db, "git");
        db.set_mcp_server_app_enabled("fs", "opencode", true).unwrap();
        let profile = ProfileService::create(&db, &registry, "MCP项目", "opencode").unwrap();

        // 当前态改为：fs 关、git 开 → 应用后应恢复 fs 开（git 关）
        db.set_mcp_server_app_enabled("fs", "opencode", false).unwrap();
        db.set_mcp_server_app_enabled("git", "opencode", true).unwrap();
        let warnings =
            ProfileService::apply(&db, &registry, &paths, &profile.id, "opencode").unwrap();

        let servers = db.list_mcp_servers().unwrap();
        let enabled: Vec<&str> = servers
            .iter()
            .filter(|s| s.apps.iter().any(|(pid, en)| pid == "opencode" && *en))
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(enabled, vec!["fs"], "最小 toggle 恢复目标集，实际: {warnings:?}");
        assert!(warnings.is_empty());
    }

    #[test]
    fn apply_warns_on_dangling_and_missing_provider() {
        let (_temp, db, registry, paths, _guard) = setup_env();

        // 手工构造 payload：引用不存在的供应商与 MCP
        let profile = Profile {
            id: "proj-x".into(),
            name: "悬空".into(),
            payload: serde_json::json!({
                "opencode": {
                    "provider": "gone",
                    "mcpEnabledIds": ["gone-mcp"],
                    "skillEnabledIds": [],
                    "activePromptId": null
                }
            }),
            sort_order: Some(0),
            created_at: Some(0),
            updated_at: Some(0),
        };
        db.upsert_profile(&profile).unwrap();

        let warnings = ProfileService::apply(&db, &registry, &paths, "proj-x", "opencode")
            .unwrap();
        assert!(warnings.iter().any(|w| w.contains("供应商 'gone' 已不存在")));
        assert!(warnings.iter().any(|w| w.contains("MCP 'gone-mcp' 已不存在")));
    }

    #[test]
    fn delete_clears_plugin_pointers() {
        let (_temp, db, registry, _paths, _guard) = setup_env();
        seed_provider(&db, "p1", "opencode");
        set_current(&db, "opencode", Some("p1"));
        let profile = ProfileService::create(&db, &registry, "项目", "opencode").unwrap();
        db.set_current_profile_for_plugin("opencode", Some(&profile.id))
            .unwrap();

        db.delete_profile(&profile.id).unwrap();
        assert_eq!(db.current_profile_for_plugin("opencode").unwrap(), None);
    }
}
