use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_GPU_PREF: &str = "set_gpu_preference";
const GPU_PREF_PATH: &str = "Software\\Microsoft\\DirectX\\UserGpuPreferences";

fn parse_preference(data: &str) -> u32 {
    data.split(';')
        .find_map(|part| part.trim().strip_prefix("GpuPreference="))
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

fn preference_label(value: u32) -> &'static str {
    match value {
        1 => "省电",
        2 => "高性能",
        _ => "由 Windows 决定",
    }
}

fn read_preferences() -> Vec<Value> {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(GPU_PREF_PATH, KEY_READ)
    else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for (name, value) in key.enum_values().flatten() {
        let data = String::from_utf16_lossy(
            &value
                .bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        );
        let data = data.trim_end_matches('\0').to_string();
        let preference = parse_preference(&data);
        items.push(json!({
            "path": name,
            "preference": preference,
            "label": preference_label(preference),
        }));
    }
    items.sort_by(|left, right| {
        left.get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase()
            .cmp(
                &right
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase(),
            )
    });
    items
}

fn hags_enabled() -> Option<bool> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers",
            KEY_READ,
        )
        .ok()?
        .get_value::<u32, _>("HwSchMode")
        .ok()
        .map(|mode| mode == 2)
}

fn set_gpu_preference(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new("CONFIRMATION_REQUIRED", "修改 GPU 偏好需要确认令牌，请先预览。")
    })?;
    tool_protocol::verify_token(token, ACTION_GPU_PREF, &digest_of(&["gpu_pref"]))?;
    let path = request
        .parameters
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 path。"))?
        .trim();
    if path.is_empty() || path.len() > 260 || !Path::new(path).is_absolute() {
        return Err(ToolError::new("INVALID_INPUT", "必须是绝对路径。"));
    }
    if !path.to_ascii_lowercase().ends_with(".exe") {
        return Err(ToolError::new("INVALID_INPUT", "只接受 .exe 路径。"));
    }
    let preference = request
        .parameters
        .get("preference")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 preference。"))?;
    if preference > 2 {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "preference 只接受 0（默认）、1（省电）、2（高性能）。",
        ));
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .create_subkey(GPU_PREF_PATH)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开 GPU 偏好注册表：{error}")))?
        .0;
    if preference == 0 {
        let _ = key.delete_value(path);
    } else {
        key.set_value(path, &format!("GpuPreference={preference};"))
            .map_err(|error| ToolError::new("INTERNAL", format!("无法写入 GPU 偏好：{error}")))?;
    }
    let after = read_preferences();
    let current = after
        .iter()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(path))
        .cloned()
        .unwrap_or(json!({"preference": 0}));
    let applied = current.get("preference").and_then(Value::as_u64).unwrap_or(0);
    if applied != preference {
        return Err(ToolError::new("INTERNAL", "写入后回读不一致。"));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "gpu_opt",
        "action": ACTION_GPU_PREF,
        "path": path,
        "preference": preference,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([
            item("应用", path),
            item("偏好", preference_label(preference as u32)),
        ]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{
            "item": path,
            "value": preference_label(preference as u32),
        }]),
        json!([
            "只写当前用户的 UserGpuPreferences，不修改驱动或全局显卡设置。",
            "设为“由 Windows 决定”会删除该应用的偏好值。",
        ]),
    ))
}

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

    let mut data = table(
        json!([
            item("显卡", gpus.len().to_string()),
            item("电源计划", if plan_short.is_empty() { "未知".to_string() } else { plan_short }),
            item("建议关注", attention.to_string()),
            item("硬件加速 GPU 计划", match hags_enabled() { Some(true) => "已开启", Some(false) => "已关闭", None => "未知" }),
        ]),
        json!([
            column("item", "项目"),
            column("value", "当前值"),
            column("suggestion", "建议"),
        ]),
        Value::Array(rows),
        json!([
            "分析部分只读；下方“按应用 GPU 偏好”写当前用户 HKCU，可随时恢复默认。",
            "驱动日期超过 540 天仅提示关注，不代表一定需要更新。",
            "不修改驱动、不超频、不改全局显卡设置。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("gpuPreferences".into(), Value::Array(read_preferences()));
        if let Ok(token) =
            tool_protocol::issue_token(ACTION_GPU_PREF, &digest_of(&["gpu_pref"]))
        {
            attach_token(&mut data, token);
        }
    }
    Ok(data)
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
    tool_protocol::serve_with_request(
        "gpu_opt",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_GPU_PREF],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_GPU_PREF => set_gpu_preference(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_preference_strings() {
        assert_eq!(parse_preference("GpuPreference=2;"), 2);
        assert_eq!(parse_preference("GpuPreference=1;Other=1;"), 1);
        assert_eq!(parse_preference(""), 0);
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(preference_label(1), "省电");
        assert_eq!(preference_label(2), "高性能");
        assert_eq!(preference_label(0), "由 Windows 决定");
    }
}