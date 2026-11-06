use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_STARTUP: usize = 40;
const MAX_SERVICES: usize = 60;
const MAX_TASKS: usize = 40;
const MAX_NOTIFICATIONS: usize = 20;
const ACTION_TOGGLE: &str = "toggle_startup";
const RUN_SUBKEYS: [&str; 2] = ["Run", "RunOnce"];
const BACKUP_PATH: &str = "Software\\SystemLens\\DisabledStartup";
const MAX_STARTUP_ITEMS: usize = 200;

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

#[derive(Clone)]
struct StartupValue {
    key: String,
    name: String,
    command: String,
    enabled: bool,
}

fn run_key_path(key: &str) -> String {
    format!("Software\\Microsoft\\Windows\\CurrentVersion\\{key}")
}

fn backup_value_name(key: &str, name: &str) -> String {
    format!("{key}::{name}")
}

fn parse_backup_name(value: &str) -> Option<(String, String)> {
    let (key, name) = value.split_once("::")?;
    if key.is_empty() || name.is_empty() || !RUN_SUBKEYS.contains(&key) {
        return None;
    }
    Some((key.to_string(), name.to_string()))
}

fn validate_startup_name(name: &str) -> Result<(), ToolError> {
    if name.trim().is_empty() || name.chars().count() > 256 || name.contains("::") {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "启动项名称无效：不能为空、不能超过 256 字符、不能包含“::”。",
        ));
    }
    Ok(())
}

fn registry_error(context: &str, error: std::io::Error) -> ToolError {
    if error.raw_os_error() == Some(5) {
        ToolError::new("PERMISSION_DENIED", format!("{context}：拒绝访问。"))
    } else {
        ToolError::new("INTERNAL", format!("{context}：{error}"))
    }
}

fn read_startup_items() -> Result<Vec<StartupValue>, ToolError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut items = Vec::new();
    for key in RUN_SUBKEYS {
        let Ok(run) = hkcu.open_subkey_with_flags(run_key_path(key), KEY_READ) else {
            continue;
        };
        for (name, value) in run.enum_values().flatten() {
            let command = value.to_string();
            if command.trim().is_empty() {
                continue;
            }
            items.push(StartupValue {
                key: key.to_string(),
                name,
                command,
                enabled: true,
            });
        }
    }
    if let Ok(backup) = hkcu.open_subkey_with_flags(BACKUP_PATH, KEY_READ) {
        for (value_name, value) in backup.enum_values().flatten() {
            let Some((key, name)) = parse_backup_name(&value_name) else {
                continue;
            };
            items.push(StartupValue {
                key,
                name,
                command: value.to_string(),
                enabled: false,
            });
        }
    }
    items.sort_by(|left, right| {
        (left.key.as_str(), left.name.to_lowercase())
            .cmp(&(right.key.as_str(), right.name.to_lowercase()))
    });
    items.truncate(MAX_STARTUP_ITEMS);
    Ok(items)
}

fn startup_digest(items: &[StartupValue]) -> String {
    let parts = items
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}",
                entry.key, entry.name, entry.command, entry.enabled
            )
        })
        .collect::<Vec<_>>();
    let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
    digest_of(&refs)
}

fn startup_items_json(items: &[StartupValue]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|entry| {
                json!({
                    "key": entry.key,
                    "name": entry.name,
                    "command": entry.command,
                    "enabled": entry.enabled,
                    "location": format!("HKCU\\...\\{}", entry.key),
                })
            })
            .collect(),
    )
}

fn render(payload: &Value) -> Value {
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
    let firewall_error = text(payload, "firewallError");

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
        item("用户计划任务", task_entries.len().to_string()),
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

    table(
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
    )
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
    let mut data = render(&payload);
    match read_startup_items() {
        Ok(items) => {
            let token = tool_protocol::issue_token(ACTION_TOGGLE, &startup_digest(&items))?;
            if let Value::Object(map) = &mut data {
                map.insert("startupItems".into(), startup_items_json(&items));
            }
            attach_token(&mut data, token);
        }
        Err(error) => {
            if let Value::Object(map) = &mut data {
                map.insert(
                    "startupRegistryError".into(),
                    Value::String(error.message.clone()),
                );
                if let Some(notes) = map.get_mut("notes").and_then(Value::as_array_mut) {
                    notes.push(Value::String(format!(
                        "无法读取当前用户启动项，启停功能暂不可用：{}",
                        error.message
                    )));
                }
            }
        }
    }
    Ok(data)
}

#[derive(Clone)]
struct ChangeRequest {
    key: String,
    name: String,
    enable: bool,
}

struct ChangeRecord {
    key: String,
    name: String,
    enable: bool,
    status: &'static str,
    reason: String,
}

fn change_from_value(value: &Value) -> Result<ChangeRequest, ToolError> {
    let key = value
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "启动项变更缺少 key。"))?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "启动项变更缺少 name。"))?;
    let enable = value
        .get("enable")
        .and_then(Value::as_bool)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "启动项变更缺少 enable。"))?;
    if !RUN_SUBKEYS.contains(&key) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "仅支持当前用户 Run / RunOnce 启动项。",
        ));
    }
    validate_startup_name(name)?;
    Ok(ChangeRequest {
        key: key.to_string(),
        name: name.to_string(),
        enable,
    })
}

fn parse_changes(request: &Request) -> Result<Vec<ChangeRequest>, ToolError> {
    let changes = request
        .parameters
        .get("changes")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 changes。"))?;
    if changes.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "没有需要应用的启动项变更。"));
    }
    let parsed = changes
        .iter()
        .map(change_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = std::collections::HashSet::new();
    for change in &parsed {
        if !seen.insert((change.key.clone(), change.name.clone())) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("启动项 {} 重复出现。", change.name),
            ));
        }
    }
    Ok(parsed)
}

/// 在注册表 Run 与停用备份之间移动同一个启动项。
fn apply_change(entry: &StartupValue, enable: bool) -> Result<(), ToolError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enable {
        let backup = hkcu
            .open_subkey_with_flags(BACKUP_PATH, KEY_READ | KEY_WRITE)
            .map_err(|error| registry_error("读取停用备份", error))?;
        let (run, _) = hkcu
            .create_subkey(run_key_path(&entry.key))
            .map_err(|error| registry_error("打开启动项注册表", error))?;
        run.set_value(&entry.name, &entry.command)
            .map_err(|error| registry_error("恢复启动项", error))?;
        if let Err(error) = backup.delete_value(backup_value_name(&entry.key, &entry.name)) {
            let _ = run.delete_value(&entry.name);
            return Err(registry_error("清理停用备份", error));
        }
    } else {
        let run = hkcu
            .open_subkey_with_flags(run_key_path(&entry.key), KEY_READ | KEY_WRITE)
            .map_err(|error| registry_error("打开启动项注册表", error))?;
        let (backup, _) = hkcu
            .create_subkey(BACKUP_PATH)
            .map_err(|error| registry_error("创建停用备份", error))?;
        backup
            .set_value(backup_value_name(&entry.key, &entry.name), &entry.command)
            .map_err(|error| registry_error("写入停用备份", error))?;
        if let Err(error) = run.delete_value(&entry.name) {
            let _ = backup.delete_value(backup_value_name(&entry.key, &entry.name));
            return Err(registry_error("停用启动项", error));
        }
    }
    Ok(())
}

/// 直接回读注册表，确认启动项确实处于目标位置。
fn verify_state(key: &str, name: &str, enable: bool) -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let in_run = hkcu
        .open_subkey_with_flags(run_key_path(key), KEY_READ)
        .ok()
        .and_then(|run| run.get_value::<String, _>(name).ok())
        .is_some();
    let in_backup = hkcu
        .open_subkey_with_flags(BACKUP_PATH, KEY_READ)
        .ok()
        .and_then(|backup| backup.get_value::<String, _>(backup_value_name(key, name)).ok())
        .is_some();
    if enable {
        in_run && !in_backup
    } else {
        !in_run && in_backup
    }
}

fn snapshot_dir() -> std::path::PathBuf {
    tool_protocol::state_dir().join("software_pm")
}

fn write_snapshot(payload: &Value) -> Result<std::path::PathBuf, String> {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建快照目录：{error}"))?;
    let path = dir.join(format!(
        "startup-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化快照：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入快照：{error}"))?;
    Ok(path)
}

/// 逐项执行启动项变更：单项失败不中断其他项，执行后回读并写入快照。
fn execute_changes(
    before: &[StartupValue],
    changes: Vec<ChangeRequest>,
    action: &str,
) -> Result<Value, ToolError> {
    let started_at = tool_protocol::now_ms();
    let mut records = Vec::with_capacity(changes.len());
    for change in changes {
        let entry = before
            .iter()
            .find(|entry| entry.key == change.key && entry.name == change.name);
        let Some(entry) = entry else {
            records.push(ChangeRecord {
                key: change.key,
                name: change.name,
                enable: change.enable,
                status: "跳过",
                reason: "未找到该启动项，可能已被其他程序修改".into(),
            });
            continue;
        };
        if entry.enabled == change.enable {
            records.push(ChangeRecord {
                key: change.key,
                name: change.name,
                enable: change.enable,
                status: "跳过",
                reason: "已是目标状态".into(),
            });
            continue;
        }
        match apply_change(entry, change.enable) {
            Ok(()) if verify_state(&change.key, &change.name, change.enable) => {
                records.push(ChangeRecord {
                    key: change.key,
                    name: change.name,
                    enable: change.enable,
                    status: if change.enable { "已启用" } else { "已停用" },
                    reason: String::new(),
                });
            }
            Ok(()) => records.push(ChangeRecord {
                key: change.key,
                name: change.name,
                enable: change.enable,
                status: "失败",
                reason: "写入后回读状态不一致".into(),
            }),
            Err(error) => records.push(ChangeRecord {
                key: change.key,
                name: change.name,
                enable: change.enable,
                status: "失败",
                reason: error.message,
            }),
        }
    }

    let after = read_startup_items()?;
    let succeeded = records
        .iter()
        .filter(|record| record.status == "已启用" || record.status == "已停用")
        .count();
    let skipped = records
        .iter()
        .filter(|record| record.status == "跳过")
        .count();
    let failed = records
        .iter()
        .filter(|record| record.status == "失败")
        .count();
    let disabled_after = after.iter().filter(|entry| !entry.enabled).count();

    let finished_at = tool_protocol::now_ms();
    let snapshot = json!({
        "tool": "software_pm",
        "version": env!("CARGO_PKG_VERSION"),
        "action": action,
        "startedAt": started_at,
        "finishedAt": finished_at,
        "before": startup_items_json(before),
        "after": startup_items_json(&after),
        "records": records
            .iter()
            .map(|record| json!({
                "key": record.key,
                "name": record.name,
                "enable": record.enable,
                "status": record.status,
                "reason": record.reason,
            }))
            .collect::<Vec<_>>(),
    });
    let snapshot_path = write_snapshot(&snapshot);

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "software_pm",
        "action": action,
        "succeeded": succeeded,
        "skipped": skipped,
        "failed": failed,
        "snapshot": snapshot_path.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
        "at": finished_at,
    }));

    let rows = records
        .iter()
        .map(|record| {
            json!({
                "name": record.name,
                "location": format!("HKCU\\...\\{}", record.key),
                "action": if record.enable { "启用" } else { "停用" },
                "status": record.status,
                "reason": record.reason,
            })
        })
        .collect::<Vec<_>>();

    let mut data = table(
        json!([
            item("成功", succeeded.to_string()),
            item("跳过", skipped.to_string()),
            item("失败", failed.to_string()),
            item("剩余停用项", disabled_after.to_string()),
        ]),
        json!([
            column("name", "启动项"),
            column("location", "位置"),
            column("action", "操作"),
            column("status", "结果"),
            column("reason", "原因"),
        ]),
        Value::Array(rows),
        json!([
            "每项独立执行：单项失败不会中断其他项，写入后会直接回读注册表确认。",
            "只处理当前用户 Run / RunOnce；系统级启动项保持只读。",
            "停用项保存在 HKCU\\Software\\SystemLens\\DisabledStartup，可随时恢复。",
            snapshot_path
                .as_ref()
                .map(|path| format!("本次执行快照：{}", path.display()))
                .unwrap_or_else(|error| format!("快照写入失败：{error}")),
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("startupItems".into(), startup_items_json(&after));
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_TOGGLE, &startup_digest(&after)) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn toggle_startup(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "启停启动项需要确认令牌，请先预览并选择。",
        )
    })?;
    let change = change_from_value(&request.parameters)?;
    let before = read_startup_items()?;
    tool_protocol::verify_token(token, ACTION_TOGGLE, &startup_digest(&before))?;
    execute_changes(&before, vec![change], ACTION_TOGGLE)
}

fn toggle_startups(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "批量启停启动项需要确认令牌，请先预览并选择。",
        )
    })?;
    let changes = parse_changes(request)?;
    let before = read_startup_items()?;
    tool_protocol::verify_token(token, ACTION_TOGGLE, &startup_digest(&before))?;
    execute_changes(&before, changes, "toggle_startups")
}

fn restore_all(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "恢复停用启动项需要确认令牌，请先预览。",
        )
    })?;
    let before = read_startup_items()?;
    tool_protocol::verify_token(token, ACTION_TOGGLE, &startup_digest(&before))?;
    let changes = before
        .iter()
        .filter(|entry| !entry.enabled)
        .map(|entry| ChangeRequest {
            key: entry.key.clone(),
            name: entry.name.clone(),
            enable: true,
        })
        .collect::<Vec<_>>();
    if changes.is_empty() {
        return Err(ToolError::new("CONFLICT", "当前没有已停用的启动项。"));
    }
    execute_changes(&before, changes, "restore_all")
}

fn main() {
    tool_protocol::serve_with_request(
        "software_pm",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_TOGGLE, "toggle_startups", "restore_all"],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_TOGGLE => toggle_startup(request),
            "toggle_startups" => toggle_startups(request),
            "restore_all" => restore_all(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_roundtrip_and_reject_unsupported_keys() {
        let name = backup_value_name("Run", "OneDrive");
        assert_eq!(
            parse_backup_name(&name),
            Some(("Run".to_string(), "OneDrive".to_string()))
        );
        assert!(parse_backup_name("Services::Spooler").is_none());
        assert!(parse_backup_name("Run::").is_none());
        assert!(validate_startup_name("OneDrive").is_ok());
        assert!(validate_startup_name("").is_err());
        assert!(validate_startup_name("a::b").is_err());
    }

    #[test]
    fn startup_digest_changes_with_enabled_state_and_order() {
        let entry = |name: &str, enabled: bool| StartupValue {
            key: "Run".into(),
            name: name.into(),
            command: "C:\\app.exe".into(),
            enabled,
        };
        let enabled = vec![entry("A", true), entry("B", true)];
        let disabled = vec![entry("A", false), entry("B", true)];
        let reordered = vec![entry("B", true), entry("A", true)];
        assert_eq!(startup_digest(&enabled), startup_digest(&reordered));
        assert_ne!(startup_digest(&enabled), startup_digest(&disabled));
    }

    fn fixture() -> Value {
        json!({
            "startup": [
                {"name": "OneDrive", "command": "C:\\Users\\me\\AppData\\Local\\Microsoft\\OneDrive\\OneDrive.exe /background", "location": "HKU\\S-1-5-21\\...\\Run", "user": "me"}
            ],
            "services": [
                {"name": "VendorAgent", "display": "Vendor Agent", "state": "Running", "path": "C:\\Program Files\\Vendor\\agent.exe", "account": "LocalSystem"},
                {"name": "Dnscache", "display": "DNS Client", "state": "Running", "path": "C:\\Windows\\System32\\svchost.exe -k NetworkService", "account": "NT AUTHORITY\\NetworkService"}
            ],
            "tasks": [
                {"name": "VendorUpdate", "path": "\\Vendor\\", "author": "Vendor", "state": "Ready", "trigger": "Daily"}
            ],
            "notifications": [
                {"app": "Vendor.App", "enabled": 0},
                {"app": "Other.App", "enabled": 1}
            ],
            "firewall": {"total": 5, "inbound": 3, "outbound": 2, "block": 1},
            "firewallError": "",
            "errors": []
        })
    }

    #[test]
    fn renders_rows_in_category_order() {
        let value = render(&fixture());
        let rows = value.get("rows").and_then(Value::as_array).expect("rows");
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0].get("kind").and_then(Value::as_str), Some("启动项"));
        assert_eq!(rows[1].get("kind").and_then(Value::as_str), Some("服务"));
        assert_eq!(
            rows[1].get("source").and_then(Value::as_str),
            Some("第三方软件")
        );
        assert_eq!(
            rows[2].get("source").and_then(Value::as_str),
            Some("系统组件")
        );
        assert_eq!(
            rows[3].get("kind").and_then(Value::as_str),
            Some("计划任务")
        );
        assert_eq!(
            rows[3].get("detail").and_then(Value::as_str),
            Some("触发：Daily · 作者：Vendor")
        );
        assert_eq!(
            rows[4].get("status").and_then(Value::as_str),
            Some("已关闭")
        );
        assert_eq!(
            rows[5].get("status").and_then(Value::as_str),
            Some("已开启")
        );
    }

    #[test]
    fn renders_summary_and_firewall_row() {
        let value = render(&fixture());
        let summary = value.get("summary").and_then(Value::as_array).expect("summary");
        assert!(summary
            .iter()
            .any(|item| item.get("label").and_then(Value::as_str) == Some("自动服务")
                && item.get("value").and_then(Value::as_str) == Some("2")));
        assert!(summary
            .iter()
            .any(|item| item.get("value").and_then(Value::as_str) == Some("5")));
        let rows = value.get("rows").and_then(Value::as_array).expect("rows");
        let firewall = rows
            .iter()
            .find(|row| row.get("kind").and_then(Value::as_str) == Some("防火墙"))
            .expect("firewall row");
        assert_eq!(
            firewall.get("status").and_then(Value::as_str),
            Some("5 条")
        );
    }

    #[test]
    fn reports_query_failures_without_panicking() {
        let value = render(&json!({
            "startup": [],
            "services": [],
            "tasks": [],
            "notifications": [],
            "firewall": {"total": 0, "inbound": 0, "outbound": 0, "block": 0},
            "firewallError": "拒绝访问",
            "errors": ["启动项查询失败：拒绝访问"]
        }));
        let rows = value.get("rows").and_then(Value::as_array).expect("rows");
        assert!(rows.is_empty());
        let notes = value.get("notes").and_then(Value::as_array).expect("notes");
        assert!(notes
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("防火墙规则查询失败")));
        assert!(notes
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("启动项查询失败")));
    }

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

    #[test]
    fn parses_change_values_and_rejects_invalid_input() {
        let change = change_from_value(&json!({
            "key": "Run",
            "name": "App",
            "enable": true
        }))
        .expect("valid change");
        assert_eq!(change.key, "Run");
        assert_eq!(change.name, "App");
        assert!(change.enable);

        assert!(change_from_value(&json!({
            "key": "Services",
            "name": "App",
            "enable": true
        }))
        .is_err());
        assert!(change_from_value(&json!({
            "key": "Run",
            "name": "",
            "enable": true
        }))
        .is_err());
        assert!(change_from_value(&json!({ "key": "Run", "name": "App" })).is_err());
    }
}
