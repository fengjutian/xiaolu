use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    Ok(table(
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
            "所有建议都需要用户在“设置”中手动调整；本工具不提供“一键优化”。",
            "部分策略值受组织管理或需要管理员权限，读取失败会显示“未知”，不推断。",
            "修改系统设置前建议创建还原点；本工具不创建、不删除还原点。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "winopt_assistant",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
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
}
