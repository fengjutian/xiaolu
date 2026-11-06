use serde_json::{json, Value};
use std::cmp::Ordering;
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, Request, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_OPEN: &str = "open_official";

fn error_label(code: u64) -> &'static str {
    match code {
        1 => "设备未配置正确",
        10 => "无法启动",
        12 => "资源不足",
        18 => "需重新安装驱动",
        22 => "设备已禁用",
        28 => "未安装驱动",
        31 => "无法加载驱动",
        43 => "驱动报告设备故障",
        _ => "设备异常",
    }
}

fn official_target(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "device_manager" => Some(("mmc.exe", "devmgmt.msc")),
        "windows_update" => Some(("explorer.exe", "ms-settings:windowsupdate")),
        "optional_updates" => Some(("explorer.exe", "ms-settings:windowsupdate-optionalupdates")),
        _ => None,
    }
}

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$drivers=@()
$problems=@()
Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue | ForEach-Object {
  $name=[string]$_.DeviceName
  if([string]::IsNullOrWhiteSpace($name)){ return }
  $id=[string]$_.DeviceID
  $d=@{id=$id; version=[string]$_.DriverVersion; name=$name; signed=[bool]$_.IsSigned; error=[int]$_.ConfigManagerErrorCode}
  $drivers += $d
  if(((-not $d.signed) -or ($d.error -ne 0)) -and $problems.Count -lt 30){ $problems += $d }
}
$updates=@()
$wuError=''
try {
  $session=New-Object -ComObject Microsoft.Update.Session
  $searcher=$session.CreateUpdateSearcher()
  $result=$searcher.Search("IsInstalled=0 and Type='Driver'")
  foreach($u in $result.Updates){
    try {
      $updates += @{
        title=[string]$u.Title
        version=if($u.DriverVerVersion){[string]$u.DriverVerVersion}else{''}
        hw=if($u.DriverHardwareID){[string]$u.DriverHardwareID}else{''}
        signed=if($null -eq $u.IsSigned){$null}else{[bool]$u.IsSigned}
        class=if($u.DriverClass){[string]$u.DriverClass}else{''}
      }
    } catch {}
  }
} catch { $wuError=[string]$_.Exception.Message }
@{drivers=@($drivers); problems=@($problems); updates=@($updates); wuError=$wuError} | ConvertTo-Json -Depth 5 -Compress
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .last()
        .ok_or_else(|| ToolError::new("INTERNAL", "驱动预检没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
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
    let wu_error = payload
        .get("wuError")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut rows = Vec::new();
    for problem in &problems {
        let name = problem.get("name").and_then(Value::as_str).unwrap_or("未知设备");
        let version = problem.get("version").and_then(Value::as_str).unwrap_or("");
        let signed = problem.get("signed").and_then(Value::as_bool).unwrap_or(true);
        let error = problem.get("error").and_then(Value::as_u64).unwrap_or(0);
        let detail = if !signed {
            "驱动未签名".to_string()
        } else {
            format!("{}（PnP {error}）", error_label(error))
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

    let mut notes = vec![
        json!("只读预检：读取驱动清单与 Windows Update 候选，不下载、不安装、不回滚。"),
        json!("Windows Update 常不返回签名标记，本工具不把“未知”当作未签名。"),
        json!("安装/回滚请通过设备管理器或 Windows 更新完成；本工具只打开官方入口。"),
    ];
    if !wu_error.is_empty() {
        notes.push(json!(format!("Windows Update 查询失败：{wu_error}")));
    }

    Ok(table(
        json!([
            item("驱动总数", drivers.len().to_string()),
            item("异常驱动", problems.len().to_string()),
            item("更新候选", if wu_error.is_empty() { updates.len().to_string() } else { "查询失败".to_string() }),
            item("可更新", applicable.to_string()),
        ]),
        json!([
            column("kind", "类型"),
            column("name", "名称"),
            column("status", "状态"),
            column("detail", "详情"),
        ]),
        Value::Array(rows),
        Value::Array(notes),
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

fn open_official(request: &Request) -> Result<Value, ToolError> {
    let id = request
        .parameters
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 id。"))?;
    let (executable, argument) = official_target(id)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "不支持的官方入口。"))?;
    Command::new(executable)
        .creation_flags(CREATE_NO_WINDOW)
        .arg(argument)
        .spawn()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开系统界面：{error}")))?;
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "driver_manager",
        "action": ACTION_OPEN,
        "id": id,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([item("已打开", id)]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{"item": "系统界面", "value": id}]),
        json!([
            "已打开 Windows 自带界面，请在其中手动完成安装、更新或回滚。",
            "本工具不下载、不安装、不替换驱动。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "driver_manager",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_OPEN],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_OPEN => open_official(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_official_targets() {
        assert!(official_target("device_manager").is_some());
        assert!(official_target("windows_update").is_some());
        assert!(official_target("optional_updates").is_some());
        assert!(official_target("cmd").is_none());
        assert!(official_target("").is_none());
    }

    #[test]
    fn error_codes_have_labels() {
        assert_eq!(error_label(28), "未安装驱动");
        assert_eq!(error_label(10), "无法启动");
        assert_eq!(error_label(99), "设备异常");
    }
}
