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

/// 盘符级扫描默认跳过的系统目录（只作用于盘根一级）。
const DRIVE_EXCLUSIONS: [&str; 6] = [
    "Windows",
    "$Recycle.Bin",
    "System Volume Information",
    "Recovery",
    "PerfLogs",
    "Config.Msi",
];
/// 盘根下由系统托管的大文件，统计时跳过以避免误导。
const DRIVE_EXCLUDED_FILES: [&str; 3] = ["pagefile.sys", "hiberfil.sys", "swapfile.sys"];

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDrive {
    letter: String,
    label: String,
    mount_point: String,
    file_system: String,
    total_bytes: u64,
    free_bytes: u64,
    removable: bool,
}

/// 一次扫描的作用域：用户目录或某个盘符，并携带该作用域的排除规则。
struct Scope {
    kind: String,
    label: String,
    root: PathBuf,
    exclusions: Vec<String>,
}

fn scope(kind: &str) -> Result<Scope, String> {
    if let Some(letter) = kind.strip_prefix("drive:") {
        let letter = letter.trim();
        let bytes = letter.as_bytes();
        if bytes.len() != 1 || !bytes[0].is_ascii_alphabetic() {
            return Err("不支持的盘符：只接受单个字母（如 C）".into());
        }
        let upper = letter.to_ascii_uppercase();
        let root = PathBuf::from(format!("{upper}:\\"));
        if !root.is_dir() {
            return Err(format!("盘符 {upper}: 不存在或未就绪"));
        }
        let mut exclusions = DRIVE_EXCLUSIONS
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();
        exclusions.extend(DRIVE_EXCLUDED_FILES.iter().map(|name| (*name).to_string()));
        return Ok(Scope {
            kind: kind.to_string(),
            label: format!("{upper} 盘"),
            root,
            exclusions,
        });
    }
    let profile = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位用户目录".to_string())?;
    let (label, root) = match kind {
        "downloads" => ("下载", profile.join("Downloads")),
        "documents" => ("文档", profile.join("Documents")),
        "desktop" => ("桌面", profile.join("Desktop")),
        _ => return Err("不支持的扫描目录".into()),
    };
    Ok(Scope {
        kind: kind.to_string(),
        label: label.to_string(),
        root,
        exclusions: Vec::new(),
    })
}

fn is_excluded(name: &std::ffi::OsStr, exclusions: &[String]) -> bool {
    if exclusions.is_empty() {
        return false;
    }
    let name = name.to_string_lossy();
    exclusions
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&name))
}

fn walk_root<'a>(
    root: &'a Path,
    exclusions: &'a [String],
) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> + 'a {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(move |entry| {
            entry.depth() != 1 || !is_excluded(entry.file_name(), exclusions)
        })
}

fn category(path: &Path) -> String {
    path.extension().and_then(|value| value.to_str()).map(|value| format!(".{}", value.to_lowercase())).filter(|value| value.len() <= 20).unwrap_or_else(|| "无扩展名".into())
}

fn scan(kind: &str) -> Result<StorageScan, String> {
    if kind.starts_with("drive:") {
        return Err("盘符总览请使用目录分析或大文件扫描".into());
    }
    let scope = scope(kind)?;
    let path = scope.root.clone();
    if !path.is_dir() { return Err(format!("{}目录不存在：{}", scope.label, path.display())); }
    let mut file_count = 0usize;
    let mut directory_count = 0usize;
    let mut total_bytes = 0u64;
    let mut error_count = 0usize;
    let mut totals: HashMap<String, (usize, u64)> = HashMap::new();
    let mut truncated = false;
    for entry in walk_root(&path, &scope.exclusions) {
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
    Ok(StorageScan { kind: kind.into(), label: scope.label, path: path.to_string_lossy().into_owned(), file_count: file_count.min(MAX_FILES), directory_count, total_bytes, error_count, truncated, categories })
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
    exclusions: &[String],
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

    for entry in walk_root(root, exclusions) {
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
    exclusions: &[String],
) -> Result<DuplicateScan, String> {
    if !root.is_dir() {
        return Err(format!("{label}目录不存在：{}", root.display()));
    }
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut error_count = 0usize;
    let mut file_count = 0usize;
    let mut truncated = false;

    for entry in walk_root(root, exclusions) {
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
        let scope = scope(&kind)?;
        let min_bytes = min_bytes.clamp(1024 * 1024, 10 * 1024 * 1024 * 1024);
        let limit = (limit as usize).clamp(10, 200);
        let scan = scan_large_files_in(
            &scope.root,
            &scope.kind,
            &scope.label,
            min_bytes,
            limit,
            &scope.exclusions,
        )?;
        // 扫描期间目录可能被删除，给出明确结果而不是空表。
        if scan.file_count == 0 {
            return Err(format!("{}没有可统计的文件", scope.label));
        }
        Ok(scan)
    })
    .await
    .map_err(|error| format!("大文件扫描任务失败：{error}"))?
}

#[tauri::command]
pub async fn scan_duplicates(kind: String, limit: u32) -> Result<DuplicateScan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let scope = scope(&kind)?;
        let limit = (limit as usize).clamp(5, 200);
        scan_duplicates_in(
            &scope.root,
            &scope.kind,
            &scope.label,
            limit,
            &scope.exclusions,
        )
    })
    .await
    .map_err(|error| format!("重复文件扫描任务失败：{error}"))?
}

#[tauri::command]
pub async fn get_storage_drives() -> Result<Vec<StorageDrive>, String> {
    let disks = sysinfo::Disks::new_with_refreshed_list()
        .map_err(|error| format!("无法枚举磁盘：{error}"))?;
    let mut drives = disks
        .iter()
        .filter(|disk| !disk.mount_point().as_os_str().is_empty())
        .map(|disk| {
            let mount_point = disk.mount_point().to_string_lossy().into_owned();
            let letter = mount_point
                .chars()
                .next()
                .map(|value| value.to_ascii_uppercase().to_string())
                .unwrap_or_default();
            let name = disk.name().to_string_lossy().trim().to_string();
            StorageDrive {
                letter: letter.clone(),
                label: if name.is_empty() {
                    format!("{letter} 盘")
                } else {
                    name
                },
                mount_point,
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                free_bytes: disk.available_space(),
                removable: disk.is_removable(),
            }
        })
        .collect::<Vec<_>>();
    drives.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
    Ok(drives)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryChild {
    name: String,
    path: String,
    relative_path: String,
    is_dir: bool,
    bytes: u64,
    file_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    kind: String,
    label: String,
    root: String,
    relative_path: String,
    path: String,
    parent_relative_path: Option<String>,
    children: Vec<DirectoryChild>,
    total_bytes: u64,
    total_files: usize,
    error_count: usize,
    truncated: bool,
    exclusions: Vec<String>,
}

fn resolve_within(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("无法解析目录：{error}"))?;
    let trimmed = relative.trim().trim_start_matches(|value| value == '\\' || value == '/');
    let normalized = trimmed.replace('/', "\\");
    if normalized.split('\\').any(|part| part == "..") {
        return Err("路径不能包含上级引用".into());
    }
    let candidate = if normalized.is_empty() {
        canonical_root.clone()
    } else {
        canonical_root.join(&normalized)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "目标目录不存在".to_string())?;
    if !canonical.starts_with(&canonical_root) {
        return Err("路径不在允许的目录内".into());
    }
    if !canonical.is_dir() {
        return Err("目标不是目录".into());
    }
    Ok(canonical)
}

fn dir_stats(root: &Path, budget: usize) -> (u64, usize, usize, bool) {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut errors = 0usize;
    let mut truncated = false;
    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        if files > budget {
            truncated = true;
            break;
        }
        match entry.metadata() {
            Ok(metadata) => bytes = bytes.saturating_add(metadata.len()),
            Err(_) => errors += 1,
        }
    }
    (bytes, files, errors, truncated)
}

fn listing_in(
    root: &Path,
    kind: &str,
    label: &str,
    relative: &str,
    limit: usize,
    exclusions: &[String],
) -> Result<DirectoryListing, String> {
    if !root.is_dir() {
        return Err(format!("{label}目录不存在：{}", root.display()));
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("无法解析目录：{error}"))?;
    let directory = resolve_within(root, relative)?;
    let relative_path = directory
        .strip_prefix(&canonical_root)
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .replace('/', "\\");

    let mut children = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;
    let mut error_count = 0usize;
    let mut truncated = false;
    let mut scanned = 0usize;

    let entries = std::fs::read_dir(&directory)
        .map_err(|error| format!("无法读取目录：{error}"))?;
    for entry in entries {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };
        if relative_path.is_empty() && is_excluded(&entry.file_name(), exclusions) {
            continue;
        }
        let child_relative = path
            .strip_prefix(&canonical_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('/', "\\");
        if metadata.is_dir() {
            let (bytes, files, errors, child_truncated) = dir_stats(&path, MAX_FILES.saturating_sub(scanned));
            scanned += files;
            error_count += errors;
            truncated |= child_truncated;
            total_bytes = total_bytes.saturating_add(bytes);
            total_files += files;
            children.push(DirectoryChild {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                relative_path: child_relative,
                is_dir: true,
                bytes,
                file_count: files,
            });
        } else if metadata.is_file() {
            let size = metadata.len();
            total_bytes = total_bytes.saturating_add(size);
            total_files += 1;
            children.push(DirectoryChild {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                relative_path: child_relative,
                is_dir: false,
                bytes: size,
                file_count: 1,
            });
        }
    }

    children.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.name.cmp(&right.name))
    });
    children.truncate(limit);

    let parent_relative_path = if relative_path.is_empty() {
        None
    } else {
        Some(
            Path::new(&relative_path)
                .parent()
                .map(|parent| parent.to_string_lossy().replace('/', "\\"))
                .unwrap_or_default(),
        )
    };

    let at_root = relative_path.is_empty();
    Ok(DirectoryListing {
        kind: kind.into(),
        label: label.into(),
        root: canonical_root.to_string_lossy().into_owned(),
        relative_path,
        path: directory.to_string_lossy().into_owned(),
        parent_relative_path,
        children,
        total_bytes,
        total_files,
        error_count,
        truncated,
        exclusions: if at_root {
            exclusions.to_vec()
        } else {
            Vec::new()
        },
    })
}

#[tauri::command]
pub async fn scan_directory(
    kind: String,
    relative: String,
    limit: u32,
) -> Result<DirectoryListing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let scope = scope(&kind)?;
        listing_in(
            &scope.root,
            &scope.kind,
            &scope.label,
            &relative,
            (limit as usize).clamp(10, 500),
            &scope.exclusions,
        )
    })
    .await
    .map_err(|error| format!("目录扫描任务失败：{error}"))?
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
        assert!(scope("C:\\").is_err());
        assert!(scope("../Downloads").is_err());
        assert!(scope("drive:CC").is_err());
        assert!(scope("drive:1").is_err());
    }

    #[test]
    fn drive_scope_labels_and_excludes_system_dirs() {
        if !std::path::Path::new("C:\\").is_dir() {
            return;
        }
        let scope = scope("drive:c").expect("C drive scope");
        assert_eq!(scope.label, "C 盘");
        assert!(scope.exclusions.iter().any(|item| item == "Windows"));
        assert!(scope.exclusions.iter().any(|item| item == "$Recycle.Bin"));
        assert!(scope.exclusions.iter().any(|item| item == "pagefile.sys"));
    }

    #[test]
    fn finds_large_files_and_directory_usage() {
        let root = temp_root("large");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.bin"), vec![1u8; 2 * 1024 * 1024]).unwrap();
        fs::write(root.join("small.txt"), b"tiny").unwrap();
        fs::write(root.join("sub").join("b.bin"), vec![2u8; 3 * 1024 * 1024]).unwrap();

        let scan = scan_large_files_in(&root, "downloads", "下载", 1024 * 1024, 10, &[]).unwrap();
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

        let scan = scan_duplicates_in(&root, "downloads", "下载", 10, &[]).unwrap();
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
        assert!(scan_large_files_in(&root, "downloads", "下载", 1024, 10, &[]).is_err());
        assert!(scan_duplicates_in(&root, "downloads", "下载", 10, &[]).is_err());
    }

    #[test]
    fn lists_directory_children_and_totals() {
        let root = temp_root("listing");
        fs::create_dir_all(root.join("big").join("inner")).unwrap();
        fs::write(root.join("a.txt"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("big").join("b.bin"), vec![0u8; 2000]).unwrap();
        fs::write(root.join("big").join("inner").join("c.bin"), vec![0u8; 500]).unwrap();

        let listing = listing_in(&root, "downloads", "下载", "", 50, &[]).unwrap();
        assert_eq!(listing.total_files, 3);
        assert_eq!(listing.total_bytes, 3500);
        assert_eq!(listing.children.len(), 2);
        assert!(listing.parent_relative_path.is_none());
        let big = listing.children.iter().find(|item| item.name == "big").unwrap();
        assert!(big.is_dir);
        assert_eq!(big.bytes, 2500);
        assert_eq!(big.file_count, 2);
        assert_eq!(big.relative_path, "big");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn drills_down_and_reports_parent() {
        let root = temp_root("drill");
        fs::create_dir_all(root.join("big").join("inner")).unwrap();
        fs::write(root.join("big").join("b.bin"), vec![0u8; 2000]).unwrap();
        fs::write(root.join("big").join("inner").join("c.bin"), vec![0u8; 500]).unwrap();

        let listing = listing_in(&root, "downloads", "下载", "big\\inner", 50, &[]).unwrap();
        assert_eq!(listing.relative_path, "big\\inner");
        assert_eq!(listing.parent_relative_path.as_deref(), Some("big"));
        assert_eq!(listing.children.len(), 1);
        assert_eq!(listing.children[0].name, "c.bin");

        let top_level = listing_in(&root, "downloads", "下载", "big", 50, &[]).unwrap();
        assert_eq!(top_level.parent_relative_path.as_deref(), Some(""));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn skips_excluded_top_level_directories() {
        let root = temp_root("exclude");
        fs::create_dir_all(root.join("Windows")).unwrap();
        fs::create_dir_all(root.join("Users")).unwrap();
        fs::write(root.join("Windows").join("system.bin"), vec![0u8; 4096]).unwrap();
        fs::write(root.join("Users").join("data.bin"), vec![0u8; 2048]).unwrap();
        fs::write(root.join("pagefile.sys"), vec![0u8; 1024]).unwrap();

        let exclusions = vec!["Windows".to_string(), "pagefile.sys".to_string()];
        let scan = scan_large_files_in(&root, "drive:C", "C 盘", 1024, 10, &exclusions).unwrap();
        assert_eq!(scan.file_count, 1);
        assert_eq!(scan.total_bytes, 2048);
        assert!(scan.directories.iter().all(|item| item.name != "Windows"));

        let listing = listing_in(&root, "drive:C", "C 盘", "", 50, &exclusions).unwrap();
        assert_eq!(listing.children.len(), 1);
        assert_eq!(listing.children[0].name, "Users");
        assert_eq!(listing.exclusions, exclusions);
        assert_eq!(listing.total_files, 1);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_path_escape_and_absolute_paths() {
        let root = temp_root("escape");
        fs::create_dir_all(&root).unwrap();
        assert!(listing_in(&root, "downloads", "下载", "..\\Windows", 50, &[]).is_err());
        assert!(listing_in(&root, "downloads", "下载", "C:\\Windows", 50, &[]).is_err());
        fs::remove_dir_all(&root).ok();
    }
}
