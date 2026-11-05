use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tool_protocol::{column, human_bytes, item, table};
use walkdir::WalkDir;

const MIN_SIZE: u64 = 1024 * 1024;
const MAX_FILES: usize = 20_000;
const SAMPLE_BYTES: usize = 64 * 1024;
const MAX_GROUPS: usize = 50;

fn user_dirs() -> Vec<PathBuf> {
    let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };
    ["Downloads", "Documents", "Desktop"]
        .iter()
        .map(|name| profile.join(name))
        .filter(|path| path.is_dir())
        .collect()
}

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

fn scan() -> Value {
    let directories = user_dirs();
    if directories.is_empty() {
        return table(
            json!([item("状态", "不可用")]),
            json!([]),
            json!([]),
            json!(["无法定位用户目录，未执行扫描。"]),
        );
    }

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

    let mut groups: Vec<(u64, Vec<PathBuf>)> = Vec::new();
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
            if duplicates.len() >= 2 {
                duplicates.sort();
                groups.push((size, duplicates));
            }
        }
    }
    groups.sort_by(|left, right| {
        (right.0 * (right.1.len() as u64 - 1)).cmp(&(left.0 * (left.1.len() as u64 - 1)))
    });

    let mut reclaim = 0u64;
    let mut rows = Vec::new();
    for (index, (size, files)) in groups.iter().take(MAX_GROUPS).enumerate() {
        let wasted = size * (files.len() as u64 - 1);
        reclaim = reclaim.saturating_add(wasted);
        rows.push(json!({
            "group": (index + 1).to_string(),
            "files": files.len().to_string(),
            "size": human_bytes(*size),
            "reclaim": human_bytes(wasted),
            "sample": files.first().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        }));
    }

    table(
        json!([
            item("扫描文件", scanned.to_string()),
            item("重复组", groups.len().to_string()),
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
            "只读预览：只读取文件大小与首尾片段计算指纹，不删除、不移动文件。",
            "指纹为快速启发式（大小 + 首尾 64 KiB），偶尔可能把不同文件归为一组；清理前需逐组确认。",
            if truncated {
                "已达到 2 万文件上限，结果可能不完整。".to_string()
            } else {
                "扫描范围为下载、文档、桌面三个目录。".to_string()
            },
        ]),
    )
}

fn main() {
    tool_protocol::serve(
        "DuplicateFileClean",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| Ok(scan()),
    );
}
