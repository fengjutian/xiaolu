use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
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
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析卷信息：{error}")))?;
    let trim_text = payload.get("trim").and_then(Value::as_str).unwrap_or("");
    let trim_enabled = trim_text.contains("DisableDeleteNotify = 0");
    let trim_known = trim_text.contains("DisableDeleteNotify");

    let mut rows = Vec::new();
    let mut volumes = 0usize;
    let mut ssd = 0usize;
    if let Some(entries) = payload.get("rows").and_then(Value::as_array) {
        for entry in entries {
            let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
            let fs = entry.get("fs").and_then(Value::as_str).unwrap_or("");
            let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
            let free = entry.get("free").and_then(Value::as_u64).unwrap_or(0);
            let media = entry.get("media").and_then(Value::as_str).unwrap_or("");
            if kind == "卷" {
                volumes += 1;
            }
            if media.eq_ignore_ascii_case("SSD") {
                ssd += 1;
            }
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

    Ok(table(
        json!([
            item("固定卷", volumes.to_string()),
            item("SSD", ssd.to_string()),
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
            "只读检查：只查询卷、物理磁盘与 TRIM 状态，不执行整理或优化。",
            "TRIM 由 fsutil 查询结果判断；ReFS/NTFS 可能返回多行。",
            "碎片整理/TRIM 执行需后续版本确认磁盘类型后提供。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve("defrag", env!("CARGO_PKG_VERSION"), &["preview"], |_| scan());
}
