use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function GetVal($path,$name){
  try { (Get-ItemProperty -Path $path -Name $name -ErrorAction Stop).$name } catch { $null }
}
$telemetry = GetVal 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'AllowTelemetry'
if($null -eq $telemetry){ $telemetry = GetVal 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection' 'AllowTelemetry' }
$advertising = GetVal 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo' 'Enabled'
$location = GetVal 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location' 'Value'
$camera = GetVal 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam' 'Value'
$microphone = GetVal 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone' 'Value'
@{telemetry=$telemetry; advertising=$advertising; location=$location; camera=$camera; microphone=$microphone} | ConvertTo-Json -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取隐私设置：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析隐私设置：{error}")))?;

    let telemetry = payload.get("telemetry").and_then(Value::as_u64);
    let advertising = payload.get("advertising").and_then(Value::as_u64);
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    let telemetry_text = match telemetry {
        Some(0) => "仅安全（0）",
        Some(1) => "基本（1）",
        Some(2) => "增强（2）",
        Some(3) => "完整（3）",
        Some(_) => "未知级别",
        None => "未显式配置",
    };
    let advertising_text = match advertising {
        Some(0) => "已关闭",
        Some(1) => "已启用",
        _ => "未知",
    };
    let location = text("location");
    let camera = text("camera");
    let microphone = text("microphone");

    let mut attention = 0usize;
    let mut rows = Vec::new();
    let mut push = |name: &str, value: String, concern: bool, note: &str| {
        if concern {
            attention += 1;
        }
        rows.push(json!({
            "name": name,
            "value": value,
            "suggestion": if concern { "建议检查" } else { "保持" },
            "note": note,
        }));
    };
    push(
        "诊断数据级别",
        telemetry_text.into(),
        telemetry.map(|level| level >= 2).unwrap_or(false),
        "策略未配置时由系统默认值决定；2/3 会发送更多诊断数据。",
    );
    push(
        "广告 ID",
        advertising_text.into(),
        advertising == Some(1),
        "启用后应用可使用广告 ID 做个性化推荐。",
    );
    push(
        "位置权限",
        if location.is_empty() { "未记录".into() } else { location.clone() },
        location.eq_ignore_ascii_case("Allow"),
        "Allow 表示应用可请求位置；可在系统设置中逐应用调整。",
    );
    push(
        "相机权限",
        if camera.is_empty() { "未记录".into() } else { camera.clone() },
        camera.eq_ignore_ascii_case("Allow"),
        "Allow 表示应用可请求相机；此处不打开相机。",
    );
    push(
        "麦克风权限",
        if microphone.is_empty() { "未记录".into() } else { microphone.clone() },
        microphone.eq_ignore_ascii_case("Allow"),
        "Allow 表示应用可请求麦克风；此处不录音。",
    );

    Ok(table(
        json!([
            item("检查项", rows.len().to_string()),
            item("建议关注", attention.to_string()),
        ]),
        json!([
            column("name", "设置"),
            column("value", "当前值"),
            column("suggestion", "建议"),
            column("note", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "只读检查：只读取当前用户与策略注册表值，不修改任何设置。",
            "权限为 Allow 仅代表系统允许应用请求，不代表已被使用。",
            "修改隐私设置需后续版本逐项确认后提供。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "privacy_protection",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}
