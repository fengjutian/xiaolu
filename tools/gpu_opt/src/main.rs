use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$gpus=@()
Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ForEach-Object {
  $gpus += @{
    name=[string]$_.Name
    driver=[string]$_.DriverVersion
    date=if($_.DriverDate){$_.DriverDate.ToString('yyyy-MM-dd')}else{''}
    ram=[int64]$_.AdapterRAM
    status=if($_.Status){[string]$_.Status}else{''}
  }
}
$plan=(powercfg /getactivescheme 2>$null) -join ' '
@{gpus=@($gpus); plan=$plan} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取显卡信息：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析显卡信息：{error}")))?;
    let plan = payload.get("plan").and_then(Value::as_str).unwrap_or("");
    let gpus = payload
        .get("gpus")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut rows = Vec::new();
    let mut attention = 0usize;
    for gpu in &gpus {
        let name = gpu.get("name").and_then(Value::as_str).unwrap_or("未知显卡");
        let driver = gpu.get("driver").and_then(Value::as_str).unwrap_or("");
        let date = gpu.get("date").and_then(Value::as_str).unwrap_or("");
        let ram = gpu.get("ram").and_then(Value::as_u64).unwrap_or(0);
        let age_days = parse_date_days(date)
            .map(|days| (now - days) / 86_400)
            .unwrap_or(0);
        let suggestion = if driver.is_empty() {
            "未报告驱动版本，建议检查驱动"
        } else if age_days > 540 {
            "驱动日期较旧，建议关注厂商更新"
        } else if ram > 0 && ram < 1024 * 1024 * 1024 {
            "显存较小，降低游戏/渲染负载"
        } else {
            "保持"
        };
        if suggestion != "保持" {
            attention += 1;
        }
        rows.push(json!({
            "item": name,
            "value": format!("驱动 {driver} · {date}"),
            "suggestion": suggestion,
        }));
    }
    let plan_short = plan
        .split('(')
        .next()
        .unwrap_or("未知")
        .trim()
        .to_string();
    let plan_ok = plan_short.contains("高性能") || plan_short.contains("卓越");
    if !plan_ok {
        attention += 1;
    }
    rows.push(json!({
        "item": "电源计划",
        "value": if plan.is_empty() { "未知".to_string() } else { plan.to_string() },
        "suggestion": if plan_ok { "保持" } else { "游戏/渲染时可考虑高性能计划" },
    }));

    Ok(table(
        json!([
            item("显卡", gpus.len().to_string()),
            item("电源计划", if plan_short.is_empty() { "未知".to_string() } else { plan_short }),
            item("建议关注", attention.to_string()),
        ]),
        json!([
            column("item", "项目"),
            column("value", "当前值"),
            column("suggestion", "建议"),
        ]),
        Value::Array(rows),
        json!([
            "只读分析：只读取显卡与电源计划，不修改任何设置。",
            "驱动日期超过 540 天仅提示关注，不代表一定需要更新。",
            "切换电源计划/超频等写操作需后续版本确认后提供。",
        ]),
    ))
}

fn parse_date_days(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1970..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * 86_400)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_index = (month + 9) % 12;
    let doy = (153 * month_index + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn main() {
    tool_protocol::serve("gpu_opt", env!("CARGO_PKG_VERSION"), &["preview"], |_| scan());
}
