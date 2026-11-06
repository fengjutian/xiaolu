use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function RegCount($paths,$names){
  $count=0
  foreach($path in $paths){
    foreach($name in $names){
      try { $count += @((Get-Item -Path $path -ErrorAction Stop).GetValueNames() | Where-Object { $name -eq '*' -or $_ -like $name }).Count } catch {}
    }
  }
  return $count
}
$runPaths=@('HKCU:\Software\Microsoft\Windows\CurrentVersion\Run','HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce','HKLM:\Software\Microsoft\Windows\CurrentVersion\Run','HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce')
$startupCount=0
foreach($path in $runPaths){
  try { $startupCount += @((Get-Item -Path $path -ErrorAction Stop).GetValueNames()).Count } catch {}
}
$autoServices=0
try { $autoServices=@(Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.StartMode -eq 'Auto' }).Count } catch {}
$tasks=0
try { $tasks=@(Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' }).Count } catch {}

function FileSize($path){ if(Test-Path $path){ try { return (Get-Item $path -Force -ErrorAction Stop).Length } catch { return 0 } } return 0 }
$hiber=FileSize (Join-Path $env:SystemDrive 'hiberfil.sys')
$pagefile=FileSize (Join-Path $env:SystemDrive 'pagefile.sys')
$hibernateEnabled=0
try { $hibernateEnabled=[int](Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' -Name 'HibernateEnabled' -ErrorAction Stop).HibernateEnabled } catch {}

$tempBytes=0L; $tempFiles=0
try {
  $files=@(Get-ChildItem -Path $env:TEMP -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 20000)
  $tempFiles=$files.Count
  if($tempFiles -gt 0){ $tempBytes=[long](($files | Measure-Object -Property Length -Sum).Sum) }
} catch {}

@{startupCount=$startupCount;autoServices=$autoServices;tasks=$tasks;hiberBytes=$hiber;hiberEnabled=$hibernateEnabled;pagefileBytes=$pagefile;tempBytes=$tempBytes;tempFiles=$tempFiles} | ConvertTo-Json -Compress
"#
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn build_rows(payload: &Value) -> Vec<Value> {
    let mut rows = Vec::new();

    rows.push(json!({
        "item": "启动项",
        "current": format!("{} 项", number(payload, "startupCount")),
        "suggestion": "非必要项可用“软件权限管理”停用当前用户启动项",
        "impact": "中",
        "risk": "需确认",
        "note": "统计 HKCU/HKLM 的 Run 与 RunOnce",
    }));

    rows.push(json!({
        "item": "自动服务",
        "current": format!("{} 个", number(payload, "autoServices")),
        "suggestion": "检查第三方自动服务是否必须随系统启动",
        "impact": "中",
        "risk": "需管理员",
        "note": "StartMode = Auto 的服务总数",
    }));

    rows.push(json!({
        "item": "用户计划任务",
        "current": format!("{} 个", number(payload, "tasks")),
        "suggestion": "停用不再需要的更新/同步任务",
        "impact": "低",
        "risk": "需确认",
        "note": "排除 Microsoft 系统任务与已禁用任务",
    }));

    let hiber_bytes = number(payload, "hiberBytes");
    rows.push(json!({
        "item": "休眠文件",
        "current": if hiber_bytes > 0 { human_bytes(hiber_bytes) } else { "未启用".to_string() },
        "suggestion": if hiber_bytes > 0 {
            "不使用休眠/快速启动时，可关闭休眠释放 hiberfil.sys 空间"
        } else {
            "已关闭或无休眠文件"
        },
        "impact": if hiber_bytes > 0 { "中" } else { "无" },
        "risk": "需管理员",
        "note": "关闭休眠会同时禁用快速启动",
    }));

    let pagefile_bytes = number(payload, "pagefileBytes");
    rows.push(json!({
        "item": "页面文件",
        "current": if pagefile_bytes > 0 { human_bytes(pagefile_bytes) } else { "未检测到".to_string() },
        "suggestion": "建议保持系统托管；不要为释放空间直接关闭",
        "impact": "低",
        "risk": "高风险",
        "note": "禁用页面文件可能导致崩溃或应用异常",
    }));

    rows.push(json!({
        "item": "用户临时文件",
        "current": human_bytes(number(payload, "tempBytes")),
        "suggestion": "可用“清理大师”扫描后移入回收站",
        "impact": "低",
        "risk": "需确认",
        "note": format!("{} 个文件（单目录最多统计 2 万）", number(payload, "tempFiles")),
    }));

    rows
}

fn scan() -> Result<Value, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取优化候选：{error}")))?;
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
        .ok_or_else(|| ToolError::new("INTERNAL", "优化候选扫描没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析优化候选：{error}")))?;
    let rows = build_rows(&payload);
    let attention = rows
        .iter()
        .filter(|row| row.get("impact").and_then(Value::as_str) != Some("无"))
        .count();

    Ok(table(
        json!([
            item("候选类别", rows.len().to_string()),
            item("可关注项", attention.to_string()),
            item("启动项", number(&payload, "startupCount").to_string()),
            item("自动服务", number(&payload, "autoServices").to_string()),
            item("临时文件", human_bytes(number(&payload, "tempBytes"))),
        ]),
        json!([
            column("item", "候选"),
            column("current", "当前状态"),
            column("suggestion", "建议"),
            column("impact", "影响"),
            column("risk", "风险"),
            column("note", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "只读汇总：只统计候选项目，不修改启动项、服务、计划任务、休眠或页面文件。",
            "每项给出影响与风险；实际修改请使用对应工具或系统设置，并逐项确认。",
            "页面文件与休眠设置风险较高，不建议为“跑分”或“释放空间”直接关闭。",
            "本工具不提供一键优化，也不下载或执行任何优化脚本。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve("sys_boost", env!("CARGO_PKG_VERSION"), &["preview"], |_| scan());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        json!({
            "startupCount": 12,
            "autoServices": 80,
            "tasks": 7,
            "hiberBytes": 6_442_450_944u64,
            "hiberEnabled": 1,
            "pagefileBytes": 2_147_483_648u64,
            "tempBytes": 1_073_741_824u64,
            "tempFiles": 1234
        })
    }

    #[test]
    fn builds_six_candidates_with_risk_labels() {
        let rows = build_rows(&fixture());
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().any(|row| row["item"] == "休眠文件"
            && row["risk"] == "需管理员"
            && row["current"] == "6.0 GB"));
        assert!(rows.iter().any(|row| row["item"] == "页面文件"
            && row["risk"] == "高风险"));
        assert!(rows.iter().any(|row| row["item"] == "用户临时文件"
            && row["current"] == "1.0 GB"));
    }

    #[test]
    fn missing_values_do_not_fabricate() {
        let rows = build_rows(&json!({}));
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().any(|row| row["item"] == "休眠文件"
            && row["current"] == "未启用"));
        assert!(rows.iter().any(|row| row["item"] == "启动项"
            && row["current"] == "0 项"));
    }
}
