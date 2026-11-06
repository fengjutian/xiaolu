use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub drive: String,
    pub label: String,
    pub filesystem: String,
    pub size: u64,
    pub free: u64,
    pub disk_number: i64,
    pub partition_style: String,
    pub gap_bytes: u64,
    pub boot: bool,
}

fn feasible(volume: &VolumeInfo) -> (&'static str, &'static str) {
    if volume.gap_bytes == 0 {
        return ("不可直接扩展", "右侧没有相邻未分配空间；Windows 磁盘管理无法扩展");
    }
    if volume.gap_bytes < 1024 * 1024 * 1024 {
        return ("空间过小", "右侧未分配不足 1 GB，扩展收益有限；可先清理或评估第三方工具");
    }
    if volume.free == 0 && volume.size == 0 {
        return ("信息不足", "未读取到卷大小");
    }
    if volume.partition_style.eq_ignore_ascii_case("MBR") && volume.free + volume.gap_bytes > 2 * 1024 * 1024 * 1024 * 1024 {
        return ("需先转换 GPT", "MBR 受 2 TB 限制；扩容前需评估转换风险并备份");
    }
    ("可评估扩展", "右侧存在未分配空间；可用磁盘管理扩展，但必须先备份并确认相邻分区")
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$disks=@(Get-Disk -ErrorAction SilentlyContinue)
$parts=@(Get-Partition -ErrorAction SilentlyContinue)
$vols=@(Get-Volume -ErrorAction SilentlyContinue)
$out=[System.Collections.Generic.List[object]]::new()
foreach($vol in $vols){
  if(-not $vol.DriveLetter){ continue }
  $part=$parts | Where-Object { $_.DriveLetter -eq $vol.DriveLetter } | Select-Object -First 1
  if(-not $part){ continue }
  $disk=$disks | Where-Object { $_.Number -eq $part.DiskNumber } | Select-Object -First 1
  $end=[uint64]$part.Offset + [uint64]$part.Size
  $next=$parts | Where-Object { $_.DiskNumber -eq $part.DiskNumber -and [uint64]$_.Offset -gt $end } | Sort-Object Offset | Select-Object -First 1
  $gap=0L
  if($next){ $gap=[uint64]$next.Offset - $end }
  elseif($disk){ $gap=[uint64]$disk.Size - $end }
  $out.Add(@{
    drive=[string]$vol.DriveLetter; label=[string]$vol.FileSystemLabel; filesystem=[string]$vol.FileSystem;
    size=[uint64]$vol.Size; free=[uint64]$vol.SizeRemaining;
    diskNumber=if($disk){[int64]$disk.Number}else{-1};
    partitionStyle=if($disk){[string]$disk.PartitionStyle}else{'未知'};
    gapBytes=[uint64]$gap;
    boot=[bool]($part.IsBoot -or $part.IsSystem)
  })
}
@($out) | ConvertTo-Json -Depth 3 -Compress
"#
}

fn collect() -> Result<Vec<VolumeInfo>, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取分区信息：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{') || line.starts_with('['))
        .last()
        .ok_or_else(|| ToolError::new("INTERNAL", "分区查询没有返回 JSON"))?;
    let value: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析分区信息：{error}")))?;
    let array = match value {
        Value::Array(items) => items,
        other => vec![other],
    };
    let mut volumes = array
        .into_iter()
        .filter_map(|item| serde_json::from_value::<VolumeInfo>(item).ok())
        .collect::<Vec<_>>();
    volumes.sort_by(|left, right| left.drive.cmp(&right.drive));
    Ok(volumes)
}

fn build_rows(volumes: &[VolumeInfo]) -> Vec<Value> {
    volumes
        .iter()
        .map(|volume| {
            let (status, hint) = feasible(volume);
            json!({
                "drive": format!("{}:", volume.drive),
                "label": if volume.label.is_empty() { "未命名".to_string() } else { volume.label.clone() },
                "filesystem": if volume.filesystem.is_empty() { "未知".to_string() } else { volume.filesystem.clone() },
                "size": human_bytes(volume.size),
                "free": human_bytes(volume.free),
                "disk": format!("磁盘 {} · {}", volume.disk_number, volume.partition_style),
                "gap": if volume.gap_bytes == 0 { "无".to_string() } else { human_bytes(volume.gap_bytes) },
                "status": status,
                "hint": hint,
                "system": if volume.boot { "是" } else { "否" },
            })
        })
        .collect()
}

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
    let dir = exe.parent().ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    let candidates = [
        dir.join("lens_helper.exe"),
        dir.join("..").join("lens_helper").join("lens_helper.exe"),
    ];
    candidates.into_iter().find(|path| path.is_file()).ok_or_else(|| {
        ToolError::new("HELPER_MISSING", "缺少提权助手 lens_helper.exe。")
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
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;
    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);
    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new("ELEVATION_CANCELLED", "已取消管理员授权，未调整分区。"));
        }
        return Err(ToolError::new("ELEVATION_FAILED", format!("提权助手执行失败（退出码 {:?}）。", status.code())));
    }
    let text = result_text.ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value.get("error").and_then(Value::as_str).unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

fn volume_digest(volumes: &[VolumeInfo]) -> String {
    let parts = volumes
        .iter()
        .map(|volume| format!("{}|{}|{}", volume.drive, volume.size, volume.gap_bytes))
        .collect::<Vec<_>>();
    tool_protocol::digest_of(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn scan() -> Result<Value, ToolError> {
    let volumes = collect()?;
    let rows = build_rows(&volumes);
    let extendable = rows
        .iter()
        .filter(|row| row["status"] == "可评估扩展")
        .count();
    let system = volumes.iter().filter(|volume| volume.boot).count();
    let total_free: u64 = volumes.iter().map(|volume| volume.free).sum();

    let mut data = table(
        json!([
            item("卷数量", volumes.len().to_string()),
            item("可评估扩展", extendable.to_string()),
            item("系统/引导卷", system.to_string()),
            item("可用空间合计", human_bytes(total_free)),
        ]),
        json!([
            column("drive", "卷"),
            column("label", "标签"),
            column("filesystem", "文件系统"),
            column("size", "容量"),
            column("free", "可用"),
            column("disk", "磁盘/样式"),
            column("gap", "右侧未分配"),
            column("status", "可行性"),
            column("hint", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "分析部分只读；扩容只支持 NTFS 卷向右扩展相邻未分配空间。",
            "执行前必须备份重要数据、确认电源稳定并保留恢复介质；系统卷风险更高。",
            "不移动数据、不跨盘、不缩小、不新建或删除分区。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert(
            "expandItems".into(),
            Value::Array(
                volumes
                    .iter()
                    .filter(|volume| feasible(volume).0 == "可评估扩展")
                    .map(|volume| {
                        json!({
                            "drive": volume.drive,
                            "label": if volume.label.is_empty() { "未命名".to_string() } else { volume.label.clone() },
                            "filesystem": volume.filesystem,
                            "sizeBytes": volume.size,
                            "size": human_bytes(volume.size),
                            "gapBytes": volume.gap_bytes,
                            "gap": human_bytes(volume.gap_bytes),
                            "maxBytes": volume.size.saturating_add(volume.gap_bytes),
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Ok(token) =
        tool_protocol::issue_token("expand_volume", &volume_digest(&volumes))
    {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn expand_volume(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "扩容需要确认令牌，请先分析并确认备份。",
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
    let target = request
        .parameters
        .get("targetBytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 targetBytes。"))?;

    let volumes = collect()?;
    tool_protocol::verify_token(token, "expand_volume", &volume_digest(&volumes))?;
    let volume = volumes
        .iter()
        .find(|volume| volume.drive.eq_ignore_ascii_case(drive))
        .ok_or_else(|| ToolError::new("CONFLICT", "卷不存在。"))?;
    if !volume.filesystem.eq_ignore_ascii_case("NTFS") {
        return Err(ToolError::new("CONFLICT", "只支持 NTFS 卷扩容。"));
    }
    if feasible(volume).0 != "可评估扩展" {
        return Err(ToolError::new(
            "CONFLICT",
            "该卷当前不满足扩容条件（右侧无足够相邻未分配空间）。",
        ));
    }
    if target <= volume.size {
        return Err(ToolError::new("INVALID_INPUT", "目标容量必须大于当前容量。"));
    }
    if target > volume.size.saturating_add(volume.gap_bytes) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "目标容量超过右侧相邻未分配空间。",
        ));
    }

    run_elevated(json!({
        "op": "expand_volume",
        "drive": volume.drive,
        "targetBytes": target,
    }))?;

    let after = collect()?;
    let updated = after
        .iter()
        .find(|item| item.drive.eq_ignore_ascii_case(drive))
        .ok_or_else(|| ToolError::new("INTERNAL", "扩容后无法读取该卷。"))?;
    if updated.size < target {
        return Err(ToolError::new(
            "INTERNAL",
            "扩容后容量未达到目标，请检查磁盘管理。",
        ));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "c_expand",
        "action": "expand_volume",
        "drive": volume.drive,
        "before": volume.size,
        "after": updated.size,
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("卷", format!("{}:", volume.drive)),
            item("原容量", human_bytes(volume.size)),
            item("新容量", human_bytes(updated.size)),
        ]),
        json!([
            column("item", "项目"),
            column("value", "结果"),
        ]),
        json!([{
            "item": format!("{}:", volume.drive),
            "value": format!("{} → {}", human_bytes(volume.size), human_bytes(updated.size)),
        }]),
        json!([
            "扩容已完成，并已重新读取卷容量确认。",
            "如出现文件系统异常，请立即停止写入并运行 chkdsk。",
            "该操作不移动数据；失败时可能部分完成，请以磁盘管理状态为准。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "c_expand",
        env!("CARGO_PKG_VERSION"),
        &["preview", "expand_volume"],
        |request| match request.action.as_str() {
            "preview" => scan(),
            "expand_volume" => expand_volume(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(gap: u64, style: &str) -> VolumeInfo {
        VolumeInfo {
            drive: "C".into(),
            label: "System".into(),
            filesystem: "NTFS".into(),
            size: 500 * 1024 * 1024 * 1024,
            free: 100 * 1024 * 1024 * 1024,
            disk_number: 0,
            partition_style: style.into(),
            gap_bytes: gap,
            boot: true,
        }
    }

    #[test]
    fn requires_adjacent_unallocated_space() {
        let (status, _) = feasible(&volume(0, "GPT"));
        assert_eq!(status, "不可直接扩展");
        let (status, _) = feasible(&volume(10 * 1024 * 1024 * 1024, "GPT"));
        assert_eq!(status, "可评估扩展");
    }

    #[test]
    fn mbr_large_volume_recommends_gpt_conversion() {
        let mut item = volume(2 * 1024 * 1024 * 1024 * 1024, "MBR");
        item.size = 2 * 1024 * 1024 * 1024 * 1024;
        let (status, _) = feasible(&item);
        assert_eq!(status, "需先转换 GPT");
    }

    #[test]
    fn rows_include_gap_and_system_flag() {
        let rows = build_rows(&[volume(3 * 1024 * 1024 * 1024, "GPT")]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["drive"], "C:");
        assert_eq!(rows[0]["gap"], "3.0 GB");
        assert_eq!(rows[0]["system"], "是");
    }

    #[test]
    fn small_gap_is_limited() {
        let (status, _) = feasible(&volume(700 * 1024 * 1024, "GPT"));
        assert_eq!(status, "空间过小");
    }
}