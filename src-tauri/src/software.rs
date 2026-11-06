use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftware {
    name: String,
    version: String,
    publisher: String,
    install_date: String,
    install_location: String,
    estimated_size_bytes: u64,
    scope: String,
    architecture: String,
    uninstall_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareInventory {
    captured_at_unix_ms: u128,
    total: usize,
    estimated_size_bytes: u64,
    items: Vec<InstalledSoftware>,
    source: String,
}

#[derive(Debug, Deserialize)]
struct SoftwarePayload { items: Vec<InstalledSoftware> }

#[cfg(windows)]
fn collect() -> Result<SoftwareInventory, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$targets=@(
  @{path='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='所有用户';arch='64-bit'},
  @{path='HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='所有用户';arch='32-bit'},
  @{path='HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='当前用户';arch='当前用户'}
)
$seen=@{};$items=[System.Collections.Generic.List[object]]::new()
foreach($target in $targets){Get-ItemProperty $target.path -ErrorAction SilentlyContinue|ForEach-Object{
  if($_.DisplayName -and -not $_.SystemComponent -and -not $_.ParentKeyName){
    $key=([string]$_.DisplayName+'|'+[string]$_.DisplayVersion+'|'+$target.scope).ToLowerInvariant()
    if(-not $seen.ContainsKey($key)){$seen[$key]=$true;$items.Add(@{name=[string]$_.DisplayName;version=if($_.DisplayVersion){[string]$_.DisplayVersion}else{''};publisher=if($_.Publisher){[string]$_.Publisher}else{''};installDate=if($_.InstallDate){[string]$_.InstallDate}else{''};installLocation=if($_.InstallLocation){[string]$_.InstallLocation}else{''};estimatedSizeBytes=if($_.EstimatedSize){[uint64]$_.EstimatedSize*1024}else{0};scope=$target.scope;architecture=$target.arch;uninstallAvailable=([bool]($_.UninstallString -or $_.QuietUninstallString))})}
  }
}}
@{items=@($items|Sort-Object name)}|ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动软件清单扫描：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let payload: SoftwarePayload = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析软件清单：{error}"))?;
    let total = payload.items.len();
    let estimated_size_bytes = payload.items.iter().map(|item| item.estimated_size_bytes).sum();
    Ok(SoftwareInventory { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), total, estimated_size_bytes, items: payload.items, source: "Windows Uninstall Registry (HKLM/HKCU)".into() })
}

#[cfg(not(windows))]
fn collect() -> Result<SoftwareInventory, String> { Ok(SoftwareInventory { captured_at_unix_ms: 0, total: 0, estimated_size_bytes: 0, items: vec![], source: "unsupported".into() }) }

#[tauri::command]
pub async fn get_software_inventory() -> Result<SoftwareInventory, String> {
    tauri::async_runtime::spawn_blocking(collect).await.map_err(|error| format!("软件清单任务失败：{error}"))?
}
