use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAX_FILES: usize = 150_000;
const MAX_RECURSION_DEPTH: usize = 64;

struct Target {
    id: &'static str,
    label: &'static str,
    path: Option<PathBuf>,
    requires_admin: bool,
    note: &'static str,
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
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
            path: windir
                .as_ref()
                .map(|root| root.join("SoftwareDistribution").join("Download")),
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

fn drive_path(root: &Path, child: &str) -> PathBuf {
    let mut base = root.as_os_str().to_os_string();
    let text = base.to_string_lossy();
    if !text.ends_with('\\') && !text.ends_with('/') {
        base.push("\\");
    }
    PathBuf::from(base).join(child)
}

fn category_for(id: &str) -> &'static str {
    match id {
        "user-temp" | "thumbnail-cache" | "edge-cache" | "inet-cache" | "crash-dumps"
        | "error-reporting" => "safe",
        _ => "caution",
    }
}

fn directory_stats(root: &Path) -> (u64, usize, usize, bool) {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut errors = 0usize;
    let mut truncated = false;
    for entry in WalkDir::new(root)
        .follow_links(false)
        .max_depth(MAX_RECURSION_DEPTH)
        .into_iter()
    {
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
        if files > MAX_FILES {
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

fn system_file(path: &str, label: &str, note: &str) -> Option<Value> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(json!({
        "id": format!("system-{label}"),
        "label": label,
        "path": path,
        "bytes": metadata.len(),
        "fileCount": 1,
        "category": "system",
        "requiresAdmin": false,
        "truncated": false,
        "note": note,
    }))
}

fn scan() -> Value {
    let mut items: Vec<Value> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for target in targets() {
        let Some(path) = target.path else {
            continue;
        };
        if !path.is_dir() {
            continue;
        }
        let (bytes, file_count, error_count, truncated) = directory_stats(&path);
        if bytes == 0 {
            continue;
        }
        seen.insert(target.id.to_string(), ());
        items.push(json!({
            "id": target.id,
            "label": target.label,
            "path": path.to_string_lossy(),
            "bytes": bytes,
            "fileCount": file_count,
            "errorCount": error_count,
            "category": category_for(target.id),
            "requiresAdmin": target.requires_admin,
            "truncated": truncated,
            "note": target.note,
        }));
    }

    for (path, label, note) in [
        (
            r"C:\pagefile.sys",
            "页面文件 pagefile.sys",
            "由系统管理；可在“高级系统设置”中调整大小或移动到其他盘。",
        ),
        (
            r"C:\hiberfil.sys",
            "休眠文件 hiberfil.sys",
            "由系统管理；可在电源设置中关闭休眠以释放空间。",
        ),
        (
            r"C:\swapfile.sys",
            "交换文件 swapfile.sys",
            "由系统管理，不建议直接删除。",
        ),
    ] {
        if let Some(item) = system_file(path, label, note) {
            items.push(item);
        }
    }

    items.sort_by(|left, right| {
        right["bytes"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&left["bytes"].as_u64().unwrap_or(0))
    });

    let sum = |category: &str| -> u64 {
        items
            .iter()
            .filter(|item| item["category"] == category)
            .map(|item| item["bytes"].as_u64().unwrap_or(0))
            .sum()
    };
    let file_count: u64 = items
        .iter()
        .map(|item| item["fileCount"].as_u64().unwrap_or(0))
        .sum();

    let mut total = 0u64;
    let mut free = 0u64;
    if let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() {
        if let Some(disk) = disks.iter().find(|disk| {
            disk.mount_point()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("c:")
        }) {
            total = disk.total_space();
            free = disk.available_space();
        }
    }

    json!({
        "capturedAtUnixMs": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        "driveLetter": "C:",
        "driveTotalBytes": total,
        "driveFreeBytes": free,
        "items": items,
        "safeBytes": sum("safe"),
        "cautionBytes": sum("caution"),
        "systemBytes": sum("system"),
        "fileCount": file_count,
        "note": "只读预览：只统计占用，不删除任何文件；删除能力将在后续版本按项确认后提供。",
        "scannedTargets": seen.len(),
    })
}

fn write_message(out: &mut impl Write, message: Value) {
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn main() {
    if !env::args().any(|argument| argument == "--stdio") {
        eprintln!("cclean tool: 仅支持由 System Lens 以 --stdio 启动");
        std::process::exit(2);
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    write_message(
        &mut stdout,
        json!({
            "type": "hello",
            "protocol": 1,
            "toolId": "cclean",
            "version": env!("CARGO_PKG_VERSION"),
            "actions": ["preview"],
            "elevated": false
        }),
    );

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                write_message(
                    &mut stdout,
                    json!({"type":"error","id":0,"code":"INVALID_INPUT","message":"请求不是合法 JSON"}),
                );
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(json!(0));
        let action = request
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match action {
            "preview" => {
                write_message(
                    &mut stdout,
                    json!({"type":"result","id":id,"ok":true,"data":scan(),"warnings":[]}),
                );
            }
            "apply" => {
                write_message(
                    &mut stdout,
                    json!({
                        "type":"error","id":id,"code":"UNSUPPORTED_ACTION",
                        "message":"当前版本仅提供只读预览，不执行删除。",
                        "remediation":"等待后续版本提供删除能力。"
                    }),
                );
            }
            _ => {
                write_message(
                    &mut stdout,
                    json!({"type":"error","id":id,"code":"INVALID_INPUT","message":"不支持的动作"}),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_candidates() {
        assert_eq!(category_for("user-temp"), "safe");
        assert_eq!(category_for("edge-cache"), "safe");
        assert_eq!(category_for("windows-update"), "caution");
        assert_eq!(category_for("recycle-bin"), "caution");
    }

    #[test]
    fn drive_path_normalizes_root() {
        assert_eq!(
            drive_path(Path::new("C:"), "$Recycle.Bin"),
            PathBuf::from("C:\\$Recycle.Bin")
        );
    }

    #[test]
    fn directory_stats_counts_without_content() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "cclean-tool-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
        std::fs::create_dir_all(root.join("nested")).unwrap();        std::fs::write(root.join("a.tmp"), vec![0u8; 1024]).unwrap();
        std::fs::write(root.join("nested").join("b.tmp"), vec![0u8; 2048]).unwrap();
        let (bytes, files, errors, truncated) = directory_stats(&root);
        assert_eq!(bytes, 3072);
        assert_eq!(files, 2);
        assert_eq!(errors, 0);
        assert!(!truncated);
        std::fs::remove_dir_all(&root).ok();
    }
}
