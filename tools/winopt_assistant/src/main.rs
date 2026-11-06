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
enum SettingType {
    Dword,
    Text,
}

struct Setting {
    id: &'static str,
    label: &'static str,
    path: &'static str,
    name: &'static str,
    kind: SettingType,
    min: i64,
    max: i64,
    options: &'static [(i64, &'static str)],
}

/// 只包含当前用户（HKCU）可安全读写的 5 项；HKLM 项继续只读。
const SETTINGS: [Setting; 5] = [
    Setting {
        id: "visualFx",
        label: "视觉效果",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects",
        name: "VisualFXSetting",
        kind: SettingType::Dword,
        min: 0,
        max: 3,
        options: &[
            (0, "由 Windows 选择"),
            (1, "最佳外观"),
            (2, "最佳性能"),
            (3, "自定义"),
        ],
    },
    Setting {
        id: "transparency",
        label: "透明效果",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        name: "EnableTransparency",
        kind: SettingType::Dword,
        min: 0,
        max: 1,
        options: &[(0, "关闭"), (1, "开启")],
    },
    Setting {
        id: "windowAnimate",
        label: "窗口动画",
        path: "Control Panel\\Desktop\\WindowMetrics",
        name: "MinAnimate",
        kind: SettingType::Text,
        min: 0,
        max: 1,
        options: &[(0, "关闭"), (1, "开启")],
    },
    Setting {
        id: "storageSense",
        label: "存储感知",
        path: "Software\\Microsoft\\Windows\\CurrentVersion\\StorageSense\\Parameters\\StoragePolicy",
        name: "01",
        kind: SettingType::Dword,
        min: 0,
        max: 1,
        options: &[(0, "关闭"), (1, "开启")],
    },
    Setting {
        id: "gameMode",
        label: "游戏模式",
        path: "Software\\Microsoft\\GameBar",
        name: "AutoGameModeEnabled",
        kind: SettingType::Dword,
        min: 0,
        max: 1,
        options: &[(0, "关闭"), (1, "开启")],
    },
];

fn registry_error(context: &str, error: std::io::Error) -> ToolError {
    if error.raw_os_error() == Some(5) {
        ToolError::new("PERMISSION_DENIED", format!("{context}：拒绝访问。"))
    } else {
        ToolError::new("INTERNAL", format!("{context}：{error}"))
    }
}

fn read_setting(setting: &Setting) -> Option<i64> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(setting.path, KEY_READ)
        .ok()?;
    match setting.kind {
        SettingType::Dword => key
            .get_value::<u32, _>(setting.name)
            .ok()
            .map(|value| value as i64),
        SettingType::Text => key
            .get_value::<String, _>(setting.name)
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok()),
    }
}

fn write_setting(setting: &Setting, value: i64) -> Result<(), ToolError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(setting.path)
        .map_err(|error| registry_error("打开设置注册表", error))?;
    let result = match setting.kind {
        SettingType::Dword => key.set_value(setting.name, &(value as u32)),
        SettingType::Text => key.set_value(setting.name, &value.to_string()),
    };
    result.map_err(|error| registry_error("写入设置", error))
}

/// 删除设置值，用于把“原本未设置”的项还原为未设置。
fn delete_setting(setting: &Setting) -> Result<(), ToolError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(setting.path, KEY_WRITE) {
        Ok(key) => key,
        Err(error) if error.raw_os_error() == Some(2) => return Ok(()),
        Err(error) => return Err(registry_error("打开设置注册表", error)),
    };
    match key.delete_value(setting.name) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(2) => Ok(()),
        Err(error) => Err(registry_error("删除设置", error)),
    }
}

fn validate_setting_value(setting: &Setting, value: i64) -> Result<(), ToolError> {
    if value < setting.min || value > setting.max {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!("{}的取值必须在 {}–{} 之间。", setting.label, setting.min, setting.max),
        ));
    }
    if !setting.options.is_empty() && !setting.options.iter().any(|(option, _)| *option == value) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!("{}不接受取值 {value}。", setting.label),
        ));
    }
    Ok(())
}

fn display_setting(setting: &Setting, value: Option<i64>) -> String {
    match value {
        Some(value) => setting
            .options
            .iter()
            .find(|(option, _)| *option == value)
            .map(|(_, label)| (*label).to_string())
            .unwrap_or_else(|| value.to_string()),
        None => "未设置".into(),
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
                "min": setting.min,
                "max": setting.max,
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
    let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
    digest_of(&refs)
}

fn snapshot_dir() -> PathBuf {
    tool_protocol::state_dir().join("winopt_assistant")
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

fn setting_by_id(id: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.id == id)
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function Reg($path,$name){
  try { return (Get-ItemProperty -Path $path -Name $name -ErrorAction Stop).$name } catch { return $null }
}
$svc = Get-Service -Name WSearch -ErrorAction SilentlyContinue
$data=@{
  visualFx = Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects' 'VisualFXSetting'
  transparency = Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency'
  windowAnimate = Reg 'HKCU:\Control Panel\Desktop\WindowMetrics' 'MinAnimate'
  storageSense = Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy' '01'
  hibernate = Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' 'HibernateEnabled'
  fastStartup = Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power' 'HiberbootEnabled'
  gameMode = Reg 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled'
  telemetry = Reg 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection' 'AllowTelemetry'
  systemRestore = Reg 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' 'RPSessionInterval'
  systemRestoreDisabled = Reg 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' 'DisableSR'
  longPaths = Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' 'LongPathsEnabled'
  searchService = if($svc){ [string]$svc.Status } else { 'NotFound' }
}
$data | ConvertTo-Json -Compress
"#
}

fn text(value: &Value, key: &str) -> Option<String> {
    let item = value.get(key)?;
    if item.is_null() {
        return None;
    }
    match item {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(if *flag { "1" } else { "0" }.to_string()),
        _ => None,
    }
}

fn number(value: &Value, key: &str) -> Option<i64> {
    text(value, key)?.parse::<i64>().ok()
}

fn row(item: &str, current: impl Into<String>, suggestion: &str, level: &str) -> Value {
    json!({
        "item": item,
        "current": current.into(),
        "suggestion": suggestion,
        "level": level,
    })
}

fn build_rows(payload: &Value) -> Vec<Value> {
    let mut rows = Vec::new();

    rows.push(match text(payload, "visualFx").as_deref() {
        Some("0") => row("视觉效果", "由 Windows 选择", "保持默认即可", "正常"),
        Some("1") => row(
            "视觉效果",
            "最佳外观",
            "如追求流畅，可改为“调整为最佳性能”或自定义",
            "可选",
        ),
        Some("2") => row(
            "视觉效果",
            "最佳性能",
            "外观优先时可改回“由 Windows 选择”",
            "可选",
        ),
        Some("3") => row("视觉效果", "自定义", "按需保留当前组合", "正常"),
        Some(_) => row("视觉效果", "未知", "读取到的值无法识别", "未知"),
        None => row(
            "视觉效果",
            "未设置（由 Windows 选择）",
            "Windows 会按硬件能力自动选择",
            "正常",
        ),
    });

    rows.push(match number(payload, "transparency") {
        Some(1) => row(
            "透明效果",
            "已开启",
            "在意省电或低性能设备可关闭透明效果",
            "可选",
        ),
        Some(0) => row("透明效果", "已关闭", "保持关闭可减少合成开销", "正常"),
        _ => row("透明效果", "未知", "读取用户设置失败", "未知"),
    });

    rows.push(match text(payload, "windowAnimate").as_deref() {
        Some("1") => row("窗口动画", "已开启", "保持开启便于识别窗口切换", "正常"),
        Some("0") => row(
            "窗口动画",
            "已关闭",
            "关闭后操作更直接，但可能降低切换反馈",
            "可选",
        ),
        _ => row("窗口动画", "未知", "读取用户设置失败", "未知"),
    });

    rows.push(match number(payload, "storageSense") {
        Some(1) => row(
            "存储感知",
            "已开启",
            "保持开启可自动清理临时文件与回收站",
            "正常",
        ),
        Some(0) => row(
            "存储感知",
            "已关闭",
            "建议开启，自动清理临时文件与旧回收站内容",
            "建议调整",
        ),
        _ => row(
            "存储感知",
            "未知",
            "可在“设置 → 系统 → 存储”中检查",
            "未知",
        ),
    });

    rows.push(match number(payload, "hibernate") {
        Some(1) => row(
            "休眠",
            "已开启",
            "台式机不常用休眠时可关闭以释放 hiberfil.sys 空间",
            "可选",
        ),
        Some(0) => row("休眠", "已关闭", "如需休眠/快速启动可重新开启", "正常"),
        _ => row(
            "休眠",
            "未知",
            "注册表未配置或系统不支持休眠，可在“电源选项”中查看",
            "未知",
        ),
    });

    rows.push(match number(payload, "fastStartup") {
        Some(1) => row(
            "快速启动",
            "已开启",
            "双系统或更新故障排查时可临时关闭",
            "正常",
        ),
        Some(0) => row(
            "快速启动",
            "已关闭",
            "关闭后关机更彻底，但启动稍慢",
            "可选",
        ),
        _ => row("快速启动", "未知", "读取电源设置失败", "未知"),
    });

    rows.push(match number(payload, "gameMode") {
        Some(1) => row("游戏模式", "已开启", "保持开启即可", "正常"),
        Some(0) => row(
            "游戏模式",
            "已关闭",
            "常玩游戏时建议开启，减少后台干扰",
            "可选",
        ),
        _ => row(
            "游戏模式",
            "未设置（Windows 默认开启）",
            "未显式配置时使用系统默认值",
            "正常",
        ),
    });

    rows.push(match number(payload, "telemetry") {
        Some(0) => row("诊断数据", "仅必需（安全）", "保持最低级别即可", "正常"),
        Some(1) => row("诊断数据", "基本", "如需更少上报可降到“仅必需”", "可选"),
        Some(2) => row("诊断数据", "增强", "建议降到“基本”或“仅必需”", "建议调整"),
        Some(3) => row("诊断数据", "完整", "建议降到“基本”或“仅必需”", "建议调整"),
        _ => row(
            "诊断数据",
            "未知",
            "可能由组织策略管理，普通用户无权读取",
            "未知",
        ),
    });

    let restore_interval = number(payload, "systemRestore");
    let restore_disabled_reg = number(payload, "systemRestoreDisabled");
    let restore_known = restore_interval.is_some() || restore_disabled_reg.is_some();
    let restore_disabled =
        restore_disabled_reg == Some(1) || restore_interval == Some(0);
    rows.push(if !restore_known {
        row(
            "系统还原",
            "未知",
            "读取系统还原配置失败，可能由组织策略管理",
            "未知",
        )
    } else if restore_disabled {
        row(
            "系统还原",
            "已关闭",
            "建议至少保留系统盘还原点，便于故障恢复",
            "建议调整",
        )
    } else {
        row("系统还原", "已启用或未禁用", "保持可用即可", "正常")
    });

    rows.push(match text(payload, "searchService").as_deref() {
        Some("Running") => row("Windows 搜索", "运行中", "保持运行即可", "正常"),
        Some("Stopped") => row(
            "Windows 搜索",
            "已停止",
            "如需快速搜索文件与设置，可启动 WSearch 服务",
            "可选",
        ),
        Some("Paused") => row("Windows 搜索", "已暂停", "恢复服务可继续索引", "可选"),
        Some("NotFound") => row(
            "Windows 搜索",
            "未安装或已移除",
            "如需搜索文件与设置，可启用 Windows Search 功能",
            "可选",
        ),
        _ => row("Windows 搜索", "未知", "读取服务状态失败", "未知"),
    });

    rows.push(match number(payload, "longPaths") {
        Some(1) => row("长路径支持", "已开启", "保持开启即可", "正常"),
        Some(0) => row(
            "长路径支持",
            "已关闭",
            "如遇路径过长错误，可开启 LongPathsEnabled",
            "可选",
        ),
        _ => row("长路径支持", "未知", "读取文件系统设置失败", "未知"),
    });

    rows
}

fn scan() -> Result<Value, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取系统设置：{error}")))?;
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
        .ok_or_else(|| ToolError::new("INTERNAL", "系统设置检查没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析系统设置：{error}")))?;
    let rows = build_rows(&payload);
    let suggest = rows
        .iter()
        .filter(|row| row.get("level").and_then(Value::as_str) == Some("建议调整"))
        .count();
    let optional = rows
        .iter()
        .filter(|row| row.get("level").and_then(Value::as_str) == Some("可选"))
        .count();
    let unknown = rows
        .iter()
        .filter(|row| row.get("level").and_then(Value::as_str) == Some("未知"))
        .count();

    let mut data = table(
        json!([
            item("检查项", rows.len().to_string()),
            item("建议调整", suggest.to_string()),
            item("可选优化", optional.to_string()),
            item("无法读取", unknown.to_string()),
        ]),
        json!([
            column("item", "设置项"),
            column("current", "当前状态"),
            column("suggestion", "建议"),
            column("level", "级别"),
        ]),
        Value::Array(rows),
        json!([
            "只读体检：只读取注册表与服务状态，不修改任何设置。",
            "视觉效果、透明效果、窗口动画、存储感知、游戏模式 5 项支持直接应用与还原；其余项需要管理员权限，保持只读。",
            "部分策略值受组织管理或需要管理员权限，读取失败会显示“未知”，不推断。",
            "修改系统设置前建议创建还原点；本工具不创建、不删除还原点。",
        ]),
    );
    let settings = settings_json();
    if let Value::Object(map) = &mut data {
        map.insert("winoptSettings".into(), Value::Array(settings.clone()));
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
            "修改系统设置需要确认令牌，请先体检并选择要应用的项。",
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
        let setting = setting_by_id(id).ok_or_else(|| {
            ToolError::new("INVALID_INPUT", format!("不支持或需要管理员的设置：{id}"))
        })?;
        if !seen.insert(setting.id) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("{} 重复出现。", setting.label),
            ));
        }
        let value = match change.get("value") {
            Some(Value::Null) => None,
            Some(value) => Some(value.as_i64().ok_or_else(|| {
                ToolError::new("INVALID_INPUT", "设置项的 value 必须是整数或 null。")
            })?),
            None => return Err(ToolError::new("INVALID_INPUT", "设置项缺少 value。")),
        };
        if let Some(value) = value {
            validate_setting_value(setting, value)?;
        }

        let before = read_setting(setting);
        if before == value {
            skipped += 1;
            rows.push(json!({
                "label": setting.label,
                "before": display_setting(setting, before),
                "after": display_setting(setting, value),
                "status": "跳过",
                "reason": "已是目标值",
            }));
            continue;
        }
        let result = match value {
            Some(value) => write_setting(setting, value),
            None => delete_setting(setting),
        };
        match result {
            Ok(()) => {
                let after = read_setting(setting);
                if after == value {
                    applied += 1;
                    // 原值为 null 时，撤销动作会删除该值，回到“未设置”。
                    undo.push(json!({"id": setting.id, "value": before}));
                    rows.push(json!({
                        "label": setting.label,
                        "before": display_setting(setting, before),
                        "after": display_setting(setting, value),
                        "status": "已应用",
                        "reason": "",
                    }));
                } else {
                    failed += 1;
                    rows.push(json!({
                        "label": setting.label,
                        "before": display_setting(setting, before),
                        "after": display_setting(setting, after),
                        "status": "失败",
                        "reason": "写入后回读值与目标不一致",
                    }));
                }
            }
            Err(error) => {
                failed += 1;
                rows.push(json!({
                    "label": setting.label,
                    "before": display_setting(setting, before),
                    "after": display_setting(setting, before),
                    "status": "失败",
                    "reason": error.message,
                }));
            }
        }
    }

    let after_settings = settings_json();
    let finished_at = tool_protocol::now_ms();
    let snapshot = json!({
        "tool": "winopt_assistant",
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
        "tool": "winopt_assistant",
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
            "视觉效果、窗口动画等可能在下次登录或资源管理器重启后生效。",
            "休眠、快速启动、诊断数据、系统还原、长路径需要管理员权限，本工具保持只读。",
            snapshot_path
                .as_ref()
                .map(|path| format!("本次原值与回读已写入快照：{}", path.display()))
                .unwrap_or_else(|error| format!("快照写入失败：{error}")),
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("winoptSettings".into(), Value::Array(after_settings.clone()));
        map.insert("winoptUndo".into(), Value::Array(undo));
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_APPLY, &settings_digest(&after_settings)) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn main() {
    tool_protocol::serve_with_request(
        "winopt_assistant",
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

    fn levels(rows: &[Value]) -> Vec<&str> {
        rows.iter()
            .map(|row| row.get("level").and_then(Value::as_str).unwrap_or(""))
            .collect()
    }

    #[test]
    fn flags_common_recommendations() {
        let payload = json!({
            "visualFx": 1,
            "transparency": 1,
            "windowAnimate": "1",
            "storageSense": 0,
            "hibernate": 1,
            "fastStartup": 1,
            "gameMode": 0,
            "telemetry": 3,
            "systemRestore": 0,
            "systemRestoreDisabled": 0,
            "longPaths": 0,
            "searchService": "Running"
        });
        let rows = build_rows(&payload);
        assert_eq!(rows.len(), 11);
        let levels = levels(&rows);
        assert_eq!(
            levels.iter().filter(|level| **level == "建议调整").count(),
            3
        );
        assert!(rows
            .iter()
            .any(|row| row["item"] == "诊断数据"
                && row["level"] == "建议调整"));
        assert!(rows
            .iter()
            .any(|row| row["item"] == "系统还原" && row["level"] == "建议调整"));
    }

    #[test]
    fn treats_missing_values_as_unknown_without_guessing() {
        let rows = build_rows(&json!({}));
        assert_eq!(rows.len(), 11);
        let unknown = rows
            .iter()
            .filter(|row| row.get("level").and_then(Value::as_str) == Some("未知"))
            .count();
        assert!(unknown >= 9, "most missing values stay unknown: {unknown}");
        assert!(rows
            .iter()
            .any(|row| row["item"] == "视觉效果" && row["level"] == "正常"));
    }

    #[test]
    fn accepts_string_dwords_from_powershell() {
        let rows = build_rows(&json!({
            "storageSense": "1",
            "telemetry": "1",
            "searchService": "Stopped"
        }));
        assert!(rows
            .iter()
            .any(|row| row["item"] == "存储感知" && row["level"] == "正常"));
        assert!(rows
            .iter()
            .any(|row| row["item"] == "诊断数据" && row["level"] == "可选"));
        assert!(rows
            .iter()
            .any(|row| row["item"] == "Windows 搜索" && row["level"] == "可选"));
    }

    #[test]
    fn validates_setting_values_against_options() {
        let transparency = setting_by_id("transparency").expect("transparency");
        assert!(validate_setting_value(transparency, 0).is_ok());
        assert!(validate_setting_value(transparency, 1).is_ok());
        assert!(validate_setting_value(transparency, 2).is_err());

        let visual = setting_by_id("visualFx").expect("visual fx");
        assert!(validate_setting_value(visual, 3).is_ok());
        assert!(validate_setting_value(visual, 4).is_err());
        assert!(setting_by_id("hibernate").is_none());
    }

    #[test]
    fn displays_option_labels_and_missing_values() {
        let setting = setting_by_id("visualFx").expect("visual fx");
        assert_eq!(display_setting(setting, Some(2)), "最佳性能");
        assert_eq!(display_setting(setting, Some(9)), "9");
        assert_eq!(display_setting(setting, None), "未设置");
    }

    #[test]
    fn settings_digest_binds_current_values() {
        let first = vec![json!({"id": "visualFx", "current": 1})];
        let second = vec![json!({"id": "visualFx", "current": 2})];
        assert_ne!(settings_digest(&first), settings_digest(&second));
        assert_eq!(
            settings_digest(&first),
            settings_digest(&vec![json!({"id": "visualFx", "current": 1})])
        );
    }
}
