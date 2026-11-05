use serde::Serialize;
use std::{collections::HashMap, path::{Path, PathBuf}};
use walkdir::WalkDir;

const MAX_FILES: usize = 250_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCategory {
    extension: String,
    file_count: usize,
    total_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageScan {
    kind: String,
    label: String,
    path: String,
    file_count: usize,
    directory_count: usize,
    total_bytes: u64,
    error_count: usize,
    truncated: bool,
    categories: Vec<FileCategory>,
}

fn target(kind: &str) -> Result<(&'static str, PathBuf), String> {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from).ok_or_else(|| "无法定位用户目录".to_string())?;
    match kind {
        "downloads" => Ok(("下载", profile.join("Downloads"))),
        "documents" => Ok(("文档", profile.join("Documents"))),
        "desktop" => Ok(("桌面", profile.join("Desktop"))),
        _ => Err("不支持的扫描目录".into()),
    }
}

fn category(path: &Path) -> String {
    path.extension().and_then(|value| value.to_str()).map(|value| format!(".{}", value.to_lowercase())).filter(|value| value.len() <= 20).unwrap_or_else(|| "无扩展名".into())
}

fn scan(kind: &str) -> Result<StorageScan, String> {
    let (label, path) = target(kind)?;
    if !path.is_dir() { return Err(format!("{}目录不存在：{}", label, path.display())); }
    let mut file_count = 0usize;
    let mut directory_count = 0usize;
    let mut total_bytes = 0u64;
    let mut error_count = 0usize;
    let mut totals: HashMap<String, (usize, u64)> = HashMap::new();
    let mut truncated = false;
    for entry in WalkDir::new(&path).follow_links(false).into_iter() {
        let entry = match entry { Ok(value) => value, Err(_) => { error_count += 1; continue; } };
        if entry.file_type().is_dir() { directory_count += 1; continue; }
        if !entry.file_type().is_file() { continue; }
        file_count += 1;
        if file_count > MAX_FILES { truncated = true; break; }
        match entry.metadata() {
            Ok(metadata) => {
                let size = metadata.len(); total_bytes = total_bytes.saturating_add(size);
                let item = totals.entry(category(entry.path())).or_insert((0, 0)); item.0 += 1; item.1 = item.1.saturating_add(size);
            }
            Err(_) => error_count += 1,
        }
    }
    let mut categories: Vec<_> = totals.into_iter().map(|(extension, (file_count, total_bytes))| FileCategory { extension, file_count, total_bytes }).collect();
    categories.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    categories.truncate(30);
    Ok(StorageScan { kind: kind.into(), label: label.into(), path: path.to_string_lossy().into_owned(), file_count: file_count.min(MAX_FILES), directory_count, total_bytes, error_count, truncated, categories })
}

#[tauri::command]
pub async fn scan_storage(kind: String) -> Result<StorageScan, String> {
    tauri::async_runtime::spawn_blocking(move || scan(&kind)).await.map_err(|error| format!("空间扫描任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_arbitrary_paths() {
        assert!(target("C:\\").is_err());
        assert!(target("../Downloads").is_err());
    }
}
