use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_OPTIMIZE: &str = "optimize_disk";

fn elevated_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("SystemLens")
        .join("elevated")
}

fn helper_path() -> Result<PathBuf, ToolError> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法定位工具路径：{error}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    let candidates = [
        dir.join("lens_helper.exe"),
        dir.join("..").join("lens_helper").join("lens_helper.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            ToolError::new(
                "HELPER_MISSING",
                "缺少提权助手 lens_helper.exe，请重新打包或安装。",
            )
        })
}

fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn run_elevated(mut payload: Value) -> Result<Value, ToolError> {
    let dir = elevated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建提权请求目录：{error}")))?;
    let nonce_source = format!("{}-{}", tool_protocol::now_ms(), std::process::id());
    let nonce_full = tool_protocol::digest_of(&[&nonce_source]);
    let nonce = &nonce_full[..32];
    let request_path = dir.join(format!("req-{nonce}.json"));
    let result_path = dir.join(format!("res-{nonce}.json"));
    if let Value::Object(map) = &mut payload {
        map.insert("version".into(), json!(1));
        map.insert("nonce".into(), json!(nonce));
        map.insert("resultPath".into(), json!(result_path));
    }
    let serialized = serde_json::to_string(&payload)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化提权请求：{error}")))?;
    std::fs::write(&request_path, serialized)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入提权请求：{error}")))?;

    let helper = helper_path()?;
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath {} -ArgumentList ('--request',('\"' + {} + '\"')) -Verb RunAs -PassThru -Wait -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_quote(&helper.to_string_lossy()),
        ps_quote(&request_path.to_string_lossy())
    );
    let status = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;

    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);

    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new(
                "ELEVATION_CANCELLED",
                "已取消管理员授权，未执行磁盘操作。",
            ));
        }
        return Err(ToolError::new(
            "ELEVATION_FAILED",
            format!("提权助手执行失败（退出码 {:?}）。", status.code()),
        ));
    }
    let text = result_text
        .ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

#[derive(Clone)]
struct Volume {
    drive: String,
    media: String,
    size: u64,
    free: u64,
}

fn media_kind(media: &str) -> &'static str {
    if media.eq_ignore_ascii_case("SSD") {
        "SSD"
    } else if media.eq_ignore_ascii_case("HDD") {
        "HDD"
    } else if media.is_empty() {
        "未知"
    } else {
        "其他"
    }
}

fn volumes_from(payload: &Value) -> Vec<Volume> {
    let mut volumes = Vec::new();
    if let Some(entries) = payload.get("rows").and_then(Value::as_array) {
        for entry in entries {
            if entry.get("kind").and_then(Value::as_str) != Some("卷") {
                continue;
            }
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
            let Some(drive) = name.chars().next() else {
                continue;
            };
            if !drive.is_ascii_alphabetic() {
                continue;
            }
            volumes.push(Volume {
                drive: drive.to_ascii_uppercase().to_string(),
                media: entry
                    .get("media")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                size: entry.get("size").and_then(Value::as_u64).unwrap_or(0),
                free: entry.get("free").and_then(Value::as_u64).unwrap_or(0),
            });
        }
    }
    volumes
}

fn volume_digest(volumes: &[Volume]) -> String {
    let parts = volumes
        .iter()
        .map(|volume| format!("{}|{}", volume.drive, media_kind(&volume.media)))
        .collect::<Vec<_>>();
    digest_of(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn scan_payload() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$rows=@()
Get-Volume -ErrorAction SilentlyContinue | Where-Object {$_.DriveType -eq 'Fixed' -and $_.DriveLetter} | ForEach-Object {
  $v=$_
  $part=Get-Partition -DriveLetter $v.DriveLetter -ErrorAction SilentlyContinue
  $pd=if($part){ Get-PhysicalDisk -DeviceNumber $part.DiskNumber -ErrorAction SilentlyContinue } else { $null }
  $rows += @{
    kind='卷'; name=("{0}:" -f $v.DriveLetter); fs=[string]$v.FileSystemType;
    size=[int64]$v.Size; free=[int64]$v.SizeRemaining;
    media=if($pd -and $pd.MediaType){[string]$pd.MediaType}else{'未知'}
  }
}
Get-PhysicalDisk -ErrorAction SilentlyContinue | ForEach-Object {
  $rows += @{kind='物理磁盘'; name=[string]$_.FriendlyName; fs=''; size=[int64]$_.Size; free=0; media=[string]$_.MediaType}
}
$trim = (fsutil behavior query DisableDeleteNotify 2>$null) -join ' '
@{rows=@($rows); trim=$trim} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取卷信息：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析卷信息：{error}")))
}

fn scan() -> Result<Value, ToolError> {
    let payload = scan_payload()?;
    let trim_text = payload.get("trim").and_then(Value::as_str).unwrap_or("");
    let trim_enabled = trim_text.contains("DisableDeleteNotify = 0");
    let trim_known = trim_text.contains("DisableDeleteNotify");
    let volumes = volumes_from(&payload);
    let ssd = volumes
        .iter()
        .filter(|volume| media_kind(&volume.media) == "SSD")
        .count();
    let hdd = volumes
        .iter()
        .filter(|volume| media_kind(&volume.media) == "HDD")
        .count();

    let mut rows = Vec::new();
    if let Some(entries) = payload.get("rows").and_then(Value::as_array) {
        for entry in entries {
            let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
            let fs = entry.get("fs").and_then(Value::as_str).unwrap_or("");
            let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
            let free = entry.get("free").and_then(Value::as_u64).unwrap_or(0);
            let media = entry.get("media").and_then(Value::as_str).unwrap_or("");
            rows.push(json!({
                "kind": kind,
                "name": name,
                "filesystem": if fs.is_empty() { "-" } else { fs },
                "size": human_bytes(size),
                "free": if kind == "卷" { human_bytes(free) } else { "-".to_string() },
                "media": if media.is_empty() { "未知" } else { media },
            }));
        }
    }

    let mut data = table(
        json!([
            item("固定卷", volumes.len().to_string()),
            item("SSD", ssd.to_string()),
            item("HDD", hdd.to_string()),
            item("TRIM", if !trim_known { "未知" } else if trim_enabled { "已启用" } else { "已禁用" }),
        ]),
        json!([
            column("kind", "类型"),
            column("name", "名称"),
            column("filesystem", "文件系统"),
            column("size", "容量"),
            column("free", "可用"),
            column("media", "介质"),
        ]),
        Value::Array(rows),
        json!([
            "分析/优化/TRIM 需要管理员授权，由独立 lens_helper.exe 调用系统 defrag.exe 执行。",
            "HDD 只允许“优化”，SSD 只允许“分析/TRIM”，不会对 SSD 执行传统碎片整理。",
            "执行时间取决于磁盘容量与碎片程度，可能持续数分钟到数小时。",
            "TRIM 由 fsutil 查询结果判断；ReFS/NTFS 可能返回多行。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert(
            "volumeItems".into(),
            Value::Array(
                volumes
                    .iter()
                    .map(|volume| {
                        let kind = media_kind(&volume.media);
                        json!({
                            "drive": volume.drive,
                            "name": format!("{}:", volume.drive),
                            "media": kind,
                            "size": human_bytes(volume.size),
                            "free": human_bytes(volume.free),
                            "canOptimize": kind == "HDD",
                            "canTrim": kind == "SSD",
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_OPTIMIZE, &volume_digest(&volumes)) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn optimize_disk(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "磁盘操作需要确认令牌，请先预览。",
        )
    })?;
    let drive = request
        .parameters
        .get("drive")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 drive。"))?;
    if drive.len() != 1 || !drive.chars().next().unwrap().is_ascii_alphabetic() {
        return Err(ToolError::new("INVALID_INPUT", "drive 必须是单个盘符字母。"));
    }
    let mode = request
        .parameters
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 mode。"))?;
    if !matches!(mode, "analyze" | "optimize" | "trim") {
        return Err(ToolError::new("INVALID_INPUT", "不支持的磁盘操作。"));
    }

    let payload = scan_payload()?;
    let volumes = volumes_from(&payload);
    tool_protocol::verify_token(token, ACTION_OPTIMIZE, &volume_digest(&volumes))?;
    let volume = volumes
        .iter()
        .find(|volume| volume.drive.eq_ignore_ascii_case(drive))
        .ok_or_else(|| ToolError::new("CONFLICT", "卷不存在或不是固定卷。"))?;
    let kind = media_kind(&volume.media);
    if mode == "optimize" && kind != "HDD" {
        return Err(ToolError::new(
            "CONFLICT",
            "只有 HDD 允许执行传统优化；SSD 请使用 TRIM。",
        ));
    }
    if mode == "trim" && kind != "SSD" {
        return Err(ToolError::new(
            "CONFLICT",
            "TRIM 只适用于 SSD。",
        ));
    }

    let result = run_elevated(json!({
        "op": "defrag_volume",
        "drive": volume.drive,
        "mode": mode,
    }))?;
    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    let output = data.get("output").and_then(Value::as_str).unwrap_or("");
    let exit_code = data.get("exitCode").and_then(Value::as_i64).unwrap_or(-1);
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "defrag",
        "action": ACTION_OPTIMIZE,
        "drive": volume.drive,
        "mode": mode,
        "exitCode": exit_code,
        "at": tool_protocol::now_ms(),
    }));

    let label = match mode {
        "analyze" => "分析",
        "optimize" => "优化",
        _ => "TRIM",
    };
    Ok(table(
        json!([
            item("卷", format!("{}:", volume.drive)),
            item("操作", label),
            item("退出码", exit_code.to_string()),
        ]),
        json!([
            column("name", "卷"),
            column("mode", "操作"),
            column("status", "结果"),
        ]),
        json!([{
            "name": format!("{}:", volume.drive),
            "mode": label,
            "status": if exit_code == 0 { "已完成" } else { "已执行（查看输出）" },
        }]),
        json!([
            "命令已由管理员助手执行；输出可能因系统语言而不同。",
            "SSD 只执行 TRIM/优化，不做传统碎片整理。",
            format!("output: {}", output.trim()),
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "defrag",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_OPTIMIZE],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_OPTIMIZE => optimize_disk(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_kind_normalizes() {
        assert_eq!(media_kind("SSD"), "SSD");
        assert_eq!(media_kind("ssd"), "SSD");
        assert_eq!(media_kind("HDD"), "HDD");
        assert_eq!(media_kind("Unspecified"), "其他");
        assert_eq!(media_kind(""), "未知");
    }

    #[test]
    fn volume_digest_binds_drive_and_media() {
        let first = vec![Volume {
            drive: "C".into(),
            media: "SSD".into(),
            size: 1,
            free: 1,
        }];
        let second = vec![Volume {
            drive: "C".into(),
            media: "HDD".into(),
            size: 1,
            free: 1,
        }];
        assert_ne!(volume_digest(&first), volume_digest(&second));
    }
}
