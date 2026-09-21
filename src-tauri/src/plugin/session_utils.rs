//! 会话扫描共享工具（移植自 v1 session_manager/providers/utils.rs）。

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use chrono::{DateTime, FixedOffset};
use serde_json::Value;

/// 会话标题最大字符数。
pub const TITLE_MAX_CHARS: usize = 80;

/// 读取文件头 `head_n` 行与尾 `tail_n` 行；小文件（<16KB）一次读完。
pub fn read_head_tail_lines(
    path: &Path,
    head_n: usize,
    tail_n: usize,
) -> io::Result<(Vec<String>, Vec<String>)> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();

    if file_len < 16_384 {
        let reader = BufReader::new(file);
        let all: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let head = all.iter().take(head_n).cloned().collect();
        let skip = all.len().saturating_sub(tail_n);
        let tail = all.into_iter().skip(skip).collect();
        return Ok((head, tail));
    }

    let reader = BufReader::new(file);
    let head: Vec<String> = reader.lines().take(head_n).map_while(Result::ok).collect();

    let seek_pos = file_len.saturating_sub(16_384);
    let mut file2 = File::open(path)?;
    file2.seek(SeekFrom::Start(seek_pos))?;
    let tail_reader = BufReader::new(file2);
    let all_tail: Vec<String> = tail_reader.lines().map_while(Result::ok).collect();

    let skip_first = if seek_pos > 0 { 1 } else { 0 };
    let usable: Vec<String> = all_tail.into_iter().skip(skip_first).collect();
    let skip = usable.len().saturating_sub(tail_n);
    let tail = usable.into_iter().skip(skip).collect();

    Ok((head, tail))
}

/// 时间戳解析：毫秒整数 / 秒整数 / RFC3339 字符串 → 毫秒。
pub fn parse_timestamp_to_ms(value: &Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    if let Some(n) = value.as_f64() {
        let n = n as i64;
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    let raw = value.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt: DateTime<FixedOffset>| dt.timestamp_millis())
}

/// 从消息 content 提取纯文本（字符串 / 分段数组 / {text} 对象）。
pub fn extract_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(extract_text_from_item)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn extract_text_from_item(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

    if item_type == "tool_use" {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Some(format!("[Tool: {name}]"));
    }

    if item_type == "tool_result" {
        if let Some(content) = item.get("content") {
            let text = extract_text(content);
            if !text.is_empty() {
                return Some(text);
            }
        }
        return None;
    }

    for key in ["text", "input_text", "output_text"] {
        if let Some(text) = item.get(key).and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }
    }

    if let Some(content) = item.get("content") {
        let text = extract_text(content);
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// 截断摘要到 max_chars（超长加省略号）。
pub fn truncate_summary(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut result = trimmed.chars().take(max_chars).collect::<String>();
    result.push_str("...");
    result
}

/// 路径最后一段（兼容 / 与 \）。
pub fn path_basename(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.trim_end_matches(['/', '\\']);
    let last = normalized
        .split(['/', '\\'])
        .next_back()
        .filter(|segment| !segment.is_empty())?;
    Some(last.to_string())
}

/// 递归收集目录下指定扩展名的文件。
pub fn collect_files_with_ext(root: &Path, ext: &str, files: &mut Vec<std::path::PathBuf>) {
    if !root.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_ext(&path, ext, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_timestamp_to_ms_supports_integers_and_rfc3339() {
        assert_eq!(
            parse_timestamp_to_ms(&json!(1_771_061_953_033_i64)),
            Some(1_771_061_953_033)
        );
        assert_eq!(
            parse_timestamp_to_ms(&json!(1_771_061_953_i64)),
            Some(1_771_061_953_000)
        );
        assert_eq!(
            parse_timestamp_to_ms(&json!("1970-01-01T00:00:01Z")),
            Some(1_000)
        );
    }

    #[test]
    fn extract_text_handles_array_and_object() {
        assert_eq!(extract_text(&json!("plain")), "plain");
        assert_eq!(
            extract_text(&json!([{"type": "text", "text": "a"}, {"type": "tool_use", "name": "W"}])),
            "a\n[Tool: W]"
        );
        assert_eq!(extract_text(&json!({"text": "x"})), "x");
    }
}
