use serde_json::{json, Value};
use std::env;
use std::path::{Path, PathBuf};
use tool_protocol::{column, human_bytes, item, table};
use walkdir::WalkDir;

const MAX_FILES: usize = 50_000;

struct Candidate {
    label: &'static str,
    path: Option<PathBuf>,
    note: &'static str,
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn candidates() -> Vec<Candidate> {
    let local = env_path("LOCALAPPDATA");
    let roaming = env_path("APPDATA");
    let edge = local
        .as_ref()
        .map(|root| root.join("Microsoft").join("Edge").join("User Data").join("Default"));
    let chrome = local
        .as_ref()
        .map(|root| root.join("Google").join("Chrome").join("User Data").join("Default"));
    vec![
        Candidate {
            label: "Edge 浏览记录",
            path: edge.as_ref().map(|root| root.join("History")),
            note: "仅统计数据库文件大小，不读取具体访问记录。",
        },
        Candidate {
            label: "Edge Cookie",
            path: edge.as_ref().map(|root| root.join("Cookies")),
            note: "仅统计大小，不解析内容。",
        },
        Candidate {
            label: "Edge 缓存",
            path: edge.as_ref().map(|root| root.join("Cache")),
            note: "浏览器运行时可能被占用。",
        },
        Candidate {
            label: "Chrome 浏览记录",
            path: chrome.as_ref().map(|root| root.join("History")),
            note: "仅统计数据库文件大小。",
        },
        Candidate {
            label: "Chrome 缓存",
            path: chrome.as_ref().map(|root| root.join("Cache")),
            note: "浏览器运行时可能被占用。",
        },
        Candidate {
            label: "最近使用文件",
            path: roaming
                .as_ref()
                .map(|root| root.join("Microsoft").join("Windows").join("Recent")),
            note: "快捷方式与跳转列表，删除会清空“最近使用”。",
        },
        Candidate {
            label: "跳转列表",
            path: roaming.as_ref().map(|root| {
                root.join("Microsoft")
                    .join("Windows")
                    .join("Recent")
                    .join("AutomaticDestinations")
            }),
            note: "任务栏右键跳转记录。",
        },
        Candidate {
            label: "Windows 时间线缓存",
            path: local
                .as_ref()
                .map(|root| root.join("ConnectedDevicesPlatform")),
            note: "仅统计大小。",
        },
    ]
}

fn path_stats(path: &Path) -> (u64, usize, bool) {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.is_file() {
            return (metadata.len(), 1, false);
        }
    }
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut truncated = false;
    for entry in WalkDir::new(path).follow_links(false).into_iter() {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        if files > MAX_FILES {
            truncated = true;
            break;
        }
        if let Ok(metadata) = entry.metadata() {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    (bytes, files, truncated)
}

fn scan() -> Value {
    let mut rows = Vec::new();
    let mut total = 0u64;
    let mut found = 0usize;
    for candidate in candidates() {
        let Some(path) = candidate.path else {
            continue;
        };
        if !path.exists() {
            continue;
        }
        let (bytes, files, truncated) = path_stats(&path);
        if bytes == 0 && files == 0 {
            continue;
        }
        found += 1;
        total = total.saturating_add(bytes);
        rows.push(json!({
            "label": candidate.label,
            "path": path.to_string_lossy(),
            "size": human_bytes(bytes),
            "files": files.to_string(),
            "truncated": if truncated { "是" } else { "否" },
            "note": candidate.note,
        }));
    }
    table(
        json!([
            item("发现痕迹位置", found.to_string()),
            item("合计占用", human_bytes(total)),
        ]),
        json!([
            column("label", "位置"),
            column("path", "路径"),
            column("size", "占用"),
            column("files", "文件数"),
            column("truncated", "已截断"),
            column("note", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "只读统计：不读取、不上传、不删除任何浏览记录或 Cookie。",
            "清理能力需后续版本提供逐项确认与回收站/恢复点。",
            "浏览器运行时部分文件会被占用，统计值可能偏小。",
        ]),
    )
}

fn main() {
    tool_protocol::serve(
        "privacy_clean",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| Ok(scan()),
    );
}
