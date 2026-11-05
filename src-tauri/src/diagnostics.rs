use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryReport {
    present: bool,
    name: String,
    status: String,
    charge_percent: Option<f64>,
    design_capacity_mwh: Option<u64>,
    full_charge_capacity_mwh: Option<u64>,
    health_percent: Option<f64>,
    cycle_count: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    time_created: String,
    provider: String,
    event_id: u32,
    level: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    captured_at_unix_ms: u128,
    pending_reboot: bool,
    pending_windows_update_reboot: bool,
    battery: BatteryReport,
    events: Vec<DiagnosticEvent>,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellReport {
    pending_reboot: bool,
    pending_windows_update_reboot: bool,
    battery: BatteryReport,
    events: Vec<DiagnosticEvent>,
}

#[cfg(windows)]
fn collect() -> Result<DiagnosticReport, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$battery=Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue|Select-Object -First 1
$static=Get-CimInstance -Namespace root/wmi -ClassName BatteryStaticData -ErrorAction SilentlyContinue|Select-Object -First 1
$full=Get-CimInstance -Namespace root/wmi -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue|Select-Object -First 1
$cycle=Get-CimInstance -Namespace root/wmi -ClassName BatteryCycleCount -ErrorAction SilentlyContinue|Select-Object -First 1
$design=if($static){[uint64]$static.DesignedCapacity}else{$null};$fullCapacity=if($full){[uint64]$full.FullChargedCapacity}else{$null}
$health=if($design -and $fullCapacity){[math]::Round(($fullCapacity/$design)*100,1)}else{$null}
$batteryResult=@{present=($null -ne $battery);name=if($battery.Name){[string]$battery.Name}else{'未检测到电池'};status=if($battery.Status){[string]$battery.Status}else{'Unknown'};chargePercent=if($battery){[double]$battery.EstimatedChargeRemaining}else{$null};designCapacityMwh=$design;fullChargeCapacityMwh=$fullCapacity;healthPercent=$health;cycleCount=if($cycle){[uint64]$cycle.CycleCount}else{$null}}
$events=@()
Get-WinEvent -FilterHashtable @{LogName='System';Level=1,2;StartTime=(Get-Date).AddHours(-24)} -MaxEvents 25 -ErrorAction SilentlyContinue|ForEach-Object{$message=[string]$_.Message;if($message.Length -gt 500){$message=$message.Substring(0,500)+'…'};$events+=@{timeCreated=$_.TimeCreated.ToString('o');provider=[string]$_.ProviderName;eventId=[uint32]$_.Id;level=[string]$_.LevelDisplayName;message=$message}}
$cbs=Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending'
$wu=Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'
$pendingRename=$null -ne (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -Name PendingFileRenameOperations -ErrorAction SilentlyContinue)
@{pendingReboot=($cbs -or $wu -or $pendingRename);pendingWindowsUpdateReboot=$wu;battery=$batteryResult;events=@($events)}|ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动系统诊断：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let report: PowerShellReport = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析诊断结果：{error}"))?;
    Ok(DiagnosticReport { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), pending_reboot: report.pending_reboot, pending_windows_update_reboot: report.pending_windows_update_reboot, battery: report.battery, events: report.events, source: "Windows CIM / WMI / Event Log / Registry".into() })
}

#[cfg(not(windows))]
fn collect() -> Result<DiagnosticReport, String> {
    Ok(DiagnosticReport { captured_at_unix_ms: 0, pending_reboot: false, pending_windows_update_reboot: false, battery: BatteryReport { present: false, name: "未支持".into(), status: "Unknown".into(), charge_percent: None, design_capacity_mwh: None, full_charge_capacity_mwh: None, health_percent: None, cycle_count: None }, events: vec![], source: "unsupported".into() })
}

#[tauri::command]
pub async fn get_diagnostic_report() -> Result<DiagnosticReport, String> {
    tauri::async_runtime::spawn_blocking(collect).await.map_err(|error| format!("系统诊断任务失败：{error}"))?
}
