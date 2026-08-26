//! TypeScript 插件宿主命令。
//!
//! TS 插件由前端动态加载脚本执行；脚本通过这些宿主命令与后端交互。
//! 文件读写严格限定在插件目录内，防止越权访问。

use std::path::{Path, PathBuf};

use tauri::State;

use crate::registry::PluginRegistry;

/// 规范化并校验路径位于插件目录内，返回绝对路径。
fn plugin_path(registry: &PluginRegistry, plugin_id: &str, rel: &str) -> Result<PathBuf, String> {
    resolve_plugin_path(&registry.plugins_dir().join(plugin_id), rel)
}

/// 校验 `rel` 解析后位于 `plugin_dir` 内，返回规范化绝对路径。
fn resolve_plugin_path(plugin_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    if !plugin_dir.is_dir() {
        return Err(format!("插件目录不存在: {}", plugin_dir.display()));
    }
    let base = plugin_dir
        .canonicalize()
        .map_err(|e| format!("无法解析插件目录: {e}"))?;
    let target = plugin_dir.join(rel);
    let canonical = target
        .canonicalize()
        .map_err(|e| format!("无法解析目标文件: {e}"))?;
    if !canonical.starts_with(&base) {
        return Err("路径越出插件目录范围".to_string());
    }
    Ok(canonical)
}

/// 校验 `rel` 解析后位于资源根 `root` 内（`root` 可指向文件或目录）。
///
/// - `root` 指向文件：`rel` 必须为空，目标即文件本身。
/// - `root` 指向目录：`rel` 是其相对路径，目标须位于目录内。
///
/// 根或目标尚不存在时（首次写入）也允许：对「最近存在的祖先」做 canonicalize 校验
/// 仍在白名单根内，防止符号链接越界；文件本身用词法路径返回（写入时建父目录）。
fn resolve_resource_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let base = lexical_absolute(root);
    if base.exists() && base.is_file() {
        if rel.trim().is_empty() {
            return Ok(base);
        }
        return Err("资源指向单个文件，不允许再指定相对路径".to_string());
    }
    let normalized_rel = normalize_rel(rel)?;
    let target = if normalized_rel.as_os_str().is_empty() {
        base.clone()
    } else {
        base.join(&normalized_rel)
    };

    // 目标存在：canonicalize 后校验仍在根内（防符号链接越界）。
    if target.exists() {
        let canonical = target
            .canonicalize()
            .map_err(|e| format!("无法解析目标路径: {e}"))?;
        if !canonical.starts_with(canonical_base(&base)) {
            return Err("路径越出资源白名单范围".to_string());
        }
        return Ok(canonical);
    }

    // 目标尚不存在（写入）：词法上 normalized_rel 已保证不越出根；再对
    // 「目标与根的最近存在祖先」做 canonicalize 校验，防符号链接越界。
    let canonical_target_anc = nearest_existing_canonical(&target)?;
    let canonical_root_anc = nearest_existing_canonical(&base)?;
    if !canonical_target_anc.starts_with(&canonical_root_anc) {
        return Err("路径越出资源白名单范围".to_string());
    }
    Ok(target)
}

/// 词法绝对化：绝对路径原样返回（保留 Windows `\\?\` 前缀），相对路径拼到 cwd。
fn lexical_absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// 对「最近存在的祖先」做 canonicalize。
fn nearest_existing_canonical(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    while !ancestor.exists() {
        match ancestor.parent() {
            Some(p) if !p.as_os_str().is_empty() => ancestor = p.to_path_buf(),
            _ => break,
        }
    }
    ancestor
        .canonicalize()
        .map_err(|e| format!("无法解析路径: {e}"))
}

/// 根存在时的 canonical 基（用于目标存在分支的 `starts_with` 校验）。
fn canonical_base(base: &Path) -> PathBuf {
    base.canonicalize().unwrap_or_else(|_| base.to_path_buf())
}

/// 词法规范化相对路径：合并 `..`、去掉 `.`；越出根（`..` 弹出空栈）或绝对路径则报错。
fn normalize_rel(rel: &str) -> Result<PathBuf, String> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err("路径越出资源白名单范围".to_string());
                }
            }
            Component::Normal(n) => out.push(n),
            Component::RootDir | Component::Prefix(_) => {
                return Err("不允许绝对路径".to_string());
            }
        }
    }
    Ok(out)
}

/// 读取 TS 插件的主脚本内容（供前端动态加载）。
#[tauri::command]
pub fn plugin_get_script(
    registry: State<'_, PluginRegistry>,
    id: String,
    main: String,
) -> Result<String, String> {
    let path = plugin_path(&registry, &id, &main)?;
    std::fs::read_to_string(&path).map_err(|e| format!("读取脚本失败: {e}"))
}

/// 读取插件目录内的文件。
#[tauri::command]
pub fn host_read_file(
    registry: State<'_, PluginRegistry>,
    id: String,
    path: String,
) -> Result<String, String> {
    let target = plugin_path(&registry, &id, &path)?;
    std::fs::read_to_string(&target).map_err(|e| format!("读取文件失败: {e}"))
}

/// 写入插件目录内的文件（自动创建父目录）。
#[tauri::command]
pub fn host_write_file(
    registry: State<'_, PluginRegistry>,
    id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let plugin_dir = registry.plugins_dir().join(&id);
    if !plugin_dir.is_dir() {
        return Err(format!("插件目录不存在: {}", plugin_dir.display()));
    }
    let base = plugin_dir
        .canonicalize()
        .map_err(|e| format!("无法解析插件目录: {e}"))?;
    let target = plugin_dir.join(&path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let canonical = target
        .canonicalize()
        .or_else(|_| {
            // 文件尚不存在：规范化为父目录 + 文件名。
            let parent = target.parent().unwrap_or(&plugin_dir);
            parent
                .canonicalize()
                .map(|p| p.join(target.file_name().unwrap_or_default()))
        })
        .map_err(|e| format!("无法解析目标路径: {e}"))?;
    if !canonical.starts_with(&base) {
        return Err("路径越出插件目录范围".to_string());
    }
    std::fs::write(&canonical, content).map_err(|e| format!("写入文件失败: {e}"))
}

/// 列出插件目录内容（TS 插件运行时探活等）。
#[tauri::command]
pub fn host_list_files(
    registry: State<'_, PluginRegistry>,
    id: String,
    dir: Option<String>,
) -> Result<Vec<String>, String> {
    let plugin_dir = registry.plugins_dir().join(&id);
    let target = match dir {
        Some(d) => plugin_dir.join(&d),
        None => plugin_dir,
    };
    let entries = std::fs::read_dir(&target).map_err(|e| format!("读取目录失败: {e}"))?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().to_string());
    }
    names.sort();
    Ok(names)
}

// ---------------------------------------------------------------------------
// 资源白名单命令（方案 A：manifest 声明 `resources`，后端代劳文件 I/O）
// ---------------------------------------------------------------------------

/// 解析插件声明的一个资源根（按名）。
fn resource_root(
    registry: &PluginRegistry,
    plugin_id: &str,
    name: &str,
) -> Result<PathBuf, String> {
    let roots = registry
        .resource_roots(plugin_id)
        .map_err(|e| e.to_string())?;
    roots
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, p)| p.clone())
        .ok_or_else(|| format!("插件 '{plugin_id}' 未声明资源 '{name}'"))
}

/// 读取插件声明资源下的文件（沙箱放宽到 manifest `resources` 白名单）。
#[tauri::command]
pub fn host_read_resource(
    registry: State<'_, PluginRegistry>,
    id: String,
    name: String,
    rel: Option<String>,
) -> Result<String, String> {
    let root = resource_root(&registry, &id, &name)?;
    let target = resolve_resource_path(&root, rel.as_deref().unwrap_or(""))?;
    std::fs::read_to_string(&target).map_err(|e| format!("读取文件失败: {e}"))
}

/// 写入插件声明资源下的文件（自动创建父目录）。
#[tauri::command]
pub fn host_write_resource(
    registry: State<'_, PluginRegistry>,
    id: String,
    name: String,
    content: String,
    rel: Option<String>,
) -> Result<(), String> {
    let root = resource_root(&registry, &id, &name)?;
    let target = resolve_resource_path(&root, rel.as_deref().unwrap_or(""))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    std::fs::write(&target, content).map_err(|e| format!("写入文件失败: {e}"))
}

/// 列出插件声明资源目录下的条目（`rel` 为空时列根目录本身）。
#[tauri::command]
pub fn host_list_resource(
    registry: State<'_, PluginRegistry>,
    id: String,
    name: String,
    rel: Option<String>,
) -> Result<Vec<String>, String> {
    let root = resource_root(&registry, &id, &name)?;
    let target = resolve_resource_path(&root, rel.as_deref().unwrap_or(""))?;
    if target.is_file() {
        return Err("资源指向单个文件，不能列出目录".to_string());
    }
    let entries = std::fs::read_dir(&target).map_err(|e| format!("读取目录失败: {e}"))?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().to_string());
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_allows_inside_plugin_dir() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("plugins/demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("main.ts"), "x").unwrap();

        let resolved = resolve_plugin_path(&plugin_dir, "main.ts").unwrap();
        assert!(resolved.ends_with("main.ts"));
    }

    #[test]
    fn resolve_path_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("plugins/demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        // 越界文件
        std::fs::write(dir.path().join("secret.txt"), "s").unwrap();

        assert!(resolve_plugin_path(&plugin_dir, "../secret.txt").is_err());
        assert!(resolve_plugin_path(&plugin_dir, "../../etc/passwd").is_err());
    }

    #[test]
    fn resolve_path_rejects_missing() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("plugins/demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        assert!(resolve_plugin_path(&plugin_dir, "nope.txt").is_err());
    }

    #[test]
    fn resolve_resource_allows_inside_dir_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("a.json"), "{}").unwrap();
        let t = resolve_resource_path(&root, "sub/a.json").unwrap();
        assert!(t.ends_with("sub/a.json"));
    }

    #[test]
    fn resolve_resource_allows_file_root_with_empty_rel() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cfg.json");
        std::fs::write(&file, "{}").unwrap();
        let t = resolve_resource_path(&file, "").unwrap();
        assert_eq!(t, lexical_absolute(&file));
    }

    #[test]
    fn resolve_resource_rejects_file_root_with_rel() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cfg.json");
        std::fs::write(&file, "{}").unwrap();
        assert!(resolve_resource_path(&file, "sub/x").is_err());
    }

    #[test]
    fn resolve_resource_allows_new_file_in_dir_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        let base = lexical_absolute(&root);
        let t = resolve_resource_path(&root, "new/file.json").unwrap();
        assert!(t.ends_with("new/file.json"));
        assert!(t.starts_with(&base));
    }

    #[test]
    fn resolve_resource_allows_missing_file_root() {
        // 复现「添加 MCP 保存报错」：资源根指向尚不存在的文件（如 ~/.claude.json）。
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nonexistent.json");
        let t = resolve_resource_path(&missing, "").unwrap();
        assert_eq!(t, lexical_absolute(&missing));
        // 且父目录创建后能写入。
        std::fs::write(&t, "{}").unwrap();
        assert!(t.exists());
    }

    #[test]
    fn resolve_resource_allows_missing_dir_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("missing/projects");
        let t = resolve_resource_path(&root, "sub/a.json").unwrap();
        assert!(t.ends_with("sub/a.json"));
        std::fs::create_dir_all(t.parent().unwrap()).unwrap();
        std::fs::write(&t, "x").unwrap();
        assert!(t.exists());
    }

    #[test]
    fn resolve_resource_rejects_escape_when_root_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(dir.path().join("secret.txt"), "s").unwrap();
        assert!(resolve_resource_path(&root, "../secret.txt").is_err());
        assert!(resolve_resource_path(&root, "../../etc/passwd").is_err());
        assert!(resolve_resource_path(&root, "/abs/path").is_err());
    }
}
