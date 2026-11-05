use serde_json::{json, Value};
use std::cmp::Ordering;
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$drivers=@()
Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue | ForEach-Object {
  $drivers += @{id=[string]$_.DeviceID; version=[string]$_.DriverVersion; name=[string]$_.DeviceName; signed=[bool]$_.IsSigned; error=[int]$_.ConfigManagerErrorCode}
}
$problems=@()
$drivers | Where-Object { (-not $_.signed) -or ($_.error -ne 0) } | Select-Object -First 30 | ForEach-Object {
  $problems += $_
}
$updates=@()
try {
  $session=New-Object -ComObject Microsoft.Update.Session
  $searcher=$session.CreateUpdateSearcher()
  $result=$searcher.Search("IsInstalled=0 and Type='Driver'")
  foreach($u in $result.Updates){
    $updates += @{
      title=[string]$u.Title
      version=if($u.DriverVerVersion){[string]$u.DriverVerVersion}else{''}
      hw=if($u.DriverHardwareID){[string]$u.DriverHardwareID}else{''}
      signed=if($null -eq $u.IsSigned){$null}else{[bool]$u.IsSigned}
      class=if($u.DriverClass){[string]$u.DriverClass}else{''}
    }
  }
} catch {}
@{drivers=@($drivers); problems=@($problems); updates=@($updates)} | ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动驱动预检：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析驱动预检：{error}")))?;
    let drivers = payload
        .get("drivers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let problems = payload
        .get("problems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let updates = payload
        .get("updates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rows = Vec::new();
    for problem in &problems {
        let name = problem.get("name").and_then(Value::as_str).unwrap_or("未知设备");
        let version = problem.get("version").and_then(Value::as_str).unwrap_or("");
        let signed = problem.get("signed").and_then(Value::as_bool).unwrap_or(true);
        let error = problem.get("error").and_then(Value::as_u64).unwrap_or(0);
        let detail = if !signed {
            "驱动未签名".to_string()
        } else {
            format!("PnP 错误码 {error}")
        };
        rows.push(json!({
            "kind": "驱动异常",
            "name": name,
            "status": detail,
            "detail": format!("当前版本 {version}"),
        }));
    }

    let mut applicable = 0usize;
    for update in &updates {
        let title = update.get("title").and_then(Value::as_str).unwrap_or("驱动候选");
        let hardware = update.get("hw").and_then(Value::as_str).unwrap_or("");
        let candidate = update.get("version").and_then(Value::as_str).unwrap_or("");
        let (status, detail) = if hardware.is_empty() {
            ("未知".to_string(), "候选未提供硬件 ID".to_string())
        } else {
            let matching = drivers.iter().any(|driver| {
                driver
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| hardware_matches(hardware, id))
                    .unwrap_or(false)
            });
            if !matching {
                ("无匹配设备".to_string(), hardware.to_string())
            } else {
                let installed = drivers
                    .iter()
                    .filter(|driver| {
                        driver
                            .get("id")
                            .and_then(Value::as_str)
                            .map(|id| hardware_matches(hardware, id))
                            .unwrap_or(false)
                    })
                    .filter_map(|driver| driver.get("version").and_then(Value::as_str))
                    .max_by(|left, right| compare_versions(left, right))
                    .unwrap_or("");
                if candidate.is_empty() {
                    ("未知".to_string(), "候选未提供版本号".to_string())
                } else if compare_versions(candidate, installed) == Ordering::Greater {
                    applicable += 1;
                    (
                        "可更新".to_string(),
                        format!("{candidate} > 已装 {installed}"),
                    )
                } else {
                    (
                        "已是最新".to_string(),
                        format!("候选 {candidate} ≤ 已装 {installed}"),
                    )
                }
            }
        };
        rows.push(json!({
            "kind": "更新候选",
            "name": title,
            "status": status,
            "detail": detail,
        }));
    }

    Ok(table(
        json!([
            item("驱动总数", drivers.len().to_string()),
            item("异常驱动", problems.len().to_string()),
            item("更新候选", updates.len().to_string()),
            item("可更新", applicable.to_string()),
        ]),
        json!([
            column("kind", "类型"),
            column("name", "名称"),
            column("status", "状态"),
            column("detail", "详情"),
        ]),
        Value::Array(rows),
        json!([
            "只读预检：只读取驱动清单与 Windows Update 候选，不下载、不安装、不回滚。",
            "Windows Update 常不返回签名标记，本工具不把“未知”当作未签名。",
            "安装与驱动回滚需后续版本单独授权并提供恢复点。",
        ]),
    ))
}

fn hardware_matches(hardware_id: &str, device_id: &str) -> bool {
    hardware_id
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .any(|item| {
            let item = item.to_lowercase();
            let device = device_id.to_lowercase();
            device == item || device.starts_with(&item)
        })
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left = version_parts(left);
    let right = version_parts(right);
    let length = left.len().max(right.len());
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

fn main() {
    tool_protocol::serve(
        "driver_manager",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}
