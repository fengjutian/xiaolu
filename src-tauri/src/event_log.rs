use serde::Serialize;
use serde_json::Value;
use std::process::Command;

const ALLOWED_LOGS: [&str; 4] = ["System", "Application", "Setup", "Security"];
const MAX_MESSAGE_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLogEntry {
    time_created_unix_ms: u64,
    log_name: String,
    provider: String,
    event_id: u32,
    level: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLogResult {
    log_name: String,
    level: String,
    hours: u32,
    total: usize,
    events: Vec<EventLogEntry>,
    truncated: bool,
    source: String,
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

fn level_filter(level: &str) -> Option<u8> {
    match level {
        "critical" => Some(1),
        "error" => Some(2),
        "warning" => Some(3),
        "information" => Some(4),
        _ => None,
    }
}

/// 解析 Get-WinEvent 的 JSON，供固定样本测试复用。
pub fn parse_payload(payload: &Value) -> Vec<EventLogEntry> {
    items(Some(payload))
        .into_iter()
        .filter_map(|item| {
            let message = text(item.get("message"), "");
            Some(EventLogEntry {
                time_created_unix_ms: item.get("time").and_then(Value::as_u64)?,
                log_name: text(item.get("log"), "Unknown"),
                provider: text(item.get("provider"), "未知来源"),
                event_id: item.get("id").and_then(Value::as_u64).unwrap_or_default() as u32,
                level: text(item.get("level"), "Unknown"),
                message: if message.chars().count() > MAX_MESSAGE_CHARS {
                    let mut truncated = message.chars().take(MAX_MESSAGE_CHARS).collect::<String>();
                    truncated.push('…');
                    truncated
                } else {
                    message
                },
            })
        })
        .collect()
}

#[cfg(windows)]
pub fn query(
    log_name: &str,
    level: &str,
    hours: u32,
    search: &str,
    limit: u32,
) -> Result<EventLogResult, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    if !ALLOWED_LOGS.contains(&log_name) {
        return Err("不支持的事件日志名称".into());
    }
    let hours = hours.clamp(1, 24 * 30);
    let limit = limit.clamp(10, 500);
    let max_events = (limit.saturating_mul(4)).clamp(40, 2000);
    let filter_level = level_filter(level);
    let level_clause = filter_level
        .map(|value| format!("; Level={value}"))
        .unwrap_or_default();
    let script = format!(
        r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$filter=@{{LogName='{log_name}'; StartTime=(Get-Date).AddHours(-{hours}){level_clause}}}
$events=@()
try {{ $events=@(Get-WinEvent -FilterHashtable $filter -MaxEvents {max_events} -ErrorAction Stop) }}
catch {{ if($_.Exception.Message -notmatch 'No events'){{ throw }} }}
$rows=@()
foreach($event in $events){{
  $rows += @{{
    time=[DateTimeOffset]::new($event.TimeCreated).ToUnixTimeMilliseconds()
    log=[string]$event.LogName
    provider=[string]$event.ProviderName
    id=[int]$event.Id
    level=[string]$event.LevelDisplayName
    message=[string]$event.Message
  }}
}}
@($rows)|ConvertTo-Json -Depth 4 -Compress
"#
    );

    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| format!("无法启动事件日志查询：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    let mut entries = if trimmed.is_empty() {
        Vec::new()
    } else {
        let payload: Value = serde_json::from_str(trimmed)
            .map_err(|error| format!("无法解析事件日志：{error}"))?;
        parse_payload(&payload)
    };
    let needle = search.trim().to_lowercase();
    if !needle.is_empty() {
        entries.retain(|entry| {
            entry.message.to_lowercase().contains(&needle)
                || entry.provider.to_lowercase().contains(&needle)
        });
    }
    let total = entries.len();
    let truncated = total > limit as usize;
    entries.truncate(limit as usize);

    Ok(EventLogResult {
        log_name: log_name.into(),
        level: level.into(),
        hours,
        total,
        events: entries,
        truncated,
        source: "Windows Event Log (Get-WinEvent, 只读)".into(),
    })
}

#[cfg(not(windows))]
pub fn query(
    _log_name: &str,
    _level: &str,
    _hours: u32,
    _search: &str,
    _limit: u32,
) -> Result<EventLogResult, String> {
    Ok(EventLogResult {
        log_name: "System".into(),
        level: "all".into(),
        hours: 24,
        total: 0,
        events: Vec::new(),
        truncated: false,
        source: "unsupported".into(),
    })
}

#[tauri::command]
pub async fn query_event_log(
    log_name: String,
    level: String,
    hours: u32,
    search: String,
    limit: u32,
) -> Result<EventLogResult, String> {
    tauri::async_runtime::spawn_blocking(move || query(&log_name, &level, hours, &search, limit))
        .await
        .map_err(|error| format!("事件日志查询任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_events_and_truncates_long_messages() {
        let payload = json!([
            {
                "time": 1700000000000i64,
                "log": "System",
                "provider": "Demo-Provider",
                "id": 41,
                "level": "错误",
                "message": "x".repeat(MAX_MESSAGE_CHARS + 50)
            },
            {
                "time": 1700000001000i64,
                "log": "Application",
                "provider": "App",
                "id": 1000,
                "level": "信息",
                "message": "启动完成"
            }
        ]);
        let events = parse_payload(&payload);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, 41);
        assert!(events[0].message.chars().count() <= MAX_MESSAGE_CHARS + 1);
        assert_eq!(events[1].provider, "App");
    }

    #[test]
    fn handles_single_object_payload() {
        let payload = json!({
            "time": 1700000000000i64,
            "log": "System",
            "provider": "Demo",
            "id": 1,
            "level": "警告",
            "message": "单条事件"
        });
        let events = parse_payload(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, "警告");
    }

    #[cfg(windows)]
    #[test]
    fn rejects_unknown_log_names() {
        assert!(!ALLOWED_LOGS.contains(&"C:\\Windows"));
        assert!(query("C:\\Windows", "all", 24, "", 50).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "需要在本机真实读取 Windows 事件日志"]
    fn live_event_log_query() {
        let result = query("System", "error", 24, "", 20).expect("event log query should succeed");
        println!(
            "log={} total={} returned={} truncated={}",
            result.log_name,
            result.total,
            result.events.len(),
            result.truncated
        );
        assert!(result.events.len() <= 20);
        assert!(result.total >= result.events.len());
    }
}
