use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_APPLY: &str = "apply_settings";

#[derive(Clone, Copy)]
enum SettingKind {
    Dword,
    Text,
}

struct PrivacySetting {
    id: &'static str,
    label: &'static str,
    path: &'static str,
    name: &'static str,
    kind: SettingKind,
    options: &'static [(&'static str, &'static str)],
}

/// 只包含当前用户（HKCU）可写、可回读的 4 项；诊断数据策略仍是 HKLM 只读。
const SETTINGS: [PrivacySetting; 4] = [
    PrivacySetting {
        id: "advertising",
        label: "广告 ID",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\AdvertisingInfo",
        name: "Enabled",
        kind: SettingKind::Dword,
        options: &[("0", "关闭"), ("1", "启用")],
    },
    PrivacySetting {
        id: "location",
        label: "位置权限",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location",
        name: "Value",
        kind: SettingKind::Text,
        options: &[("Allow", "允许"), ("Deny", "拒绝")],
    },
    PrivacySetting {
        id: "camera",
        label: "相机权限",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam",
        name: "Value",
        kind: SettingKind::Text,
        options: &[("Allow", "允许"), ("Deny", "拒绝")],
    },
    PrivacySetting {
        id: "microphone",
        label: "麦克风权限",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone",
        name: "Value",
        kind: SettingKind::Text,
        options: &[("Allow", "允许"), ("Deny", "拒绝")],
    },
];

fn registry_error(context: &str, error: std::io::Error) -> ToolError {
    if error.raw_os_error() == Some(5) {
        ToolError::new("PERMISSION_DENIED", format!("{context}：拒绝访问。"))
    } else {
        ToolError::new("INTERNAL", format!("{context}：{error}"))
    }
}

fn read_setting(setting: &PrivacySetting) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(setting.path, KEY_READ)
        .ok()?;
    match setting.kind {
        SettingKind::Dword => key
            .get_value::<u32, _>(setting.name)
            .ok()
            .map(|value| value.to_string()),
        SettingKind::Text => key.get_value::<String, _>(setting.name).ok(),
    }
}

fn write_setting(setting: &PrivacySetting, value: &str) -> Result<(), ToolError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(setting.path)
        .map_err(|error| registry_error("打开隐私设置", error))?;
    match setting.kind {
        SettingKind::Dword => {
            let value = value.parse::<u32>().map_err(|_| {
                ToolError::new("INVALID_INPUT", format!("{}不是有效的数字。", setting.label))
            })?;
            key.set_value(setting.name, &value)
                .map_err(|error| registry_error("写入隐私设置", error))
        }
        SettingKind::Text => key
            .set_value(setting.name, &value.to_string())
            .map_err(|error| registry_error("写入隐私设置", error)),
    }
}

fn validate_option(setting: &PrivacySetting, value: &str) -> Result<(), ToolError> {
    if setting.options.iter().any(|(option, _)| *option == value) {
        Ok(())
    } else {
        Err(ToolError::new(
            "INVALID_INPUT",
            format!("{}不接受取值 {value}。", setting.label),
        ))
    }
}

fn settings_json() -> Vec<Value> {
    SETTINGS
        .iter()
        .map(|setting| {
            json!({
                "id": setting.id,
                "label": setting.label,
                "current": read_setting(setting),
                "options": setting
                    .options
                    .iter()
                    .map(|(value, label)| json!({"value": value, "label": label}))
                    .collect::<Vec<_>>(),
                "requiresAdmin": false,
            })
        })
        .collect()
}

fn settings_digest(settings: &[Value]) -> String {
    let parts = settings
        .iter()
        .map(|setting| {
            format!(
                "{}|{}",
                setting.get("id").and_then(Value::as_str).unwrap_or(""),
                setting
                    .get("current")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".into())
            )
        })
        .collect::<Vec<_>>();
    digest_of(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn snapshot_dir() -> PathBuf {
    tool_protocol::state_dir().join("privacy_protection")
}

fn write_snapshot(payload: &Value) -> Result<PathBuf, String> {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建快照目录：{error}"))?;
    let path = dir.join(format!(
        "apply-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化快照：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入快照：{error}"))?;
    Ok(path)
}

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function GetVal($path,$name){
  try { (Get-ItemProperty -Path $path -Name $name -ErrorAction Stop).$name } catch { $null }
}
$telemetry = GetVal 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'AllowTelemetry'
if($null -eq $telemetry){ $telemetry = GetVal 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection' 'AllowTelemetry' }
@{telemetry=$telemetry} | ConvertTo-Json -Compress
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

    let telemetry_text = match telemetry {
        Some(0) => "仅安全（0）",
        Some(1) => "基本（1）",
        Some(2) => "增强（2）",
        Some(3) => "完整（3）",
        Some(_) => "未知级别",
        None => "未显式配置",
    };
    let telemetry_concern = telemetry.map(|level| level >= 2).unwrap_or(false);

    let settings = settings_json();
    let mut attention = if telemetry_concern { 1 } else { 0 };
    let mut rows = Vec::new();
    rows.push(json!({
        "name": "诊断数据级别",
        "value": telemetry_text,
        "suggestion": if telemetry_concern { "建议检查" } else { "保持" },
        "note": "策略未配置时由系统默认值决定；2/3 会发送更多诊断数据。需管理员权限，本工具只读。",
    }));
    for setting in &settings {
        let id = setting.get("id").and_then(Value::as_str).unwrap_or("");
        let label = setting.get("label").and_then(Value::as_str).unwrap_or("");
        let current = setting.get("current").and_then(Value::as_str);
        let value = current
            .and_then(|current| {
                SETTINGS
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .and_then(|candidate| {
                        candidate
                            .options
                            .iter()
                            .find(|(option, _)| *option == current)
                            .map(|(_, label)| *label)
                    })
            })
            .unwrap_or("未记录");
        if current == Some("Allow") {
            attention += 1;
        }
        rows.push(json!({
            "name": label,
            "value": value,
            "suggestion": if current == Some("Allow") { "建议检查" } else { "保持" },
            "note": "可在下方直接改为“拒绝/关闭”，写入后可撤销。",
        }));
    }

    let mut data = table(
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
            "广告 ID、位置、相机、麦克风权限为当前用户设置，可直接应用并撤销。",
            "诊断数据级别位于 HKLM 策略，普通权限只读；修改需管理员。",
            "权限为 Allow 仅代表系统允许应用请求，不代表已被使用。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("privacySettings".into(), Value::Array(settings.clone()));
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_APPLY, &settings_digest(&settings)) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn apply_settings(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "修改隐私设置需要确认令牌，请先预览并选择。",
        )
    })?;
    let changes = request
        .parameters
        .get("changes")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 changes。"))?;
    if changes.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "没有需要应用的设置。"));
    }

    let before_settings = settings_json();
    tool_protocol::verify_token(token, ACTION_APPLY, &settings_digest(&before_settings))?;

    let started_at = tool_protocol::now_ms();
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    let mut undo = Vec::new();
    let mut applied = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for change in changes {
        let id = change
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", "设置项缺少 id。"))?;
        let setting = SETTINGS
            .iter()
            .find(|setting| setting.id == id)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", format!("不支持的设置：{id}")))?;
        if !seen.insert(setting.id) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("{} 重复出现。", setting.label),
            ));
        }
        let value = change
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", "设置项缺少 value。"))?;
        validate_option(setting, value)?;

        let before = read_setting(setting);
        if before.as_deref() == Some(value) {
            skipped += 1;
            rows.push(json!({
                "label": setting.label,
                "before": before.unwrap_or_else(|| "未记录".into()),
                "after": value,
                "status": "跳过",
                "reason": "已是目标值",
            }));
            continue;
        }
        match write_setting(setting, value) {
            Ok(()) => {
                let after = read_setting(setting);
                if after.as_deref() == Some(value) {
                    applied += 1;
                    rows.push(json!({
                        "label": setting.label,
                        "before": before.clone().unwrap_or_else(|| "未记录".into()),
                        "after": value,
                        "status": "已应用",
                        "reason": "",
                    }));
                    if let Some(before) = before {
                        undo.push(json!({"id": setting.id, "value": before}));
                    }
                } else {
                    failed += 1;
                    rows.push(json!({
                        "label": setting.label,
                        "before": before.unwrap_or_else(|| "未记录".into()),
                        "after": after.unwrap_or_else(|| "未记录".into()),
                        "status": "失败",
                        "reason": "写入后回读值与目标不一致",
                    }));
                }
            }
            Err(error) => {
                failed += 1;
                let before_text = before.unwrap_or_else(|| "未记录".into());
                rows.push(json!({
                    "label": setting.label,
                    "before": before_text,
                    "after": before_text,
                    "status": "失败",
                    "reason": error.message,
                }));
            }
        }
    }

    let after_settings = settings_json();
    let finished_at = tool_protocol::now_ms();
    let snapshot = json!({
        "tool": "privacy_protection",
        "version": env!("CARGO_PKG_VERSION"),
        "action": ACTION_APPLY,
        "startedAt": started_at,
        "finishedAt": finished_at,
        "before": before_settings,
        "after": after_settings,
        "results": rows.clone(),
    });
    let snapshot_path = write_snapshot(&snapshot);
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "privacy_protection",
        "action": ACTION_APPLY,
        "applied": applied,
        "skipped": skipped,
        "failed": failed,
        "snapshot": snapshot_path.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
        "at": finished_at,
    }));

    let mut data = table(
        json!([
            item("成功", applied.to_string()),
            item("跳过", skipped.to_string()),
            item("失败", failed.to_string()),
        ]),
        json!([
            column("label", "设置项"),
            column("before", "原值"),
            column("after", "新值"),
            column("status", "结果"),
            column("reason", "原因"),
        ]),
        Value::Array(rows),
        json!([
            "每项独立执行：单项失败不影响其他项，写入后立即回读确认。",
            "已生成新的一次性令牌，可直接“撤销本次修改”恢复原值。",
            "诊断数据级别位于 HKLM 策略，本工具不修改。",
            snapshot_path
                .as_ref()
                .map(|path| format!("本次原值与回读已写入快照：{}", path.display()))
                .unwrap_or_else(|error| format!("快照写入失败：{error}")),
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("privacySettings".into(), Value::Array(after_settings.clone()));
        map.insert("privacyUndo".into(), Value::Array(undo));
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_APPLY, &settings_digest(&after_settings)) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn main() {
    tool_protocol::serve_with_request(
        "privacy_protection",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_APPLY],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_APPLY => apply_settings(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_ids_are_unique() {
        let mut ids = SETTINGS
            .iter()
            .map(|setting| setting.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SETTINGS.len());
    }

    #[test]
    fn accepts_only_defined_options() {
        let advertising = SETTINGS.iter().find(|s| s.id == "advertising").unwrap();
        assert!(validate_option(advertising, "0").is_ok());
        assert!(validate_option(advertising, "1").is_ok());
        assert!(validate_option(advertising, "2").is_err());

        let camera = SETTINGS.iter().find(|s| s.id == "camera").unwrap();
        assert!(validate_option(camera, "Allow").is_ok());
        assert!(validate_option(camera, "Deny").is_ok());
        assert!(validate_option(camera, "allow").is_err());
    }

    #[test]
    fn digest_changes_with_current_value() {
        let first = vec![json!({"id": "advertising", "current": "1"})];
        let second = vec![json!({"id": "advertising", "current": "0"})];
        assert_ne!(settings_digest(&first), settings_digest(&second));
    }
}
