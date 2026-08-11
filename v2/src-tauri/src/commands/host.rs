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
}
