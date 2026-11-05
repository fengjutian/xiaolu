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
pub struct ServiceIssue {
    name: String,
    display_name: String,
    state: String,
    start_mode: String,
    exit_code: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    captured_at_unix_ms: u128,
    pending_reboot: bool,
    pending_windows_update_reboot: bool,
    battery: BatteryReport,
    events: Vec<DiagnosticEvent>,
    crash_events: Vec<DiagnosticEvent>,
    service_issues: Vec<ServiceIssue>,
    minidump_count: usize,
    latest_minidump: String,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellReport {
    pending_reboot: bool,
    pending_windows_update_reboot: bool,
    battery: BatteryReport,
    events: Vec<DiagnosticEvent>,
    crash_events: Vec<DiagnosticEvent>,
    service_issues: Vec<ServiceIssue>,
    minidump_count: usize,
    latest_minidump: String,
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
$crashes=@()
Get-WinEvent -FilterHashtable @{LogName='System';Id=41,6008;StartTime=(Get-Date).AddDays(-7)} -MaxEvents 15 -ErrorAction SilentlyContinue|ForEach-Object{$message=[string]$_.Message;if($message.Length -gt 500){$message=$message.Substring(0,500)+'…'};$crashes+=@{timeCreated=$_.TimeCreated.ToString('o');provider=[string]$_.ProviderName;eventId=[uint32]$_.Id;level=[string]$_.LevelDisplayName;message=$message}}
Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000,1001;StartTime=(Get-Date).AddDays(-7)} -MaxEvents 15 -ErrorAction SilentlyContinue|ForEach-Object{$message=[string]$_.Message;if($message.Length -gt 500){$message=$message.Substring(0,500)+'…'};$crashes+=@{timeCreated=$_.TimeCreated.ToString('o');provider=[string]$_.ProviderName;eventId=[uint32]$_.Id;level=[string]$_.LevelDisplayName;message=$message}}
$serviceIssues=@();Get-CimInstance Win32_Service -ErrorAction SilentlyContinue|Where-Object{$_.StartMode -eq 'Auto' -and $_.State -ne 'Running' -and -not $_.DelayedAutoStart}|Select-Object -First 50|ForEach-Object{$serviceIssues+=@{name=[string]$_.Name;displayName=[string]$_.DisplayName;state=[string]$_.State;startMode=[string]$_.StartMode;exitCode=[uint32]$_.ExitCode}}
$dumps=@();if(Test-Path "$env:SystemRoot\Minidump"){$dumps=@(Get-ChildItem "$env:SystemRoot\Minidump\*.dmp" -File -ErrorAction SilentlyContinue|Sort-Object LastWriteTime -Descending)}
$cbs=Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending'
$wu=Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'
$pendingRename=$null -ne (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -Name PendingFileRenameOperations -ErrorAction SilentlyContinue)
@{pendingReboot=($cbs -or $wu -or $pendingRename);pendingWindowsUpdateReboot=$wu;battery=$batteryResult;events=@($events);crashEvents=@($crashes|Sort-Object timeCreated -Descending);serviceIssues=@($serviceIssues);minidumpCount=[int]$dumps.Count;latestMinidump=if($dumps.Count){$dumps[0].LastWriteTime.ToString('o')}else{''}}|ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动系统诊断：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let report: PowerShellReport = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析诊断结果：{error}"))?;
    Ok(DiagnosticReport { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), pending_reboot: report.pending_reboot, pending_windows_update_reboot: report.pending_windows_update_reboot, battery: report.battery, events: report.events, crash_events: report.crash_events, service_issues: report.service_issues, minidump_count: report.minidump_count, latest_minidump: report.latest_minidump, source: "Windows CIM / WMI / Event Log / Registry".into() })
}

#[cfg(not(windows))]
fn collect() -> Result<DiagnosticReport, String> {
    Ok(DiagnosticReport { captured_at_unix_ms: 0, pending_reboot: false, pending_windows_update_reboot: false, battery: BatteryReport { present: false, name: "未支持".into(), status: "Unknown".into(), charge_percent: None, design_capacity_mwh: None, full_charge_capacity_mwh: None, health_percent: None, cycle_count: None }, events: vec![], crash_events: vec![], service_issues: vec![], minidump_count: 0, latest_minidump: String::new(), source: "unsupported".into() })
}

#[tauri::command]
pub async fn get_diagnostic_report() -> Result<DiagnosticReport, String> {
    tauri::async_runtime::spawn_blocking(collect).await.map_err(|error| format!("系统诊断任务失败：{error}"))?
}
