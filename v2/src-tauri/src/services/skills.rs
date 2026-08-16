//! Skills 服务。
//!
//! SSOT：技能存储在 `~/.cc-switch/skills/`（或 `~/.agents/skills/`，设置可切换），
//! 对齐 v1；`skills` 表记录清单，`skill_apps` 记录各插件启用状态，`skill_repos` 记录
//! GitHub 技能仓库。启用时把技能复制/软链到插件的 skills 目录（路径由插件协议
//! `AgentPlugin::skills_dir` 提供）。
//!
//! 能力对齐 v1 `src-tauri/src/services/skill.rs`：仓库/ZIP 安装、skills.sh 搜索、
//! SHA-256 更新检测、卸载自动备份 + 恢复、未管理导入、软链/复制分发、存储位置迁移。

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::Database;

/// 仓库归档解压安全上限（归档由第三方完全控制，压缩炸弹防护）。
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_ARCHIVE_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// symlink 目标就是一条路径，几十字节就够；4 KiB 是宽松上限。
const MAX_SYMLINK_TARGET_BYTES: u64 = 4 * 1024;
/// 物化一个目录按一个目录块计费（空目录也吃 inode 与磁盘块）。
const DIRECTORY_BUDGET_COST: u64 = 4096;
/// 压缩体下载上限。zip 预算在解压时才生效，下载必须先卡一次上限。
const MAX_ARCHIVE_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
/// 卸载/更新备份保留份数。
const SKILL_BACKUP_RETAIN_COUNT: usize = 20;
/// 仓库下载整体超时。
const REPO_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

// ========== 数据结构 ==========

/// 技能清单记录（含仓库来源与更新检测信息）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub directory: String,
    pub source_path: Option<String>,
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub repo_branch: Option<String>,
    pub readme_url: Option<String>,
    pub enabled_plugins: Vec<String>,
    pub installed_at: i64,
    pub content_hash: Option<String>,
    pub updated_at: i64,
}

/// 技能分发（同步）方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    /// 自动：优先软链，失败回退复制。
    #[default]
    Auto,
    /// 仅软链。
    Symlink,
    /// 仅文件复制。
    Copy,
}

/// 技能存储位置（SSOT 目录选择）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillStorageLocation {
    /// CC Switch 管理目录 `~/.cc-switch/skills/`（对齐 v1）。
    #[default]
    CcSwitch,
    /// Agent Skills 统一标准目录 `~/.agents/skills/`。
    Unified,
}

/// 仓库配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRepo {
    pub owner: String,
    pub name: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_true() -> bool {
    true
}

/// 从仓库中发现的、可安装的技能。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverableSkill {
    /// 唯一标识：`owner/name:directory` 或 `owner/name:skillId`。
    pub key: String,
    pub name: String,
    pub description: String,
    /// 技能目录（可含多级，如 `skills/find-skills`）。
    pub directory: String,
    pub readme_url: Option<String>,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
}

/// skills.sh 搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSearchResult {
    pub skills: Vec<SkillsShDiscoverableSkill>,
    pub total_count: usize,
    pub query: String,
}

/// skills.sh 可安装技能。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShDiscoverableSkill {
    pub key: String,
    pub name: String,
    pub directory: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
    pub installs: u64,
    pub readme_url: Option<String>,
}

/// 技能更新检测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateInfo {
    pub id: String,
    pub name: String,
    pub current_hash: Option<String>,
    pub remote_hash: String,
}

/// 技能备份条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupEntry {
    pub backup_id: String,
    pub backup_path: String,
    pub created_at: i64,
    pub name: String,
    pub directory: String,
    pub description: Option<String>,
}

/// 未管理的技能（在应用/SSOT 目录中发现但未入库）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmanagedSkill {
    pub directory: String,
    pub name: String,
    pub description: Option<String>,
    /// 在哪些插件/来源中发现（插件 id 或 "cc-switch"）。
    pub found_in: Vec<String>,
    pub path: String,
}

/// 导入已有技能时，前端显式提交的目录 + 启用插件选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillSelection {
    pub directory: String,
    #[serde(default)]
    pub plugins: Vec<String>,
}

/// 同步设置（同步方式 + 存储位置）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub sync_method: SyncMethod,
    pub storage_location: SkillStorageLocation,
}

/// 存储位置迁移结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub migrated_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

/// 备份元数据（`meta.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillBackupMetadata {
    name: String,
    directory: String,
    description: Option<String>,
    backup_created_at: i64,
    source_path: String,
}

/// skills.sh API 原始响应（字段命名不一致，逐字段指定）。
#[derive(Debug, Deserialize)]
struct SkillsShApiResponse {
    #[allow(dead_code)]
    pub query: String,
    #[allow(dead_code)]
    pub skills: Vec<SkillsShApiSkill>,
    pub count: usize,
}

/// skills.sh API 原始技能条目。
#[derive(Debug, Deserialize)]
struct SkillsShApiSkill {
    pub id: String,
    #[serde(rename = "skillId")]
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub source: String,
}

/// 技能元数据（从 SKILL.md frontmatter 解析）。
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

// ========== 结构化错误 ==========

/// 格式化为 JSON 错误字符串，前端可解析为结构化错误（对齐 v1 `format_skill_error`）。
pub fn skill_error(code: &str, context: &[(&str, &str)], suggestion: Option<&str>) -> String {
    let mut ctx = serde_json::Map::new();
    for (k, v) in context {
        ctx.insert(k.to_string(), serde_json::json!(v));
    }
    serde_json::json!({
        "code": code,
        "context": ctx,
        "suggestion": suggestion,
    })
    .to_string()
}

// ========== 路径与校验 ==========

/// 测试/真实用户主目录（`CC_SWITCH_TEST_HOME` 优先，对齐 native 插件测试约定）。
fn home_dir() -> PathBuf {
    std::env::var("CC_SWITCH_TEST_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
}

/// CC Switch 数据根目录：对齐 v1 的 `~/.cc-switch/`。
fn cc_switch_home() -> PathBuf {
    home_dir().join(".cc-switch")
}

/// SSOT 目录。默认（`CcSwitch`）对齐 v1：`~/.cc-switch/skills/`；
/// `Unified` 为 `~/.agents/skills/`。
pub fn ssot_dir(data_dir: &Path, location: SkillStorageLocation) -> PathBuf {
    let _ = data_dir;
    match location {
        SkillStorageLocation::CcSwitch => cc_switch_home().join("skills"),
        SkillStorageLocation::Unified => home_dir().join(".agents").join("skills"),
    }
}

/// 技能卸载/更新备份目录（对齐 v1：`~/.cc-switch/skill-backups/`）。
fn backup_dir(data_dir: &Path) -> PathBuf {
    let _ = data_dir;
    cc_switch_home().join("skill-backups")
}

/// 技能名允许的字符白名单。只放行 ASCII 字母数字与 `-` `_` `.`。
fn sanitize_install_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.starts_with('.')
        || name == "."
        || name == ".."
    {
        return None;
    }
    Some(name.to_string())
}

/// 技能源目录：允许多级相对路径，拒绝绝对路径 / 空 / `.` / `..` / 盘符。
fn sanitize_skill_source_path(directory: &str) -> Option<PathBuf> {
    let p = Path::new(directory);
    if p.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(part) => out.push(part),
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        return None;
    }
    Some(out)
}

/// 校验 DB 中的 directory 字段：单段、不含分隔符/前导点、字节级往返一致
/// （拦截 Unicode 规范化不一致的脏值）。用于任何 join 后会有删除/复制风险的地方。
fn require_valid_directory(directory: &str) -> Result<PathBuf, String> {
    let sanitized = sanitize_skill_source_path(directory)
        .ok_or_else(|| format!("非法技能目录: {}", directory.escape_debug()))?;
    if sanitized.components().count() != 1 {
        return Err(format!("非法技能目录: {}", directory.escape_debug()));
    }
    if directory.starts_with('.') {
        return Err(format!("非法技能目录: {}", directory.escape_debug()));
    }
    if sanitized.to_string_lossy() != directory {
        return Err(format!("非法技能目录: {}", directory.escape_debug()));
    }
    Ok(sanitized)
}

/// GitHub 账号名（user/org login）：ASCII 字母数字与 `-`。
fn is_valid_github_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// GitHub 仓库名：允许 `.` `-` `_`，整体不能是 `.` 或 `..`。
fn is_valid_github_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// git 分支名：逐段白名单，额外禁 `#` 与 `%`（URL 改写风险）。
fn is_valid_git_branch(branch: &str) -> bool {
    if branch.is_empty() || branch.eq_ignore_ascii_case("HEAD") {
        return true;
    }
    if branch.len() > 255 {
        return false;
    }
    if branch.starts_with('/') || branch.ends_with('/') || branch.contains("//") {
        return false;
    }
    if branch.contains("@{") {
        return false;
    }
    if branch
        .chars()
        .any(|c| c.is_ascii_control() || " ~^:?*[\\#%".contains(c))
    {
        return false;
    }
    branch.split('/').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && !segment.ends_with('.')
            && !segment.ends_with(".lock")
    })
}

/// 校验仓库坐标（任何会被拼进 github.com URL 的地方都要走这里）。
pub fn validate_repo_ref(owner: &str, name: &str, branch: &str) -> Result<(), String> {
    if !is_valid_github_owner(owner) || !is_valid_github_repo_name(name) {
        return Err(skill_error(
            "INVALID_REPO_REF",
            &[("owner", owner), ("name", name)],
            Some("checkRepoUrl"),
        ));
    }
    if !is_valid_git_branch(branch) {
        return Err(skill_error(
            "INVALID_REPO_REF",
            &[("owner", owner), ("name", name), ("branch", branch)],
            Some("checkRepoUrl"),
        ));
    }
    Ok(())
}

/// 出口断言：URL 拼好后再确认它确实指向预期的 github.com 路径（纵深防御）。
fn assert_github_archive_url(url: &str, owner: &str, name: &str) -> Result<(), String> {
    let parsed =
        url::Url::parse(url).map_err(|e| format!("Invalid archive URL: {e}"))?;
    let expected_prefix = format!("/{owner}/{name}/archive/refs/heads/");
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || !parsed.path().starts_with(&expected_prefix)
    {
        return Err(skill_error(
            "INVALID_REPO_REF",
            &[("owner", owner), ("name", name)],
            Some("checkRepoUrl"),
        ));
    }
    Ok(())
}

// ========== 元数据与哈希 ==========

/// 从 `SKILL.md` frontmatter 解析名称与描述。
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

/// 解析 `SKILL.md` 内容为 `SkillMetadata`（安装名回退时用）。
fn parse_skill_metadata_content(dir: &Path) -> Option<SkillMetadata> {
    let content = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let mut name = None;
    let mut description = None;
    for line in content.lines() {
        if line.starts_with("---") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !v.is_empty() {
                name = Some(v);
            }
        } else if let Some(rest) = line.strip_prefix("description:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !v.is_empty() {
                description = Some(v);
            }
        }
    }
    Some(SkillMetadata { name, description })
}

/// 递归复制目录。
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 计算目录内容 SHA-256：按相对路径字典序，feed `relpath\0content\0`，跳过隐藏文件。
fn compute_dir_hash(dir: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let mut files = Vec::new();
    collect_files_for_hash(dir, dir, &mut files).map_err(|e| e.to_string())?;
    files.sort();

    let mut hasher = Sha256::new();
    for file_path in &files {
        let relative = file_path.strip_prefix(dir).unwrap_or(file_path);
        let rel_str = relative.to_string_lossy().replace('\\', "/");
        hasher.update(rel_str.as_bytes());
        hasher.update(b"\0");
        let content = std::fs::read(file_path).map_err(|e| e.to_string())?;
        hasher.update(&content);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files_for_hash(
    _base: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files_for_hash(_base, &path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

// ========== 归档解压（压缩炸弹防护） ==========

/// 归档预算唯一扣费点。
fn charge_archive_budget(total_bytes: &mut u64, amount: u64) -> Result<(), String> {
    if total_bytes.saturating_add(amount) > MAX_ARCHIVE_TOTAL_BYTES {
        let limit_mb = (MAX_ARCHIVE_TOTAL_BYTES / 1024 / 1024).to_string();
        return Err(skill_error(
            "ARCHIVE_TOO_LARGE",
            &[("limit_mb", &limit_mb)],
            Some("checkZipContent"),
        ));
    }
    *total_bytes += amount;
    Ok(())
}

/// 按实际写入字节累计（不信任归档头声明的 size）。
fn copy_entry_within_budget<R: std::io::Read, W: std::io::Write>(
    reader: &mut R,
    writer: &mut W,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            return Ok(());
        }
        charge_archive_budget(total_bytes, read as u64)?;
        writer.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
    }
}

/// 读取 symlink 目标（限量，超长/非 UTF-8 返回 None）。
fn read_symlink_target<R: std::io::Read>(
    reader: &mut R,
    total_bytes: &mut u64,
) -> Result<Option<String>, String> {
    let mut raw = Vec::new();
    let mut limited = std::io::Read::take(reader, MAX_SYMLINK_TARGET_BYTES + 1);
    std::io::Read::read_to_end(&mut limited, &mut raw).map_err(|e| e.to_string())?;
    if raw.len() as u64 > MAX_SYMLINK_TARGET_BYTES {
        return Ok(None);
    }
    charge_archive_budget(total_bytes, raw.len() as u64)?;
    Ok(String::from_utf8(raw)
        .ok()
        .map(|target| target.trim().to_string()))
}

/// 建目录并按实际新建的层数计费。
fn create_dir_all_within_budget(path: &Path, total_bytes: &mut u64) -> Result<(), String> {
    let missing = path.ancestors().take_while(|p| !p.exists()).count() as u64;
    if missing > 0 {
        charge_archive_budget(total_bytes, missing * DIRECTORY_BUDGET_COST)?;
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}

/// 复制目录（计入归档预算，仅用于解压期 symlink 物化）。
fn copy_dir_within_budget(src: &Path, dest: &Path, total_bytes: &mut u64) -> Result<(), String> {
    create_dir_all_within_budget(dest, total_bytes)?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_within_budget(&path, &dest_path, total_bytes)?;
        } else {
            let mut reader = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            if let Some(parent) = dest_path.parent() {
                create_dir_all_within_budget(parent, total_bytes)?;
            }
            let mut writer = std::fs::File::create(&dest_path).map_err(|e| e.to_string())?;
            copy_entry_within_budget(&mut reader, &mut writer, total_bytes)?;
        }
    }
    Ok(())
}

/// 第二遍：把 symlink 目标内容复制到 symlink 位置（物化，跨平台自包含）。
fn resolve_symlinks_in_dir(
    base_dir: &Path,
    symlinks: &[(PathBuf, String)],
    total_bytes: &mut u64,
) -> Result<(), String> {
    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());

    for (link_path, target) in symlinks {
        let parent = link_path.parent().unwrap_or(base_dir);
        let resolved = match parent.join(target).canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !resolved.starts_with(&canonical_base) {
            continue;
        }
        // 目标不能包含 link 自身（递归自复制防护），比较必须在规范形式上做。
        let canonical_link = match parent.canonicalize() {
            Ok(canonical_parent) => match link_path.file_name() {
                Some(name) => canonical_parent.join(name),
                None => canonical_parent,
            },
            Err(_) => match link_path.strip_prefix(base_dir) {
                Ok(relative) => canonical_base.join(relative),
                Err(_) => link_path.clone(),
            },
        };
        if canonical_link.starts_with(&resolved) {
            continue;
        }
        if resolved.is_dir() {
            copy_dir_within_budget(&resolved, link_path, total_bytes)?;
        } else if resolved.is_file() {
            if let Some(parent) = link_path.parent() {
                create_dir_all_within_budget(parent, total_bytes)?;
            }
            let mut reader = std::fs::File::open(&resolved).map_err(|e| e.to_string())?;
            let mut writer = std::fs::File::create(link_path).map_err(|e| e.to_string())?;
            copy_entry_within_budget(&mut reader, &mut writer, total_bytes)?;
        }
    }
    Ok(())
}

/// 解压 GitHub 仓库归档到 `dest`（剥掉归档自带一层根目录）。
fn extract_repo_archive<R: std::io::Read + std::io::Seek>(
    mut archive: zip::ZipArchive<R>,
    dest: &Path,
) -> Result<(), String> {
    let root_name = if !archive.is_empty() {
        let first_file = archive.by_index(0).map_err(|e| e.to_string())?;
        first_file
            .name()
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    } else {
        return Err(skill_error("EMPTY_ARCHIVE", &[], Some("checkRepoUrl")));
    };

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(skill_error(
            "ARCHIVE_TOO_MANY_ENTRIES",
            &[
                ("count", &archive.len().to_string()),
                ("limit", &MAX_ARCHIVE_ENTRIES.to_string()),
            ],
            Some("checkZipContent"),
        ));
    }
    let mut total_bytes: u64 = 0;
    let mut symlinks: Vec<(PathBuf, String)> = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        // 第一道：enclosed_name() 拒绝绝对路径、盘符、净深度为负。
        let Some(safe_path) = file.enclosed_name() else {
            log::warn!("跳过不安全的压缩包条目: {}", file.name());
            continue;
        };
        // 剥掉 `<repo>-<branch>/` 根目录。
        let Ok(relative_path) = safe_path.strip_prefix(&root_name) else {
            continue;
        };
        // 第二道：enclosed_name() 不规范化 `..`，剥根后必须对实际路径再验一次。
        if relative_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            log::warn!("跳过越界的压缩包条目: {}", file.name());
            continue;
        }
        if relative_path.as_os_str().is_empty() {
            continue;
        }

        let outpath = dest.join(relative_path);

        if file.is_symlink() {
            let Some(target) = read_symlink_target(&mut file, &mut total_bytes)? else {
                log::warn!("跳过目标不合法的 symlink 条目: {}", file.name());
                continue;
            };
            symlinks.push((outpath, target));
        } else if file.is_dir() {
            create_dir_all_within_budget(&outpath, &mut total_bytes)?;
        } else {
            if let Some(parent) = outpath.parent() {
                create_dir_all_within_budget(parent, &mut total_bytes)?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            copy_entry_within_budget(&mut file, &mut outfile, &mut total_bytes)?;
        }
    }

    resolve_symlinks_in_dir(dest, &symlinks, &mut total_bytes)
}

/// 解压本地 ZIP 到临时目录（守卫负责清理）。
fn extract_local_zip(zip_path: &Path) -> Result<tempfile::TempDir, String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("打开 ZIP 失败: {}: {e}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("读取 ZIP 失败: {}: {e}", zip_path.display()))?;
    if archive.is_empty() {
        return Err(skill_error("EMPTY_ARCHIVE", &[], Some("checkZipContent")));
    }
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(skill_error(
            "ARCHIVE_TOO_MANY_ENTRIES",
            &[
                ("count", &archive.len().to_string()),
                ("limit", &MAX_ARCHIVE_ENTRIES.to_string()),
            ],
            Some("checkZipContent"),
        ));
    }

    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let temp_path = temp_dir.path().to_path_buf();
    let mut symlinks: Vec<(PathBuf, String)> = Vec::new();
    let mut total_bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let file_path = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        if file_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            log::warn!("跳过越界的压缩包条目: {}", file.name());
            continue;
        }
        let outpath = temp_path.join(&file_path);
        if file.is_symlink() {
            let Some(target) = read_symlink_target(&mut file, &mut total_bytes)? else {
                log::warn!("跳过目标不合法的 symlink 条目: {}", file.name());
                continue;
            };
            symlinks.push((outpath, target));
        } else if file.is_dir() {
            create_dir_all_within_budget(&outpath, &mut total_bytes)?;
        } else {
            if let Some(parent) = outpath.parent() {
                create_dir_all_within_budget(parent, &mut total_bytes)?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            copy_entry_within_budget(&mut file, &mut outfile, &mut total_bytes)?;
        }
    }
    resolve_symlinks_in_dir(&temp_path, &symlinks, &mut total_bytes)?;
    Ok(temp_dir)
}

/// 递归扫描目录，收集包含 `SKILL.md` 的技能目录（遇到技能目录即不深入）。
fn scan_skills_in_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut results = Vec::new();
    scan_skills_recursive(dir, &mut results)?;
    Ok(results)
}

fn scan_skills_recursive(current: &Path, results: &mut Vec<PathBuf>) -> Result<(), String> {
    if current.join("SKILL.md").is_file() {
        results.push(current.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        scan_skills_recursive(&path, results)?;
    }
    Ok(())
}

/// 在目录树中查找名称匹配且包含 SKILL.md 的子目录（深度 ≤ 3）。
fn find_skill_dir_by_name(root: &Path, target_name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, target: &str, depth: usize) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        let entries: Vec<std::fs::DirEntry> =
            std::fs::read_dir(dir).ok()?.flatten().collect();
        for entry in &entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if name.eq_ignore_ascii_case(target) && path.join("SKILL.md").is_file() {
                return Some(path);
            }
        }
        for entry in &entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if let Some(found) = walk(&path, target, depth + 1) {
                return Some(found);
            }
        }
        None
    }
    walk(root, target_name, 0)
}

/// 解析仓库解压目录中的技能源目录（必须含 SKILL.md）。
fn resolve_skill_source_dir(root: &Path, directory: &str) -> Option<PathBuf> {
    if let Some(rel) = sanitize_skill_source_path(directory) {
        let direct = root.join(&rel);
        if direct.is_dir() && direct.join("SKILL.md").is_file() {
            return Some(direct);
        }
    }
    let install_name = directory.rsplit('/').next().unwrap_or(directory);
    if let Some(found) = find_skill_dir_by_name(root, install_name) {
        return Some(found);
    }
    if root.join("SKILL.md").is_file() {
        return Some(root.to_path_buf());
    }
    None
}

/// 从真实源目录推导仓库内 SKILL.md 相对路径（供 readme_url）。
fn doc_path_for_source(temp: &Path, source: &Path) -> Option<String> {
    let rel = source.strip_prefix(temp).ok()?;
    Some(format!(
        "{}/SKILL.md",
        rel.to_string_lossy().replace('\\', "/")
    ))
}

/// 从旧 readme_url 提取仓库内文档路径（兼容 `blob`/`tree`）。
fn extract_doc_path_from_url(url: &str) -> Option<String> {
    let marker = if url.contains("/blob/") {
        "/blob/"
    } else if url.contains("/tree/") {
        "/tree/"
    } else {
        return None;
    };
    let (_, tail) = url.split_once(marker)?;
    let (_, path) = tail.split_once('/')?;
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// 构建指向仓库内 SKILL.md 的 GitHub 链接。
fn build_skill_doc_url(
    owner: &str,
    repo: &str,
    branch: &str,
    doc_path: &str,
) -> Option<String> {
    if validate_repo_ref(owner, repo, branch).is_err() {
        return None;
    }
    Some(format!(
        "https://github.com/{owner}/{repo}/blob/{branch}/{doc_path}"
    ))
}

/// 选择文档路径：真实解析出的优先，其次旧 readme_url，最后 directory 拼接。
fn choose_doc_path(
    resolved: Option<String>,
    readme_url: Option<&str>,
    directory: &str,
) -> String {
    resolved.unwrap_or_else(|| {
        readme_url
            .and_then(extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", directory.trim_end_matches('/')))
    })
}

// ========== HTTP ==========

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REPO_DOWNLOAD_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 下载并卡住压缩体大小，返回完整字节。
async fn download_archive(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_e| skill_error("DOWNLOAD_FAILED", &[("status", "0")], Some("checkNetwork")))?;
    if !response.status().is_success() {
        let status = response.status().as_u16().to_string();
        return Err(skill_error(
            "DOWNLOAD_FAILED",
            &[("status", &status)],
            match status.as_str() {
                "403" => Some("http403"),
                "404" => Some("http404"),
                "429" => Some("http429"),
                _ => Some("checkNetwork"),
            },
        ));
    }
    let mut body: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if body.len().saturating_add(chunk.len()) as u64 > MAX_ARCHIVE_DOWNLOAD_BYTES {
            let limit_mb = (MAX_ARCHIVE_DOWNLOAD_BYTES / 1024 / 1024).to_string();
            return Err(skill_error(
                "ARCHIVE_TOO_LARGE",
                &[("limit_mb", &limit_mb)],
                Some("checkZipContent"),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// 下载仓库归档并解压，返回 `(临时目录, 实际使用的分支)`。
async fn download_repo(
    client: &reqwest::Client,
    repo: &SkillRepo,
) -> Result<(tempfile::TempDir, String), String> {
    validate_repo_ref(&repo.owner, &repo.name, &repo.branch)?;

    let mut branches: Vec<String> = Vec::new();
    if !repo.branch.is_empty() && !repo.branch.eq_ignore_ascii_case("HEAD") {
        branches.push(repo.branch.clone());
    }
    if !branches.iter().any(|b| b.eq_ignore_ascii_case("main")) {
        branches.push("main".to_string());
    }
    if !branches.iter().any(|b| b.eq_ignore_ascii_case("master")) {
        branches.push("master".to_string());
    }

    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut last_err: Option<String> = None;
    for branch in &branches {
        let url = format!(
            "https://github.com/{}/{}/archive/refs/heads/{}.zip",
            repo.owner, repo.name, branch
        );
        assert_github_archive_url(&url, &repo.owner, &repo.name)?;
        let _ = std::fs::remove_dir_all(temp.path());
        let _ = std::fs::create_dir_all(temp.path());
        match download_archive(client, &url).await {
            Ok(bytes) => {
                let cursor = std::io::Cursor::new(bytes);
                let archive = zip::ZipArchive::new(cursor)
                    .map_err(|e| format!("归档解析失败: {e}"))?;
                match extract_repo_archive(archive, temp.path()) {
                    Ok(()) => return Ok((temp, branch.clone())),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| "下载仓库失败".to_string()))
}

// ========== skills.sh ==========

/// 搜索 skills.sh 公共注册表。
pub async fn search_skills_sh(
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<SkillsShSearchResult, String> {
    let url = url::Url::parse_with_params(
        "https://skills.sh/api/search",
        &[
            ("q", query),
            ("limit", &limit.to_string()),
            ("offset", &offset.to_string()),
        ],
    )
    .map_err(|e| e.to_string())?;
    let client = http_client();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_e| skill_error("DOWNLOAD_FAILED", &[("status", "0")], Some("checkNetwork")))?
        .error_for_status()
        .map_err(|_e| skill_error("DOWNLOAD_FAILED", &[("status", "0")], Some("checkNetwork")))?;
    let data: SkillsShApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("skills.sh 响应解析失败: {e}"))?;

    let mut skills = Vec::new();
    for s in data.skills {
        let mut parts = s.source.splitn(2, '/');
        let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
            continue;
        };
        if validate_repo_ref(owner, repo, "main").is_err() {
            continue;
        }
        skills.push(SkillsShDiscoverableSkill {
            key: s.id.clone(),
            name: s.name,
            directory: s.skill_id,
            repo_owner: owner.to_string(),
            repo_name: repo.to_string(),
            repo_branch: "main".to_string(),
            installs: s.installs,
            readme_url: Some(format!("https://github.com/{owner}/{repo}")),
        });
    }

    Ok(SkillsShSearchResult {
        skills,
        total_count: data.count,
        query: query.to_string(),
    })
}

// ========== 分发（软链/复制） ==========

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 判断路径是否为符号链接。
fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// 删除路径（支持 symlink 与真实目录/文件）。
fn remove_path(path: &Path) -> Result<(), String> {
    if is_symlink(path) {
        #[cfg(unix)]
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        std::fs::remove_dir(path).map_err(|e| e.to_string())?;
    } else if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    } else if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(src: &Path, dest: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(src, dest).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn create_symlink(src: &Path, dest: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(src, dest).map_err(|e| e.to_string())
}

/// 校验同步源目录：必须是目录且含 SKILL.md（避免覆盖目标目录）。
fn validate_sync_source_dir(source: &Path, directory: &str) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("技能不存在于 SSOT: {directory}"));
    }
    if !source.join("SKILL.md").is_file() {
        return Err(format!(
            "技能源目录缺少 SKILL.md，拒绝同步: {}",
            source.display()
        ));
    }
    Ok(())
}

/// 先复制到临时目录再原子替换目标（复制路径安全替换）。
fn replace_dest_with_copy(source: &Path, dest: &Path, directory: &str) -> Result<(), String> {
    validate_sync_source_dir(source, directory)?;
    let parent = dest
        .parent()
        .ok_or_else(|| format!("非法技能目标目录: {}", dest.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let nonce = now_ts();
    let slug = sanitize_backup_segment(directory);
    let tmp = parent.join(format!(".{slug}.tmp-{}-{nonce}", std::process::id()));
    if tmp.exists() || is_symlink(&tmp) {
        remove_path(&tmp)?;
    }
    if let Err(err) = copy_dir_recursive(source, &tmp) {
        let _ = remove_path(&tmp);
        return Err(err);
    }
    if dest.exists() || is_symlink(dest) {
        remove_path(dest)?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = remove_path(&tmp);
        format!("替换技能目录失败: {} -> {}: {e}", tmp.display(), dest.display())
    })
}

/// 将技能同步到目标插件目录（按同步方式）。
pub fn sync_skill_to_dir(
    ssot: &Path,
    directory: &str,
    dest_root: &Path,
    method: SyncMethod,
) -> Result<(), String> {
    let directory = require_valid_directory(directory)?;
    let source = ssot.join(&directory);
    validate_sync_source_dir(&source, &directory.to_string_lossy())?;

    std::fs::create_dir_all(dest_root).map_err(|e| e.to_string())?;
    let dest = dest_root.join(&directory);

    match method {
        SyncMethod::Auto => {
            if dest.exists() && !is_symlink(&dest) {
                return replace_dest_with_copy(&source, &dest, &directory.to_string_lossy());
            }
            if is_symlink(&dest) {
                remove_path(&dest)?;
            }
            match create_symlink(&source, &dest) {
                Ok(()) => Ok(()),
                Err(_) => replace_dest_with_copy(&source, &dest, &directory.to_string_lossy()),
            }
        }
        SyncMethod::Symlink => {
            if dest.exists() || is_symlink(&dest) {
                remove_path(&dest)?;
            }
            create_symlink(&source, &dest)
        }
        SyncMethod::Copy => {
            replace_dest_with_copy(&source, &dest, &directory.to_string_lossy())
        }
    }
}

/// 从插件目录移除技能。
pub fn remove_skill_from_dir(directory: &str, dest_root: &Path) -> Result<(), String> {
    let directory = require_valid_directory(directory)?;
    let path = dest_root.join(&directory);
    if path.exists() || is_symlink(&path) {
        remove_path(&path)?;
    }
    Ok(())
}

/// 备份目录名段 sanitize。
fn sanitize_backup_segment(segment: &str) -> String {
    let sanitized = segment
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "skill".to_string()
    } else {
        sanitized
    }
}

/// 清理超出保留份数的旧备份。
fn cleanup_old_skill_backups(dir: &Path) -> Result<(), String> {
    let mut entries: Vec<(PathBuf, Option<std::time::SystemTime>)> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_dir() {
                return None;
            }
            Some((entry.path(), metadata.modified().ok()))
        })
        .collect();
    if entries.len() <= SKILL_BACKUP_RETAIN_COUNT {
        return Ok(());
    }
    entries.sort_by_key(|(_, modified)| *modified);
    let remove_count = entries.len().saturating_sub(SKILL_BACKUP_RETAIN_COUNT);
    for (path, _) in entries.into_iter().take(remove_count) {
        let _ = std::fs::remove_dir_all(&path);
    }
    Ok(())
}

/// 创建卸载/更新备份，返回备份目录路径。
fn create_backup(data_dir: &Path, directory: &str, source: &Path) -> Result<Option<PathBuf>, String> {
    let backup_root = backup_dir(data_dir);
    std::fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let slug = sanitize_backup_segment(directory);
    let mut backup_path = backup_root.join(format!("{timestamp}_{slug}"));
    let mut counter = 1;
    while backup_path.exists() {
        backup_path = backup_root.join(format!("{timestamp}_{slug}_{counter}"));
        counter += 1;
    }

    let result = (|| -> Result<(), String> {
        let skill_backup_dir = backup_path.join("skill");
        copy_dir_recursive(source, &skill_backup_dir)?;
        let metadata = SkillBackupMetadata {
            name: directory.to_string(),
            directory: directory.to_string(),
            description: None,
            backup_created_at: now_ts(),
            source_path: source.to_string_lossy().to_string(),
        };
        let json = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
        std::fs::write(backup_path.join("meta.json"), json).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&backup_path);
        return Err(e);
    }
    let _ = cleanup_old_skill_backups(&backup_root);
    Ok(Some(backup_path))
}

/// 校验备份 id（用于 join 备份目录，防穿越）。
fn backup_path_for_id(data_dir: &Path, backup_id: &str) -> Result<PathBuf, String> {
    if backup_id.contains("..")
        || backup_id.contains('/')
        || backup_id.contains('\\')
        || backup_id.trim().is_empty()
    {
        return Err(format!("非法备份 id: {backup_id}"));
    }
    Ok(backup_dir(data_dir).join(backup_id))
}

// ========== SkillService ==========

/// Skills 业务逻辑（对齐 v1 `SkillService`，路径参数化以便测试）。
pub struct SkillService;

impl SkillService {
    /// 读取同步设置。
    pub fn get_sync_settings(db: &Database) -> Result<SyncSettings, String> {
        let sync_method = db
            .get_setting("skills.syncMethod")
            .map_err(|e| e.to_string())?
            .map(|v| serde_json::from_str::<SyncMethod>(&v))
            .transpose()
            .map_err(|e| format!("同步方式设置非法: {e}"))?
            .unwrap_or_default();
        let storage_location = db
            .get_setting("skills.storageLocation")
            .map_err(|e| e.to_string())?
            .map(|v| serde_json::from_str::<SkillStorageLocation>(&v))
            .transpose()
            .map_err(|e| format!("存储位置设置非法: {e}"))?
            .unwrap_or_default();
        Ok(SyncSettings {
            sync_method,
            storage_location,
        })
    }

    /// 设置同步方式。
    pub fn set_sync_method(db: &Database, method: SyncMethod) -> Result<(), String> {
        let value = serde_json::to_string(&method).map_err(|e| e.to_string())?;
        db.set_setting("skills.syncMethod", &value)
            .map_err(|e| e.to_string())
    }

    /// 迁移存储位置：先移文件，后改设置。
    pub fn migrate_storage(
        db: &Database,
        data_dir: &Path,
        target: SkillStorageLocation,
    ) -> Result<MigrationResult, String> {
        let settings = Self::get_sync_settings(db)?;
        if settings.storage_location == target {
            return Ok(MigrationResult {
                migrated_count: 0,
                skipped_count: 0,
                errors: vec![],
            });
        }
        let old_dir = ssot_dir(data_dir, settings.storage_location);
        let new_dir = ssot_dir(data_dir, target);
        std::fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?;

        let mut result = MigrationResult {
            migrated_count: 0,
            skipped_count: 0,
            errors: vec![],
        };
        for skill in db.list_skills().map_err(|e| e.to_string())? {
            let directory = match require_valid_directory(&skill.directory) {
                Ok(d) => d,
                Err(e) => {
                    result.errors.push(e);
                    continue;
                }
            };
            let src = old_dir.join(&directory);
            let dst = new_dir.join(&directory);
            if !src.exists() {
                result.skipped_count += 1;
                continue;
            }
            if dst.exists() {
                result.skipped_count += 1;
                continue;
            }
            match std::fs::rename(&src, &dst) {
                Ok(()) => result.migrated_count += 1,
                Err(_) => match copy_dir_recursive(&src, &dst) {
                    Ok(()) => {
                        let _ = std::fs::remove_dir_all(&src);
                        result.migrated_count += 1;
                    }
                    Err(e) => result.errors.push(e),
                },
            }
        }
        // 先移文件后改设置。
        db.set_setting(
            "skills.storageLocation",
            &serde_json::to_string(&target).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(result)
    }

    /// 从仓库安装技能（下载 → 复制到 SSOT → 写库 → 启用当前插件）。
    pub async fn install_from_repo(
        db: &Database,
        data_dir: &Path,
        skill: &DiscoverableSkill,
        current_plugin: &str,
    ) -> Result<SkillRecord, String> {
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        std::fs::create_dir_all(&ssot).map_err(|e| e.to_string())?;

        let source_rel = sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            )
        })?;
        let install_name = source_rel
            .file_name()
            .and_then(|n| sanitize_install_name(&n.to_string_lossy()))
            .ok_or_else(|| {
                skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                )
            })?;

        // 目录冲突检测：同名目录来自不同仓库时拒绝。
        let existing = db.list_skills().map_err(|e| e.to_string())?;
        if let Some(conflict) = existing
            .iter()
            .find(|s| s.directory.eq_ignore_ascii_case(&install_name))
        {
            let same_repo = conflict.repo_owner.as_deref() == Some(&skill.repo_owner)
                && conflict.repo_name.as_deref() == Some(&skill.repo_name);
            if same_repo {
                db.set_skill_plugin_enabled(&conflict.id, current_plugin, true)
                    .map_err(|e| e.to_string())?;
                return Ok(conflict.clone());
            }
            return Err(skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", &install_name),
                    (
                        "existing_repo",
                        &format!(
                            "{}/{}",
                            conflict.repo_owner.as_deref().unwrap_or("unknown"),
                            conflict.repo_name.as_deref().unwrap_or("unknown")
                        ),
                    ),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name),
                    ),
                ],
                Some("uninstallFirst"),
            ));
        }

        let dest = ssot.join(&install_name);
        let mut repo_branch = skill.repo_branch.clone();
        let mut resolved_doc_path: Option<String> = None;

        if !dest.exists() {
            let repo = SkillRepo {
                owner: skill.repo_owner.clone(),
                name: skill.repo_name.clone(),
                branch: skill.repo_branch.clone(),
                enabled: true,
            };
            let client = http_client();
            let (temp_guard, used_branch) = tokio::time::timeout(
                REPO_DOWNLOAD_TIMEOUT,
                download_repo(&client, &repo),
            )
            .await
            .map_err(|_| {
                skill_error(
                    "DOWNLOAD_TIMEOUT",
                    &[
                        ("owner", &repo.owner),
                        ("name", &repo.name),
                        ("timeout", "60"),
                    ],
                    Some("checkNetwork"),
                )
            })??;
            let temp_dir = temp_guard.path();
            repo_branch = used_branch;

            let source = resolve_skill_source_dir(temp_dir, &skill.directory).ok_or_else(|| {
                let missing = temp_dir.join(&source_rel).display().to_string();
                skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &missing)],
                    Some("checkRepoUrl"),
                )
            })?;

            let canonical_temp = temp_dir
                .canonicalize()
                .unwrap_or_else(|_| temp_dir.to_path_buf());
            let canonical_source = source.canonicalize().map_err(|_| {
                skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &source.display().to_string())],
                    Some("checkRepoUrl"),
                )
            })?;
            if !canonical_source.starts_with(&canonical_temp) || !canonical_source.is_dir() {
                return Err(skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ));
            }
            resolved_doc_path = doc_path_for_source(&canonical_temp, &canonical_source);
            copy_dir_recursive(&canonical_source, &dest)?;
        }

        let doc_path = choose_doc_path(
            resolved_doc_path,
            skill.readme_url.as_deref(),
            &skill.directory,
        );
        let readme_url =
            build_skill_doc_url(&skill.repo_owner, &skill.repo_name, &repo_branch, &doc_path);
        let content_hash = compute_dir_hash(&dest).ok();
        let (name, description) = parse_skill_metadata(&dest, &install_name);

        let record = SkillRecord {
            id: skill.key.clone(),
            name,
            description,
            directory: install_name.clone(),
            source_path: Some(format!("{}/{}", skill.repo_owner, skill.repo_name)),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch),
            readme_url,
            enabled_plugins: vec![current_plugin.to_string()],
            installed_at: now_ts(),
            content_hash,
            updated_at: 0,
        };
        db.save_skill(&record).map_err(|e| e.to_string())?;
        db.set_skill_plugin_enabled(&record.id, current_plugin, true)
            .map_err(|e| e.to_string())?;
        Ok(record)
    }

    /// 从本地 ZIP 安装技能。
    pub fn install_from_zip(
        db: &Database,
        data_dir: &Path,
        zip_path: &Path,
        current_plugin: &str,
    ) -> Result<Vec<SkillRecord>, String> {
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        std::fs::create_dir_all(&ssot).map_err(|e| e.to_string())?;

        let temp_guard = extract_local_zip(zip_path)?;
        let temp_dir = temp_guard.path();
        let skill_dirs = scan_skills_in_dir(temp_dir)?;
        if skill_dirs.is_empty() {
            return Err(skill_error("NO_SKILLS_IN_ZIP", &[], Some("checkZipContent")));
        }

        let existing = db.list_skills().map_err(|e| e.to_string())?;
        let zip_stem = zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let mut installed = Vec::new();

        for skill_dir in skill_dirs {
            let meta = parse_skill_metadata_content(&skill_dir);
            let dir_name = skill_dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let install_name = if skill_dir.as_path() == temp_dir
                || dir_name.is_empty()
                || dir_name.starts_with('.')
            {
                meta.as_ref()
                    .and_then(|m| m.name.as_deref())
                    .and_then(sanitize_install_name)
                    .or_else(|| zip_stem.as_deref().and_then(sanitize_install_name))
            } else {
                sanitize_install_name(&dir_name)
                    .or_else(|| {
                        meta.as_ref()
                            .and_then(|m| m.name.as_deref())
                            .and_then(sanitize_install_name)
                    })
                    .or_else(|| zip_stem.as_deref().and_then(sanitize_install_name))
            };
            let install_name = match install_name {
                Some(name) => name,
                None => {
                    return Err(skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    ));
                }
            };

            // 同名目录跳过（不同来源）。
            if existing
                .iter()
                .any(|s| s.directory.eq_ignore_ascii_case(&install_name))
            {
                continue;
            }

            let (name, description) = match meta {
                Some(m) => (
                    m.name.unwrap_or_else(|| install_name.clone()),
                    m.description,
                ),
                None => (install_name.clone(), None),
            };

            let dest = ssot.join(&install_name);
            if dest.exists() {
                let _ = std::fs::remove_dir_all(&dest);
            }
            copy_dir_recursive(&skill_dir, &dest)?;
            let content_hash = compute_dir_hash(&dest).ok();

            let record = SkillRecord {
                id: format!("local:{install_name}"),
                name,
                description,
                directory: install_name.clone(),
                source_path: None,
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                enabled_plugins: vec![current_plugin.to_string()],
                installed_at: now_ts(),
                content_hash,
                updated_at: 0,
            };
            db.save_skill(&record).map_err(|e| e.to_string())?;
            db.set_skill_plugin_enabled(&record.id, current_plugin, true)
                .map_err(|e| e.to_string())?;
            installed.push(record);
        }
        Ok(installed)
    }

    /// 从本地目录安装技能（兼容旧命令，安装名为目录名）。
    pub fn install_local_dir(
        db: &Database,
        data_dir: &Path,
        source: &Path,
        id: &str,
    ) -> Result<SkillRecord, String> {
        if !source.is_dir() {
            return Err(format!("技能源目录不存在: {}", source.display()));
        }
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        std::fs::create_dir_all(&ssot).map_err(|e| e.to_string())?;
        let dest = ssot.join(id);
        if dest.exists() {
            return Err(format!("技能已安装: {id}"));
        }
        copy_dir_recursive(source, &dest)?;

        let (name, description) = parse_skill_metadata(&dest, id);
        let record = SkillRecord {
            id: id.to_string(),
            name,
            description,
            directory: id.to_string(),
            source_path: None,
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            enabled_plugins: vec![],
            installed_at: now_ts(),
            content_hash: compute_dir_hash(&dest).ok(),
            updated_at: 0,
        };
        db.save_skill(&record).map_err(|e| e.to_string())?;
        Ok(record)
    }

    /// 发现全部启用仓库中的技能（并发拉取、去重、排序）。
    pub async fn discover(
        _db: &Database,
        _data_dir: &Path,
        repos: Vec<SkillRepo>,
    ) -> Result<Vec<DiscoverableSkill>, String> {
        let enabled: Vec<SkillRepo> = repos.into_iter().filter(|r| r.enabled).collect();
        let client = http_client();
        let futures = enabled.iter().map(|repo| fetch_repo_skills(&client, repo));
        let results = futures::future::join_all(futures).await;

        let mut skills = Vec::new();
        for (repo, result) in enabled.iter().zip(results) {
            match result {
                Ok(mut repo_skills) => {
                    for s in &mut repo_skills {
                        if s.repo_branch.is_empty() {
                            s.repo_branch = repo.branch.clone();
                        }
                    }
                    skills.extend(repo_skills);
                }
                Err(e) => log::warn!(
                    "获取仓库 {}/{} 技能失败: {}",
                    repo.owner,
                    repo.name,
                    e
                ),
            }
        }
        skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(skills)
    }

    /// 检查全部已安装技能是否有更新（按仓库分组各下载一次）。
    pub async fn check_updates(
        db: &Database,
        data_dir: &Path,
    ) -> Result<Vec<SkillUpdateInfo>, String> {
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        let skills = db.list_skills().map_err(|e| e.to_string())?;

        let mut groups: std::collections::HashMap<(String, String, String), Vec<SkillRecord>> =
            std::collections::HashMap::new();
        for skill in &skills {
            let (Some(owner), Some(name), Some(branch)) = (
                &skill.repo_owner,
                &skill.repo_name,
                &skill.repo_branch,
            ) else {
                continue;
            };
            groups
                .entry((owner.clone(), name.clone(), branch.clone()))
                .or_default()
                .push(skill.clone());
        }

        let client = http_client();
        let mut updates = Vec::new();
        for ((owner, name, branch), group) in &groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch: branch.clone(),
                enabled: true,
            };
            let (temp_guard, _) = match tokio::time::timeout(
                REPO_DOWNLOAD_TIMEOUT,
                download_repo(&client, &repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                _ => continue,
            };
            let temp_dir = temp_guard.path();
            let mut remote_skills = Vec::new();
            let _ = scan_dir_recursive(temp_dir, temp_dir, &repo, &mut remote_skills);

            for skill in group {
                let remote_match = remote_skills.iter().find(|rs| {
                    rs.directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&rs.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                });
                let Some(rs) = remote_match else {
                    continue;
                };
                let Some(remote_dir) = resolve_skill_source_dir(temp_dir, &rs.directory) else {
                    continue;
                };
                let Ok(remote_hash) = compute_dir_hash(&remote_dir) else {
                    continue;
                };
                let local_hash = match &skill.content_hash {
                    Some(h) => Some(h.clone()),
                    None => {
                        let directory = match require_valid_directory(&skill.directory) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        let local_dir = ssot.join(&directory);
                        if local_dir.is_dir() {
                            match compute_dir_hash(&local_dir) {
                                Ok(h) => {
                                    let _ = db.update_skill_hash(&skill.id, &h, 0);
                                    Some(h)
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                };
                if local_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id.clone(),
                        name: skill.name.clone(),
                        current_hash: local_hash,
                        remote_hash,
                    });
                }
            }
        }
        Ok(updates)
    }

    /// 更新单个技能（重新下载、备份旧版、替换 SSOT、重算哈希、重同步）。
    pub async fn update_skill(
        db: &Database,
        data_dir: &Path,
        id: &str,
    ) -> Result<SkillRecord, String> {
        let skill = db
            .get_skill(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("技能不存在: {id}"))?;
        let directory = require_valid_directory(&skill.directory)?;

        let (Some(owner), Some(name)) = (&skill.repo_owner, &skill.repo_name) else {
            return Err(format!("本地技能无法更新: {id}"));
        };
        let branch = skill
            .repo_branch
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch,
            enabled: true,
        };

        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);

        let client = http_client();
        let (temp_guard, used_branch) = tokio::time::timeout(
            REPO_DOWNLOAD_TIMEOUT,
            download_repo(&client, &repo),
        )
        .await
        .map_err(|_| {
            skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", owner), ("name", name), ("timeout", "60")],
                Some("checkNetwork"),
            )
        })??;
        let temp_dir = temp_guard.path();

        let mut remote_skills = Vec::new();
        let _ = scan_dir_recursive(temp_dir, temp_dir, &repo, &mut remote_skills);
        let remote_match = remote_skills
            .iter()
            .find(|rs| {
                rs.directory
                    .rsplit('/')
                    .next()
                    .unwrap_or(&rs.directory)
                    .eq_ignore_ascii_case(&skill.directory)
            })
            .ok_or_else(|| {
                skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &skill.directory)],
                    Some("checkRepoUrl"),
                )
            })?;
        let source =
            resolve_skill_source_dir(temp_dir, &remote_match.directory).ok_or_else(|| {
                skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &skill.directory)],
                    Some("checkRepoUrl"),
                )
            })?;

        // 下载期间用户可能卸载/修改，重新校验。
        let current = db
            .get_skill(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("技能不存在: {id}"))?;
        if current.directory != skill.directory
            || current.repo_owner != skill.repo_owner
            || current.repo_name != skill.repo_name
            || current.installed_at != skill.installed_at
        {
            return Err(format!("更新期间技能被修改: {id}"));
        }
        require_valid_directory(&current.directory)?;

        // 备份旧版 → 替换 → 重算。
        let dest = ssot.join(&directory);
        if dest.is_dir() {
            let _ = create_backup(data_dir, &skill.directory, &dest);
            let _ = std::fs::remove_dir_all(&dest);
        }
        copy_dir_recursive(&source, &dest)?;
        let new_hash = compute_dir_hash(&dest).ok();
        let (new_name, new_description) = parse_skill_metadata(&dest, &skill.directory);

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let readme_url = build_skill_doc_url(owner, name, &used_branch, &doc_path);

        let updated = SkillRecord {
            id: skill.id.clone(),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            source_path: skill.source_path.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(used_branch),
            readme_url,
            enabled_plugins: vec![],
            installed_at: skill.installed_at,
            content_hash: new_hash,
            updated_at: now_ts(),
        };
        db.update_skill_metadata(&updated).map_err(|e| e.to_string())?;
        Ok(updated)
    }

    /// 卸载技能：备份 → 从各插件目录删除 → 删 SSOT → 删记录。
    ///
    /// `skills_dirs` 为全部插件的 skills 目录（插件 id 无感知，仅用于清理）。
    pub fn uninstall(
        db: &Database,
        data_dir: &Path,
        skills_dirs: &[&Path],
        id: &str,
    ) -> Result<Option<String>, String> {
        let skill = db
            .get_skill(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("技能不存在: {id}"))?;

        let backup_path = match require_valid_directory(&skill.directory) {
            Ok(directory) => {
                let settings = Self::get_sync_settings(db)?;
                let ssot = ssot_dir(data_dir, settings.storage_location);
                let ssot_path = ssot.join(&directory);
                let backup = if ssot_path.is_dir() {
                    create_backup(data_dir, &skill.directory, &ssot_path)?
                } else {
                    None
                };
                for dest_root in skills_dirs {
                    let _ = remove_skill_from_dir(&skill.directory, dest_root);
                }
                if ssot_path.exists() {
                    std::fs::remove_dir_all(&ssot_path).map_err(|e| e.to_string())?;
                }
                backup.map(|p| p.to_string_lossy().to_string())
            }
            Err(err) => {
                log::warn!(
                    "Skill {id} 的 directory 非法（{:?}），跳过文件清理，仅删除记录: {err}",
                    skill.directory
                );
                None
            }
        };
        db.delete_skill(id).map_err(|e| e.to_string())?;
        Ok(backup_path)
    }

    /// 列出备份。
    pub fn list_backups(data_dir: &Path) -> Result<Vec<SkillBackupEntry>, String> {
        let backup_root = backup_dir(data_dir);
        let mut entries = Vec::new();
        if !backup_root.exists() {
            return Ok(entries);
        }
        for entry in std::fs::read_dir(&backup_root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let metadata_path = path.join("meta.json");
            let Ok(content) = std::fs::read_to_string(&metadata_path) else {
                continue;
            };
            let Ok(metadata) = serde_json::from_str::<SkillBackupMetadata>(&content) else {
                continue;
            };
            entries.push(SkillBackupEntry {
                backup_id: entry.file_name().to_string_lossy().to_string(),
                backup_path: path.to_string_lossy().to_string(),
                created_at: metadata.backup_created_at,
                name: metadata.name,
                directory: metadata.directory,
                description: metadata.description,
            });
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        Ok(entries)
    }

    /// 删除备份。
    pub fn delete_backup(data_dir: &Path, backup_id: &str) -> Result<(), String> {
        let backup_path = backup_path_for_id(data_dir, backup_id)?;
        let metadata = std::fs::symlink_metadata(&backup_path)
            .map_err(|e| format!("访问备份失败: {}: {e}", backup_path.display()))?;
        if !metadata.is_dir() {
            return Err(format!("备份不是目录: {}", backup_path.display()));
        }
        std::fs::remove_dir_all(&backup_path).map_err(|e| e.to_string())
    }

    /// 从备份恢复技能，并为当前插件启用。
    pub fn restore_backup(
        db: &Database,
        data_dir: &Path,
        backup_id: &str,
        current_plugin: &str,
    ) -> Result<SkillRecord, String> {
        let backup_path = backup_path_for_id(data_dir, backup_id)?;
        let metadata_path = backup_path.join("meta.json");
        let content = std::fs::read_to_string(&metadata_path)
            .map_err(|e| format!("读取备份元数据失败: {e}"))?;
        let metadata: SkillBackupMetadata =
            serde_json::from_str(&content).map_err(|e| format!("解析备份元数据失败: {e}"))?;
        let skill_dir = backup_path.join("skill");
        if !skill_dir.join("SKILL.md").is_file() {
            return Err(format!(
                "备份无效或缺少 SKILL.md: {}",
                backup_path.display()
            ));
        }

        let directory = require_valid_directory(&metadata.directory)?;
        let existing = db.list_skills().map_err(|e| e.to_string())?;
        if existing
            .iter()
            .any(|s| s.directory.eq_ignore_ascii_case(&directory.to_string_lossy()))
        {
            return Err(format!(
                "技能已存在，请先卸载当前同名技能: {}",
                directory.to_string_lossy()
            ));
        }

        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        std::fs::create_dir_all(&ssot).map_err(|e| e.to_string())?;
        let restore_path = ssot.join(&directory);
        if restore_path.exists() || is_symlink(&restore_path) {
            return Err(format!(
                "恢复目标已存在: {}",
                restore_path.display()
            ));
        }

        copy_dir_recursive(&skill_dir, &restore_path)?;
        let content_hash = compute_dir_hash(&restore_path).ok();
        let (name, description) = parse_skill_metadata(&restore_path, &directory.to_string_lossy());

        let id = format!("local:{}", directory.to_string_lossy());
        let record = SkillRecord {
            id: id.clone(),
            name,
            description,
            directory: directory.to_string_lossy().to_string(),
            source_path: Some(metadata.source_path),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            enabled_plugins: vec![current_plugin.to_string()],
            installed_at: now_ts(),
            content_hash,
            updated_at: 0,
        };
        if let Err(e) = db.save_skill(&record) {
            let _ = std::fs::remove_dir_all(&restore_path);
            return Err(e.to_string());
        }
        db.set_skill_plugin_enabled(&id, current_plugin, true)
            .map_err(|e| e.to_string())?;
        Ok(record)
    }

    /// 扫描未管理的技能（各插件 skills 目录 + SSOT）。
    pub fn scan_unmanaged(
        db: &Database,
        _data_dir: &Path,
        scan_sources: &[(String, PathBuf)],
    ) -> Result<Vec<UnmanagedSkill>, String> {
        let managed_dirs: std::collections::HashSet<String> = db
            .list_skills()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|s| s.directory.to_lowercase())
            .collect();

        let mut unmanaged: std::collections::HashMap<String, UnmanagedSkill> =
            std::collections::HashMap::new();
        for (label, scan_dir) in scan_sources {
            let entries = match std::fs::read_dir(scan_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') || managed_dirs.contains(&dir_name.to_lowercase()) {
                    continue;
                }
                if !path.join("SKILL.md").is_file() {
                    continue;
                }
                let (name, description) = parse_skill_metadata(&path, &dir_name);
                unmanaged
                    .entry(dir_name.clone())
                    .and_modify(|s| s.found_in.push(label.clone()))
                    .or_insert(UnmanagedSkill {
                        directory: dir_name,
                        name,
                        description,
                        found_in: vec![label.clone()],
                        path: path.display().to_string(),
                    });
            }
        }
        Ok(unmanaged.into_values().collect())
    }

    /// 从应用/SSOT 目录导入技能（honor 用户勾选的插件）。
    pub fn import_from_dirs(
        db: &Database,
        data_dir: &Path,
        scan_sources: &[(String, PathBuf)],
        selections: Vec<ImportSkillSelection>,
    ) -> Result<Vec<SkillRecord>, String> {
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        std::fs::create_dir_all(&ssot).map_err(|e| e.to_string())?;

        let mut imported = Vec::new();

        for selection in selections {
            let dir_name = match require_valid_directory(&selection.directory) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!("跳过导入：{e}");
                    continue;
                }
            };
            let mut source_path: Option<PathBuf> = None;
            for (_, base) in scan_sources {
                let skill_path = base.join(&dir_name);
                if skill_path.is_dir() && source_path.is_none() {
                    source_path = Some(skill_path);
                }
            }
            let Some(source) = source_path else {
                continue;
            };
            if !source.join("SKILL.md").is_file() {
                continue;
            }

            let dest = ssot.join(&dir_name);
            if !dest.exists() {
                copy_dir_recursive(&source, &dest)?;
            }
            let (name, description) = parse_skill_metadata(&dest, &dir_name.to_string_lossy());
            let content_hash = compute_dir_hash(&dest).ok();

            let id = format!("local:{}", dir_name.to_string_lossy());
            let record = SkillRecord {
                id: id.clone(),
                name,
                description,
                directory: dir_name.to_string_lossy().to_string(),
                source_path: None,
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                enabled_plugins: selection.plugins.clone(),
                installed_at: now_ts(),
                content_hash,
                updated_at: 0,
            };
            db.save_skill(&record).map_err(|e| e.to_string())?;
            for plugin in &selection.plugins {
                db.set_skill_plugin_enabled(&id, plugin, true)
                    .map_err(|e| e.to_string())?;
            }
            imported.push(record);
        }
        Ok(imported)
    }

    /// 为缺少 content_hash 的已安装技能补算哈希（启动时调用）。
    pub fn backfill_content_hashes(db: &Database, data_dir: &Path) -> Result<usize, String> {
        let settings = Self::get_sync_settings(db)?;
        let ssot = ssot_dir(data_dir, settings.storage_location);
        let mut count = 0;
        for skill in db.list_skills().map_err(|e| e.to_string())? {
            if skill.content_hash.is_some() {
                continue;
            }
            let Ok(directory) = require_valid_directory(&skill.directory) else {
                continue;
            };
            let skill_dir = ssot.join(&directory);
            if !skill_dir.is_dir() {
                continue;
            }
            if let Ok(hash) = compute_dir_hash(&skill_dir) {
                let _ = db.update_skill_hash(&skill.id, &hash, 0);
                count += 1;
            }
        }
        Ok(count)
    }
}

/// 从仓库归档扫描 SKILL.md（发现用）。
fn scan_dir_recursive(
    current_dir: &Path,
    base_dir: &Path,
    repo: &SkillRepo,
    skills: &mut Vec<DiscoverableSkill>,
) -> Result<(), String> {
    let skill_md = current_dir.join("SKILL.md");
    if skill_md.is_file() {
        let directory = if current_dir == base_dir {
            repo.name.clone()
        } else {
            current_dir
                .strip_prefix(base_dir)
                .unwrap_or(current_dir)
                .to_string_lossy()
                .replace('\\', "/")
        };
        let doc_path = skill_md
            .strip_prefix(base_dir)
            .unwrap_or(skill_md.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(skill) = build_skill_from_metadata(&skill_md, &directory, &doc_path, repo) {
            skills.push(skill);
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(current_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        scan_dir_recursive(&path, base_dir, repo, skills)?;
    }
    Ok(())
}

fn build_skill_from_metadata(
    skill_md: &Path,
    directory: &str,
    doc_path: &str,
    repo: &SkillRepo,
) -> Option<DiscoverableSkill> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let (name, description) = parse_skill_metadata(skill_md.parent()?, &repo.name);
    let readme_url = build_skill_doc_url(&repo.owner, &repo.name, &repo.branch, doc_path);
    let _ = content;
    Some(DiscoverableSkill {
        key: format!("{}/{}:{}", repo.owner, repo.name, directory),
        name,
        description: description.unwrap_or_default(),
        directory: directory.to_string(),
        readme_url,
        repo_owner: repo.owner.clone(),
        repo_name: repo.name.clone(),
        repo_branch: repo.branch.clone(),
    })
}

/// 拉取单个仓库的技能列表。
async fn fetch_repo_skills(
    client: &reqwest::Client,
    repo: &SkillRepo,
) -> Result<Vec<DiscoverableSkill>, String> {
    let (temp_guard, resolved_branch) =
        tokio::time::timeout(REPO_DOWNLOAD_TIMEOUT, download_repo(client, repo))
            .await
            .map_err(|_| {
                skill_error(
                    "DOWNLOAD_TIMEOUT",
                    &[
                        ("owner", &repo.owner),
                        ("name", &repo.name),
                        ("timeout", "60"),
                    ],
                    Some("checkNetwork"),
                )
            })??;
    let mut skills = Vec::new();
    let mut resolved = repo.clone();
    resolved.branch = resolved_branch;
    scan_dir_recursive(temp_guard.path(), temp_guard.path(), &resolved, &mut skills)?;
    Ok(skills)
}

// ========== 数据库 DAO ==========

const SKILL_COLUMNS: &str = "id, name, description, directory, source_path, repo_owner, repo_name, repo_branch, readme_url, installed_at, content_hash, updated_at";

fn row_to_skill(row: &Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        directory: row.get(3)?,
        source_path: row.get(4)?,
        repo_owner: row.get(5)?,
        repo_name: row.get(6)?,
        repo_branch: row.get(7)?,
        readme_url: row.get(8)?,
        installed_at: row.get(9)?,
        content_hash: row.get(10)?,
        updated_at: row.get(11)?,
        enabled_plugins: Vec::new(),
    })
}

impl Database {
    /// 列出全部技能（含启用插件）。
    pub fn list_skills(&self) -> rusqlite::Result<Vec<SkillRecord>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SKILL_COLUMNS} FROM skills ORDER BY name"
        ))?;
        let skills = stmt
            .query_map([], row_to_skill)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut app_stmt = conn.prepare(
            "SELECT skill_id, plugin_id FROM skill_apps WHERE enabled = 1 ORDER BY plugin_id",
        )?;
        let plugin_rows = app_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut result = Vec::with_capacity(skills.len());
        for mut skill in skills {
            skill.enabled_plugins = plugin_rows
                .iter()
                .filter(|(sid, _)| sid == &skill.id)
                .map(|(_, pid)| pid.clone())
                .collect();
            result.push(skill);
        }
        Ok(result)
    }

    /// 读取单个技能。
    pub fn get_skill(&self, id: &str) -> rusqlite::Result<Option<SkillRecord>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SKILL_COLUMNS} FROM skills WHERE id = ?1"
        ))?;
        let mut rows = stmt.query_map(params![id], row_to_skill)?;
        let Some(skill) = rows.next().transpose()? else {
            return Ok(None);
        };
        let plugins: Vec<String> = conn
            .prepare("SELECT plugin_id FROM skill_apps WHERE skill_id = ?1 AND enabled = 1 ORDER BY plugin_id")?
            .query_map(params![id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut skill = skill;
        skill.enabled_plugins = plugins;
        Ok(Some(skill))
    }

    /// 保存技能记录（插入或全字段更新；不触碰 skill_apps）。
    pub fn save_skill(&self, skill: &SkillRecord) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO skills (id, name, description, directory, source_path, repo_owner, repo_name, repo_branch, readme_url, installed_at, content_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name, description = excluded.description,
               directory = excluded.directory, source_path = excluded.source_path,
               repo_owner = excluded.repo_owner, repo_name = excluded.repo_name,
               repo_branch = excluded.repo_branch, readme_url = excluded.readme_url,
               installed_at = excluded.installed_at, content_hash = excluded.content_hash,
               updated_at = excluded.updated_at",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory,
                skill.source_path,
                skill.repo_owner,
                skill.repo_name,
                skill.repo_branch,
                skill.readme_url,
                skill.installed_at,
                skill.content_hash,
                skill.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 更新技能元数据（保留启用状态，更新后重新读取权威记录）。
    pub fn update_skill_metadata(&self, skill: &SkillRecord) -> rusqlite::Result<bool> {
        let affected = self.lock().execute(
            "UPDATE skills SET name = ?2, description = ?3, directory = ?4,
               repo_owner = ?5, repo_name = ?6, repo_branch = ?7, readme_url = ?8,
               content_hash = ?9, updated_at = ?10
             WHERE id = ?1",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory,
                skill.repo_owner,
                skill.repo_name,
                skill.repo_branch,
                skill.readme_url,
                skill.content_hash,
                skill.updated_at,
            ],
        )?;
        Ok(affected > 0)
    }

    /// 更新技能内容哈希。
    pub fn update_skill_hash(&self, id: &str, hash: &str, updated_at: i64) -> rusqlite::Result<bool> {
        let affected = self.lock().execute(
            "UPDATE skills SET content_hash = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, hash, updated_at],
        )?;
        Ok(affected > 0)
    }

    /// 删除技能（级联删除 skill_apps）。
    pub fn delete_skill(&self, id: &str) -> rusqlite::Result<()> {
        self.lock()
            .execute("DELETE FROM skills WHERE id = ?1", params![id])?;
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

    /// 列出全部技能仓库。
    pub fn list_skill_repos(&self) -> rusqlite::Result<Vec<SkillRepo>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT owner, name, branch, enabled FROM skill_repos ORDER BY owner, name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SkillRepo {
                    owner: row.get(0)?,
                    name: row.get(1)?,
                    branch: row.get(2)?,
                    enabled: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 保存技能仓库（插入或替换）。
    pub fn save_skill_repo(&self, repo: &SkillRepo) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(owner, name) DO UPDATE SET branch = excluded.branch, enabled = excluded.enabled",
            params![repo.owner, repo.name, repo.branch, repo.enabled],
        )?;
        Ok(())
    }

    /// 删除技能仓库。
    pub fn delete_skill_repo(&self, owner: &str, name: &str) -> rusqlite::Result<()> {
        self.lock().execute(
            "DELETE FROM skill_repos WHERE owner = ?1 AND name = ?2",
            params![owner, name],
        )?;
        Ok(())
    }

    /// 一次性写入默认技能仓库（由设置键守护）。
    pub fn init_default_skill_repos(&self) -> rusqlite::Result<()> {
        let key = "skills.defaultReposInitialized";
        if self.get_setting(key)?.as_deref() == Some("1") {
            return Ok(());
        }
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM skill_repos",
            [],
            |r| r.get(0),
        )?;
        if count == 0 {
            for repo in [
                ("anthropics", "skills", "main"),
                ("ComposioHQ", "awesome-claude-skills", "master"),
                ("cexll", "myclaude", "master"),
                ("JimLiu", "baoyu-skills", "main"),
            ] {
                conn.execute(
                    "INSERT INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, 1)",
                    params![repo.0, repo.1, repo.2],
                )?;
            }
        }
        drop(conn);
        self.set_setting(key, "1")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write_skill(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n{body}")).unwrap();
    }

    /// 把 SSOT/备份目录隔离到临时主目录（对齐 v1 的 `~/.cc-switch`）。
    ///
    /// 持有全局 env 锁，避免并行测试互相污染 `CC_SWITCH_TEST_HOME`。
    struct TestHome {
        _lock: std::sync::MutexGuard<'static, ()>,
        home: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = crate::test_support::env_lock().lock().unwrap();
            let home = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
            Self {
                _lock: lock,
                home,
                previous,
            }
        }

        fn path(&self) -> &Path {
            self.home.path()
        }

        fn cc_switch_skills(&self) -> PathBuf {
            self.path().join(".cc-switch").join("skills")
        }

        fn cc_switch_backups(&self) -> PathBuf {
            self.path().join(".cc-switch").join("skill-backups")
        }

        fn agents_skills(&self) -> PathBuf {
            self.path().join(".agents").join("skills")
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(v) => std::env::set_var("CC_SWITCH_TEST_HOME", v),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

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
    fn sanitize_install_name_rejects_dangerous() {
        assert!(sanitize_install_name("ok-name_1.2").is_some());
        assert!(sanitize_install_name("a/b").is_none());
        assert!(sanitize_install_name("a\\b").is_none());
        assert!(sanitize_install_name(".hidden").is_none());
        assert!(sanitize_install_name("..").is_none());
        assert!(sanitize_install_name("").is_none());
    }

    #[test]
    fn validate_repo_ref_accepts_valid_and_rejects_evil() {
        assert!(validate_repo_ref("anthropics", "skills", "main").is_ok());
        assert!(validate_repo_ref("owner", "repo", "feature/x").is_ok());
        assert!(validate_repo_ref("owner", "repo", "").is_ok());
        assert!(validate_repo_ref("owner", "repo", "../../releases").is_err());
        assert!(validate_repo_ref("own%er", "repo", "main").is_err());
        assert!(validate_repo_ref("owner", "repo", "bad~branch").is_err());
    }

    #[test]
    fn assert_github_archive_url_guards() {
        assert!(
            assert_github_archive_url(
                "https://github.com/o/r/archive/refs/heads/main.zip",
                "o",
                "r"
            )
            .is_ok()
        );
        assert!(
            assert_github_archive_url(
                "https://github.com/o/r/releases/download/v1/evil.zip",
                "o",
                "r"
            )
            .is_err()
        );
        assert!(
            assert_github_archive_url(
                "https://evil.com/o/r/archive/refs/heads/main.zip",
                "o",
                "r"
            )
            .is_err()
        );
    }

    #[test]
    fn require_valid_directory_rejects_traversal_and_noncanonical() {
        assert!(require_valid_directory("my-skill").is_ok());
        assert!(require_valid_directory("a/b").is_err());
        assert!(require_valid_directory("a\\b").is_err());
        assert!(require_valid_directory("../x").is_err());
        assert!(require_valid_directory(".hidden").is_err());
    }

    #[test]
    fn compute_dir_hash_is_content_sensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.txt"), "world").unwrap();
        std::fs::write(dir.path().join(".hidden"), "ignore").unwrap();
        let h1 = compute_dir_hash(dir.path()).unwrap();
        assert!(!h1.is_empty());
        // 同内容哈希一致
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir2.path().join("a.txt"), "hello").unwrap();
        std::fs::create_dir_all(dir2.path().join("sub")).unwrap();
        std::fs::write(dir2.path().join("sub/b.txt"), "world").unwrap();
        assert_eq!(h1, compute_dir_hash(dir2.path()).unwrap());
        // 内容变化哈希变化
        std::fs::write(dir.path().join("a.txt"), "HELLO").unwrap();
        assert_ne!(h1, compute_dir_hash(dir.path()).unwrap());
    }

    #[test]
    fn extract_repo_archive_rejects_zip_slip() {
        use std::io::Write;
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        // GitHub 归档带一层根目录；`repo-main/../evil` 剥根后含 `..`
        zip_writer
            .start_file("repo-main/../evil.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"pwned").unwrap();
        let bytes = zip_writer.finish().unwrap().into_inner();
        let cursor = std::io::Cursor::new(bytes);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        let dest = tempfile::tempdir().unwrap();
        extract_repo_archive(archive, dest.path()).unwrap();
        assert!(!dest.path().join("evil.txt").exists());
        assert!(dest.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn extract_repo_archive_extracts_skill() {
        use std::io::Write;
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip_writer
            .start_file("myrepo-main/SKILL.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer
            .write_all(b"---\nname: Demo\n---\nbody")
            .unwrap();
        let bytes = zip_writer.finish().unwrap().into_inner();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let dest = tempfile::tempdir().unwrap();
        extract_repo_archive(archive, dest.path()).unwrap();
        assert!(dest.path().join("SKILL.md").is_file());
    }

    #[test]
    fn extract_local_zip_installs_skill() {
        let dir = tempfile::tempdir().unwrap();
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip_writer
            .start_file("my-skill/SKILL.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"---\nname: My Skill\n---\nbody").unwrap();
        let bytes = zip_writer.finish().unwrap().into_inner();
        let zip_path = dir.path().join("skills.zip");
        std::fs::write(&zip_path, bytes).unwrap();

        let temp = extract_local_zip(&zip_path).unwrap();
        assert!(temp.path().join("my-skill/SKILL.md").is_file());
    }

    #[test]
    fn install_list_uninstall_with_backup() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let src = dir.path().join("src-skill");
        write_skill(&src, "Test Skill", "body");

        let record = SkillService::install_local_dir(&db, dir.path(), &src, "test-skill").unwrap();
        assert_eq!(record.name, "Test Skill");
        assert!(db.get_skill("test-skill").unwrap().is_some());
        // SSOT 位于 ~/.cc-switch/skills（对齐 v1）
        assert!(th.cc_switch_skills().join("test-skill/SKILL.md").is_file());

        db.set_skill_plugin_enabled("test-skill", "opencode", true)
            .unwrap();
        let skill = db.get_skill("test-skill").unwrap().unwrap();
        assert_eq!(skill.enabled_plugins, vec!["opencode".to_string()]);

        let app_dir = dir.path().join("app-skills");
        let skills_dirs: Vec<&Path> = vec![app_dir.as_path()];
        let backup = SkillService::uninstall(&db, dir.path(), &skills_dirs, "test-skill").unwrap();
        assert!(backup.is_some());
        assert!(db.get_skill("test-skill").unwrap().is_none());
        // 备份位于 ~/.cc-switch/skill-backups（对齐 v1）
        assert!(th.cc_switch_backups().read_dir().unwrap().next().is_some());
    }

    #[test]
    fn backup_list_delete_restore_roundtrip() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let src = dir.path().join("src-skill");
        write_skill(&src, "My Skill", "body");
        let record = SkillService::install_local_dir(&db, dir.path(), &src, "my-skill").unwrap();
        assert_eq!(record.name, "My Skill");

        // 卸载触发自动备份
        let app_dir = dir.path().join("app-skills");
        let skills_dirs: Vec<&Path> = vec![app_dir.as_path()];
        SkillService::uninstall(&db, dir.path(), &skills_dirs, "my-skill").unwrap();
        let backups = SkillService::list_backups(dir.path()).unwrap();
        assert_eq!(backups.len(), 1);
        let backup_id = backups[0].backup_id.clone();

        // 从备份恢复，并重新启用插件
        let restored = SkillService::restore_backup(&db, dir.path(), &backup_id, "opencode").unwrap();
        assert_eq!(restored.name, "My Skill");
        assert!(db.get_skill(&restored.id).unwrap().is_some());
        assert!(th.cc_switch_skills().join("my-skill/SKILL.md").is_file());

        // 删除备份后列表为空
        SkillService::delete_backup(dir.path(), &backup_id).unwrap();
        assert!(SkillService::list_backups(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn scan_unmanaged_and_import() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let app_dir = dir.path().join("app-skills");
        write_skill(&app_dir.join("find-skill"), "Find Skill", "body");
        write_skill(&app_dir.join("taken"), "Taken", "body");

        // taken 已在库中
        SkillService::install_local_dir(&db, dir.path(), &app_dir.join("taken"), "taken").unwrap();

        let scan_sources = vec![("opencode".to_string(), app_dir.clone())];
        let unmanaged = SkillService::scan_unmanaged(&db, dir.path(), &scan_sources).unwrap();
        assert_eq!(unmanaged.len(), 1);
        assert_eq!(unmanaged[0].directory, "find-skill");
        assert_eq!(unmanaged[0].found_in, vec!["opencode".to_string()]);

        let imported = SkillService::import_from_dirs(
            &db,
            dir.path(),
            &scan_sources,
            vec![ImportSkillSelection {
                directory: "find-skill".to_string(),
                plugins: vec!["opencode".to_string()],
            }],
        )
        .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].enabled_plugins, vec!["opencode".to_string()]);
        assert!(th.cc_switch_skills().join("find-skill/SKILL.md").is_file());
    }

    #[test]
    fn migrate_storage_moves_skills() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let src = dir.path().join("src-skill");
        write_skill(&src, "Migrate", "body");
        SkillService::install_local_dir(&db, dir.path(), &src, "migrate").unwrap();

        // 默认 SSOT 在 ~/.cc-switch/skills（对齐 v1）
        assert!(th.cc_switch_skills().join("migrate/SKILL.md").is_file());

        let result = SkillService::migrate_storage(&db, dir.path(), SkillStorageLocation::Unified).unwrap();
        assert_eq!(result.migrated_count, 1);
        // 文件移到 ~/.agents/skills/（测试主目录）
        assert!(th.agents_skills().join("migrate/SKILL.md").is_file());
        assert!(!th.cc_switch_skills().join("migrate").exists());
        let settings = SkillService::get_sync_settings(&db).unwrap();
        assert_eq!(settings.storage_location, SkillStorageLocation::Unified);
    }

    #[test]
    fn sync_skill_to_dir_copy_and_auto_fallback() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let src = dir.path().join("src-skill");
        write_skill(&src, "Sync", "body");
        SkillService::install_local_dir(&db, dir.path(), &src, "sync-skill").unwrap();

        let ssot = ssot_dir(dir.path(), SkillStorageLocation::CcSwitch);
        assert_eq!(ssot, th.cc_switch_skills());
        let dest_root = dir.path().join("app-skills");
        let record = db.get_skill("sync-skill").unwrap().unwrap();

        // Copy 模式
        sync_skill_to_dir(&ssot, &record.directory, &dest_root, SyncMethod::Copy).unwrap();
        assert!(dest_root.join("sync-skill/SKILL.md").is_file());

        // Auto：Windows 上无权限建目录 symlink 时回退复制，不能失败
        sync_skill_to_dir(&ssot, &record.directory, &dest_root, SyncMethod::Auto).unwrap();
        assert!(dest_root.join("sync-skill/SKILL.md").is_file());

        // 移除
        remove_skill_from_dir(&record.directory, &dest_root).unwrap();
        assert!(!dest_root.join("sync-skill").exists());
    }

    #[test]
    fn install_from_zip_writes_records() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip_writer
            .start_file("skill-a/SKILL.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"---\nname: Skill A\n---\nbody").unwrap();
        zip_writer
            .start_file("skill-b/SKILL.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"---\nname: Skill B\n---\nbody").unwrap();
        let bytes = zip_writer.finish().unwrap().into_inner();
        let zip_path = dir.path().join("skills.zip");
        std::fs::write(&zip_path, bytes).unwrap();

        let installed =
            SkillService::install_from_zip(&db, dir.path(), &zip_path, "opencode").unwrap();
        assert_eq!(installed.len(), 2);
        assert!(db.get_skill(&installed[0].id).unwrap().is_some());
        assert_eq!(installed[0].enabled_plugins, vec!["opencode".to_string()]);
        assert!(th.cc_switch_skills().join("skill-a/SKILL.md").is_file());
    }

    #[test]
    fn install_from_repo_conflict_detection() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let src = dir.path().join("src-skill");
        write_skill(&src, "Conflict", "body");
        SkillService::install_local_dir(&db, dir.path(), &src, "same-dir").unwrap();
        assert!(th.cc_switch_skills().join("same-dir/SKILL.md").is_file());

        // 同一目录名来自不同仓库 → 先写入另一条同目录记录，模拟冲突来源。
        let record = SkillRecord {
            id: "other/repo:same-dir".to_string(),
            name: "Other".to_string(),
            description: None,
            directory: "same-dir".to_string(),
            source_path: Some("other/repo".to_string()),
            repo_owner: Some("other".to_string()),
            repo_name: Some("repo".to_string()),
            repo_branch: Some("main".to_string()),
            readme_url: None,
            enabled_plugins: vec![],
            installed_at: 0,
            content_hash: None,
            updated_at: 0,
        };
        db.save_skill(&record).unwrap();

        let skills = db.list_skills().unwrap();
        assert!(skills.iter().any(|s| s.directory == "same-dir"));
        let owners: Vec<Option<String>> = skills
            .iter()
            .filter(|s| s.directory == "same-dir")
            .map(|s| s.repo_owner.clone())
            .collect();
        assert!(owners.contains(&Some("other".to_string())));
    }

    #[test]
    fn skill_repo_crud_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.init_default_skill_repos().unwrap();
        db.init_default_skill_repos().unwrap(); // 幂等
        let repos = db.list_skill_repos().unwrap();
        assert_eq!(repos.len(), 4);
        assert!(repos.iter().any(|r| r.owner == "anthropics"));

        db.save_skill_repo(&SkillRepo {
            owner: "me".to_string(),
            name: "mine".to_string(),
            branch: "dev".to_string(),
            enabled: true,
        })
        .unwrap();
        assert_eq!(db.list_skill_repos().unwrap().len(), 5);

        db.delete_skill_repo("me", "mine").unwrap();
        assert_eq!(db.list_skill_repos().unwrap().len(), 4);
    }

    #[test]
    fn backfill_content_hashes() {
        let th = TestHome::new();
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        let src = dir.path().join("src-skill");
        write_skill(&src, "Hash", "body");
        SkillService::install_local_dir(&db, dir.path(), &src, "hash-skill").unwrap();
        assert!(th.cc_switch_skills().join("hash-skill/SKILL.md").is_file());
        // 手动清空哈希
        db.lock()
            .execute("UPDATE skills SET content_hash = NULL", [])
            .unwrap();
        let n = SkillService::backfill_content_hashes(&db, dir.path()).unwrap();
        assert_eq!(n, 1);
        assert!(db.get_skill("hash-skill").unwrap().unwrap().content_hash.is_some());
    }
}
