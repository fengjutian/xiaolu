//! C 盘瘦身：`clean_core` 清理引擎的 C 盘视图。
//!
//! 清理白名单、扫描器与删除能力与 `clean_master` 完全一致（同属 `clean_core`），
//! 两者对同一位置给出的占用/文件数必然一致。
//! 本工具额外给出 C 盘容量、分类占用与系统托管文件（pagefile/hiberfil/swapfile）。

use serde_json::{json, Value};
use std::path::PathBuf;
use tool_protocol::{human_bytes, ToolError};

const TOOL_ID: &str = "cclean";
const ACTION: &str = "apply";

fn main() {
    tool_protocol::serve_with_request(
        TOOL_ID,
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => clean_core::apply(TOOL_ID, ACTION, request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

/// 系统盘盘符。缺失时按 C 盘处理。
fn system_drive() -> PathBuf {
    let drive = std::env::var_os("SystemDrive")
        .map(PathBuf::from)
        .unwrap_or_default();
    if drive.as_os_str().is_empty() {
        PathBuf::from("C:")
    } else {
        drive
    }
}

/// 系统托管的大文件：只统计，不清理。
fn system_files() -> Vec<Value> {
    let drive = system_drive();
    let mut items = Vec::new();
    for (name, label, note) in [
        (
            "pagefile.sys",
            "页面文件 pagefile.sys",
            "由系统管理；可在“高级系统设置”中调整大小或移动到其他盘。",
        ),
        (
            "hiberfil.sys",
            "休眠文件 hiberfil.sys",
            "由系统管理；可在电源设置中关闭休眠以释放空间。",
        ),
        (
            "swapfile.sys",
            "交换文件 swapfile.sys",
            "由系统管理，不建议直接删除。",
        ),
    ] {
        let path = clean_core::drive_path(&drive, name);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        items.push(json!({
            "id": format!("system-{name}"),
            "label": label,
            "path": path.to_string_lossy(),
            "bytes": metadata.len(),
            "size": human_bytes(metadata.len()),
            "fileCount": 1,
            "locked": 0,
            "truncated": false,
            "status": "系统托管",
            "category": "system",
            "requiresAdmin": false,
            "cleanable": false,
            "note": note,
        }));
    }
    items
}

/// 系统盘容量（总量 / 可用）。
fn drive_space() -> (u64, u64) {
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
    (total, free)
}

/// 只读预览：共享表格 + C 盘视图的附加信息。
fn preview() -> Result<Value, ToolError> {
    let preview = clean_core::scan_preview(TOOL_ID, ACTION)?;
    let (safe_bytes, caution_bytes) = clean_core::category_bytes(&preview.rows);
    let system_items = system_files();
    let system_count = system_items.len();
    let system_bytes = system_items
        .iter()
        .filter_map(|item| item["bytes"].as_u64())
        .sum::<u64>();
    let mut items: Vec<Value> = preview
        .rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "label": row.label,
                "path": row.path,
                "bytes": row.bytes,
                "size": human_bytes(row.bytes),
                "fileCount": row.files,
                "locked": row.locked,
                "truncated": row.truncated,
                "status": row.status,
                "category": row.category,
                "requiresAdmin": row.requires_admin,
                "cleanable": row.cleanable,
                "note": row.note,
            })
        })
        .collect();
    items.extend(system_items);
    let file_count =
        preview.rows.iter().map(|row| row.files).sum::<usize>() + system_count;
    let (total, free) = drive_space();

    let mut data = preview.data;
    if let Value::Object(map) = &mut data {
        map.insert("capturedAtUnixMs".into(), json!(tool_protocol::now_ms()));
        map.insert("driveLetter".into(), json!("C:"));
        map.insert("driveTotalBytes".into(), json!(total));
        map.insert("driveFreeBytes".into(), json!(free));
        map.insert("items".into(), Value::Array(items));
        map.insert("safeBytes".into(), json!(safe_bytes));
        map.insert("cautionBytes".into(), json!(caution_bytes));
        map.insert("systemBytes".into(), json!(system_bytes));
        map.insert("fileCount".into(), json!(file_count));
        map.insert(
            "note".into(),
            json!("扫描只统计占用，不会删除任何文件；勾选位置并确认后，文件会移入回收站，可从回收站还原。"),
        );
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn manifest() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tool.json");
        let text = fs::read_to_string(path).expect("read tool.json");
        serde_json::from_str(&text).expect("parse tool.json")
    }

    #[test]
    fn manifest_matches_tool_identity() {
        let manifest = manifest();
        assert_eq!(manifest["id"].as_str(), Some(TOOL_ID));
        assert_eq!(
            manifest["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn manifest_declares_served_actions() {
        let manifest = manifest();
        let actions = manifest["actions"].as_array().expect("actions");
        let preview = actions
            .iter()
            .find(|action| action["id"] == "preview")
            .expect("preview action");
        assert_eq!(preview["write"], false);
        let apply = actions
            .iter()
            .find(|action| action["id"] == ACTION)
            .expect("apply action");
        assert_eq!(apply["write"], true);
        assert_eq!(apply["requiresConfirmation"], true);
        assert_eq!(manifest["risk"], "write");
    }

    #[test]
    fn cclean_items_match_the_shared_engine_rows() {
        // C 盘视图与清理大师共用同一份扫描结果：同一位置的数字必须一致。
        // 预览要签发确认令牌，所以测试需要一份可用密钥。
        std::env::set_var(
            "SYSTEM_LENS_TOKEN_SECRET",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        );
        let preview = clean_core::scan_preview(TOOL_ID, ACTION).expect("preview");
        std::env::remove_var("SYSTEM_LENS_TOKEN_SECRET");
        let items = preview.data["items"].as_array().expect("items");
        let shared = preview
            .rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "label": row.label,
                    "path": row.path,
                    "bytes": row.bytes,
                    "size": human_bytes(row.bytes),
                    "fileCount": row.files,
                    "locked": row.locked,
                    "truncated": row.truncated,
                    "status": row.status,
                    "category": row.category,
                    "requiresAdmin": row.requires_admin,
                    "cleanable": row.cleanable,
                    "note": row.note,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(&items[..shared.len()], &shared[..]);
    }
}
