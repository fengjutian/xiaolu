use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_ROWS: usize = 300;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$items=@()
Get-AppxPackage -ErrorAction Stop | ForEach-Object {
  $items += @{
    name = [string]$_.Name
    version = [string]$_.Version
    location = if($_.InstallLocation){[string]$_.InstallLocation}else{''}
  }
}
@{items=@($items)} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动应用清单查询：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析应用清单：{error}")))?;
    let packages = payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rows = Vec::new();
    let mut with_location = 0usize;
    let mut builtin = 0usize;
    for package in packages.iter().take(MAX_ROWS) {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("未知应用");
        let version = package
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("");
        let location = package
            .get("location")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !location.is_empty() {
            with_location += 1;
        }
        if name.starts_with("Microsoft.") || name.starts_with("MicrosoftWindows.") {
            builtin += 1;
        }
        rows.push(json!({
            "name": name,
            "version": version,
            "location": location,
            "type": if name.starts_with("Microsoft.") || name.starts_with("MicrosoftWindows.") {
                "系统内置"
            } else {
                "第三方"
            }
        }));
    }

    Ok(table(
        json!([
            item("已安装应用包", packages.len().to_string()),
            item("有安装目录", with_location.to_string()),
            item("系统内置候选", builtin.to_string()),
        ]),
        json!([
            column("name", "应用包"),
            column("version", "版本"),
            column("type", "类型"),
            column("location", "安装目录"),
        ]),
        Value::Array(rows),
        json!([
            "只读清单：不会卸载或修改任何应用包。",
            "“系统内置候选”按 Microsoft.* 前缀判断，仅供人工核对。",
            "卸载/精简能力需后续版本按项确认后提供。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve("sys_clean", env!("CARGO_PKG_VERSION"), &["preview"], |_| {
        scan()
    });
}
