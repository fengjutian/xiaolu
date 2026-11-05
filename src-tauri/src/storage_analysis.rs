use serde::Serialize;
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const MAX_FILES: usize = 250_000;
const MIN_DUPLICATE_BYTES: u64 = 1024 * 1024;
const MAX_HASH_BYTES: u64 = 4 * 1024 * 1024 * 1024;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileEntry {
    path: String,
    name: String,
    bytes: u64,
    modified_unix_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryUsage {
    path: String,
    name: String,
    bytes: u64,
    file_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileScan {
    kind: String,
    label: String,
    root: String,
    min_bytes: u64,
    files: Vec<LargeFileEntry>,
    directories: Vec<DirectoryUsage>,
    total_bytes: u64,
    file_count: usize,
    error_count: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    path: String,
    name: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    size_bytes: u64,
    hash: String,
    files: Vec<DuplicateFile>,
    wasted_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScan {
    kind: String,
    label: String,
    root: String,
    groups: Vec<DuplicateGroup>,
    candidate_files: usize,
    scanned_bytes: u64,
    error_count: usize,
    truncated: bool,
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

fn modified_unix_ms(metadata: &Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
}

fn root_bucket(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut components = relative.components();
    let first = components.next();
    let nested = components.next().is_some();
    if nested {
        first
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "(根目录)".into())
    } else {
        "(根目录)".into()
    }
}

/// 扫描指定根目录下最大的文件，并按一级子目录聚合占用。
fn scan_large_files_in(
    root: &Path,
    kind: &str,
    label: &str,
    min_bytes: u64,
    limit: usize,
) -> Result<LargeFileScan, String> {
    if !root.is_dir() {
        return Err(format!("{label}目录不存在：{}", root.display()));
    }
    let mut files = Vec::new();
    let mut directories: HashMap<String, (u64, usize)> = HashMap::new();
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut error_count = 0usize;
    let mut truncated = false;

    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        file_count += 1;
        if file_count > MAX_FILES {
            truncated = true;
            break;
        }
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        let size = metadata.len();
        total_bytes = total_bytes.saturating_add(size);
        let bucket = directories.entry(root_bucket(root, entry.path())).or_insert((0, 0));
        bucket.0 = bucket.0.saturating_add(size);
        bucket.1 += 1;
        if size < min_bytes {
            continue;
        }
        files.push(LargeFileEntry {
            path: entry.path().to_string_lossy().into_owned(),
            name: entry
                .file_name()
                .to_string_lossy()
                .into_owned(),
            bytes: size,
            modified_unix_ms: modified_unix_ms(&metadata),
        });
    }

    files.sort_by(|left, right| right.bytes.cmp(&left.bytes));
    files.truncate(limit);
    let mut directories: Vec<DirectoryUsage> = directories
        .into_iter()
        .map(|(name, (bytes, file_count))| DirectoryUsage {
            path: if name == "(根目录)" {
                root.to_string_lossy().into_owned()
            } else {
                root.join(&name).to_string_lossy().into_owned()
            },
            name,
            bytes,
            file_count,
        })
        .collect();
    directories.sort_by(|left, right| right.bytes.cmp(&left.bytes));
    directories.truncate(limit);

    Ok(LargeFileScan {
        kind: kind.into(),
        label: label.into(),
        root: root.to_string_lossy().into_owned(),
        min_bytes,
        files,
        directories,
        total_bytes,
        file_count,
        error_count,
        truncated,
    })
}

fn hash_file(path: &Path) -> Result<u64, String> {
    let file = File::open(path).map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok(hash)
}

/// 按“大小分组 → 全量哈希”两级筛选重复文件；只处理 ≥1 MiB 的文件。
fn scan_duplicates_in(
    root: &Path,
    kind: &str,
    label: &str,
    limit: usize,
) -> Result<DuplicateScan, String> {
    if !root.is_dir() {
        return Err(format!("{label}目录不存在：{}", root.display()));
    }
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut error_count = 0usize;
    let mut file_count = 0usize;
    let mut truncated = false;

    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        file_count += 1;
        if file_count > MAX_FILES {
            truncated = true;
            break;
        }
        let size = match entry.metadata() {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        if size < MIN_DUPLICATE_BYTES {
            continue;
        }
        by_size.entry(size).or_default().push(entry.path().to_path_buf());
    }

    let mut groups = Vec::new();
    let mut budget = MAX_HASH_BYTES;
    let mut candidate_files = 0usize;
    let mut scanned_bytes = 0u64;

    for (size, paths) in by_size {
        if paths.len() < 2 || truncated {
            continue;
        }
        let mut buckets: HashMap<u64, Vec<DuplicateFile>> = HashMap::new();
        for path in paths {
            if budget < size {
                truncated = true;
                break;
            }
            match hash_file(&path) {
                Ok(hash) => {
                    budget -= size;
                    scanned_bytes = scanned_bytes.saturating_add(size);
                    candidate_files += 1;
                    buckets.entry(hash).or_default().push(DuplicateFile {
                        path: path.to_string_lossy().into_owned(),
                        name: path
                            .file_name()
                            .map(|value| value.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        bytes: size,
                    });
                }
                Err(_) => error_count += 1,
            }
        }
        for (hash, files) in buckets {
            if files.len() > 1 {
                groups.push(DuplicateGroup {
                    size_bytes: size,
                    hash: format!("{hash:016x}"),
                    wasted_bytes: size.saturating_mul(files.len() as u64 - 1),
                    files,
                });
            }
        }
    }

    groups.sort_by(|left, right| right.wasted_bytes.cmp(&left.wasted_bytes));
    groups.truncate(limit);

    Ok(DuplicateScan {
        kind: kind.into(),
        label: label.into(),
        root: root.to_string_lossy().into_owned(),
        groups,
        candidate_files,
        scanned_bytes,
        error_count,
        truncated,
    })
}

#[tauri::command]
pub async fn scan_storage(kind: String) -> Result<StorageScan, String> {
    tauri::async_runtime::spawn_blocking(move || scan(&kind)).await.map_err(|error| format!("空间扫描任务失败：{error}"))?
}

#[tauri::command]
pub async fn scan_large_files(
    kind: String,
    min_bytes: u64,
    limit: u32,
) -> Result<LargeFileScan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (label, path) = target(&kind)?;
        let min_bytes = min_bytes.clamp(1024 * 1024, 10 * 1024 * 1024 * 1024);
        let limit = (limit as usize).clamp(10, 200);
        let scan = scan_large_files_in(&path, &kind, label, min_bytes, limit)?;
        // 扫描期间目录可能被删除，给出明确结果而不是空表。
        if scan.file_count == 0 {
            return Err(format!("{}目录没有可统计的文件", label));
        }
        Ok(scan)
    })
    .await
    .map_err(|error| format!("大文件扫描任务失败：{error}"))?
}

#[tauri::command]
pub async fn scan_duplicates(kind: String, limit: u32) -> Result<DuplicateScan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (label, path) = target(&kind)?;
        let limit = (limit as usize).clamp(5, 200);
        scan_duplicates_in(&path, &kind, label, limit)
    })
    .await
    .map_err(|error| format!("重复文件扫描任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("system-lens-storage-{name}-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn rejects_arbitrary_paths() {
        assert!(target("C:\\").is_err());
        assert!(target("../Downloads").is_err());
    }

    #[test]
    fn finds_large_files_and_directory_usage() {
        let root = temp_root("large");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.bin"), vec![1u8; 2 * 1024 * 1024]).unwrap();
        fs::write(root.join("small.txt"), b"tiny").unwrap();
        fs::write(root.join("sub").join("b.bin"), vec![2u8; 3 * 1024 * 1024]).unwrap();

        let scan = scan_large_files_in(&root, "downloads", "下载", 1024 * 1024, 10).unwrap();
        assert_eq!(scan.file_count, 3);
        assert_eq!(scan.files.len(), 2);
        assert_eq!(scan.files[0].bytes, 3 * 1024 * 1024);
        assert!(scan.directories.iter().any(|item| item.name == "sub" && item.bytes == 3 * 1024 * 1024));
        assert!(scan.directories.iter().any(|item| item.name == "(根目录)" && item.bytes == 2 * 1024 * 1024 + 4));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn groups_identical_files_only() {
        let root = temp_root("dupes");
        fs::create_dir_all(root.join("copy")).unwrap();
        let payload = vec![7u8; 2 * 1024 * 1024];
        fs::write(root.join("a.bin"), &payload).unwrap();
        fs::write(root.join("copy").join("b.bin"), &payload).unwrap();
        fs::write(root.join("c.bin"), vec![9u8; 2 * 1024 * 1024]).unwrap();
        fs::write(root.join("tiny.bin"), b"same").unwrap();

        let scan = scan_duplicates_in(&root, "downloads", "下载", 10).unwrap();
        assert_eq!(scan.groups.len(), 1);
        assert_eq!(scan.groups[0].files.len(), 2);
        assert_eq!(scan.groups[0].size_bytes, 2 * 1024 * 1024);
        assert_eq!(scan.groups[0].wasted_bytes, 2 * 1024 * 1024);
        assert!(scan.groups[0].files.iter().any(|item| item.name == "a.bin"));
        assert!(scan.groups[0].files.iter().any(|item| item.name == "b.bin"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_directory_is_reported() {
        let root = temp_root("missing");
        assert!(scan_large_files_in(&root, "downloads", "下载", 1024, 10).is_err());
        assert!(scan_duplicates_in(&root, "downloads", "下载", 10).is_err());
    }
}
