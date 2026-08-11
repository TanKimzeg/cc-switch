//! Skills 服务。
//!
//! SSOT：技能存储在 `{app_data_dir}/skills/`；`skills` 表记录清单，
//! `skill_apps` 记录各插件启用状态。启用时把技能目录复制到插件的
//! skills 目录（约定路径）。

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension, Row};

use crate::db::Database;

/// 技能清单行。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub directory: String,
    pub source_path: String,
    pub enabled_plugins: Vec<String>,
    pub installed_at: i64,
}

fn row_to_skill(row: &Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        directory: row.get(3)?,
        source_path: row.get(4)?,
        enabled_plugins: Vec::new(),
        installed_at: row.get(5)?,
    })
}

impl Database {
    /// 列出全部技能（含启用插件）。
    pub fn list_skills(&self) -> rusqlite::Result<Vec<SkillRecord>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, directory, source_path, installed_at FROM skills ORDER BY name",
        )?;
        let mut skills = stmt
            .query_map([], row_to_skill)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut app_stmt = conn.prepare(
            "SELECT skill_id, plugin_id FROM skill_apps WHERE enabled = 1 ORDER BY plugin_id",
        )?;
        let rows = app_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for skill in &mut skills {
            skill.enabled_plugins = rows
                .iter()
                .filter(|(sid, _)| sid == &skill.id)
                .map(|(_, pid)| pid.clone())
                .collect();
        }
        Ok(skills)
    }

    /// 读取单个技能。
    pub fn get_skill(&self, id: &str) -> rusqlite::Result<Option<SkillRecord>> {
        let skill = self
            .lock()
            .query_row(
                "SELECT id, name, description, directory, source_path, installed_at FROM skills WHERE id = ?1",
                params![id],
                row_to_skill,
            )
            .optional()?;
        if let Some(mut skill) = skill {
            let plugins: Vec<String> = self
                .lock()
                .prepare("SELECT plugin_id FROM skill_apps WHERE skill_id = ?1 AND enabled = 1 ORDER BY plugin_id")?
                .query_map(params![id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            skill.enabled_plugins = plugins;
            return Ok(Some(skill));
        }
        Ok(None)
    }

    /// 安装技能：复制目录到 SSOT，写入清单。
    pub fn install_skill(
        &self,
        skills_root: &Path,
        source: &Path,
        id: &str,
    ) -> Result<SkillRecord, String> {
        if !source.is_dir() {
            return Err(format!("技能源目录不存在: {}", source.display()));
        }
        std::fs::create_dir_all(skills_root).map_err(|e| e.to_string())?;
        let dest = skills_root.join(id);
        if dest.exists() {
            return Err(format!("技能已安装: {id}"));
        }
        copy_dir_recursive(source, &dest).map_err(|e| e.to_string())?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let name = id.to_string();
        let (name, description) = parse_skill_metadata(&dest, &name);
        self.lock()
            .execute(
                "INSERT INTO skills (id, name, description, directory, source_path, installed_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![id, name, description, id, source.display().to_string(), now],
            )
            .map_err(|e| e.to_string())?;
        self.get_skill(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "技能写入失败".to_string())
    }

    /// 删除技能：删除 SSOT 目录与记录。
    pub fn uninstall_skill(&self, skills_root: &Path, id: &str) -> Result<(), String> {
        let conn = self.lock();
        conn.execute("DELETE FROM skill_apps WHERE skill_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM skills WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        drop(conn);
        let dest = skills_root.join(id);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 更新某技能在指定插件的启用状态。
    pub fn set_skill_plugin_enabled(
        &self,
        skill_id: &str,
        plugin_id: &str,
        enabled: bool,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO skill_apps (skill_id, plugin_id, enabled) VALUES (?1, ?2, ?3)
             ON CONFLICT(skill_id, plugin_id) DO UPDATE SET enabled = excluded.enabled",
            params![skill_id, plugin_id, if enabled { 1 } else { 0 }],
        )?;
        Ok(())
    }
}

/// 递归复制目录。
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 从 SKILL.md 解析名称与描述。
fn parse_skill_metadata(dir: &Path, fallback_name: &str) -> (String, Option<String>) {
    let readme = dir.join("SKILL.md");
    let Ok(content) = std::fs::read_to_string(&readme) else {
        return (fallback_name.to_string(), None);
    };
    let mut name = fallback_name.to_string();
    let mut description = None;
    for line in content.lines() {
        if line.starts_with("---") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !v.is_empty() {
                name = v;
            }
        } else if let Some(rest) = line.strip_prefix("description:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !v.is_empty() {
                description = Some(v);
            }
        }
        if name != fallback_name && description.is_some() {
            break;
        }
    }
    (name, description)
}

/// 插件 skills 目录（约定路径）。
pub fn plugin_skills_dir(plugin_id: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    match plugin_id {
        "opencode" => home.join(".config").join("opencode").join("skill"),
        _ => home.join(".cc-switch").join("skills").join(plugin_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_metadata_reads_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: My Skill\ndescription: Does things\n---\n# Body",
        )
        .unwrap();
        let (name, desc) = parse_skill_metadata(dir.path(), "fallback");
        assert_eq!(name, "My Skill");
        assert_eq!(desc.as_deref(), Some("Does things"));
    }

    #[test]
    fn parse_skill_metadata_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# No frontmatter").unwrap();
        let (name, desc) = parse_skill_metadata(dir.path(), "fallback");
        assert_eq!(name, "fallback");
        assert!(desc.is_none());
    }

    #[test]
    fn skill_install_list_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let skills_root = dir.path().join("skills");

        let src = dir.path().join("src-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: Test Skill\n---\nbody").unwrap();

        let skill = db.install_skill(&skills_root, &src, "test-skill").unwrap();
        assert_eq!(skill.name, "Test Skill");
        assert!(skills_root.join("test-skill").exists());

        db.set_skill_plugin_enabled("test-skill", "opencode", true)
            .unwrap();
        let skill = db.get_skill("test-skill").unwrap().unwrap();
        assert_eq!(skill.enabled_plugins, vec!["opencode".to_string()]);

        db.uninstall_skill(&skills_root, "test-skill").unwrap();
        assert!(!skills_root.join("test-skill").exists());
        assert!(db.get_skill("test-skill").unwrap().is_none());
    }
}
