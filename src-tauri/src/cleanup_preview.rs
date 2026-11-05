use serde::Serialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAX_FILES_PER_CANDIDATE: usize = 150_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidate {
    pub id: String,
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub file_count: usize,
    pub error_count: usize,
    pub requires_admin: bool,
    pub truncated: bool,
    pub note: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreview {
    pub captured_at_unix_ms: u64,
    pub candidates: Vec<CleanupCandidate>,
    pub total_bytes: u64,
    pub total_files: usize,
    pub source: String,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|path| !path.as_os_str().is_empty())
}

/// 统计目录占用，只读取元数据，不打开文件内容。
fn directory_stats(root: &Path) -> (u64, usize, usize, bool) {
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
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        if files > MAX_FILES_PER_CANDIDATE {
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

fn drive_path(root: &Path, child: &str) -> PathBuf {
    let mut base = root.as_os_str().to_os_string();
    let text = base.to_string_lossy();
    if !text.ends_with('\\') && !text.ends_with('/') {
        base.push("\\");
    }
    PathBuf::from(base).join(child)
}

struct Target {
    id: &'static str,
    label: &'static str,
    path: Option<PathBuf>,
    requires_admin: bool,
    note: &'static str,
}

fn targets() -> Vec<Target> {
    let temp = env_path("TEMP");
    let windir = env_path("WINDIR").or_else(|| env_path("SystemRoot"));
    let local = env_path("LOCALAPPDATA");
    let drive = env_path("SystemDrive");
    vec![
        Target {
            id: "user-temp",
            label: "用户临时文件",
            path: temp,
            requires_admin: false,
            note: "应用运行时可能仍在写入，统计值会波动。",
        },
        Target {
            id: "windows-temp",
            label: "Windows 临时文件",
            path: windir.as_ref().map(|root| root.join("Temp")),
            requires_admin: true,
            note: "部分系统临时文件需要管理员权限才能完整统计。",
        },
        Target {
            id: "windows-update",
            label: "Windows Update 下载缓存",
            path: windir.as_ref().map(|root| root.join("SoftwareDistribution").join("Download")),
            requires_admin: true,
            note: "更新缓存由系统管理，删除可能导致更新重新下载。",
        },
        Target {
            id: "recycle-bin",
            label: "回收站",
            path: drive.as_ref().map(|root| drive_path(root, "$Recycle.Bin")),
            requires_admin: true,
            note: "普通权限只能看到当前用户的部分内容。",
        },
        Target {
            id: "crash-dumps",
            label: "应用崩溃转储",
            path: local.as_ref().map(|root| root.join("CrashDumps")),
            requires_admin: false,
            note: "仅统计崩溃转储文件占用。",
        },
        Target {
            id: "error-reporting",
            label: "Windows 错误报告",
            path: local
                .as_ref()
                .map(|root| root.join("Microsoft").join("Windows").join("WER")),
            requires_admin: false,
            note: "包含错误报告的排队与归档目录。",
        },
        Target {
            id: "thumbnail-cache",
            label: "缩略图与图标缓存",
            path: local
                .as_ref()
                .map(|root| root.join("Microsoft").join("Windows").join("Explorer")),
            requires_admin: false,
            note: "仅统计资源管理器缓存目录，不含文档。",
        },
        Target {
            id: "edge-cache",
            label: "Edge 浏览器缓存",
            path: local.as_ref().map(|root| {
                root.join("Microsoft")
                    .join("Edge")
                    .join("User Data")
                    .join("Default")
                    .join("Cache")
            }),
            requires_admin: false,
            note: "浏览器运行时缓存文件可能被占用，统计值会波动。",
        },
        Target {
            id: "inet-cache",
            label: "系统网络缓存",
            path: local
                .as_ref()
                .map(|root| root.join("Microsoft").join("Windows").join("INetCache")),
            requires_admin: false,
            note: "IE/部分桌面程序共享的临时网络缓存。",
        },
    ]
}

pub fn build_preview(filter: Option<&str>) -> CleanupPreview {
    let mut candidates = Vec::new();
    for target in targets() {
        if let Some(needle) = filter {
            if target.id != needle {
                continue;
            }
        }
        let (exists, path, error) = match &target.path {
            Some(path) if path.is_dir() => (true, path.to_string_lossy().into_owned(), None),
            Some(path) => (
                false,
                path.to_string_lossy().into_owned(),
                Some("目录不存在".to_string()),
            ),
            None => (false, String::new(), Some("无法解析系统目录".to_string())),
        };
        let (bytes, file_count, error_count, truncated) = if exists {
            let (bytes, files, errors, truncated) = directory_stats(Path::new(&path));
            (bytes, files, errors, truncated)
        } else {
            (0, 0, 0, false)
        };
        candidates.push(CleanupCandidate {
            id: target.id.into(),
            label: target.label.into(),
            path,
            exists,
            bytes,
            file_count,
            error_count,
            requires_admin: target.requires_admin,
            truncated,
            note: target.note.into(),
            error,
        });
    }
    let total_bytes = candidates.iter().map(|item| item.bytes).sum();
    let total_files = candidates.iter().map(|item| item.file_count).sum();
    CleanupPreview {
        captured_at_unix_ms: now_unix_ms(),
        candidates,
        total_bytes,
        total_files,
        source: "Windows 目录元数据（只读）".into(),
    }
}

#[tauri::command]
pub async fn scan_cleanup_candidates() -> Result<CleanupPreview, String> {
    tauri::async_runtime::spawn_blocking(|| Ok(build_preview(None)))
        .await
        .map_err(|error| format!("清理候选扫描任务失败：{error}"))?
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
        std::env::temp_dir().join(format!(
            "system-lens-cleanup-{name}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn counts_files_without_reading_content() {
        let root = temp_root("stats");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a.tmp"), vec![0u8; 1024]).unwrap();
        fs::write(root.join("nested").join("b.tmp"), vec![0u8; 2048]).unwrap();

        let (bytes, files, errors, truncated) = directory_stats(&root);
        assert_eq!(bytes, 3072);
        assert_eq!(files, 2);
        assert_eq!(errors, 0);
        assert!(!truncated);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn drive_path_normalizes_drive_root() {
        assert_eq!(
            drive_path(Path::new("C:"), "$Recycle.Bin"),
            PathBuf::from("C:\\$Recycle.Bin")
        );
        assert_eq!(
            drive_path(Path::new("C:\\"), "$Recycle.Bin"),
            PathBuf::from("C:\\$Recycle.Bin")
        );
    }

    #[test]
    fn targets_cover_expected_locations() {
        let targets = targets();
        assert_eq!(targets.len(), 9);
        let mut ids = targets.iter().map(|item| item.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 9);
        assert!(targets.iter().all(|item| !item.label.is_empty()));
        assert!(targets.iter().any(|item| item.id == "user-temp"));
        assert!(targets.iter().any(|item| item.id == "recycle-bin"));
    }

    #[test]
    fn unknown_filter_returns_empty() {
        let preview = build_preview(Some("does-not-exist"));
        assert!(preview.candidates.is_empty());
        assert_eq!(preview.total_bytes, 0);
    }
}
