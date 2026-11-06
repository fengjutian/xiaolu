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
/// 每批移入回收站的文件数。批量失败时再逐文件重试，隔离单个坏文件。
const RECYCLE_BATCH: usize = 256;
/// 结果表格最多展示的逐文件明细行数，完整清单写入本地日志。
const MAX_DETAIL_ROWS: usize = 500;

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

#[derive(Clone)]
struct FileEntry {
    path: PathBuf,
    bytes: u64,
}

#[derive(Default)]
struct WalkResult {
    exists: bool,
    files: Vec<FileEntry>,
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
                let bytes = metadata.len();
                result.bytes = result.bytes.saturating_add(bytes);
                if collect_files {
                    result.files.push(FileEntry {
                        path: entry.path().to_path_buf(),
                        bytes,
                    });
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

/// 单个文件的处理结果。执行阶段对每个文件留痕，而不是只给目录级汇总。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Recycled,
    Locked,
    Missing,
    Failed,
    Budget,
}

impl Outcome {
    fn key(self) -> &'static str {
        match self {
            Outcome::Recycled => "recycled",
            Outcome::Locked => "locked",
            Outcome::Missing => "missing",
            Outcome::Failed => "failed",
            Outcome::Budget => "budget",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Outcome::Recycled => "已移入回收站",
            Outcome::Locked => "跳过：占用/只读/无权限",
            Outcome::Missing => "跳过：已不存在",
            Outcome::Failed => "失败",
            Outcome::Budget => "跳过：超出单次上限",
        }
    }
}

struct FileRecord {
    entry: FileEntry,
    outcome: Outcome,
    reason: String,
}

fn record(entry: FileEntry, outcome: Outcome, reason: impl Into<String>) -> FileRecord {
    FileRecord {
        entry,
        outcome,
        reason: reason.into(),
    }
}

/// 把一批候选文件移入回收站。批量失败时退回逐文件执行，避免一个坏文件拖垮整批。
/// 返回结果保持与输入相同的顺序。
fn recycle_entries(entries: &[FileEntry], budget: &mut usize) -> Vec<FileRecord> {
    let mut records: Vec<(usize, FileRecord)> = Vec::with_capacity(entries.len());
    let mut pending: Vec<(usize, FileEntry)> =
        Vec::with_capacity(RECYCLE_BATCH.min(entries.len()));

    for (index, entry) in entries.iter().enumerate() {
        if *budget == 0 {
            records.push((
                index,
                record(
                    entry.clone(),
                    Outcome::Budget,
                    format!("单次最多移入回收站 {MAX_DELETE_FILES} 个文件"),
                ),
            ));
            continue;
        }
        if !entry.path.exists() {
            records.push((
                index,
                record(
                    entry.clone(),
                    Outcome::Missing,
                    "扫描后文件已被删除或移动",
                ),
            ));
            continue;
        }
        if is_locked(&entry.path) {
            records.push((
                index,
                record(
                    entry.clone(),
                    Outcome::Locked,
                    "写句柄打开失败，可能被占用、只读或无权限",
                ),
            ));
            continue;
        }
        *budget -= 1;
        pending.push((index, entry.clone()));
        if pending.len() >= RECYCLE_BATCH {
            flush_batch(&mut pending, &mut records);
        }
    }
    flush_batch(&mut pending, &mut records);
    records.sort_by_key(|(index, _)| *index);
    records
        .into_iter()
        .map(|(_, record)| record)
        .collect()
}

fn flush_batch(
    batch: &mut Vec<(usize, FileEntry)>,
    records: &mut Vec<(usize, FileRecord)>,
) {
    if batch.is_empty() {
        return;
    }
    let paths = batch
        .iter()
        .map(|(_, entry)| entry.path.clone())
        .collect::<Vec<_>>();
    match recycle_files(&paths) {
        Ok(()) => {
            for (index, entry) in batch.drain(..) {
                if entry.path.exists() {
                    records.push((
                        index,
                        record(entry, Outcome::Failed, "回收站操作返回成功，但文件仍存在"),
                    ));
                } else {
                    records.push((index, record(entry, Outcome::Recycled, "")));
                }
            }
        }
        Err(batch_error) => {
            // 整批失败时逐文件重试，把失败隔离到具体文件。
            for (index, entry) in batch.drain(..) {
                match recycle_files(std::slice::from_ref(&entry.path)) {
                    Ok(()) if !entry.path.exists() => {
                        records.push((index, record(entry, Outcome::Recycled, "")))
                    }
                    Ok(()) => records.push((
                        index,
                        record(entry, Outcome::Failed, "回收站操作返回成功，但文件仍存在"),
                    )),
                    Err(error) => records.push((
                        index,
                        record(
                            entry,
                            Outcome::Failed,
                            format!("{batch_error}；单个文件重试：{error}"),
                        ),
                    )),
                }
            }
        }
    }
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
            "占用检测为抽样（每个位置最多 200 个文件，结果标注“占用/只读”）；执行时会逐个检测并跳过被占用、只读或无权限的文件。",
            "系统级目录（Windows 临时、更新缓存、传递优化等）通常需要管理员权限，普通权限下会记录失败或跳过。",
            "只清理固定白名单位置，不接受任意路径；单个位置最多统计 2.5 万个文件、单次最多删除 1 万个，超出部分标记为已截断。",
        ]),
    );
    attach_token(&mut data, token);
    Ok(data)
}

fn journal_dir() -> PathBuf {
    tool_protocol::state_dir().join("cleanup")
}

/// 写入完整逐文件清单。返回路径供结果页展示，失败不阻断清理。
fn write_journal(payload: &Value) -> Result<PathBuf, String> {
    let dir = journal_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建清单目录：{error}"))?;
    let path = dir.join(format!(
        "clean-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化清理清单：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入清理清单：{error}"))?;
    Ok(path)
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

    let started_at = tool_protocol::now_ms();
    let mut rows = Vec::new();
    let mut journal_candidates = Vec::new();
    let mut freed_bytes = 0u64;
    let mut recycled_files = 0usize;
    let mut skipped_locked = 0usize;
    let mut failed_files = 0usize;
    let mut budget_skipped = 0usize;
    let mut missing_files = 0usize;
    let mut before_bytes_total = 0u64;
    let mut after_bytes_total = 0u64;
    let mut delete_budget = MAX_DELETE_FILES;
    let mut details_truncated = false;

    for candidate in &list {
        if !selected.contains(candidate.id) {
            continue;
        }
        let before = walk_candidate(candidate, true);
        if !before.exists {
            rows.push(json!({
                "label": candidate.label,
                "file": candidate.base.to_string_lossy(),
                "size": "0 B",
                "status": "目录不存在，已跳过",
                "reason": "",
            }));
            journal_candidates.push(json!({
                "id": candidate.id,
                "label": candidate.label,
                "path": candidate.base.to_string_lossy(),
                "before": {"files": 0, "bytes": 0},
                "after": {"files": 0, "bytes": 0},
                "records": [],
            }));
            continue;
        }

        let records = recycle_entries(&before.files, &mut delete_budget);
        let after = walk_candidate(candidate, false);
        before_bytes_total = before_bytes_total.saturating_add(before.bytes);
        after_bytes_total = after_bytes_total.saturating_add(after.bytes);

        for entry in &records {
            match entry.outcome {
                Outcome::Recycled => {
                    recycled_files += 1;
                    freed_bytes = freed_bytes.saturating_add(entry.entry.bytes);
                }
                Outcome::Locked => skipped_locked += 1,
                Outcome::Missing => missing_files += 1,
                Outcome::Failed => failed_files += 1,
                Outcome::Budget => budget_skipped += 1,
            }
            if rows.len() < MAX_DETAIL_ROWS {
                rows.push(json!({
                    "label": candidate.label,
                    "file": entry.entry.path.to_string_lossy(),
                    "size": human_bytes(entry.entry.bytes),
                    "status": entry.outcome.label(),
                    "reason": entry.reason,
                }));
            } else {
                details_truncated = true;
            }
        }

        journal_candidates.push(json!({
            "id": candidate.id,
            "label": candidate.label,
            "path": candidate.base.to_string_lossy(),
            "before": {"files": before.count, "bytes": before.bytes},
            "after": {"files": after.count, "bytes": after.bytes},
            "records": records
                .iter()
                .map(|record| json!({
                    "path": record.entry.path.to_string_lossy(),
                    "bytes": record.entry.bytes,
                    "outcome": record.outcome.key(),
                    "reason": record.reason,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let journal = json!({
        "tool": "clean_master",
        "version": env!("CARGO_PKG_VERSION"),
        "action": ACTION,
        "startedAt": started_at,
        "finishedAt": tool_protocol::now_ms(),
        "totals": {
            "freedBytes": freed_bytes,
            "recycledFiles": recycled_files,
            "skippedLocked": skipped_locked,
            "failedFiles": failed_files,
            "budgetSkipped": budget_skipped,
            "missingFiles": missing_files,
            "beforeBytes": before_bytes_total,
            "afterBytes": after_bytes_total,
        },
        "candidates": journal_candidates,
    });
    let journal_path = write_journal(&journal);

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "clean_master",
        "action": ACTION,
        "freedBytes": freed_bytes,
        "files": recycled_files,
        "skippedLocked": skipped_locked,
        "failedFiles": failed_files,
        "budgetSkipped": budget_skipped,
        "journal": journal_path.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
        "at": tool_protocol::now_ms(),
    }));

    let mut notes = vec![
        "所选文件已移入回收站，可打开回收站按原路径还原。".to_string(),
        "被占用、只读或无权限的文件已逐个跳过，不会中断其他文件。".to_string(),
        format!("单次最多移入回收站 {MAX_DELETE_FILES} 个文件；明细最多展示 {MAX_DETAIL_ROWS} 条。"),
        "本工具只清理固定白名单位置，不删除目录本身，也不修改注册表或服务。".to_string(),
        "再次清理需要重新扫描获取新的确认令牌。".to_string(),
    ];
    match &journal_path {
        Ok(path) => notes.push(format!("完整逐文件清单：{}", path.display())),
        Err(error) => notes.push(format!("完整清单写入失败：{error}")),
    }
    if details_truncated {
        notes.push("结果表格已截断，完整明细以本地清单为准。".to_string());
    }

    let verified_freed = before_bytes_total.saturating_sub(after_bytes_total);
    let summary = json!([
        item("释放空间", human_bytes(freed_bytes)),
        item("移入回收站", format!("{recycled_files} 个文件")),
        item("跳过占用/只读", skipped_locked.to_string()),
        item("失败", failed_files.to_string()),
        item("超出上限", budget_skipped.to_string()),
        item("复核减少", human_bytes(verified_freed)),
    ]);

    let mut data = table(
        summary,
        json!([
            column("label", "位置"),
            column("file", "文件"),
            column("size", "大小"),
            column("status", "结果"),
            column("reason", "原因"),
        ]),
        Value::Array(rows),
        json!(notes),
    );
    if let Value::Object(map) = &mut data {
        if let Ok(path) = &journal_path {
            map.insert("journalPath".into(), json!(path.to_string_lossy()));
            if let Some(parent) = path.parent() {
                map.insert("journalDir".into(), json!(parent.to_string_lossy()));
            }
        }
    }
    Ok(data)
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

    fn entry(path: PathBuf, bytes: u64) -> FileEntry {
        FileEntry { path, bytes }
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

        let scanned = walk_candidate(&candidate(root.clone(), &[]), true);
        assert!(scanned.exists);
        assert_eq!(scanned.count, 2);
        assert_eq!(scanned.bytes, 300);
        assert_eq!(scanned.errors, 0);
        assert_eq!(scanned.files.len(), 2);
        assert!(scanned.files.iter().all(|entry| entry.bytes > 0));

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

    #[test]
    fn recycle_entries_records_each_file() {
        let root = temp_root("records");
        fs::create_dir_all(&root).unwrap();
        let first = root.join("a.tmp");
        let second = root.join("b.tmp");
        fs::write(&first, vec![0u8; 10]).unwrap();
        fs::write(&second, vec![0u8; 20]).unwrap();

        let mut budget = 10;
        let records = recycle_entries(
            &[entry(first.clone(), 10), entry(second.clone(), 20)],
            &mut budget,
        );

        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| record.outcome == Outcome::Recycled));
        assert!(!first.exists());
        assert!(!second.exists());
        assert_eq!(budget, 8);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recycle_entries_skips_missing_and_readonly() {
        let root = temp_root("skip");
        fs::create_dir_all(&root).unwrap();
        let missing = root.join("already-gone.tmp");
        let readonly = root.join("readonly.tmp");
        fs::write(&readonly, b"data").unwrap();
        let mut permissions = fs::metadata(&readonly).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&readonly, permissions).unwrap();

        let mut budget = 10;
        let records = recycle_entries(
            &[entry(missing.clone(), 1), entry(readonly.clone(), 4)],
            &mut budget,
        );

        assert_eq!(records[0].outcome, Outcome::Missing);
        assert_eq!(records[1].outcome, Outcome::Locked);
        assert!(readonly.exists());
        assert_eq!(budget, 10, "skipped files must not consume the delete budget");

        let mut permissions = fs::metadata(&readonly).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&readonly, permissions).unwrap();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recycle_entries_marks_budget_overflow() {
        let root = temp_root("budget");
        fs::create_dir_all(&root).unwrap();
        let first = root.join("a.tmp");
        let second = root.join("b.tmp");
        fs::write(&first, b"a").unwrap();
        fs::write(&second, b"b").unwrap();

        let mut budget = 1;
        let records = recycle_entries(
            &[entry(first.clone(), 1), entry(second.clone(), 1)],
            &mut budget,
        );

        assert_eq!(records[0].outcome, Outcome::Recycled);
        assert_eq!(records[1].outcome, Outcome::Budget);
        assert!(second.exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn journal_payload_is_writable_json() {
        let root = temp_root("journal");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("SYSTEM_LENS_STATE_DIR", &root);
        let path = write_journal(&json!({"tool": "clean_master", "totals": {"files": 2}}))
            .expect("write journal");
        let text = fs::read_to_string(&path).expect("read journal");
        std::env::remove_var("SYSTEM_LENS_STATE_DIR");
        assert!(text.contains("\"clean_master\""));
        assert!(text.contains("\"files\": 2"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    #[ignore = "manual 1 GiB benchmark; moves files to recycle bin"]
    fn bench_clean_one_gib() {
        let root = temp_root("bench1g");
        fs::create_dir_all(&root).unwrap();
        let chunk = vec![0u8; 8 * 1024 * 1024];
        for index in 0..128 {
            fs::write(root.join(format!("f{index}.bin")), &chunk).unwrap();
        }
        let scanned = walk_candidate(&candidate(root.clone(), &[]), true);
        assert_eq!(scanned.files.len(), 128);
        assert_eq!(scanned.bytes, 1024 * 1024 * 1024);

        let started = std::time::Instant::now();
        let mut budget = MAX_DELETE_FILES;
        let records = recycle_entries(&scanned.files, &mut budget);
        let elapsed = started.elapsed();
        let recycled = records
            .iter()
            .filter(|record| record.outcome == Outcome::Recycled)
            .count();
        println!("1 GiB recycle: {elapsed:?}, recycled={recycled}");
        assert_eq!(recycled, 128);
        assert!(elapsed.as_secs() < 60, "1 GiB clean took {elapsed:?}");
        fs::remove_dir_all(&root).ok();
    }
}
