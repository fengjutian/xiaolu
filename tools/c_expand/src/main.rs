use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, ToolError};

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

fn scan() -> Result<Value, ToolError> {
    let volumes = collect()?;
    let rows = build_rows(&volumes);
    let extendable = rows
        .iter()
        .filter(|row| row["status"] == "可评估扩展")
        .count();
    let system = volumes.iter().filter(|volume| volume.boot).count();
    let total_free: u64 = volumes.iter().map(|volume| volume.free).sum();

    Ok(table(
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
            "只读分析：只读取磁盘/分区/卷信息，不调整分区、不格式化、不移动数据。",
            "Windows 磁盘管理只能向“同一磁盘、紧邻右侧”的未分配空间扩展；跨盘或左侧空间需第三方工具。",
            "实际扩容前必须备份重要数据、确认电源稳定，并保留恢复介质；系统卷操作风险更高。",
            "本工具不提供实际扩盘，也不下载分区软件。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "c_expand",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
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
}
