use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};
use walkdir::WalkDir;

const ACTION: &str = "apply";
const MAX_SCAN_FILES: usize = 25_000;
const MAX_DELETE_FILES: usize = 10_000;
const LOCK_SAMPLE: usize = 200;

#[derive(Clone)]
struct Candidate {
    id: &'static str,
    label: &'static str,
    base: PathBuf,
    /// 为空表示整目录；否则只匹配指定前缀的文件（如 thumbcache_）。
    prefixes: &'static [&'static str],
    requires_admin: bool,
    note: &'static str,
}

fn env_path(key: &str) -> PathBuf {
    std::env::var_os(key).map(PathBuf::from).unwrap_or_default()
}

/// 固定清理位置白名单。工具不接受用户传入任意路径。
fn candidates() -> Vec<Candidate> {
    let local = env_path("LOCALAPPDATA");
    let windows = env_path("WINDIR");
    let program_data = env_path("ProgramData");
    let mut list = vec![
        Candidate {
            id: "user-temp",
            label: "用户临时文件",
            base: env_path("TEMP"),
            prefixes: &[],
            requires_admin: false,
            note: "应用运行时仍会写入；正在使用的文件会跳过。",
        },
        Candidate {
            id: "windows-temp",
            label: "Windows 临时文件",
            base: windows.join("Temp"),
            prefixes: &[],
            requires_admin: true,
            note: "系统级临时目录通常需要管理员权限。",
        },
        Candidate {
            id: "thumbnail-cache",
            label: "缩略图与图标缓存",
            base: local.join("Microsoft").join("Windows").join("Explorer"),
            prefixes: &["thumbcache_", "iconcache_"],
            requires_admin: false,
            note: "资源管理器运行时可能占用；只匹配 thumbcache_* 与 iconcache_*。",
        },
        Candidate {
            id: "crash-dumps",
            label: "用户崩溃转储",
            base: local.join("CrashDumps"),
            prefixes: &[],
            requires_admin: false,
            note: "保留转储便于蓝屏分析，确认不再需要后再清理。",
        },
        Candidate {
            id: "wer-user",
            label: "用户错误报告",
            base: local.join("Microsoft").join("Windows").join("WER"),
            prefixes: &[],
            requires_admin: false,
            note: "Windows 错误报告缓存。",
        },
        Candidate {
            id: "wer-machine",
            label: "系统错误报告",
            base: program_data.join("Microsoft").join("Windows").join("WER"),
            prefixes: &[],
            requires_admin: true,
            note: "机器级错误报告，通常需要管理员权限。",
        },
        Candidate {
            id: "update-download",
            label: "Windows Update 下载缓存",
            base: windows.join("SoftwareDistribution").join("Download"),
            prefixes: &[],
            requires_admin: true,
            note: "需要管理员权限；正在安装更新时请勿清理。",
        },
        Candidate {
            id: "delivery-optimization",
            label: "传递优化缓存",
            base: program_data
                .join("Microsoft")
                .join("Windows")
                .join("DeliveryOptimization")
                .join("Cache"),
            prefixes: &[],
            requires_admin: true,
            note: "需要管理员权限；清理后系统会按需重新缓存。",
        },
        Candidate {
            id: "edge-cache",
            label: "Edge 浏览器缓存",
            base: local
                .join("Microsoft")
                .join("Edge")
                .join("User Data")
                .join("Default")
                .join("Cache"),
            prefixes: &[],
            requires_admin: false,
            note: "关闭浏览器后清理更彻底；可能影响首次加载速度。",
        },
    ];
    list.retain(|candidate| !candidate.base.as_os_str().is_empty());
    list
}

#[derive(Default)]
struct WalkResult {
    exists: bool,
    files: Vec<PathBuf>,
    sample: Vec<PathBuf>,
    bytes: u64,
    count: usize,
    errors: usize,
    truncated: bool,
}

/// 目录联接/重解析点不跟随，避免通过挂载点逃出白名单目录。
fn is_reparse_directory(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
}

fn walk_candidate(candidate: &Candidate, collect_files: bool) -> WalkResult {
    let mut result = WalkResult::default();
    if !candidate.base.is_dir() {
        return result;
    }
    result.exists = true;
    let walker = if candidate.prefixes.is_empty() {
        WalkDir::new(&candidate.base).follow_links(false).min_depth(1)
    } else {
        WalkDir::new(&candidate.base)
            .follow_links(false)
            .min_depth(1)
            .max_depth(1)
    }
    .into_iter()
    .filter_entry(|entry| {
        !entry.file_type().is_dir() || !is_reparse_directory(entry.path())
    });
    for entry in walker {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                result.errors += 1;
                continue;
            }
        };
        if entry.path_is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        if !candidate.prefixes.is_empty() {
            let name = entry.file_name().to_string_lossy();
            if !candidate
                .prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
        }
        result.count += 1;
        if result.count > MAX_SCAN_FILES {
            result.count = MAX_SCAN_FILES;
            result.truncated = true;
            break;
        }
        match entry.metadata() {
            Ok(metadata) => {
                result.bytes = result.bytes.saturating_add(metadata.len());
                if collect_files {
                    result.files.push(entry.path().to_path_buf());
                } else if result.sample.len() < LOCK_SAMPLE {
                    result.sample.push(entry.path().to_path_buf());
                }
            }
            Err(_) => result.errors += 1,
        }
    }
    result
}

/// 占用检测：能拿到写句柄视为未占用；共享冲突/权限拒绝视为使用中或只读。
fn is_locked(path: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => false,
        Err(error) => matches!(
            error.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
        ),
    }
}

#[repr(C)]
struct ShFileOpStructW {
    hwnd: *mut c_void,
    w_func: u32,
    p_from: *const u16,
    p_to: *const u16,
    f_flags: u16,
    f_any_operations_aborted: i32,
    h_name_mappings: *mut c_void,
    lpsz_progress_title: *const u16,
}

#[link(name = "shell32")]
extern "system" {
    fn SHFileOperationW(operation: *mut ShFileOpStructW) -> i32;
}

const FO_DELETE: u32 = 3;
const FOF_SILENT: u16 = 0x0004;
const FOF_NOCONFIRMATION: u16 = 0x0010;
const FOF_ALLOWUNDO: u16 = 0x0040;
const FOF_NOERRORUI: u16 = 0x0400;

/// 把文件移入回收站（FOF_ALLOWUNDO），失败时返回错误码。
fn recycle_files(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut from: Vec<u16> = Vec::new();
    for path in paths {
        let text = path.to_string_lossy();
        from.extend(text.encode_utf16());
        from.push(0);
    }
    from.push(0);
    let mut operation = ShFileOpStructW {
        hwnd: std::ptr::null_mut(),
        w_func: FO_DELETE,
        p_from: from.as_ptr(),
        p_to: std::ptr::null(),
        f_flags: FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI,
        f_any_operations_aborted: 0,
        h_name_mappings: std::ptr::null_mut(),
        lpsz_progress_title: std::ptr::null(),
    };
    // SAFETY: 路径列表以双 NUL 结尾，结构体与 Shell API 约定一致。
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 {
        return Err(format!("回收站操作失败（0x{code:x}）"));
    }
    if operation.f_any_operations_aborted != 0 {
        return Err("回收站操作被中止".into());
    }
    Ok(())
}

fn candidate_digest(list: &[Candidate]) -> String {
    let ids = list
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    digest_of(&ids)
}

fn preview() -> Result<Value, ToolError> {
    let list = candidates();
    if list.is_empty() {
        return Err(ToolError::new("INTERNAL", "没有可用的清理位置。"));
    }
    let token = tool_protocol::issue_token(ACTION, &candidate_digest(&list))?;
    let mut rows = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;
    let mut locked_total = 0usize;

    for candidate in &list {
        let scanned = walk_candidate(candidate, false);
        let locked = scanned
            .sample
            .iter()
            .filter(|path| is_locked(path))
            .count();
        locked_total += locked;
        let status = if !scanned.exists {
            "不存在"
        } else if scanned.count == 0 {
            "无文件"
        } else if scanned.errors > 0 && scanned.bytes == 0 {
            "需要管理员或无权限"
        } else {
            "可用"
        };
        if scanned.exists && scanned.count > 0 {
            total_bytes = total_bytes.saturating_add(scanned.bytes);
            total_files += scanned.count;
        }
        rows.push(json!({
            "id": candidate.id,
            "label": candidate.label,
            "path": candidate.base.to_string_lossy(),
            "bytes": scanned.bytes,
            "size": human_bytes(scanned.bytes),
            "files": scanned.count,
            "locked": locked,
            "truncated": scanned.truncated,
            "status": status,
            "requiresAdmin": candidate.requires_admin,
            "note": candidate.note,
        }));
    }

    let mut data = table(
        json!([
            item("候选位置", list.len().to_string()),
            item("可清理空间", human_bytes(total_bytes)),
            item("文件总数", total_files.to_string()),
            item("占用/只读（抽样）", locked_total.to_string()),
        ]),
        json!([
            column("label", "位置"),
            column("path", "路径"),
            column("size", "占用"),
            column("files", "文件"),
            column("locked", "占用/只读"),
            column("status", "状态"),
        ]),
        Value::Array(rows),
        json!([
            "只读扫描：预览不会删除任何文件；确认后所选文件会移入回收站，可从回收站还原。",
            "占用检测为抽样（每个位置最多 200 个文件，结果标注“占用/只读”）；执行时仍会逐个检测并跳过被占用、只读或无权限的文件。",
            "系统级目录（Windows 临时、更新缓存、传递优化等）通常需要管理员权限，普通权限下会记录失败或跳过。",
            "只清理固定白名单位置，不接受任意路径；单个位置最多统计 2.5 万个文件、单次最多删除 1 万个，超出部分标记为已截断。",
        ]),
    );
    attach_token(&mut data, token);
    Ok(data)
}

fn apply(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "清理需要确认令牌，请先预览并勾选要清理的位置。",
        )
    })?;
    let requested = request
        .parameters
        .get("candidateIds")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 candidateIds。"))?;
    if requested.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "没有选择任何清理位置。"));
    }
    let selected = requested
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<HashSet<_>>();

    let list = candidates();
    tool_protocol::verify_token(token, ACTION, &candidate_digest(&list))?;

    let mut known = HashSet::new();
    for candidate in &list {
        known.insert(candidate.id);
    }
    if let Some(unknown) = selected.iter().find(|id| !known.contains(id.as_str())) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!("清理位置不在白名单中：{unknown}"),
        ));
    }

    let mut rows = Vec::new();
    let mut freed_bytes = 0u64;
    let mut recycled_files = 0usize;
    let mut skipped_locked = 0usize;
    let mut failed_candidates = 0usize;
    let mut delete_budget = MAX_DELETE_FILES;
    let mut audit = Vec::new();

    for candidate in &list {
        if !selected.contains(candidate.id) {
            continue;
        }
        let before = walk_candidate(candidate, true);
        if !before.exists {
            rows.push(json!({
                "label": candidate.label,
                "freed": "0 B",
                "files": 0usize,
                "status": "目录不存在，已跳过",
            }));
            continue;
        }
        let mut to_recycle = Vec::new();
        let mut locked = 0usize;
        for path in &before.files {
            if to_recycle.len() >= delete_budget {
                break;
            }
            if is_locked(path) {
                locked += 1;
                continue;
            }
            to_recycle.push(path.clone());
        }
        skipped_locked += locked;
        delete_budget = delete_budget.saturating_sub(to_recycle.len());

        let outcome: Result<(u64, usize), String> = if to_recycle.is_empty() {
            Ok((0, 0))
        } else {
            recycle_files(&to_recycle).map(|()| {
                let after = walk_candidate(candidate, false);
                (
                    before.bytes.saturating_sub(after.bytes),
                    before.count.saturating_sub(after.count),
                )
            })
        };

        match outcome {
            Ok((freed, removed)) => {
                freed_bytes = freed_bytes.saturating_add(freed);
                recycled_files += removed;
                audit.push(json!({
                    "id": candidate.id,
                    "freedBytes": freed,
                    "files": removed,
                    "locked": locked,
                    "result": "ok",
                }));
                let status = if removed == 0 {
                    "没有可清理的文件".to_string()
                } else if locked > 0 {
                    format!("已移入回收站，跳过 {locked} 个占用/只读/无权限文件")
                } else {
                    "已移入回收站".to_string()
                };
                rows.push(json!({
                    "label": candidate.label,
                    "freed": human_bytes(freed),
                    "files": removed,
                    "status": status,
                }));
            }
            Err(error) => {
                failed_candidates += 1;
                audit.push(json!({
                    "id": candidate.id,
                    "freedBytes": 0,
                    "files": 0,
                    "locked": locked,
                    "result": "failed",
                    "error": error,
                }));
                rows.push(json!({
                    "label": candidate.label,
                    "freed": "0 B",
                    "files": 0usize,
                    "status": error,
                }));
            }
        }
    }

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "clean_master",
        "action": ACTION,
        "freedBytes": freed_bytes,
        "files": recycled_files,
        "skippedLocked": skipped_locked,
        "candidates": audit,
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("释放空间", human_bytes(freed_bytes)),
            item("移入回收站", format!("{recycled_files} 个文件")),
            item("跳过占用/只读", skipped_locked.to_string()),
            item("失败位置", failed_candidates.to_string()),
        ]),
        json!([
            column("label", "位置"),
            column("freed", "释放"),
            column("files", "文件"),
            column("status", "结果"),
        ]),
        Value::Array(rows),
        json!([
            "所选文件已移入回收站，可在“回收站”中还原到原位置。",
            "被占用或只读的文件已跳过，关闭相关程序后可重新预览清理。",
            "本工具只清理固定白名单位置，不删除目录本身，也不修改注册表或服务。",
            "本次操作已写入本地操作日志；再次执行需要重新预览获取新令牌。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "clean_master",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => apply(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
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
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp");
        fs::create_dir_all(&base).ok();
        base.join(format!(
            "system-lens-clean-{name}-{}-{stamp}",
            std::process::id()
        ))
    }

    fn candidate(base: PathBuf, prefixes: &'static [&'static str]) -> Candidate {
        Candidate {
            id: "test",
            label: "测试",
            base,
            prefixes,
            requires_admin: false,
            note: "",
        }
    }

    #[test]
    fn candidate_ids_are_unique_and_absolute() {
        let list = candidates();
        let mut ids = list.iter().map(|candidate| candidate.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), list.len());
        assert!(list.iter().all(|candidate| candidate.base.is_absolute()));
    }

    #[test]
    fn walks_directory_and_skips_symlinks() {
        let root = temp_root("walk");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("sub").join("b.bin"), vec![0u8; 200]).unwrap();

        let scanned = walk_candidate(&candidate(root.clone(), &[]), false);
        assert!(scanned.exists);
        assert_eq!(scanned.count, 2);
        assert_eq!(scanned.bytes, 300);
        assert_eq!(scanned.errors, 0);
        assert!(scanned.sample.len() <= LOCK_SAMPLE);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prefix_filter_matches_only_cache_files() {
        let root = temp_root("prefix");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("thumbcache_32.db"), b"a").unwrap();
        fs::write(root.join("iconcache_16.db"), b"b").unwrap();
        fs::write(root.join("other.db"), b"c").unwrap();

        let scanned = walk_candidate(&candidate(root.clone(), &["thumbcache_", "iconcache_"]), false);
        assert_eq!(scanned.count, 2);
        assert_eq!(scanned.bytes, 2);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recycle_moves_file_to_recycle_bin() {
        let root = temp_root("recycle");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("trash-me.txt");
        fs::write(&file, b"temporary").unwrap();
        recycle_files(&[file.clone()]).expect("recycle");
        assert!(!file.exists(), "file should be gone after recycle");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn locked_detection_flags_readonly_files() {
        let root = temp_root("locked");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("readonly.txt");
        fs::write(&file, b"data").unwrap();
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(is_locked(&file));
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(!is_locked(&file));
        fs::remove_dir_all(&root).ok();
    }
}
