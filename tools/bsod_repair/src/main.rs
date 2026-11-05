use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$dumps=@()
Get-ChildItem "$env:SystemRoot\Minidump" -Filter *.dmp -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 20 | ForEach-Object {
    $dumps += @{time=$_.LastWriteTime.ToString('yyyy-MM-dd HH:mm'); name=$_.Name; size=$_.Length}
  }
$events=@()
Get-WinEvent -FilterHashtable @{LogName='System'; Id=1001,41; StartTime=(Get-Date).AddDays(-30)} -ErrorAction SilentlyContinue |
  Select-Object -First 20 | ForEach-Object {
    $message=($_.Message -split "\r?\n")[0]
    $events += @{time=$_.TimeCreated.ToString('yyyy-MM-dd HH:mm'); id=$_.Id; message=$message}
  }
@{dumps=@($dumps); events=@($events)} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动蓝屏记录查询：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析蓝屏记录：{error}")))?;
    let dumps = payload
        .get("dumps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let events = payload
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rows = Vec::new();
    let mut latest = String::new();
    for dump in &dumps {
        let time = dump.get("time").and_then(Value::as_str).unwrap_or("");
        let size = dump.get("size").and_then(Value::as_u64).unwrap_or(0);
        let name = dump.get("name").and_then(Value::as_str).unwrap_or("");
        if latest.is_empty() {
            latest = time.to_string();
        }
        rows.push(json!({
            "kind": "转储文件",
            "time": time,
            "detail": name,
            "size": human_bytes(size),
        }));
    }
    for event in &events {
        let time = event.get("time").and_then(Value::as_str).unwrap_or("");
        let id = event.get("id").and_then(Value::as_u64).unwrap_or(0);
        let message = event.get("message").and_then(Value::as_str).unwrap_or("");
        rows.push(json!({
            "kind": if id == 41 { "异常关机" } else { "蓝屏事件" },
            "time": time,
            "detail": message,
            "size": "-",
        }));
    }

    Ok(table(
        json!([
            item("转储文件", dumps.len().to_string()),
            item("30 天蓝屏/异常关机", events.len().to_string()),
            item("最近一次", if latest.is_empty() { "无记录".to_string() } else { latest }),
        ]),
        json!([
            column("kind", "类型"),
            column("time", "时间"),
            column("detail", "详情"),
            column("size", "大小"),
        ]),
        Value::Array(rows),
        json!([
            "只读分析：只读取 Minidump 目录与系统事件日志，不修改任何文件。",
            "事件 41 表示异常关机，1001 为 Windows 错误报告中的蓝屏记录。",
            "修复建议需结合转储分析，后续版本再提供。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "bsod_repair",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}
