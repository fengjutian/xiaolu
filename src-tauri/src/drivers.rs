use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverInfo {
    device_name: String,
    device_class: String,
    manufacturer: String,
    driver_provider: String,
    driver_version: String,
    driver_date: String,
    inf_name: String,
    signer: String,
    signed: bool,
    device_id: String,
    status: String,
    error_code: u32,
    missing_driver: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverInventory {
    captured_at_unix_ms: u128,
    total: usize,
    signed: usize,
    unsigned: usize,
    missing: usize,
    problems: usize,
    items: Vec<DriverInfo>,
    source: String,
}

#[derive(Debug, Deserialize)]
struct DriverPayload { items: Vec<DriverInfo> }

#[cfg(windows)]
pub fn collect() -> Result<DriverInventory, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$map=@{}
Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue | ForEach-Object {$map[$_.DeviceID]=$_}
$items=@()
Get-CimInstance Win32_PnPEntity -ErrorAction Stop | ForEach-Object {
  $e=$_;$d=$map[$e.DeviceID];$missing=$null -eq $d
  $items+=@{deviceName=if($e.Name){[string]$e.Name}else{'未知设备'};deviceClass=if($e.PNPClass){[string]$e.PNPClass}else{''};manufacturer=if($e.Manufacturer){[string]$e.Manufacturer}else{''};driverProvider=if($d.DriverProviderName){[string]$d.DriverProviderName}else{''};driverVersion=if($d.DriverVersion){[string]$d.DriverVersion}else{''};driverDate=if($d.DriverDate){[string]$d.DriverDate}else{''};infName=if($d.InfName){[string]$d.InfName}else{''};signer=if($d.Signer){[string]$d.Signer}else{''};signed=if($d){[bool]$d.IsSigned}else{$false};deviceId=[string]$e.DeviceID;status=if($e.Status){[string]$e.Status}else{'Unknown'};errorCode=[uint32]$e.ConfigManagerErrorCode;missingDriver=$missing}
}
@{items=@($items)}|ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动驱动检测：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let mut payload: DriverPayload = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析驱动清单：{error}"))?;
    payload.items.sort_by(|a, b| {
        let a_issue = a.missing_driver || a.error_code != 0 || !a.signed;
        let b_issue = b.missing_driver || b.error_code != 0 || !b.signed;
        b_issue.cmp(&a_issue).then_with(|| a.device_name.to_lowercase().cmp(&b.device_name.to_lowercase()))
    });
    let total = payload.items.len();
    let signed = payload.items.iter().filter(|item| item.signed).count();
    let missing = payload.items.iter().filter(|item| item.missing_driver).count();
    let problems = payload.items.iter().filter(|item| item.error_code != 0).count();
    Ok(DriverInventory {
        captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),
        total, signed, unsigned: total.saturating_sub(signed + missing), missing, problems,
        items: payload.items, source: "Windows CIM / Plug and Play".into(),
    })
}

#[cfg(not(windows))]
pub fn collect() -> Result<DriverInventory, String> {
    Ok(DriverInventory { captured_at_unix_ms: 0, total: 0, signed: 0, unsigned: 0, missing: 0, problems: 0, items: vec![], source: "unsupported".into() })
}

#[tauri::command]
pub fn get_driver_inventory() -> Result<DriverInventory, String> { collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_counts_are_consistent() {
        let inventory = collect().expect("driver inventory");
        assert_eq!(inventory.total, inventory.items.len());
        assert!(inventory.signed + inventory.unsigned + inventory.missing <= inventory.total);
    }
}
