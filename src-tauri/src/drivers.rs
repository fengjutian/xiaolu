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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverUpdate {
    title: String,
    description: String,
    update_id: String,
    revision_number: i32,
    driver_class: String,
    driver_manufacturer: String,
    driver_model: String,
    driver_provider: String,
    driver_version: String,
    driver_date: String,
    categories: Vec<String>,
    downloaded: bool,
    #[serde(default)]
    driver_hardware_id: String,
    #[serde(default)]
    signed: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverUpdateAssessment {
    update: DriverUpdate,
    signature_ok: Option<bool>,
    status: String,
    matching_devices: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverAssessmentReport {
    captured_at_unix_ms: u128,
    updates: Vec<DriverUpdateAssessment>,
    inventory_source: String,
    note: String,
}

#[derive(Debug, Deserialize)]
struct UpdatePayload { items: Vec<DriverUpdate> }

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

#[cfg(windows)]
fn scan_updates() -> Result<Vec<DriverUpdate>, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$session=New-Object -ComObject Microsoft.Update.Session
$searcher=$session.CreateUpdateSearcher()
$result=$searcher.Search("IsInstalled=0 and Type='Driver'")
$items=@()
foreach($u in $result.Updates){
  $categories=@();foreach($c in $u.Categories){$categories+=[string]$c.Name}
  $items+=@{title=[string]$u.Title;description=[string]$u.Description;updateId=[string]$u.Identity.UpdateID;revisionNumber=[int]$u.Identity.RevisionNumber;driverClass=if($u.DriverClass){[string]$u.DriverClass}else{''};driverManufacturer=if($u.DriverManufacturer){[string]$u.DriverManufacturer}else{''};driverModel=if($u.DriverModel){[string]$u.DriverModel}else{''};driverProvider=if($u.DriverProvider){[string]$u.DriverProvider}else{''};driverVersion=if($u.DriverVerVersion){[string]$u.DriverVerVersion}else{''};driverDate=if($u.DriverVerDate){[string]$u.DriverVerDate}else{''};categories=@($categories);downloaded=[bool]$u.IsDownloaded;driverHardwareId=if($u.DriverHardwareID){[string]$u.DriverHardwareID}else{''};signed=if($null -eq $u.IsSigned){$null}else{[bool]$u.IsSigned}}
}
@{items=@($items)}|ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动 Windows Update 扫描：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let payload: UpdatePayload = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析驱动更新：{error}"))?;
    Ok(payload.items)
}

#[cfg(not(windows))]
fn scan_updates() -> Result<Vec<DriverUpdate>, String> { Ok(vec![]) }

#[tauri::command]
pub async fn scan_driver_updates() -> Result<Vec<DriverUpdate>, String> {
    tauri::async_runtime::spawn_blocking(scan_updates).await.map_err(|error| format!("驱动更新扫描任务失败：{error}"))?
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left = version_parts(left);
    let right = version_parts(right);
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_part = left.get(index).copied().unwrap_or(0);
        let right_part = right.get(index).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

fn hardware_matches(hardware_id: &str, device_id: &str) -> bool {
    hardware_id
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .any(|item| {
            let item = item.to_lowercase();
            let device = device_id.to_lowercase();
            device == item || device.starts_with(&item)
        })
}

/// Windows Update 经常不填 `DriverVerVersion`，此时从标题末尾的括号回退提取。
fn effective_version(update: &DriverUpdate) -> String {
    if !update.driver_version.trim().is_empty() {
        return update.driver_version.trim().to_string();
    }
    if let Some(start) = update.title.rfind('(') {
        if let Some(offset) = update.title[start..].find(')') {
            let candidate = update.title[start + 1..start + offset].trim();
            if candidate
                .chars()
                .next()
                .map(|character| character.is_ascii_digit())
                .unwrap_or(false)
            {
                return candidate.to_string();
            }
        }
    }
    String::new()
}

fn assess_update(update: &DriverUpdate, inventory: &[DriverInfo]) -> DriverUpdateAssessment {
    let matching: Vec<&DriverInfo> = if update.driver_hardware_id.trim().is_empty() {
        Vec::new()
    } else {
        inventory
            .iter()
            .filter(|device| hardware_matches(&update.driver_hardware_id, &device.device_id))
            .collect()
    };
    let matching_devices = matching
        .iter()
        .map(|device| format!("{}（当前 {}）", device.device_name, device.driver_version))
        .collect::<Vec<_>>();
    let candidate_version = effective_version(update);
    let mut notes = Vec::new();
    if candidate_version.is_empty() {
        notes.push("候选未提供版本号，无法判断是否需要更新。".into());
    }
    let status = if update.driver_hardware_id.trim().is_empty() {
        notes.push("候选未提供硬件 ID，无法确认适用性。".into());
        "unknown"
    } else if matching.is_empty() {
        notes.push("候选硬件 ID 未匹配到本机设备。".into());
        "no_match"
    } else if candidate_version.is_empty() {
        "unknown"
    } else {
        let newest_installed = matching
            .iter()
            .map(|device| device.driver_version.as_str())
            .max_by(|left, right| compare_versions(left, right))
            .unwrap_or_default();
        match compare_versions(&candidate_version, newest_installed) {
            std::cmp::Ordering::Greater => {
                notes.push(format!(
                    "候选版本 {candidate_version} 高于已安装的 {newest_installed}。"
                ));
                "applicable"
            }
            _ => {
                notes.push(format!(
                    "已安装版本 {newest_installed} 不低于候选 {candidate_version}。"
                ));
                "current"
            }
        }
    };
    match update.signed {
        Some(false) => notes.push("Windows Update 报告该候选未签名，禁止安装。".into()),
        None => notes.push("Windows Update 未提供签名状态，安装前需人工核对发布者签名。".into()),
        Some(true) => {}
    }
    DriverUpdateAssessment {
        update: update.clone(),
        signature_ok: update.signed,
        status: status.into(),
        matching_devices,
        notes,
    }
}

#[tauri::command]
pub async fn assess_driver_updates() -> Result<DriverAssessmentReport, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let inventory = collect()?;
        let updates = scan_updates()?;
        let assessments = updates
            .iter()
            .map(|update| assess_update(update, &inventory.items))
            .collect();
        Ok(DriverAssessmentReport {
            captured_at_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            updates: assessments,
            inventory_source: inventory.source,
            note: "只读评估：按硬件 ID 匹配设备并比较版本；不会下载、安装或回滚驱动。".into(),
        })
    })
    .await
    .map_err(|error| format!("驱动兼容性评估任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, version: &str) -> DriverInfo {
        DriverInfo {
            device_name: "演示设备".into(),
            device_class: "Display".into(),
            manufacturer: "Demo".into(),
            driver_provider: "Demo".into(),
            driver_version: version.into(),
            driver_date: "2026-01-01".into(),
            inf_name: "demo.inf".into(),
            signer: "Microsoft".into(),
            signed: true,
            device_id: id.into(),
            status: "OK".into(),
            error_code: 0,
            missing_driver: false,
        }
    }

    fn update(hardware: &str, version: &str, signed: bool) -> DriverUpdate {
        DriverUpdate {
            title: "Demo driver".into(),
            description: String::new(),
            update_id: "demo".into(),
            revision_number: 1,
            driver_class: "Display".into(),
            driver_manufacturer: "Demo".into(),
            driver_model: "Demo GPU".into(),
            driver_provider: "Demo".into(),
            driver_version: version.into(),
            driver_date: "2026-02-01".into(),
            categories: vec!["Drivers".into()],
            downloaded: false,
            driver_hardware_id: hardware.into(),
            signed: Some(signed),
        }
    }

    #[test]
    fn inventory_counts_are_consistent() {
        let inventory = collect().expect("driver inventory");
        assert_eq!(inventory.total, inventory.items.len());
        assert!(inventory.signed + inventory.unsigned + inventory.missing <= inventory.total);
    }

    #[test]
    fn compares_versions_numerically() {
        assert_eq!(compare_versions("1.2.10", "1.2.9"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("32.0.101.6078", "32.0.101.6078"), std::cmp::Ordering::Equal);
        assert_eq!(compare_versions("1.0", "1.0.0.1"), std::cmp::Ordering::Less);
    }

    #[test]
    fn matches_hardware_ids_by_prefix() {
        assert!(hardware_matches(
            "PCI\\VEN_8086&DEV_7D55",
            "PCI\\VEN_8086&DEV_7D55&SUBSYS_1234"
        ));
        assert!(hardware_matches(
            "PCI\\VEN_8086&DEV_7D55;PCI\\VEN_8086&DEV_0001",
            "pci\\ven_8086&dev_7d55&subsys_5678"
        ));
        assert!(!hardware_matches(
            "PCI\\VEN_8086&DEV_7D55",
            "PCI\\VEN_1002&DEV_7D55"
        ));
    }

    #[test]
    fn assesses_applicable_current_and_unmatched() {
        let inventory = vec![device("PCI\\VEN_8086&DEV_7D55&SUBSYS_1", "1.0.0.1")];

        let applicable = assess_update(&update("PCI\\VEN_8086&DEV_7D55", "1.0.0.2", true), &inventory);
        assert_eq!(applicable.status, "applicable");
        assert_eq!(applicable.signature_ok, Some(true));
        assert_eq!(applicable.matching_devices.len(), 1);

        let current = assess_update(&update("PCI\\VEN_8086&DEV_7D55", "1.0.0.1", true), &inventory);
        assert_eq!(current.status, "current");

        let unmatched = assess_update(&update("PCI\\VEN_1002&DEV_0001", "9.9.9.9", true), &inventory);
        assert_eq!(unmatched.status, "no_match");

        let unknown = assess_update(&update("", "1.0.0.2", false), &inventory);
        assert_eq!(unknown.status, "unknown");
        assert_eq!(unknown.signature_ok, Some(false));
        assert!(unknown.notes.iter().any(|note| note.contains("未签名")));
    }

    #[test]
    fn falls_back_to_title_version() {
        let inventory = vec![device("PCI\\VEN_8086&DEV_7D55&SUBSYS_1", "32.0.101.8860")];
        let mut candidate = update("PCI\\VEN_8086&DEV_7D55", "", true);
        candidate.title = "Intel Corporation Display Driver Update (32.0.101.8991)".into();
        let assessment = assess_update(&candidate, &inventory);
        assert_eq!(assessment.status, "applicable");
        assert!(assessment
            .notes
            .iter()
            .any(|note| note.contains("32.0.101.8991")));
    }

    #[test]
    #[ignore = "需要真实访问 Windows Update 并枚举本机设备"]
    fn live_driver_assessment() {
        let inventory = collect().expect("driver inventory");
        let updates = scan_updates().expect("windows update scan");
        println!("devices={} candidates={}", inventory.total, updates.len());
        for update in &updates {
            let assessment = assess_update(update, &inventory.items);
            println!(
                "{} | hw={} | status={} | signed={:?} | devices={:?}",
                update.title,
                update.driver_hardware_id,
                assessment.status,
                assessment.signature_ok,
                assessment.matching_devices
            );
        }
    }
}
