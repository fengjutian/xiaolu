//! 清理引擎：`cclean`（C 盘瘦身）与 `clean_master`（清理大师）共用的唯一实现。
//!
//! 这两个工具以前各自维护一套清理白名单与目录遍历器：同一位置在两个工具里会给出
//! 不同数字（`cclean` 统计整个资源管理器目录、阈值 15 万，`clean_master` 只匹配
//! `thumbcache_*`/`iconcache_*`、阈值 2.5 万），而且只有 `clean_master` 具备删除能力。
//! 现在白名单、扫描、占用检测、回收站删除与清单写入全部收敛到这个 crate，
//! 两个工具只保留各自的展示层 —— 同一位置在两个工具里的数字必然一致。
//!
//! 安全约束：
//! - 只清理固定白名单位置，不接受用户传入的任意路径。
//! - 目录联接/重解析点不跟随，避免通过挂载点逃出白名单目录。
//! - 删除走回收站（`SHFileOperationW` + `FOF_ALLOWUNDO`），可从回收站还原。

use serde_json::{json, Value};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use tool_protocol::{digest_of, human_bytes, ToolError};
use walkdir::WalkDir;

/// 单个位置最多统计的文件数，超出标记为已截断。
pub const MAX_SCAN_FILES: usize = 25_000;
/// 单次最多移入回收站的文件数。
pub const MAX_DELETE_FILES: usize = 10_000;
/// 预览时每个位置最多抽样检测多少个文件的占用情况。
pub const LOCK_SAMPLE: usize = 200;
/// 每批移入回收站的文件数。批量失败时再逐文件重试，隔离单个坏文件。
pub const RECYCLE_BATCH: usize = 256;
/// 结果表格最多展示的逐文件明细行数，完整清单写入本地日志。
pub const MAX_DETAIL_ROWS: usize = 500;

/// 一个清理位置。`cleanable` 为 false 的位置只统计，不参与删除。
#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: &'static str,
    pub label: &'static str,
    pub base: PathBuf,
    /// 为空表示整目录；否则只匹配指定前缀的文件（如 `thumbcache_`）。
    pub prefixes: &'static [&'static str],
    pub requires_admin: bool,
    pub note: &'static str,
    /// `safe` = 可放心清理；`caution` = 需确认或系统托管。
    pub category: &'static str,
    /// false = 系统托管，只统计不清理。
    pub cleanable: bool,
}

fn env_path(key: &str) -> PathBuf {
    std::env::var_os(key).map(PathBuf::from).unwrap_or_default()
}

/// Windows 目录。`WINDIR` 缺失时回退 `SystemRoot`。
fn windows_root() -> PathBuf {
    let root = env_path("WINDIR");
    if root.as_os_str().is_empty() {
        env_path("SystemRoot")
    } else {
        root
    }
}

/// 把盘符根拼成完整路径（`C:` → `C:\$Recycle.Bin`）。
pub fn drive_path(root: &Path, child: &str) -> PathBuf {
    let mut base = root.as_os_str().to_os_string();
    let text = base.to_string_lossy();
    if !text.ends_with('\\') && !text.ends_with('/') {
        base.push("\\");
    }
    PathBuf::from(base).join(child)
}

/// 固定清理位置白名单。两个工具共用这一份清单，工具不接受用户传入任意路径。
///
/// `cleanable = false` 的两项由系统托管，只统计占用：
/// - `recycle-bin`：回收站里的内容已经是删除过的文件，再"移入回收站"等于永久删除，
///   所以只报占用，由用户在回收站里手动清空。
/// - `inet-cache`：旧版 IE/Office 的网络缓存，清空可能让部分旧程序需要重新登录。
pub fn candidates() -> Vec<Candidate> {
    let local = env_path("LOCALAPPDATA");
    let windows = windows_root();
    let program_data = env_path("ProgramData");
    let system_drive = env_path("SystemDrive");
    let recycle_bin = if system_drive.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        drive_path(&system_drive, "$Recycle.Bin")
    };
    let mut list = vec![
        Candidate {
            id: "user-temp",
            label: "用户临时文件",
            base: env_path("TEMP"),
            prefixes: &[],
            requires_admin: false,
            note: "应用运行时仍会写入；正在使用的文件会跳过。",
            category: "safe",
            cleanable: true,
        },
        Candidate {
            id: "windows-temp",
            label: "Windows 临时文件",
            base: windows.join("Temp"),
            prefixes: &[],
            requires_admin: true,
            note: "系统级临时目录通常需要管理员权限。",
            category: "caution",
            cleanable: true,
        },
        Candidate {
            id: "thumbnail-cache",
            label: "缩略图与图标缓存",
            base: local.join("Microsoft").join("Windows").join("Explorer"),
            prefixes: &["thumbcache_", "iconcache_"],
            requires_admin: false,
            note: "资源管理器运行时可能占用；只匹配 thumbcache_* 与 iconcache_*。",
            category: "safe",
            cleanable: true,
        },
        Candidate {
            id: "crash-dumps",
            label: "用户崩溃转储",
            base: local.join("CrashDumps"),
            prefixes: &[],
            requires_admin: false,
            note: "保留转储便于蓝屏分析，确认不再需要后再清理。",
            category: "safe",
            cleanable: true,
        },
        Candidate {
            id: "wer-user",
            label: "用户错误报告",
            base: local.join("Microsoft").join("Windows").join("WER"),
            prefixes: &[],
            requires_admin: false,
            note: "Windows 错误报告缓存。",
            category: "safe",
            cleanable: true,
        },
        Candidate {
            id: "wer-machine",
            label: "系统错误报告",
            base: program_data.join("Microsoft").join("Windows").join("WER"),
            prefixes: &[],
            requires_admin: true,
            note: "机器级错误报告，通常需要管理员权限。",
            category: "caution",
            cleanable: true,
        },
        Candidate {
            id: "update-download",
            label: "Windows Update 下载缓存",
            base: windows.join("SoftwareDistribution").join("Download"),
            prefixes: &[],
            requires_admin: true,
            note: "需要管理员权限；正在安装更新时请勿清理。",
            category: "caution",
            cleanable: true,
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
            category: "caution",
            cleanable: true,
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
            category: "safe",
            cleanable: true,
        },
        Candidate {
            id: "inet-cache",
            label: "系统网络缓存",
            base: local.join("Microsoft").join("Windows").join("INetCache"),
            prefixes: &[],
            requires_admin: false,
            note: "旧版 IE 与部分桌面程序共用的网络缓存；清空可能让旧程序需要重新登录，仅统计不清理。",
            category: "caution",
            cleanable: false,
        },
        Candidate {
            id: "recycle-bin",
            label: "回收站",
            base: recycle_bin,
            prefixes: &[],
            requires_admin: true,
            note: "系统托管：内容已经是删除过的文件，只统计占用，需在回收站中手动清空。",
            category: "caution",
            cleanable: false,
        },
    ];
    list.retain(|candidate| !candidate.base.as_os_str().is_empty());
    list
}

/// 扫描到的一个文件。
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub bytes: u64,
}

/// 一次目录扫描的结果。
#[derive(Default, Debug)]
pub struct WalkResult {
    pub exists: bool,
    pub files: Vec<FileEntry>,
    pub sample: Vec<PathBuf>,
    pub bytes: u64,
    pub count: usize,
    pub errors: usize,
    pub truncated: bool,
}

/// 目录联接/重解析点不跟随，避免通过挂载点逃出白名单目录。
pub fn is_reparse_directory(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
}

/// 遍历一个清理位置。`collect_files` 为 true 时收集全部文件（删除前用），
/// 否则只统计并抽样收集文件路径（预览用）。
pub fn walk_candidate(candidate: &Candidate, collect_files: bool) -> WalkResult {
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
    .filter_entry(|entry| !entry.file_type().is_dir() || !is_reparse_directory(entry.path()));
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
pub fn is_locked(path: &Path) -> bool {
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
pub fn recycle_files(paths: &[PathBuf]) -> Result<(), String> {
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
pub enum Outcome {
    Recycled,
    Locked,
    Missing,
    Failed,
    Budget,
}

impl Outcome {
    pub fn key(self) -> &'static str {
        match self {
            Outcome::Recycled => "recycled",
            Outcome::Locked => "locked",
            Outcome::Missing => "missing",
            Outcome::Failed => "failed",
            Outcome::Budget => "budget",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Outcome::Recycled => "已移入回收站",
            Outcome::Locked => "跳过：占用/只读/无权限",
            Outcome::Missing => "跳过：已不存在",
            Outcome::Failed => "失败",
            Outcome::Budget => "跳过：超出单次上限",
        }
    }
}

/// 一个文件的处理留痕。
#[derive(Clone, Debug)]
pub struct FileRecord {
    pub entry: FileEntry,
    pub outcome: Outcome,
    pub reason: String,
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
pub fn recycle_entries(entries: &[FileEntry], budget: &mut usize) -> Vec<FileRecord> {
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

/// 预览一行的统一结果。两个工具都用它，所以同一位置的数字必然一致。
#[derive(Clone, Debug)]
pub struct PreviewRow {
    pub id: &'static str,
    pub label: &'static str,
    pub path: String,
    pub bytes: u64,
    pub files: usize,
    pub locked: usize,
    pub truncated: bool,
    pub status: &'static str,
    pub requires_admin: bool,
    pub note: &'static str,
    pub category: &'static str,
    pub cleanable: bool,
}

impl PreviewRow {
    /// 预览行的 JSON 形式（表格与详情面板共用同一份字段）。
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "path": self.path,
            "bytes": self.bytes,
            "size": human_bytes(self.bytes),
            "files": self.files,
            "locked": self.locked,
            "truncated": self.truncated,
            "status": self.status,
            "requiresAdmin": self.requires_admin,
            "note": self.note,
            "category": self.category,
            "cleanable": self.cleanable,
        })
    }
}

/// 扫描单个清理位置，得到预览行。
pub fn preview_row(candidate: &Candidate) -> PreviewRow {
    let scanned = walk_candidate(candidate, false);
    let locked = scanned
        .sample
        .iter()
        .filter(|path| is_locked(path))
        .count();
    let status = if !scanned.exists {
        "不存在"
    } else if scanned.count == 0 {
        "无文件"
    } else if scanned.errors > 0 && scanned.bytes == 0 {
        "需要管理员或无权限"
    } else {
        "可用"
    };
    PreviewRow {
        id: candidate.id,
        label: candidate.label,
        path: candidate.base.to_string_lossy().to_string(),
        bytes: scanned.bytes,
        files: scanned.count,
        locked,
        truncated: scanned.truncated,
        status,
        requires_admin: candidate.requires_admin,
        note: candidate.note,
        category: candidate.category,
        cleanable: candidate.cleanable,
    }
}

/// 扫描全部清理位置。两个工具的预览都调用它，因此结果逐行一致。
pub fn preview_rows() -> Vec<PreviewRow> {
    candidates()
        .iter()
        .map(preview_row)
        .collect()
}

/// 预览汇总：可清理空间、文件总数与抽样占用数。
pub fn totals(rows: &[PreviewRow]) -> (u64, usize, usize) {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut locked = 0usize;
    for row in rows {
        locked += row.locked;
        if row.status != "不存在" && row.status != "无文件" {
            bytes = bytes.saturating_add(row.bytes);
            files += row.files;
        }
    }
    (bytes, files, locked)
}

/// 分类汇总：`safe` 与 `caution` 两类的占用。
pub fn category_bytes(rows: &[PreviewRow]) -> (u64, u64) {
    let mut safe = 0u64;
    let mut caution = 0u64;
    for row in rows {
        if row.status == "不存在" || row.status == "无文件" {
            continue;
        }
        if row.category == "safe" {
            safe = safe.saturating_add(row.bytes);
        } else {
            caution = caution.saturating_add(row.bytes);
        }
    }
    (safe, caution)
}

/// 确认令牌绑定的对象摘要。摘要里带工具 id，
/// 这样两个工具的预览令牌互相不通用。
pub fn candidate_digest(tool_id: &str, list: &[Candidate]) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(list.len() + 1);
    parts.push(tool_id);
    parts.extend(list.iter().map(|candidate| candidate.id));
    digest_of(&parts)
}

/// 校验勾选的位置：必须在白名单内，且是支持清理的位置。
pub fn validate_selection(list: &[Candidate], selected: &[String]) -> Result<(), ToolError> {
    for candidate in list {
        if selected.iter().any(|id| id == candidate.id) && !candidate.cleanable {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("位置「{}」由系统托管，只统计不清理。", candidate.label),
            ));
        }
    }
    if let Some(unknown) = selected
        .iter()
        .find(|id| !list.iter().any(|candidate| candidate.id == id.as_str()))
    {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!("清理位置不在白名单中：{unknown}"),
        ));
    }
    Ok(())
}

/// 预览结果：统一表格 + 逐行明细。工具可以在表格之上叠加自己的展示信息。
pub struct Preview {
    pub data: Value,
    pub rows: Vec<PreviewRow>,
}

/// 只读预览。两个工具返回的表格部分完全一致。
pub fn preview(tool_id: &str, action: &str) -> Result<Value, ToolError> {
    scan_preview(tool_id, action).map(|preview| preview.data)
}

/// 只读预览，同时把逐行明细交给调用方，避免重复扫描磁盘。
pub fn scan_preview(tool_id: &str, action: &str) -> Result<Preview, ToolError> {
    let list = candidates();
    if list.is_empty() {
        return Err(ToolError::new("INTERNAL", "没有可用的清理位置。"));
    }
    let rows = preview_rows();
    let (total_bytes, total_files, locked_total) = totals(&rows);
    let token = tool_protocol::issue_token(action, &candidate_digest(tool_id, &list))?;

    let mut data = tool_protocol::table(
        json!([
            tool_protocol::item("候选位置", list.len().to_string()),
            tool_protocol::item("可清理空间", human_bytes(total_bytes)),
            tool_protocol::item("文件总数", total_files.to_string()),
            tool_protocol::item("占用/只读（抽样）", locked_total.to_string()),
        ]),
        json!([
            tool_protocol::column("label", "位置"),
            tool_protocol::column("path", "路径"),
            tool_protocol::column("size", "占用"),
            tool_protocol::column("files", "文件"),
            tool_protocol::column("locked", "占用/只读"),
            tool_protocol::column("status", "状态"),
        ]),
        Value::Array(rows.iter().map(|row| row.to_json()).collect()),
        json!([
            "只读扫描：预览不会删除任何文件；确认后所选文件会移入回收站，可从回收站还原。",
            "占用检测为抽样（每个位置最多 200 个文件，结果标注“占用/只读”）；执行时会逐个检测并跳过被占用、只读或无权限的文件。",
            "系统级目录（Windows 临时、更新缓存、传递优化等）通常需要管理员权限，普通权限下会记录失败或跳过。",
            "只清理固定白名单位置，不接受任意路径；单个位置最多统计 2.5 万个文件、单次最多删除 1 万个，超出部分标记为已截断。",
            "标为“仅统计”的位置（回收站、旧版网络缓存）由系统托管，不会被清理。",
        ]),
    );
    tool_protocol::attach_token(&mut data, token);
    Ok(Preview { data, rows })
}

/// 逐文件清单目录。
pub fn journal_dir() -> PathBuf {
    tool_protocol::state_dir().join("cleanup")
}

/// 写入完整逐文件清单。返回路径供结果页展示，失败不阻断清理。
pub fn write_journal(tool_id: &str, payload: &Value) -> Result<PathBuf, String> {
    let dir = journal_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建清单目录：{error}"))?;
    let path = dir.join(format!(
        "{tool_id}-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text =
        serde_json::to_string_pretty(payload).map_err(|error| format!("无法序列化清理清单：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入清理清单：{error}"))?;
    Ok(path)
}

/// 清理前后对比的一行。
///
/// 用户要看到的是"这个位置清理前有多少、清理后还剩多少"，
/// 而不是只有逐文件明细与一个合计数。
fn diff_row(
    candidate: &Candidate,
    before_files: usize,
    before_bytes: u64,
    after_files: usize,
    after_bytes: u64,
) -> Value {
    let freed = before_bytes.saturating_sub(after_bytes);
    let status = if before_files == 0 && after_files == 0 {
        "目录不存在"
    } else if freed > 0 {
        "已清理"
    } else {
        "未变化"
    };
    json!({
        "id": candidate.id,
        "label": candidate.label,
        "path": candidate.base.to_string_lossy(),
        "beforeFiles": before_files,
        "beforeBytes": before_bytes,
        "beforeSize": human_bytes(before_bytes),
        "afterFiles": after_files,
        "afterBytes": after_bytes,
        "afterSize": human_bytes(after_bytes),
        "freedBytes": freed,
        "freedSize": human_bytes(freed),
        "status": status,
    })
}

/// 执行清理：校验令牌 → 逐位置收集文件 → 移入回收站 → 复核 → 写清单与审计日志。
pub fn apply(
    tool_id: &str,
    action: &str,
    request: &tool_protocol::Request,
) -> Result<Value, ToolError> {
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
        .collect::<Vec<_>>();

    let list = candidates();
    tool_protocol::verify_token(token, action, &candidate_digest(tool_id, &list))?;
    validate_selection(&list, &selected)?;

    let started_at = tool_protocol::now_ms();
    let mut rows = Vec::new();
    let mut journal_candidates = Vec::new();
    let mut diff: Vec<Value> = Vec::new();
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
        if !selected.iter().any(|id| id == candidate.id) {
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
            diff.push(diff_row(candidate, 0, 0, 0, 0));
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
        diff.push(diff_row(
            candidate,
            before.count,
            before.bytes,
            after.count,
            after.bytes,
        ));
    }

    let journal = json!({
        "tool": tool_id,
        "version": env!("CARGO_PKG_VERSION"),
        "action": action,
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
    let journal_path = write_journal(tool_id, &journal);

    let _ = tool_protocol::append_audit_log(json!({
        "tool": tool_id,
        "action": action,
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
        "「清理前后对比」按位置给出清理前后的占用与文件数，可直接核对释放量。".to_string(),
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
        tool_protocol::item("释放空间", human_bytes(freed_bytes)),
        tool_protocol::item("移入回收站", format!("{recycled_files} 个文件")),
        tool_protocol::item("跳过占用/只读", skipped_locked.to_string()),
        tool_protocol::item("失败", failed_files.to_string()),
        tool_protocol::item("超出上限", budget_skipped.to_string()),
        tool_protocol::item("复核减少", human_bytes(verified_freed)),
    ]);

    let mut data = tool_protocol::table(
        summary,
        json!([
            tool_protocol::column("label", "位置"),
            tool_protocol::column("file", "文件"),
            tool_protocol::column("size", "大小"),
            tool_protocol::column("status", "结果"),
            tool_protocol::column("reason", "原因"),
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
        // 执行前后对比：按位置给出清理前后占用与文件数。
        map.insert("diff".into(), Value::Array(diff));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 暂存目录放在测试二进制旁边，而不是系统临时目录：
    /// 受限沙箱/CI 会拒绝写 %TEMP%，放在那里整套测试会假红。
    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let base = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let dir = base.join(format!(
            "system-lens-clean-core-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).ok();
        dir
    }

    fn candidate(base: PathBuf, prefixes: &'static [&'static str]) -> Candidate {
        Candidate {
            id: "test",
            label: "测试",
            base,
            prefixes,
            requires_admin: false,
            note: "",
            category: "safe",
            cleanable: true,
        }
    }

    fn entry(path: PathBuf, bytes: u64) -> FileEntry {
        FileEntry { path, bytes }
    }

    #[test]
    fn candidate_ids_are_unique_and_absolute() {
        let list = candidates();
        let mut ids = list.iter().map(|c| c.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), list.len());
        assert!(list.iter().all(|c| c.base.is_absolute()));
    }

    #[test]
    fn cleanable_set_is_explicitly_bounded() {
        // 删除面必须显式列出：新增可清理位置等于扩大删除范围，需要走评审。
        let mut cleanable = candidates()
            .iter()
            .filter(|c| c.cleanable)
            .map(|c| c.id)
            .collect::<Vec<_>>();
        cleanable.sort_unstable();
        let mut expected = vec![
            "crash-dumps",
            "delivery-optimization",
            "edge-cache",
            "thumbnail-cache",
            "update-download",
            "user-temp",
            "wer-machine",
            "wer-user",
            "windows-temp",
        ];
        expected.sort_unstable();
        assert_eq!(cleanable, expected);
    }

    #[test]
    fn stat_only_candidates_are_rejected_by_apply() {
        let list = candidates();
        let stat_only = list
            .iter()
            .find(|c| !c.cleanable)
            .expect("stat only candidate");
        let selected = vec![stat_only.id.to_string()];
        let error = validate_selection(&list, &selected).expect_err("must reject");
        assert_eq!(error.code, "INVALID_INPUT");
        assert!(error.message.contains("只统计"));
    }

    #[test]
    fn unknown_candidate_is_rejected() {
        let list = candidates();
        let error = validate_selection(&list, &["no-such-place".to_string()]).expect_err("must reject");
        assert_eq!(error.code, "INVALID_INPUT");
        assert!(error.message.contains("不在白名单中"));
    }

    #[test]
    fn digest_is_bound_to_tool_id() {
        let list = candidates();
        assert_ne!(
            candidate_digest("cclean", &list),
            candidate_digest("clean_master", &list)
        );
        assert_eq!(candidate_digest("cclean", &list), candidate_digest("cclean", &list));
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
    fn preview_row_reports_same_numbers_as_walk() {
        let root = temp_root("preview");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.tmp"), vec![0u8; 64]).unwrap();
        fs::write(root.join("b.tmp"), vec![0u8; 128]).unwrap();

        let target = candidate(root.clone(), &[]);
        let scanned = walk_candidate(&target, false);
        let row = preview_row(&target);
        assert_eq!(row.bytes, scanned.bytes);
        assert_eq!(row.files, scanned.count);
        assert_eq!(row.status, "可用");
        assert!(row.cleanable);

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
        std::env::set_var("SYSTEM_LENS_STATE_DIR", &root);
        let path = write_journal("clean-core-test", &json!({"tool": "clean-core-test", "totals": {"files": 2}}))
            .expect("write journal");
        let text = fs::read_to_string(&path).expect("read journal");
        std::env::remove_var("SYSTEM_LENS_STATE_DIR");
        assert!(text.contains("\"clean-core-test\""));
        assert!(text.contains("\"files\": 2"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn diff_row_reports_before_after_and_freed_space() {
        let root = temp_root("diff");
        let target = candidate(root.clone(), &[]);

        let cleaned = diff_row(&target, 10, 1000, 2, 200);
        assert_eq!(cleaned["status"], "已清理");
        assert_eq!(cleaned["beforeFiles"], 10);
        assert_eq!(cleaned["afterFiles"], 2);
        assert_eq!(cleaned["freedBytes"], 800);
        assert_eq!(cleaned["id"], "test");

        let unchanged = diff_row(&target, 5, 500, 5, 500);
        assert_eq!(unchanged["status"], "未变化");
        assert_eq!(unchanged["freedBytes"], 0);

        let missing = diff_row(&target, 0, 0, 0, 0);
        assert_eq!(missing["status"], "目录不存在");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn preview_rows_are_shared_by_every_tool() {
        // 两个工具都调用 preview_rows()，因此同一位置的数字必然一致。
        let first = preview_rows();
        let second = preview_rows();
        assert_eq!(first.len(), second.len());
        for (left, right) in first.iter().zip(second.iter()) {
            assert_eq!(left.id, right.id);
            assert_eq!(left.bytes, right.bytes);
            assert_eq!(left.files, right.files);
            assert_eq!(left.status, right.status);
            assert_eq!(left.cleanable, right.cleanable);
        }
    }
}
