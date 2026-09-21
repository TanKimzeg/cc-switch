//! 插件注册表：扫描插件目录、解析并校验 manifest、同步安装记录。
//!
//! M1 原型：插件即「应用数据目录下 `plugins/<id>/manifest.json`」。
//! 后续里程碑将引入下载、解压、来源校验等安装流程。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::plugin::{
    AgentPlugin, ClaudeCodePlugin, OpenCodePlugin, PluginCapabilities, ProcessPlugin, TsPluginStub,
};
use crate::types::{InstalledPlugin, PluginManifest};

/// 当前支持的 manifest 协议版本。
pub const SUPPORTED_API_VERSION: &str = "1";

/// 内置插件随二进制分发，首次运行写入插件目录（不覆盖用户已有内容）。
const OPENCLAW_MANIFEST: &str = include_str!("../plugins/openclaw/manifest.json");
const OPENCODE_MANIFEST: &str = include_str!("../plugins/opencode/manifest.json");
const CLAUDECODE_MANIFEST: &str = include_str!("../plugins/claudecode/manifest.json");
const CODEX_MANIFEST: &str = include_str!("../plugins/codex/manifest.json");
const GROKBUILD_MANIFEST: &str = include_str!("../plugins/grokbuild/manifest.json");
const HERMES_MANIFEST: &str = include_str!("../plugins/hermes/manifest.json");

/// 内置插件 id（随应用分发，启动时 seed 覆盖）。
const BUILTIN_IDS: [&str; 6] = [
    "openclaw",
    "opencode",
    "claudecode",
    "codex",
    "grokbuild",
    "hermes",
];

/// 磁盘上的 manifest.json 文件格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// 插件能力声明（M3）。
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    /// provider 设置表单的 JSON Schema（M3，可选）。
    #[serde(default)]
    pub settings_schema: Option<serde_json::Value>,
    /// 提示词文件路径（相对 home，如 `~/.claude/CLAUDE.md`；可选）。
    #[serde(default)]
    pub prompt_file: Option<String>,
    /// Skills 同步目录（相对 home，如 `~/.claude/skills`；可选）。
    #[serde(default)]
    pub skills_dir: Option<String>,
    /// 资源白名单：TS 插件可访问的用户目录根（`~` 展开）。
    ///
    /// 键为资源名（如 `config` / `projects` / `mcp`），值为路径（可指向文件或目录）。
    /// TS 插件通过 `host_read/write/list_resource` 只允许访问此处声明的根。
    #[serde(default)]
    pub resources: std::collections::HashMap<String, String>,
    /// 插件入口。M1 仅解析为元数据；M3 按类型分派到原生/进程插件执行。
    pub entry: ManifestEntry,
}

/// 插件入口定义（serde 内部标记枚举，按 `type` 字段区分）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ManifestEntry {
    /// 原生内置插件（如 opencode），直接调用二进制内的协议实现。
    Native {
        /// 原生插件模块标识（与插件 id 一致的约定）。
        #[serde(default)]
        module: String,
    },
    /// 调用命令行程序，后续用于「切换」动作（如 `openclaw config ...`）。
    Shell { command: String, args: Vec<String> },
    /// TypeScript 插件：由前端（WebView）动态加载执行，通过宿主命令读写配置。
    Ts {
        /// 插件主脚本（相对插件目录，如 `main.ts`）。
        main: String,
    },
}

impl ManifestEntry {
    /// 入口类型字符串（供前端判断渲染路径）。
    pub fn type_str(&self) -> &'static str {
        match self {
            ManifestEntry::Native { .. } => "native",
            ManifestEntry::Shell { .. } => "shell",
            ManifestEntry::Ts { .. } => "ts",
        }
    }

    /// TS 插件主脚本路径（相对插件目录）；非 TS 返回 None。
    pub fn ts_main(&self) -> Option<&str> {
        match self {
            ManifestEntry::Ts { main } => Some(main),
            _ => None,
        }
    }
}

impl ManifestFile {
    /// 基础合法性校验：必填字段非空、apiVersion 受支持。
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::Invalid {
                id: self.id.clone(),
                reason: "id 不能为空".to_string(),
            });
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Invalid {
                id: self.id.clone(),
                reason: "name 不能为空".to_string(),
            });
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Invalid {
                id: self.id.clone(),
                reason: "version 不能为空".to_string(),
            });
        }
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(ManifestError::Invalid {
                id: self.id.clone(),
                reason: format!(
                    "apiVersion {} 不受支持（当前支持 {}）",
                    self.api_version, SUPPORTED_API_VERSION
                ),
            });
        }
        Ok(())
    }

    /// 投影为暴露给前端/命令层的清单。
    pub fn to_manifest(&self) -> PluginManifest {
        PluginManifest {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            api_version: self.api_version.clone(),
            author: self.author.clone(),
            description: self.description.clone(),
            icon: self.icon.clone(),
            capabilities: Some(self.capabilities.clone()),
            settings_schema: self.settings_schema.clone(),
            prompt_file: self.prompt_file.clone(),
            skills_dir: self.skills_dir.clone(),
            entry_type: self.entry.type_str().to_string(),
            main: self.entry.ts_main().map(|s| s.to_string()),
        }
    }
}

/// 展开 `~` 开头的路径为用户主目录下的绝对路径。
fn expand_home(path: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else if path == "~" {
        home
    } else {
        PathBuf::from(path)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("读取 {path} 失败: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("解析 {path} 失败: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("manifest 不合法（id={id}）: {reason}")]
    Invalid { id: String, reason: String },
    #[error("数据库操作失败: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("不是目录: {0}")]
    NotDirectory(PathBuf),
    #[error("插件已安装: {0}")]
    AlreadyInstalled(String),
    #[error("未找到插件: {0}")]
    NotFound(String),
    #[error("内置插件不可卸载: {0}")]
    BuiltinNotRemovable(String),
}

impl ManifestError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ManifestError::Io {
            path: path.into(),
            source,
        }
    }

    fn parse(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        ManifestError::Parse {
            path: path.into(),
            source,
        }
    }
}

/// 插件注册表：管理插件目录并维护安装记录。
pub struct PluginRegistry {
    dir: PathBuf,
    db: Database,
}

impl PluginRegistry {
    pub fn new(dir: impl Into<PathBuf>, db: Database) -> Self {
        Self {
            dir: dir.into(),
            db,
        }
    }

    /// 插件目录绝对路径。
    pub fn plugins_dir(&self) -> &Path {
        &self.dir
    }

    /// 解析已安装插件为可执行的 [`AgentPlugin`] 实例。
    ///
    /// - manifest `entry.type = native`：返回二进制内注册的原生插件；
    /// - manifest `entry.type = shell`：包装为进程插件调用外部命令；
    /// - manifest `entry.type = ts`：返回占位实现（实际由前端宿主加载脚本执行）。
    pub fn resolve_plugin(&self, id: &str) -> Result<Box<dyn AgentPlugin>, ManifestError> {
        let manifest_path = self.dir.join(id).join("manifest.json");
        if !manifest_path.exists() {
            return Err(ManifestError::NotFound(id.to_string()));
        }
        let mf = load_manifest(&manifest_path)?;
        let capabilities = mf.capabilities.clone();
        let prompt_file = mf.prompt_file.as_deref().map(expand_home);
        let skills_dir = mf.skills_dir.as_deref().map(expand_home);
        match &mf.entry {
            ManifestEntry::Native { module } => {
                let module = if module.is_empty() { &mf.id } else { module };
                match module.as_str() {
                    "opencode" => Ok(Box::new(OpenCodePlugin::new())),
                    "claudecode" => Ok(Box::new(ClaudeCodePlugin::new())),
                    "codex" => Ok(Box::new(crate::plugin::CodexPlugin::new())),
                    "grokbuild" => Ok(Box::new(crate::plugin::GrokBuildPlugin::new())),
                    "hermes" => Ok(Box::new(crate::plugin::HermesPlugin::new())),
                    other => Err(ManifestError::Invalid {
                        id: mf.id.clone(),
                        reason: format!("未知的原生插件模块: {other}"),
                    }),
                }
            }
            ManifestEntry::Shell { command, args } => Ok(Box::new(ProcessPlugin::new(
                mf.id.clone(),
                command.clone(),
                args.clone(),
                capabilities,
            ))),
            ManifestEntry::Ts { .. } => Ok(Box::new(TsPluginStub::new(
                mf.id.clone(),
                capabilities,
                prompt_file,
                skills_dir,
            ))),
        }
    }

    /// 返回插件 manifest 声明的资源白名单（`~` 已展开为绝对路径）。
    ///
    /// 供 TS 插件的宿主资源命令（`host_read/write/list_resource`）做路径校验：
    /// 只允许访问 manifest `resources` 里声明的根。
    pub fn resource_roots(&self, id: &str) -> Result<Vec<(String, PathBuf)>, ManifestError> {
        let manifest_path = self.dir.join(id).join("manifest.json");
        if !manifest_path.exists() {
            return Err(ManifestError::NotFound(id.to_string()));
        }
        let mf = load_manifest(&manifest_path)?;
        Ok(mf
            .resources
            .iter()
            .map(|(k, v)| (k.clone(), expand_home(v)))
            .collect())
    }

    /// 首次运行：将内置插件写入插件目录。
    ///
    /// 内置插件由应用分发，其 manifest 必须始终与二进制内置版本一致，
    /// 因此每次启动都覆盖写入（更新能力声明、版本等）。
    pub fn seed_builtin(&self) -> Result<(), ManifestError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| ManifestError::io(&self.dir, e))?;
        for (id, manifest) in [
            ("openclaw", OPENCLAW_MANIFEST),
            ("opencode", OPENCODE_MANIFEST),
            ("claudecode", CLAUDECODE_MANIFEST),
            ("codex", CODEX_MANIFEST),
            ("grokbuild", GROKBUILD_MANIFEST),
            ("hermes", HERMES_MANIFEST),
        ] {
            let plugin_dir = self.dir.join(id);
            std::fs::create_dir_all(&plugin_dir).map_err(|e| ManifestError::io(&plugin_dir, e))?;
            let target = plugin_dir.join("manifest.json");
            // 存量 TS 示例安装（id=claudecode）被 native manifest 覆盖即完成升级：
            // providers/prompts/skills 按 plugin_id 继承，残留 main.js 不影响解析。
            if target.exists() && manifest.contains("\"type\": \"native\"") {
                let previous = std::fs::read_to_string(&target).unwrap_or_default();
                if previous.contains("\"type\": \"ts\"") {
                    log::info!("插件 {id} 由 TS 示例升级为内置 native 实现（数据自动继承）");
                }
            }
            std::fs::write(&target, manifest).map_err(|e| ManifestError::io(&target, e))?;
        }
        Ok(())
    }

    /// 扫描插件目录，解析并校验所有 manifest；非法插件记录日志并跳过。
    pub fn discover(&self) -> Result<Vec<PluginManifest>, ManifestError> {
        let mut plugins = Vec::new();
        if !self.dir.exists() {
            return Ok(plugins);
        }
        let entries = std::fs::read_dir(&self.dir).map_err(|e| ManifestError::io(&self.dir, e))?;
        for entry in entries {
            let path = entry.map_err(|e| ManifestError::io(&self.dir, e))?.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            if !manifest_path.exists() {
                log::debug!("skip {}: no manifest.json", path.display());
                continue;
            }
            match load_manifest(&manifest_path) {
                Ok(mf) => {
                    let manifest = mf.to_manifest();
                    log::info!("plugin {} v{} discovered", manifest.id, manifest.version);
                    plugins.push(manifest);
                }
                Err(e) => log::warn!("skip {}: {e}", path.display()),
            }
        }
        plugins.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(plugins)
    }

    /// 将发现的插件同步到 `plugin_installs` 表。
    ///
    /// 内置插件标记为 builtin；手动安装（local）的插件**保留其原有来源**，
    /// 仅更新版本 —— 避免重启后把本地插件误标为 builtin。
    /// 例外：id 属于内置清单的既有 local 记录（如 TS 示例升级为 native），
    /// 来源同步改为 builtin。
    pub fn sync_installs(&self, manifests: &[PluginManifest]) -> rusqlite::Result<()> {
        for m in manifests {
            let source = if BUILTIN_IDS.contains(&m.id.as_str()) {
                "builtin"
            } else {
                "local"
            };
            self.db
                .insert_plugin_install_if_absent(&m.id, &m.version, source, None)?;
            if source == "builtin" {
                self.db
                    .update_plugin_install_source(&m.id, "builtin", &m.version)?;
            }
        }
        Ok(())
    }

    /// 返回全部已安装插件（manifest + 安装来源）。
    ///
    /// native 插件的路径能力（skills_dir/prompt_file）与 capabilities 以二进制内
    /// trait 实现为准回填到暴露的 manifest —— 避免「trait 已支持但 manifest 忘了
    /// 声明」导致前端按声明过滤时隐藏该插件（Skills/Prompts 面板的应用列表来源），
    /// 同时让暴露的路径跟随目录覆盖等动态解析。
    ///
    /// 另计算 `backend_switchable`（apply 且非 TS 入口）作为统一切换依据。
    pub fn list_installed(&self) -> Result<Vec<InstalledPlugin>, ManifestError> {
        let mut manifests = self.discover()?;
        for manifest in &mut manifests {
            if manifest.entry_type == "native" {
                if let Ok(plugin) = self.resolve_plugin(&manifest.id) {
                    manifest.capabilities = Some(plugin.capabilities().clone());
                    if let Some(dir) = plugin.skills_dir() {
                        manifest.skills_dir = Some(dir.to_string_lossy().to_string());
                    }
                    if let Some(file) = plugin.prompt_file_path() {
                        manifest.prompt_file = Some(file.to_string_lossy().to_string());
                    }
                }
            }
        }
        let installs = self.db.list_plugin_installs()?;
        Ok(manifests
            .into_iter()
            .map(|manifest| {
                let inst = installs.iter().find(|i| i.plugin_id == manifest.id);
                let backend_switchable = manifest.entry_type != "ts"
                    && manifest
                        .capabilities
                        .as_ref()
                        .is_some_and(|caps| caps.apply);
                InstalledPlugin {
                    manifest,
                    source: inst
                        .map(|i| i.source.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    installed_at: inst.map(|i| i.installed_at.clone()).unwrap_or_default(),
                    backend_switchable,
                }
            })
            .collect())
    }

    /// 从本地目录安装插件：校验 manifest、复制到插件目录、记录安装与 sha256。
    pub fn install_from_dir(&self, src: &Path) -> Result<InstalledPlugin, ManifestError> {
        if !src.is_dir() {
            return Err(ManifestError::NotDirectory(src.to_path_buf()));
        }
        let manifest_path = src.join("manifest.json");
        let mf = load_manifest(&manifest_path)?;
        mf.validate()?;

        let target = self.dir.join(&mf.id);
        if target.exists() {
            return Err(ManifestError::AlreadyInstalled(mf.id.clone()));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| ManifestError::io(&self.dir, e))?;
        copy_dir_recursive(src, &target).map_err(|e| ManifestError::io(&target, e))?;

        let target_manifest = target.join("manifest.json");
        let sha =
            sha256_file(&target_manifest).map_err(|e| ManifestError::io(&target_manifest, e))?;
        self.db
            .upsert_plugin_install(&mf.id, &mf.version, "local", Some(&sha))?;

        let installed_at = self
            .db
            .get_plugin_install(&mf.id)?
            .map(|i| i.installed_at)
            .unwrap_or_default();
        log::info!(
            "plugin {} v{} installed from {} (sha256 {})",
            mf.id,
            mf.version,
            src.display(),
            sha
        );
        let projected = mf.to_manifest();
        let backend_switchable = projected.entry_type != "ts"
            && projected
                .capabilities
                .as_ref()
                .is_some_and(|caps| caps.apply);
        Ok(InstalledPlugin {
            manifest: projected,
            source: "local".to_string(),
            installed_at,
            backend_switchable,
        })
    }

    /// 卸载插件：删除插件目录、安装记录及名下供应商数据；内置插件拒绝卸载。
    pub fn uninstall(&self, id: &str) -> Result<(), ManifestError> {
        let target = self.dir.join(id);
        if !target.exists() {
            return Err(ManifestError::NotFound(id.to_string()));
        }
        if let Some(inst) = self.db.get_plugin_install(id)? {
            if inst.source == "builtin" {
                return Err(ManifestError::BuiltinNotRemovable(id.to_string()));
            }
        }
        std::fs::remove_dir_all(&target).map_err(|e| ManifestError::io(&target, e))?;
        self.db.delete_plugin_install(id)?;
        self.db.delete_providers_by_plugin(id)?;
        log::info!("plugin {id} uninstalled");
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

/// 计算文件的 sha256 十六进制摘要。
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// 读取单个 manifest 文件并校验。
pub fn load_manifest(path: &Path) -> Result<ManifestFile, ManifestError> {
    let text = std::fs::read_to_string(path).map_err(|e| ManifestError::io(path, e))?;
    let mf: ManifestFile =
        serde_json::from_str(&text).map_err(|e| ManifestError::parse(path, e))?;
    mf.validate()?;
    Ok(mf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    const SAMPLE: &str = r#"{
        "id": "openclaw",
        "name": "OpenClaw",
        "version": "0.1.0",
        "apiVersion": "1",
        "description": "desc",
        "entry": { "type": "shell", "command": "openclaw", "args": [] }
    }"#;

    #[test]
    fn parses_sample_manifest() {
        let mf: ManifestFile = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(mf.id, "openclaw");
        assert_eq!(mf.name, "OpenClaw");
        assert_eq!(mf.api_version, "1");
        assert_eq!(mf.description.as_deref(), Some("desc"));
        mf.validate().expect("sample should be valid");
        match &mf.entry {
            ManifestEntry::Shell { command, args } => {
                assert_eq!(command, "openclaw");
                assert!(args.is_empty());
            }
            ManifestEntry::Native { .. } => unreachable!("sample manifest is shell entry"),
            ManifestEntry::Ts { .. } => unreachable!("sample manifest is shell entry"),
        }
    }

    #[test]
    fn rejects_unsupported_api_version() {
        let bad = SAMPLE.replace("\"apiVersion\": \"1\"", "\"apiVersion\": \"2\"");
        let mf: ManifestFile = serde_json::from_str(&bad).unwrap();
        assert!(matches!(mf.validate(), Err(ManifestError::Invalid { .. })));
    }

    #[test]
    fn rejects_empty_id() {
        let bad = SAMPLE.replace("\"id\": \"openclaw\"", "\"id\": \"\"");
        let mf: ManifestFile = serde_json::from_str(&bad).unwrap();
        assert!(mf.validate().is_err());
    }

    fn registry_in(dir: &Path) -> (PluginRegistry, Database) {
        let db = Database::new(&dir.join("test.db")).unwrap();
        let registry = PluginRegistry::new(dir.join("plugins"), db.clone());
        (registry, db)
    }

    #[test]
    fn discovers_only_valid_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugins = dir.path().join("plugins");

        std::fs::create_dir_all(plugins.join("openclaw")).unwrap();
        std::fs::write(plugins.join("openclaw/manifest.json"), SAMPLE).unwrap();

        std::fs::create_dir_all(plugins.join("broken")).unwrap();
        let bad = SAMPLE.replace("\"apiVersion\": \"1\"", "\"apiVersion\": \"9\"");
        std::fs::write(plugins.join("broken/manifest.json"), bad).unwrap();

        std::fs::create_dir_all(plugins.join("no-manifest")).unwrap();

        let found = registry.discover().unwrap();
        let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["openclaw"]);
    }

    #[test]
    fn seeds_builtin_openclaw() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        registry.seed_builtin().unwrap();
        let path = dir.path().join("plugins/openclaw/manifest.json");
        assert!(path.exists());
        let mf = load_manifest(&path).unwrap();
        assert_eq!(mf.id, "openclaw");
        assert_eq!(mf.version, "0.1.0");
    }

    #[test]
    fn seed_overwrites_builtin_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let path = dir.path().join("plugins/opencode/manifest.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{\"stale\": true}").unwrap();
        registry.seed_builtin().unwrap();
        // 内置插件 manifest 由应用分发，启动时必须刷新为最新版本。
        let refreshed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(refreshed["id"], "opencode");
        assert!(refreshed["capabilities"]["mcp"].as_bool().unwrap_or(false));
    }

    #[test]
    fn sync_records_installs() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());
        let plugins = dir.path().join("plugins");
        std::fs::create_dir_all(plugins.join("openclaw")).unwrap();
        std::fs::write(plugins.join("openclaw/manifest.json"), SAMPLE).unwrap();

        let found = registry.discover().unwrap();
        registry.sync_installs(&found).unwrap();

        let count: i64 = db
            .lock()
            .query_row("SELECT COUNT(*) FROM plugin_installs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let (version, source): (String, String) = db
            .lock()
            .query_row(
                "SELECT version, source FROM plugin_installs WHERE plugin_id = 'openclaw'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(version, "0.1.0");
        assert_eq!(source, "builtin");
    }

    #[test]
    fn sync_installs_preserves_local_source() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());

        // 手动安装的本地插件：先记录为 local，再 sync_installs 不应覆盖。
        let source = dir.path().join("demo-src");
        write_plugin(&source, "demo");
        registry.install_from_dir(&source).unwrap();
        assert_eq!(
            db.get_plugin_install("demo").unwrap().unwrap().source,
            "local"
        );

        let found = registry.discover().unwrap();
        registry.sync_installs(&found).unwrap();

        let after = db.get_plugin_install("demo").unwrap().unwrap();
        assert_eq!(
            after.source, "local",
            "重启同步后 local 插件不得变为 builtin"
        );
    }

    #[test]
    fn sync_installs_upgrades_builtin_id_local_record() {
        // TS 示例升级场景：同 id 的 local 安装在 sync 后改标为 builtin。
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());

        let source = dir.path().join("claudecode-src");
        write_plugin(&source, "claudecode");
        registry.install_from_dir(&source).unwrap();
        assert_eq!(
            db.get_plugin_install("claudecode").unwrap().unwrap().source,
            "local"
        );

        let found = registry.discover().unwrap();
        registry.sync_installs(&found).unwrap();

        let after = db.get_plugin_install("claudecode").unwrap().unwrap();
        assert_eq!(after.source, "builtin");
    }

    fn write_plugin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = SAMPLE.replace("\"id\": \"openclaw\"", &format!("\"id\": \"{id}\""));
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("extra.txt"), "hello").unwrap();
    }

    #[test]
    fn install_from_dir_copies_and_records() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());
        let source = dir.path().join("opencode-src");
        write_plugin(&source, "opencode");

        let installed = registry.install_from_dir(&source).unwrap();
        assert_eq!(installed.manifest.id, "opencode");
        assert_eq!(installed.source, "local");
        assert!(!installed.installed_at.is_empty());

        let target = dir.path().join("plugins/opencode");
        assert!(target.join("manifest.json").exists());
        assert!(target.join("extra.txt").exists());
        assert_eq!(
            std::fs::read_to_string(target.join("extra.txt")).unwrap(),
            "hello"
        );

        let inst = db.get_plugin_install("opencode").unwrap().unwrap();
        assert_eq!(inst.source, "local");
        assert_eq!(inst.sha256.unwrap().len(), 64);
    }

    #[test]
    fn install_rejects_already_installed() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let source = dir.path().join("opencode-src");
        write_plugin(&source, "opencode");
        registry.install_from_dir(&source).unwrap();
        assert!(matches!(
            registry.install_from_dir(&source),
            Err(ManifestError::AlreadyInstalled(_))
        ));
    }

    #[test]
    fn install_rejects_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let source = dir.path().join("empty");
        std::fs::create_dir_all(&source).unwrap();
        assert!(registry.install_from_dir(&source).is_err());
    }

    #[test]
    fn install_rejects_non_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            registry.install_from_dir(&file),
            Err(ManifestError::NotDirectory(_))
        ));
    }

    #[test]
    fn install_from_real_example() {
        let example = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/claudecode-ts"
        );
        assert!(Path::new(example).join("manifest.json").exists());
        assert!(Path::new(example).join("main.js").exists());
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let installed = registry.install_from_dir(Path::new(example)).unwrap();
        assert_eq!(installed.manifest.id, "claudecode-ts");
        assert_eq!(installed.manifest.name, "Claude Code (TS)");
        assert_eq!(installed.manifest.entry_type, "ts");
        assert_eq!(installed.manifest.main.as_deref(), Some("main.js"));
    }

    #[test]
    fn real_example_resource_roots_expand_home() {
        let example = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/claudecode-ts"
        );
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        registry.install_from_dir(Path::new(example)).unwrap();

        let roots = registry.resource_roots("claudecode-ts").unwrap();
        let root_map: std::collections::HashMap<&str, &Path> = roots
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_path()))
            .collect();
        assert_eq!(root_map.len(), 3);
        // `~` 展开为 home 目录下的绝对路径。
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            root_map["config"],
            home.join(".claude").join("settings.json")
        );
        assert_eq!(root_map["mcp"], home.join(".claude.json"));
        assert_eq!(root_map["projects"], home.join(".claude").join("projects"));
    }

    #[test]
    fn uninstall_removes_plugin_and_providers() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());
        let source = dir.path().join("opencode-src");
        write_plugin(&source, "opencode");
        registry.install_from_dir(&source).unwrap();

        db.lock()
            .execute(
                "INSERT INTO providers (id, plugin_id, name) VALUES ('p1', 'opencode', 'P')",
                [],
            )
            .unwrap();

        registry.uninstall("opencode").unwrap();
        assert!(!dir.path().join("plugins/opencode").exists());
        assert!(db.get_plugin_install("opencode").unwrap().is_none());
        let count: i64 = db
            .lock()
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn uninstall_rejects_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, db) = registry_in(dir.path());
        registry.seed_builtin().unwrap();
        registry
            .sync_installs(&registry.discover().unwrap())
            .unwrap();
        let is_builtin: String = db
            .lock()
            .query_row(
                "SELECT source FROM plugin_installs WHERE plugin_id = 'openclaw'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(is_builtin, "builtin");
        assert!(matches!(
            registry.uninstall("openclaw"),
            Err(ManifestError::BuiltinNotRemovable(_))
        ));
    }

    #[test]
    fn uninstall_rejects_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        assert!(matches!(
            registry.uninstall("ghost"),
            Err(ManifestError::NotFound(_))
        ));
    }

    #[test]
    fn list_installed_reports_source() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        registry.seed_builtin().unwrap();
        registry
            .sync_installs(&registry.discover().unwrap())
            .unwrap();
        let source = dir.path().join("demo-src");
        write_plugin(&source, "demo");
        registry.install_from_dir(&source).unwrap();

        let list = registry.list_installed().unwrap();
        assert_eq!(list.len(), 7);
        let openclaw = list.iter().find(|p| p.manifest.id == "openclaw").unwrap();
        let opencode = list.iter().find(|p| p.manifest.id == "opencode").unwrap();
        let claudecode = list
            .iter()
            .find(|p| p.manifest.id == "claudecode")
            .unwrap();
        let codex = list.iter().find(|p| p.manifest.id == "codex").unwrap();
        let grokbuild = list
            .iter()
            .find(|p| p.manifest.id == "grokbuild")
            .unwrap();
        let hermes = list.iter().find(|p| p.manifest.id == "hermes").unwrap();
        let demo = list.iter().find(|p| p.manifest.id == "demo").unwrap();
        assert_eq!(openclaw.source, "builtin");
        assert_eq!(opencode.source, "builtin");
        assert_eq!(claudecode.source, "builtin");
        assert_eq!(codex.source, "builtin");
        assert_eq!(grokbuild.source, "builtin");
        assert_eq!(hermes.source, "builtin");
        assert_eq!(demo.source, "local");
    }

    #[test]
    fn resolve_plugin_native_opencode() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/opencode");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "opencode",
                "name": "OpenCode",
                "version": "0.1.0",
                "apiVersion": "1",
                "capabilities": { "readLive": true, "apply": true, "import": true, "sessions": true },
                "entry": { "type": "native", "module": "opencode" }
            }"#,
        )
        .unwrap();

        let plugin = registry.resolve_plugin("opencode").unwrap();
        assert_eq!(plugin.id(), "opencode");
        assert!(plugin.capabilities().read_live);
        assert!(plugin.capabilities().sessions);
    }

    #[test]
    fn list_installed_backfills_native_path_capabilities() {
        // 回归：native 插件 manifest 未声明 skillsDir/promptFile 时，
        // list_installed 应以 trait 实现回填，否则前端会隐藏该插件的应用列。
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/opencode");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "opencode",
                "name": "OpenCode",
                "version": "0.1.0",
                "apiVersion": "1",
                "capabilities": { "readLive": true },
                "entry": { "type": "native", "module": "opencode" }
            }"#,
        )
        .unwrap();

        let list = registry.list_installed().unwrap();
        let opencode = list.iter().find(|p| p.manifest.id == "opencode").unwrap();
        let skills_dir = opencode.manifest.skills_dir.as_deref().unwrap_or_default();
        assert!(
            skills_dir.ends_with("skills"),
            "skills_dir 应由 trait 回填，实际: {skills_dir}"
        );
        let prompt_file = opencode
            .manifest
            .prompt_file
            .as_deref()
            .unwrap_or_default();
        assert!(
            prompt_file.ends_with("AGENTS.md"),
            "prompt_file 应由 trait 回填，实际: {prompt_file}"
        );
        // capabilities 同样以 trait 实现回填（manifest 只声明了 readLive）。
        let caps = opencode.manifest.capabilities.as_ref().unwrap();
        assert!(caps.apply, "native capabilities 应由 trait 回填");
        assert!(caps.mcp, "native capabilities 应由 trait 回填");
        assert!(opencode.backend_switchable);
    }

    #[test]
    fn backend_switchable_excludes_ts_entry() {
        // TS 插件即使声明 apply，也无法由后端切换。
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/ts-apply");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "ts-apply",
                "name": "TS Apply",
                "version": "0.1.0",
                "apiVersion": "1",
                "capabilities": { "apply": true },
                "entry": { "type": "ts", "main": "main.js" }
            }"#,
        )
        .unwrap();

        let list = registry.list_installed().unwrap();
        let ts = list.iter().find(|p| p.manifest.id == "ts-apply").unwrap();
        assert!(!ts.backend_switchable);
    }

    #[test]
    fn builtin_opencode_manifest_declares_skills_and_prompt_paths() {
        // 内置 manifest 必须声明路径能力（TS 生态与文档依赖声明本身）。
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        registry.seed_builtin().unwrap();
        let list = registry.list_installed().unwrap();
        for id in ["opencode", "claudecode"] {
            let plugin = list.iter().find(|p| p.manifest.id == id).unwrap();
            assert!(
                plugin.manifest.skills_dir.is_some(),
                "{id} 内置 manifest 应声明 skillsDir"
            );
            assert!(
                plugin.manifest.prompt_file.is_some(),
                "{id} 内置 manifest 应声明 promptFile"
            );
        }
        // shell 插件（openclaw）不参与 Skills 分发（对齐 v1 应用清单）。
        let openclaw = list.iter().find(|p| p.manifest.id == "openclaw").unwrap();
        assert!(openclaw.manifest.skills_dir.is_none());
    }

    #[test]
    fn resolve_plugin_shell_wraps_process() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "demo",
                "name": "Demo",
                "version": "0.1.0",
                "apiVersion": "1",
                "entry": { "type": "shell", "command": "demo-cli", "args": [] }
            }"#,
        )
        .unwrap();

        let plugin = registry.resolve_plugin("demo").unwrap();
        assert_eq!(plugin.id(), "demo");
        // 未声明的能力默认全部为 false
        assert!(!plugin.capabilities().read_live);
    }

    #[test]
    fn resolve_plugin_ts_returns_stub() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/my-ts");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "my-ts",
                "name": "My TS",
                "version": "0.1.0",
                "apiVersion": "1",
                "capabilities": { "readLive": true },
                "entry": { "type": "ts", "main": "main.ts" }
            }"#,
        )
        .unwrap();
        std::fs::write(plugin_dir.join("main.ts"), "export const plugin = {};").unwrap();

        let plugin = registry.resolve_plugin("my-ts").unwrap();
        assert_eq!(plugin.id(), "my-ts");
        assert!(plugin.capabilities().read_live);
        // TS 插件占位：read_live 返回"请前端宿主执行"
        assert!(plugin.read_live().is_err());

        // to_manifest 暴露 entry_type 与 main
        let mf = registry.discover().unwrap();
        let my = mf.iter().find(|m| m.id == "my-ts").unwrap();
        assert_eq!(my.entry_type, "ts");
        assert_eq!(my.main.as_deref(), Some("main.ts"));
    }

    #[test]
    fn resolve_plugin_unknown_native_module_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        let plugin_dir = dir.path().join("plugins/ghost");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "ghost",
                "name": "Ghost",
                "version": "0.1.0",
                "apiVersion": "1",
                "entry": { "type": "native", "module": "nonexistent" }
            }"#,
        )
        .unwrap();

        assert!(registry.resolve_plugin("ghost").is_err());
    }

    #[test]
    fn resolve_plugin_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _db) = registry_in(dir.path());
        assert!(matches!(
            registry.resolve_plugin("missing"),
            Err(ManifestError::NotFound(_))
        ));
    }
}
