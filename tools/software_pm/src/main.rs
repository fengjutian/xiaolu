use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_STARTUP: usize = 40;
const MAX_SERVICES: usize = 60;
const MAX_TASKS: usize = 40;
const MAX_NOTIFICATIONS: usize = 20;

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$errors=@()

$startup=@()
try {
  Get-CimInstance Win32_StartupCommand -ErrorAction Stop | ForEach-Object {
    $name=[string]$_.Name
    if([string]::IsNullOrWhiteSpace($name)){ return }
    $startup += @{name=$name; command=[string]$_.Command; location=[string]$_.Location; user=[string]$_.User}
  }
} catch { $errors += ('启动项查询失败：' + $_.Exception.Message) }

$services=@()
try {
  Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.StartMode -eq 'Auto' } | ForEach-Object {
    $services += @{name=[string]$_.Name; display=[string]$_.DisplayName; state=[string]$_.State; path=[string]$_.PathName; account=[string]$_.StartName}
  }
} catch { $errors += ('服务查询失败：' + $_.Exception.Message) }

$tasks=@()
try {
  Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' } | ForEach-Object {
    $trigger=''
    try {
      $trigger=(@($_.Triggers | ForEach-Object { $_.CimClass.CimClassName -replace '^MSFT_Task','' -replace 'Trigger$','' }) | Where-Object { $_ } | Select-Object -Unique) -join '/'
    } catch {}
    $tasks += @{name=[string]$_.TaskName; path=[string]$_.TaskPath; author=[string]$_.Author; state=[string]$_.State; trigger=$trigger}
  }
} catch { $errors += ('计划任务查询失败：' + $_.Exception.Message) }

$notifications=@()
try {
  $base='HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Notifications\Settings'
  if(Test-Path $base) {
    Get-ChildItem $base -ErrorAction Stop | ForEach-Object {
      $enabled=$null
      try { $enabled=(Get-ItemProperty -Path $_.PSPath -Name 'Enabled' -ErrorAction Stop).Enabled } catch {}
      if($null -ne $enabled){ $notifications += @{app=[string]$_.PSChildName; enabled=[int]$enabled} }
    }
  }
} catch { $errors += ('通知设置查询失败：' + $_.Exception.Message) }

$fw=@{total=0; inbound=0; outbound=0; block=0}
$fwError=''
try {
  $rules=Get-NetFirewallRule -Enabled True -ErrorAction Stop
  foreach($rule in $rules){
    $fw.total++
    if($rule.Direction -eq 'Inbound'){ $fw.inbound++ } else { $fw.outbound++ }
    if($rule.Action -eq 'Block'){ $fw.block++ }
  }
} catch { $fwError=[string]$_.Exception.Message }

@{startup=@($startup); services=@($services); tasks=@($tasks); notifications=@($notifications); firewall=$fw; firewallError=$fwError; errors=@($errors)} | ConvertTo-Json -Depth 5 -Compress
"#
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn shorten(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut result = value.chars().take(max).collect::<String>();
    result.push('…');
    result
}

/// 系统自带服务通常位于 Windows 目录；其余视为第三方软件登记的自动服务。
fn is_system_service(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("\\windows\\system32\\")
        || lower.contains("\\windows\\syswow64\\")
        || lower.starts_with("%systemroot%\\")
}

fn startup_location(location: &str) -> &str {
    if location.is_empty() {
        "启动项"
    } else {
        location
    }
}

fn scan() -> Result<Value, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动软件权限检查：{error}")))?;
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
        .ok_or_else(|| ToolError::new("INTERNAL", "软件权限检查没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析软件权限检查：{error}")))?;

    let empty = Vec::new();
    let startup = payload
        .get("startup")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let services = payload
        .get("services")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let tasks = payload
        .get("tasks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let notifications = payload
        .get("notifications")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let firewall = payload.get("firewall").cloned().unwrap_or(Value::Null);
    let firewall_error = text(&payload, "firewallError");

    let mut rows = Vec::new();

    for entry in startup.iter().take(MAX_STARTUP) {
        rows.push(json!({
            "kind": "启动项",
            "name": shorten(text(entry, "name"), 40),
            "source": shorten(startup_location(text(entry, "location")), 48),
            "status": "已登记",
            "detail": shorten(text(entry, "command"), 160),
        }));
    }

    let mut service_entries = services
        .iter()
        .map(|entry| {
            let path = text(entry, "path");
            (is_system_service(path), entry)
        })
        .collect::<Vec<_>>();
    service_entries.sort_by_key(|(system, entry)| (*system, text(entry, "display").to_lowercase()));
    for (system, entry) in service_entries.iter().take(MAX_SERVICES) {
        let detail = if text(entry, "account").is_empty() {
            shorten(text(entry, "path"), 140)
        } else {
            shorten(
                &format!("{} · {}", text(entry, "path"), text(entry, "account")),
                160,
            )
        };
        rows.push(json!({
            "kind": "服务",
            "name": shorten(text(entry, "display"), 48),
            "source": if *system { "系统组件" } else { "第三方软件" },
            "status": text(entry, "state"),
            "detail": detail,
        }));
    }

    let mut task_entries = tasks.iter().collect::<Vec<_>>();
    task_entries.sort_by_key(|entry| text(entry, "name").to_lowercase());
    for entry in task_entries.iter().take(MAX_TASKS) {
        let author = text(entry, "author");
        let trigger = text(entry, "trigger");
        let detail = match (trigger.is_empty(), author.is_empty()) {
            (false, false) => format!("触发：{trigger} · 作者：{author}"),
            (false, true) => format!("触发：{trigger}"),
            (true, false) => format!("作者：{author}"),
            (true, true) => "未提供触发条件".to_string(),
        };
        rows.push(json!({
            "kind": "计划任务",
            "name": shorten(text(entry, "name"), 48),
            "source": shorten(text(entry, "path"), 48),
            "status": text(entry, "state"),
            "detail": shorten(&detail, 160),
        }));
    }

    let disabled_notifications = notifications
        .iter()
        .filter(|entry| entry.get("enabled").and_then(Value::as_i64) == Some(0))
        .count();
    let mut notification_entries = notifications.iter().collect::<Vec<_>>();
    notification_entries.sort_by_key(|entry| entry.get("enabled").and_then(Value::as_i64));
    for entry in notification_entries.iter().take(MAX_NOTIFICATIONS) {
        let enabled = entry.get("enabled").and_then(Value::as_i64) == Some(1);
        rows.push(json!({
            "kind": "通知",
            "name": shorten(text(entry, "app"), 56),
            "source": "系统通知设置",
            "status": if enabled { "已开启" } else { "已关闭" },
            "detail": "应用级通知开关（注册表只读）",
        }));
    }

    let firewall_total = firewall.get("total").and_then(Value::as_u64).unwrap_or(0);
    let firewall_inbound = firewall.get("inbound").and_then(Value::as_u64).unwrap_or(0);
    let firewall_outbound = firewall.get("outbound").and_then(Value::as_u64).unwrap_or(0);
    let firewall_block = firewall.get("block").and_then(Value::as_u64).unwrap_or(0);
    if firewall_error.is_empty() {
        rows.push(json!({
            "kind": "防火墙",
            "name": "已启用规则统计",
            "source": "Windows 防火墙",
            "status": format!("{firewall_total} 条"),
            "detail": format!("入站 {firewall_inbound} · 出站 {firewall_outbound} · 阻止 {firewall_block}"),
        }));
    }

    let mut summary = vec![
        item("启动项", startup.len().to_string()),
        item("自动服务", services.len().to_string()),
        item(
            "用户计划任务",
            task_entries.len().to_string(),
        ),
        item("通知已关闭", disabled_notifications.to_string()),
    ];
    summary.push(item(
        "防火墙启用规则",
        if firewall_error.is_empty() {
            firewall_total.to_string()
        } else {
            "查询失败".to_string()
        },
    ));

    let mut notes = vec![
        json!("只读视图：不启用/禁用启动项、服务或计划任务，也不修改通知与防火墙规则。"),
        json!("第三方自动服务不一定是恶意程序；请结合发布者、安装来源和实际用途判断。"),
        json!("修改任何启动行为都必须走“逐项确认 + 可回滚”流程，本工具暂不提供。"),
    ];
    if !firewall_error.is_empty() {
        notes.push(json!(format!("防火墙规则查询失败：{firewall_error}")));
    }
    if let Some(errors) = payload.get("errors").and_then(Value::as_array) {
        for error in errors {
            if let Some(text) = error.as_str() {
                notes.push(json!(text));
            }
        }
    }

    Ok(table(
        Value::Array(summary),
        json!([
            column("kind", "类别"),
            column("name", "名称"),
            column("source", "来源"),
            column("status", "状态"),
            column("detail", "详情"),
        ]),
        Value::Array(rows),
        Value::Array(notes),
    ))
}

fn main() {
    tool_protocol::serve("software_pm", env!("CARGO_PKG_VERSION"), &["preview"], |_| {
        scan()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_system_services() {
        assert!(is_system_service(
            r"C:\Windows\System32\svchost.exe -k netsvcs"
        ));
        assert!(is_system_service(r"C:\Windows\SysWOW64\foo.exe"));
        assert!(is_system_service(r"%SystemRoot%\system32\foo.exe"));
        assert!(!is_system_service(
            r"C:\Program Files\Vendor\agent.exe --service"
        ));
        assert!(!is_system_service(""));
    }

    #[test]
    fn shortens_long_text_by_characters() {
        assert_eq!(shorten("abc", 10), "abc");
        assert_eq!(shorten("  中文名称  ", 10), "中文名称");
        assert_eq!(shorten("abcdef", 3), "abc…");
        assert_eq!(shorten("中文中文", 2), "中文…");
    }

    #[test]
    fn falls_back_to_generic_startup_location() {
        assert_eq!(startup_location(""), "启动项");
        assert_eq!(startup_location("HKLM\\...\\Run"), "HKLM\\...\\Run");
    }
}
