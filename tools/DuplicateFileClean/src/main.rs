use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};
use walkdir::WalkDir;

const ACTION: &str = "clean_group";
const MIN_SIZE: u64 = 1024 * 1024;
const MAX_FILES: usize = 20_000;
const SAMPLE_BYTES: usize = 64 * 1024;
const MAX_GROUPS: usize = 50;
const HASH_BUFFER: usize = 1024 * 1024;

fn user_dirs() -> Vec<PathBuf> {
    let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        "Downloads",
        "Documents",
        "Desktop",
        "Pictures",
        "Music",
        "Videos",
    ]
    .iter()
    .map(|name| profile.join(name))
    .filter(|path| path.is_dir())
    .collect()
}

/// 快速指纹：大小 + 首尾 64 KiB，仅用于扫描分组。
fn signature(path: &Path, size: u64) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunk = vec![0u8; SAMPLE_BYTES.min(size.max(1) as usize)];
    let read = file.read(&mut chunk).ok()?;
    for byte in &chunk[..read] {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    if size > SAMPLE_BYTES as u64 {
        if file.seek(SeekFrom::Start(size - SAMPLE_BYTES as u64)).is_ok() {
            let read = file.read(&mut chunk).ok()?;
            for byte in &chunk[..read] {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    Some(hash ^ size.rotate_left(32) ^ size)
}

/// 完整内容哈希，用于删除前的最终确认。
fn full_hash(path: &Path) -> Result<u64, String> {
    let mut file = File::open(path).map_err(|error| format!("无法读取：{error}"))?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buffer = vec![0u8; HASH_BUFFER];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取失败：{error}"))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    Ok(hash)
}

struct Group {
    id: String,
    size: u64,
    files: Vec<PathBuf>,
}

struct ScanResult {
    groups: Vec<Group>,
    scanned: usize,
    truncated: bool,
}

fn scan_groups() -> ScanResult {
    let directories = user_dirs();
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut scanned = 0usize;
    let mut truncated = false;

    'outer: for directory in &directories {
        for entry in WalkDir::new(directory).follow_links(false).into_iter() {
            let Ok(entry) = entry else {
                continue;
            };
            if !entry.file_type().is_file() {
                continue;
            }
            scanned += 1;
            if scanned > MAX_FILES {
                truncated = true;
                break 'outer;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() < MIN_SIZE {
                continue;
            }
            by_size.entry(metadata.len()).or_default().push(entry.into_path());
        }
    }

    let mut groups: Vec<Group> = Vec::new();
    for (size, files) in by_size {
        if files.len() < 2 {
            continue;
        }
        let mut by_signature: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for file in files {
            if let Some(hash) = signature(&file, size) {
                by_signature.entry(hash).or_default().push(file);
            }
        }
        for (_, mut duplicates) in by_signature {
            if duplicates.len() < 2 {
                continue;
            }
            duplicates.sort();
            let mut paths = duplicates
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>();
            paths.sort();
            let digest = digest_of(&paths.iter().map(String::as_str).collect::<Vec<_>>());
            groups.push(Group {
                id: digest[..16].to_string(),
                size,
                files: duplicates,
            });
        }
    }
    groups.sort_by(|left, right| {
        (right.size * (right.files.len() as u64 - 1))
            .cmp(&(left.size * (left.files.len() as u64 - 1)))
    });
    groups.truncate(MAX_GROUPS);
    ScanResult {
        groups,
        scanned,
        truncated,
    }
}

fn scan_digest(groups: &[Group]) -> String {
    let ids = groups
        .iter()
        .map(|group| group.id.clone())
        .collect::<Vec<_>>();
    digest_of(&ids.iter().map(String::as_str).collect::<Vec<_>>())
}

fn groups_json(groups: &[Group]) -> Value {
    Value::Array(
        groups
            .iter()
            .map(|group| {
                json!({
                    "id": group.id,
                    "sizeBytes": group.size,
                    "size": human_bytes(group.size),
                    "reclaimBytes": group.size * (group.files.len() as u64 - 1),
                    "reclaim": human_bytes(group.size * (group.files.len() as u64 - 1)),
                    "files": group.files.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
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

fn recycle_file(path: &Path) -> Result<(), String> {
    let mut from: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
    from.push(0);
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
    // SAFETY: 路径以双 NUL 结尾，结构体布局与 Shell API 一致。
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 {
        return Err(format!("回收站操作失败（0x{code:x}）"));
    }
    if operation.f_any_operations_aborted != 0 {
        return Err("回收站操作被中止".into());
    }
    Ok(())
}

fn journal_dir() -> PathBuf {
    tool_protocol::state_dir().join("duplicates")
}

fn write_journal(payload: &Value) -> Result<PathBuf, String> {
    let dir = journal_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建清单目录：{error}"))?;
    let path = dir.join(format!(
        "clean-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化清单：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入清单：{error}"))?;
    Ok(path)
}

fn preview() -> Result<Value, ToolError> {
    let scan = scan_groups();
    let token = tool_protocol::issue_token(ACTION, &scan_digest(&scan.groups))?;
    if scan.groups.is_empty() {
        let mut data = table(
            json!([
                item("扫描文件", scan.scanned.to_string()),
                item("重复组", "0"),
            ]),
            json!([]),
            json!([]),
            json!([
                "没有发现 1 MiB 以上的重复文件。",
                if scan.truncated {
                    "已达到 2 万文件上限，结果可能不完整。"
                } else {
                    "扫描范围为下载、文档、桌面、图片、音乐、视频六个目录。"
                },
            ]),
        );
        attach_token(&mut data, token);
        return Ok(data);
    }

    let mut rows = Vec::new();
    let mut reclaim = 0u64;
    for (index, group) in scan.groups.iter().enumerate() {
        let wasted = group.size * (group.files.len() as u64 - 1);
        reclaim = reclaim.saturating_add(wasted);
        rows.push(json!({
            "group": (index + 1).to_string(),
            "files": group.files.len().to_string(),
            "size": human_bytes(group.size),
            "reclaim": human_bytes(wasted),
            "sample": group.files.first().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        }));
    }

    let mut data = table(
        json!([
            item("扫描文件", scan.scanned.to_string()),
            item("重复组", scan.groups.len().to_string()),
            item("可释放", human_bytes(reclaim)),
        ]),
        json!([
            column("group", "组"),
            column("files", "文件数"),
            column("size", "单个大小"),
            column("reclaim", "可释放"),
            column("sample", "示例路径"),
        ]),
        Value::Array(rows),
        json!([
            "扫描只读取大小与首尾片段，不移动文件；分组为快速启发式，可能把不同文件归为一组。",
            "清理前会对保留文件与待删除文件做完整内容哈希复核，只有完全一致才会移入回收站。",
            "扫描范围为下载、文档、桌面、图片、音乐、视频六个目录；单个文件至少 1 MiB。",
            if scan.truncated {
                "已达到 2 万文件上限，结果可能不完整。".to_string()
            } else {
                "每组默认保留按路径排序的第一个文件，可在界面中改选。".to_string()
            },
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("duplicateFileGroups".into(), groups_json(&scan.groups));
    }
    attach_token(&mut data, token);
    Ok(data)
}

fn apply(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "清理重复文件需要确认令牌，请先预览并选择保留项。",
        )
    })?;
    let group_id = request
        .parameters
        .get("groupId")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 groupId。"))?;
    let keep_path = request
        .parameters
        .get("keepPath")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 keepPath。"))?;

    // 重新扫描并校验令牌，避免客户端传入任意路径。
    let scan = scan_groups();
    tool_protocol::verify_token(token, ACTION, &scan_digest(&scan.groups))?;
    let group = scan
        .groups
        .iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| ToolError::new("CONFLICT", "重复组已变化，请重新扫描。"))?;
    let keep = group
        .files
        .iter()
        .find(|path| path.to_string_lossy() == keep_path)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "保留文件不在该组中。"))?;
    let keep_hash = full_hash(keep).map_err(|error| {
        ToolError::new("INTERNAL", format!("无法校验保留文件：{error}"))
    })?;

    let started_at = tool_protocol::now_ms();
    let mut rows = Vec::new();
    let mut freed = 0u64;
    let mut recycled = 0usize;
    let mut mismatched = 0usize;
    let mut failed = 0usize;
    let mut records = Vec::new();

    for path in &group.files {
        if path == keep {
            continue;
        }
        let path_text = path.to_string_lossy().to_string();
        match full_hash(path) {
            Ok(hash) if hash != keep_hash => {
                mismatched += 1;
                rows.push(json!({
                    "file": path_text,
                    "size": human_bytes(group.size),
                    "status": "跳过：内容不一致",
                    "reason": "完整哈希与保留文件不同",
                }));
                records.push(json!({"path": path_text, "outcome": "mismatch"}));
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                failed += 1;
                rows.push(json!({
                    "file": path_text,
                    "size": human_bytes(group.size),
                    "status": "失败",
                    "reason": error,
                }));
                records.push(json!({"path": path_text, "outcome": "failed"}));
                continue;
            }
        }
        match recycle_file(path) {
            Ok(()) if !path.exists() => {
                freed = freed.saturating_add(group.size);
                recycled += 1;
                rows.push(json!({
                    "file": path_text,
                    "size": human_bytes(group.size),
                    "status": "已移入回收站",
                    "reason": "",
                }));
                records.push(json!({"path": path_text, "outcome": "recycled"}));
            }
            Ok(()) => {
                failed += 1;
                rows.push(json!({
                    "file": path_text,
                    "size": human_bytes(group.size),
                    "status": "失败",
                    "reason": "回收站操作返回成功，但文件仍存在",
                }));
                records.push(json!({"path": path_text, "outcome": "failed"}));
            }
            Err(error) => {
                failed += 1;
                rows.push(json!({
                    "file": path_text,
                    "size": human_bytes(group.size),
                    "status": "失败",
                    "reason": error,
                }));
                records.push(json!({"path": path_text, "outcome": "failed"}));
            }
        }
    }

    let finished_at = tool_protocol::now_ms();
    let journal = json!({
        "tool": "DuplicateFileClean",
        "version": env!("CARGO_PKG_VERSION"),
        "action": ACTION,
        "startedAt": started_at,
        "finishedAt": finished_at,
        "group": {
            "id": group.id,
            "size": group.size,
            "keep": keep_path,
            "files": group.files.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
        },
        "records": records,
        "freedBytes": freed,
    });
    let journal_path = write_journal(&journal);
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "DuplicateFileClean",
        "action": ACTION,
        "groupId": group_id,
        "keep": keep_path,
        "recycled": recycled,
        "mismatched": mismatched,
        "failed": failed,
        "freedBytes": freed,
        "journal": journal_path.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
        "at": finished_at,
    }));

    let mut data = table(
        json!([
            item("释放空间", human_bytes(freed)),
            item("移入回收站", format!("{recycled} 个文件")),
            item("内容不一致跳过", mismatched.to_string()),
            item("失败", failed.to_string()),
        ]),
        json!([
            column("file", "文件"),
            column("size", "大小"),
            column("status", "结果"),
            column("reason", "原因"),
        ]),
        Value::Array(rows),
        json!([
            "只有完整内容哈希与保留文件一致的文件才会被移入回收站。",
            "可在回收站按原路径还原；完整清单写入本地日志。",
            "清理后请重新预览，剩余重复组会重新计算。",
            journal_path
                .as_ref()
                .map(|path| format!("本次清单：{}", path.display()))
                .unwrap_or_else(|error| format!("清单写入失败：{error}")),
        ]),
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
        "DuplicateFileClean",
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
            "system-lens-dup-{name}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn full_hash_matches_identical_content() {
        let root = temp_root("hash");
        fs::create_dir_all(&root).unwrap();
        let first = root.join("a.bin");
        let second = root.join("b.bin");
        let other = root.join("c.bin");
        fs::write(&first, vec![7u8; 4096]).unwrap();
        fs::write(&second, vec![7u8; 4096]).unwrap();
        fs::write(&other, vec![8u8; 4096]).unwrap();
        assert_eq!(full_hash(&first).unwrap(), full_hash(&second).unwrap());
        assert_ne!(full_hash(&first).unwrap(), full_hash(&other).unwrap());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn group_id_is_stable_for_same_paths() {
        let mut paths = vec![
            "C:\\Users\\me\\Downloads\\a.bin".to_string(),
            "C:\\Users\\me\\Downloads\\b.bin".to_string(),
        ];
        let first = digest_of(&paths.iter().map(String::as_str).collect::<Vec<_>>());
        paths.reverse();
        let second = digest_of(&paths.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(first, second);
    }

    #[test]
    fn recycles_file_into_bin() {
        let root = temp_root("recycle");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("duplicate.bin");
        fs::write(&file, b"duplicate").unwrap();
        recycle_file(&file).expect("recycle");
        assert!(!file.exists());
        fs::remove_dir_all(&root).ok();
    }
}
