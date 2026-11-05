use serde::Serialize;
use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    name: String,
    path: String,
    state: String,
    author: String,
    description: String,
    run_as_user: String,
    hidden: bool,
    enabled: bool,
    triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskInventory {
    captured_at_unix_ms: u64,
    total: usize,
    enabled: usize,
    disabled: usize,
    running: usize,
    logon_or_boot: usize,
    tasks: Vec<ScheduledTask>,
    source: String,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn trigger_label(code: &str) -> String {
    let label = match code {
        "BootTrigger" => "开机",
        "LogonTrigger" => "登录",
        "TimeTrigger" => "定时",
        "DailyTrigger" => "每天",
        "WeeklyTrigger" => "每周",
        "MonthlyTrigger" => "每月",
        "MonthlyDOWTrigger" => "每月（按周）",
        "RegistrationTrigger" => "注册时",
        "IdleTrigger" => "空闲",
        "EventTrigger" => "事件",
        "SessionStateChangeTrigger" => "会话切换",
        "MaintenanceTrigger" => "维护",
        _ => code,
    };
    label.to_string()
}

fn items(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(values)) => values.iter().collect(),
        Some(value @ Value::Object(_)) => vec![value],
        _ => Vec::new(),
    }
}

fn text(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(value)) if !value.trim().is_empty() => vec![value.trim().to_string()],
        _ => Vec::new(),
    }
}

pub fn parse_payload(payload: &Value, captured_at_unix_ms: u64) -> ScheduledTaskInventory {
    let mut tasks: Vec<ScheduledTask> = items(Some(payload))
        .into_iter()
        .map(|item| {
            let state = text(item.get("state"), "Unknown");
            let triggers = strings(item.get("triggers"))
                .into_iter()
                .map(|code| trigger_label(&code))
                .collect::<Vec<_>>();
            ScheduledTask {
                name: text(item.get("name"), "未命名任务"),
                path: text(item.get("path"), "\\"),
                enabled: state != "Disabled",
                state,
                author: text(item.get("author"), "未知"),
                description: text(item.get("description"), ""),
                run_as_user: text(item.get("user"), ""),
                hidden: item.get("hidden").and_then(Value::as_bool).unwrap_or(false),
                triggers,
            }
        })
        .collect();

    tasks.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.name.cmp(&right.name))
    });

    let total = tasks.len();
    let enabled = tasks.iter().filter(|task| task.enabled).count();
    let disabled = total - enabled;
    let running = tasks.iter().filter(|task| task.state == "Running").count();
    let logon_or_boot = tasks
        .iter()
        .filter(|task| {
            task.triggers
                .iter()
                .any(|trigger| trigger == "开机" || trigger == "登录")
        })
        .count();

    ScheduledTaskInventory {
        captured_at_unix_ms,
        total,
        enabled,
        disabled,
        running,
        logon_or_boot,
        tasks,
        source: "Windows Task Scheduler (Get-ScheduledTask)".into(),
    }
}

#[cfg(windows)]
pub fn collect() -> Result<ScheduledTaskInventory, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$tasks=@()
Get-ScheduledTask -ErrorAction SilentlyContinue | ForEach-Object {
  $task=$_
  $triggers=@()
  foreach($trigger in @($task.Triggers)){
    if($trigger){ $triggers += ($trigger.CimClass.CimClassName -replace '^MSFT_Task','') }
  }
  $tasks += @{
    name=[string]$task.TaskName
    path=[string]$task.TaskPath
    state=[string]$task.State
    author=[string]$task.Author
    description=[string]$task.Description
    user=[string]$task.Principal.UserId
    hidden=[bool]$task.Settings.Hidden
    triggers=@($triggers)
  }
}
@($tasks)|ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("无法启动计划任务查询：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(parse_payload(&Value::Array(Vec::new()), now_unix_ms()));
    }
    let payload: Value = serde_json::from_str(trimmed)
        .map_err(|error| format!("无法解析计划任务数据：{error}"))?;
    Ok(parse_payload(&payload, now_unix_ms()))
}

#[cfg(not(windows))]
pub fn collect() -> Result<ScheduledTaskInventory, String> {
    Ok(parse_payload(&Value::Array(Vec::new()), 0))
}

#[tauri::command]
pub async fn get_scheduled_tasks() -> Result<ScheduledTaskInventory, String> {
    tauri::async_runtime::spawn_blocking(collect)
        .await
        .map_err(|error| format!("计划任务查询任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tasks_and_counts() {
        let payload = json!([
            {
                "name": "DemoLogon",
                "path": "\\Demo\\",
                "state": "Ready",
                "author": "Demo",
                "description": "",
                "user": "DEMO\\user",
                "hidden": false,
                "triggers": ["LogonTrigger", "TimeTrigger"]
            },
            {
                "name": "DemoBoot",
                "path": "\\Demo\\",
                "state": "Disabled",
                "author": "Demo",
                "description": "",
                "user": "SYSTEM",
                "hidden": true,
                "triggers": ["BootTrigger"]
            },
            {
                "name": "DemoRun",
                "path": "\\Microsoft\\Windows\\",
                "state": "Running",
                "author": "Microsoft",
                "description": "",
                "user": "SYSTEM",
                "hidden": false,
                "triggers": "TimeTrigger"
            }
        ]);
        let inventory = parse_payload(&payload, 123);
        assert_eq!(inventory.total, 3);
        assert_eq!(inventory.enabled, 2);
        assert_eq!(inventory.disabled, 1);
        assert_eq!(inventory.running, 1);
        assert_eq!(inventory.logon_or_boot, 2);
        assert_eq!(inventory.tasks[0].path, "\\Demo\\");
        assert_eq!(inventory.tasks[0].name, "DemoBoot");
        assert_eq!(inventory.tasks[0].triggers, vec!["开机".to_string()]);
        assert!(inventory.tasks[1].triggers.contains(&"登录".to_string()));
        // 单元素 triggers 以字符串返回时也要兼容。
        assert_eq!(inventory.tasks[2].triggers, vec!["定时".to_string()]);
    }

    #[test]
    fn handles_empty_payload() {
        let inventory = parse_payload(&json!([]), 0);
        assert_eq!(inventory.total, 0);
        assert!(inventory.tasks.is_empty());
    }
}
