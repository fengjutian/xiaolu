use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub extension: String,
    pub bytes: u64,
    pub modified_unix_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopOverview {
    pub desktop_path: String,
    pub documents_path: String,
    pub downloads_path: String,
    pub items: Vec<DesktopItem>,
    pub total: usize,
    pub error_count: usize,
}

fn user_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

fn desktop_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(profile) = user_path("USERPROFILE") {
        roots.push(profile.join("Desktop"));
    }
    if let Some(public) = user_path("PUBLIC") {
        roots.push(public.join("Desktop"));
    }
    roots
}

fn modified_unix_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
}

#[cfg(windows)]
fn is_hidden_or_system(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attributes = metadata.file_attributes();
    attributes & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden_or_system(_metadata: &fs::Metadata) -> bool {
    false
}

fn collect_items(root: &Path, items: &mut Vec<DesktopItem>, errors: &mut usize) {
    let Ok(entries) = fs::read_dir(root) else {
        *errors += 1;
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            *errors += 1;
            continue;
        };
        if is_hidden_or_system(&metadata) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_lowercase())
            .unwrap_or_default();
        items.push(DesktopItem {
            name,
            path: path.to_string_lossy().into_owned(),
            is_dir: metadata.is_dir(),
            extension,
            bytes: if metadata.is_file() { metadata.len() } else { 0 },
            modified_unix_ms: modified_unix_ms(&metadata),
        });
    }
}

fn collect() -> Result<DesktopOverview, String> {
    let profile = user_path("USERPROFILE").ok_or_else(|| "无法定位用户目录".to_string())?;
    let desktop = profile.join("Desktop");
    let documents = profile.join("Documents");
    let downloads = profile.join("Downloads");
    let mut items = Vec::new();
    let mut error_count = 0usize;
    for root in desktop_roots() {
        if root.is_dir() {
            collect_items(&root, &mut items, &mut error_count);
        }
    }
    items.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    items.truncate(300);
    Ok(DesktopOverview {
        desktop_path: desktop.to_string_lossy().into_owned(),
        documents_path: documents.to_string_lossy().into_owned(),
        downloads_path: downloads.to_string_lossy().into_owned(),
        total: items.len(),
        items,
        error_count,
    })
}

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

/// 只允许打开桌面根目录内的现有项目。
pub fn validate_desktop_path(path: &str, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err("路径必须是绝对路径".into());
    }
    if !path.exists() {
        return Err("项目不存在".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("无法解析路径：{error}"))?;
    let allowed = roots.iter().any(|root| {
        root.canonicalize()
            .map(|canonical_root| canonical.starts_with(canonical_root))
            .unwrap_or(false)
    });
    if !allowed {
        return Err("只允许打开桌面目录内的项目".into());
    }
    Ok(canonical)
}

#[cfg(windows)]
fn open_path(path: &Path) -> Result<(), String> {
    let operation: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: 字符串以 NUL 结尾。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        return Err(format!("无法打开项目（ShellExecute 返回 {result}）"));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_desktop_overview() -> Result<DesktopOverview, String> {
    tauri::async_runtime::spawn_blocking(collect)
        .await
        .map_err(|error| format!("桌面枚举任务失败：{error}"))?
}

#[tauri::command]
pub async fn open_desktop_item(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = validate_desktop_path(&path, &desktop_roots())?;
        open_path(&target)?;
        Ok(format!("已打开：{}", target.display()))
    })
    .await
    .map_err(|error| format!("打开桌面项目任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_absolute_and_outside_paths() {
        let roots = vec![PathBuf::from("C:\\Users\\me\\Desktop")];
        assert!(validate_desktop_path("relative.txt", &roots).is_err());
        assert!(validate_desktop_path("C:\\Windows\\notepad.exe", &roots).is_err());
    }

    #[test]
    fn extension_is_lowercase() {
        let path = Path::new("C:\\Users\\me\\Desktop\\Report.PDF");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_lowercase())
            .unwrap_or_default();
        assert_eq!(extension, "pdf");
    }

    #[test]
    #[ignore = "依赖真实桌面目录"]
    fn collects_real_desktop() {
        let overview = collect().expect("collect desktop");
        assert!(overview.desktop_path.to_lowercase().contains("desktop"));
    }
}
