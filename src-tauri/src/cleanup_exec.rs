use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::cleanup_preview::{measure, target_path, SAFE_CLEANUP_IDS};

const PLAN_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Default)]
pub struct CleanupPlanState(Mutex<Option<PendingPlan>>);

struct PendingPlan {
    token: String,
    ids: Vec<String>,
    paths: Vec<PathBuf>,
    total_bytes: u64,
    total_files: usize,
    created_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    token: String,
    ids: Vec<String>,
    total_bytes: u64,
    total_files: usize,
    created_at_unix_ms: u64,
    note: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    moved_items: usize,
    moved_bytes: u64,
    aborted: bool,
    errors: Vec<String>,
    audit_log: String,
    note: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn is_allowed(id: &str) -> bool {
    SAFE_CLEANUP_IDS.contains(&id)
}

fn validate_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("拒绝非绝对路径：{}", path.display()));
    }
    if path.parent().is_none() {
        return Err("拒绝磁盘根目录".into());
    }
    if path.components().count() < 2 {
        return Err(format!("路径层级过浅：{}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("目录不存在：{}", path.display()));
    }
    Ok(())
}

fn new_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}{:x}", nanos, std::process::id())
}

#[tauri::command]
pub fn prepare_cleanup(
    ids: Vec<String>,
    state: State<'_, CleanupPlanState>,
) -> Result<CleanupPlan, String> {
    let mut unique = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || unique.contains(&id) {
            continue;
        }
        if !is_allowed(&id) {
            return Err(format!("不支持清理该项：{id}"));
        }
        unique.push(id);
    }
    if unique.is_empty() {
        return Err("没有选择任何清理项".into());
    }

    let mut paths = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;
    for id in &unique {
        let path = target_path(id).ok_or_else(|| format!("无法解析清理路径：{id}"))?;
        validate_path(&path)?;
        let (bytes, files) = measure(&path);
        total_bytes = total_bytes.saturating_add(bytes);
        total_files = total_files.saturating_add(files);
        paths.push(path);
    }

    let token = new_token();
    let plan = CleanupPlan {
        token: token.clone(),
        ids: unique.clone(),
        total_bytes,
        total_files,
        created_at_unix_ms: now_ms(),
        note: "确认后会将选中位置的内容移动到回收站；目录本身保留。".into(),
    };
    let mut slot = state
        .0
        .lock()
        .map_err(|_| "清理计划状态被占用".to_string())?;
    *slot = Some(PendingPlan {
        token,
        ids: unique,
        paths,
        total_bytes,
        total_files,
        created_ms: now_ms(),
    });
    Ok(plan)
}

#[tauri::command]
pub fn execute_cleanup(
    token: String,
    app: AppHandle,
    state: State<'_, CleanupPlanState>,
) -> Result<CleanupResult, String> {
    let pending = {
        let mut slot = state
            .0
            .lock()
            .map_err(|_| "清理计划状态被占用".to_string())?;
        let Some(plan) = slot.take() else {
            return Err("没有待执行的清理计划，请重新扫描并确认".into());
        };
        if plan.token != token {
            return Err("清理令牌不匹配，请重新扫描并确认".into());
        }
        if now_ms().saturating_sub(plan.created_ms) > PLAN_TTL_MS {
            return Err("清理计划已过期，请重新扫描并确认".into());
        }
        plan
    };

    for (id, path) in pending.ids.iter().zip(pending.paths.iter()) {
        if !is_allowed(id) {
            return Err(format!("清理项不在允许列表：{id}"));
        }
        validate_path(path)?;
    }

    let mut children = Vec::new();
    for path in &pending.paths {
        let entries = std::fs::read_dir(path)
            .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
        for entry in entries.flatten() {
            children.push(entry.path());
        }
    }
    if children.is_empty() {
        return Ok(CleanupResult {
            moved_items: 0,
            moved_bytes: 0,
            aborted: false,
            errors: Vec::new(),
            audit_log: String::new(),
            note: "选中位置已经是空的，没有需要移动的内容。".into(),
        });
    }

    let (code, aborted) = recycle_paths(&children)?;
    let mut errors = Vec::new();
    if code != 0 {
        errors.push(format!("Shell 回收站操作返回错误码 {code}"));
    }
    if aborted {
        errors.push("操作被系统中止，可能只移动了部分内容".into());
    }

    let audit_log = write_audit(&app, &pending, code, aborted, &errors);
    Ok(CleanupResult {
        moved_items: children.len(),
        moved_bytes: pending.total_bytes,
        aborted,
        errors,
        audit_log,
        note: "内容已移动到回收站；可在回收站中恢复。目录结构保留，未删除系统更新缓存或回收站本身。".into(),
    })
}

fn write_audit(
    app: &AppHandle,
    pending: &PendingPlan,
    code: u32,
    aborted: bool,
    errors: &[String],
) -> String {
    let Ok(directory) = app.path().app_data_dir() else {
        return String::new();
    };
    if std::fs::create_dir_all(&directory).is_err() {
        return String::new();
    }
    let path = directory.join("cleanup-audit.jsonl");
    let entry = serde_json::json!({
        "atUnixMs": now_ms(),
        "ids": pending.ids,
        "items": pending.total_files,
        "bytes": pending.total_bytes,
        "shellCode": code,
        "aborted": aborted,
        "errors": errors,
    });
    let line = format!("{entry}\n");
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
            path.to_string_lossy().into_owned()
        }
        Err(_) => String::new(),
    }
}

#[cfg(windows)]
fn recycle_paths(paths: &[PathBuf]) -> Result<(u32, bool), String> {
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct ShFileOp {
        hwnd: *mut std::ffi::c_void,
        w_func: u32,
        p_from: *const u16,
        p_to: *const u16,
        f_flags: u16,
        any_aborted: i32,
        name_mappings: *mut std::ffi::c_void,
        progress_title: *const u16,
    }

    extern "system" {
        fn SHFileOperationW(operation: *mut ShFileOp) -> i32;
    }

    let mut from: Vec<u16> = Vec::new();
    for path in paths {
        for unit in path.as_os_str().encode_wide() {
            from.push(unit);
        }
        from.push(0);
    }
    from.push(0);

    let mut operation = ShFileOp {
        hwnd: std::ptr::null_mut(),
        w_func: 3, // FO_DELETE
        p_from: from.as_ptr(),
        p_to: std::ptr::null(),
        // FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI
        f_flags: 0x0040 | 0x0010 | 0x0004 | 0x0400,
        any_aborted: 0,
        name_mappings: std::ptr::null_mut(),
        progress_title: std::ptr::null(),
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    Ok((code as u32, operation.any_aborted != 0))
}

#[cfg(not(windows))]
fn recycle_paths(_paths: &[PathBuf]) -> Result<(u32, bool), String> {
    Err("清理功能仅支持 Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_user_cache_is_allowed() {
        assert!(is_allowed("user-temp"));
        assert!(is_allowed("edge-cache"));
        assert!(!is_allowed("windows-update"));
        assert!(!is_allowed("recycle-bin"));
        assert!(!is_allowed("../windows"));
    }

    #[test]
    fn rejects_shallow_or_relative_paths() {
        assert!(validate_path(Path::new("relative\\path")).is_err());
        assert!(validate_path(Path::new("C:\\")).is_err());
    }
}
