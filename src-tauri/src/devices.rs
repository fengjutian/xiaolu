use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_MONITORS: usize = 16;
const MAX_AUDIO: usize = 24;
const MAX_USB: usize = 64;
const MAX_BLUETOOTH: usize = 32;
const MAX_CAMERAS: usize = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub name: String,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
    pub year: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub name: String,
    pub manufacturer: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDevice {
    pub name: String,
    pub manufacturer: String,
    pub status: String,
    pub vid: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothDevice {
    pub name: String,
    pub manufacturer: String,
    pub status: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDevice {
    pub name: String,
    pub manufacturer: String,
    pub status: String,
    pub pnp_class: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetails {
    pub captured_at_unix_ms: u64,
    pub monitors: Vec<MonitorInfo>,
    pub audio: Vec<AudioDevice>,
    pub usb: Vec<UsbDevice>,
    pub bluetooth: Vec<BluetoothDevice>,
    pub cameras: Vec<CameraDevice>,
    pub unavailable: Vec<String>,
    pub source: String,
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$errors=@()
function TextOf($codes){
  if($null -eq $codes){ return '' }
  try { return ((-join ($codes | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ })).Trim()) } catch { return '' }
}

$monitors=@()
try {
  Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID -ErrorAction Stop | ForEach-Object {
    $monitors += @{
      name=(TextOf $_.UserFriendlyName)
      manufacturer=(TextOf $_.ManufacturerName)
      product=(TextOf $_.ProductCodeID)
      serial=(TextOf $_.SerialNumberID)
      year=[int]$_.YearOfManufacture
    }
  }
} catch { $errors += ('显示器查询失败：' + $_.Exception.Message) }

$audio=@()
try {
  Get-CimInstance Win32_SoundDevice -ErrorAction Stop | Where-Object { -not [string]::IsNullOrWhiteSpace($_.Name) } | ForEach-Object {
    $audio += @{name=[string]$_.Name; manufacturer=[string]$_.Manufacturer; status=[string]$_.Status}
  }
} catch { $errors += ('音频设备查询失败：' + $_.Exception.Message) }

$usb=@()
$bluetooth=@()
$cameras=@()
$pnp=@()
try {
  $pnp=@(Get-CimInstance Win32_PnPEntity -ErrorAction Stop | Where-Object { -not [string]::IsNullOrWhiteSpace($_.Name) })
} catch { $errors += ('即插即用设备查询失败：' + $_.Exception.Message) }

foreach($device in $pnp){
  $class=[string]$device.PNPClass
  if($class -eq 'USB'){
    $id=[string]$device.DeviceID; $vid=''; $pid=''
    if($id -match 'VID_([0-9A-Fa-f]{4})'){ $vid=$Matches[1].ToUpper() }
    if($id -match 'PID_([0-9A-Fa-f]{4})'){ $pid=$Matches[1].ToUpper() }
    $usb += @{name=[string]$device.Name; manufacturer=[string]$device.Manufacturer; status=[string]$device.Status; vid=$vid; pid=$pid}
  } elseif($class -eq 'Bluetooth'){
    $id=[string]$device.DeviceID
    $kind=if($id -like 'BTHENUM*' -or $id -like 'BTHLE*'){ '已配对/近场设备' } elseif($id -like 'BTH*'){ '适配器' } else { '蓝牙设备' }
    $bluetooth += @{name=[string]$device.Name; manufacturer=[string]$device.Manufacturer; status=[string]$device.Status; kind=$kind}
  } elseif($class -eq 'Camera' -or $class -eq 'Image'){
    $cameras += @{name=[string]$device.Name; manufacturer=[string]$device.Manufacturer; status=[string]$device.Status; pnpClass=$class}
  }
}

@{monitors=@($monitors);audio=@($audio);usb=@($usb);bluetooth=@($bluetooth);cameras=@($cameras);errors=@($errors)} | ConvertTo-Json -Depth 5 -Compress
"#
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn parse_details(payload: &Value) -> DeviceDetails {
    let empty = Vec::new();
    let array = |key: &str| -> &Vec<Value> {
        payload
            .get(key)
            .and_then(Value::as_array)
            .unwrap_or(&empty)
    };

    let mut monitor_seen = HashSet::new();
    let monitors = array("monitors")
        .iter()
        .filter_map(|entry| {
            let name = text(entry, "name");
            let manufacturer = text(entry, "manufacturer");
            let product = text(entry, "product");
            let serial = text(entry, "serial");
            let year = entry.get("year").and_then(Value::as_u64).unwrap_or(0) as u32;
            if name.is_empty() && manufacturer.is_empty() && product.is_empty() {
                return None;
            }
            let key = format!("{name}|{manufacturer}|{product}|{serial}");
            if !monitor_seen.insert(key) {
                return None;
            }
            Some(MonitorInfo {
                name: if name.is_empty() {
                    "通用显示器".to_string()
                } else {
                    name.to_string()
                },
                manufacturer: manufacturer.to_string(),
                product: product.to_string(),
                serial: serial.to_string(),
                year,
            })
        })
        .take(MAX_MONITORS)
        .collect();

    let mut audio_seen = HashSet::new();
    let audio = array("audio")
        .iter()
        .filter_map(|entry| {
            let name = text(entry, "name");
            if name.is_empty() || !audio_seen.insert(name.to_lowercase()) {
                return None;
            }
            Some(AudioDevice {
                name: name.to_string(),
                manufacturer: text(entry, "manufacturer").to_string(),
                status: text(entry, "status").to_string(),
            })
        })
        .take(MAX_AUDIO)
        .collect();

    let mut usb_seen = HashSet::new();
    let usb = array("usb")
        .iter()
        .filter_map(|entry| {
            let name = text(entry, "name");
            if name.is_empty() {
                return None;
            }
            let vid = text(entry, "vid").to_string();
            let pid = text(entry, "pid").to_string();
            let key = format!("{}|{}|{}", name.to_lowercase(), vid, pid);
            if !usb_seen.insert(key) {
                return None;
            }
            Some(UsbDevice {
                name: name.to_string(),
                manufacturer: text(entry, "manufacturer").to_string(),
                status: text(entry, "status").to_string(),
                vid,
                pid,
            })
        })
        .take(MAX_USB)
        .collect();

    let mut bluetooth_seen = HashSet::new();
    let bluetooth = array("bluetooth")
        .iter()
        .filter_map(|entry| {
            let name = text(entry, "name");
            if name.is_empty() || !bluetooth_seen.insert(name.to_lowercase()) {
                return None;
            }
            let kind = text(entry, "kind");
            Some(BluetoothDevice {
                name: name.to_string(),
                manufacturer: text(entry, "manufacturer").to_string(),
                status: text(entry, "status").to_string(),
                kind: if kind.is_empty() {
                    "蓝牙设备".to_string()
                } else {
                    kind.to_string()
                },
            })
        })
        .take(MAX_BLUETOOTH)
        .collect();

    let mut camera_seen = HashSet::new();
    let cameras = array("cameras")
        .iter()
        .filter_map(|entry| {
            let name = text(entry, "name");
            if name.is_empty() || !camera_seen.insert(name.to_lowercase()) {
                return None;
            }
            let pnp_class = text(entry, "pnpClass");
            Some(CameraDevice {
                name: name.to_string(),
                manufacturer: text(entry, "manufacturer").to_string(),
                status: text(entry, "status").to_string(),
                pnp_class: if pnp_class.is_empty() {
                    "Camera".to_string()
                } else {
                    pnp_class.to_string()
                },
            })
        })
        .take(MAX_CAMERAS)
        .collect();

    let unavailable = payload
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    DeviceDetails {
        captured_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        monitors,
        audio,
        usb,
        bluetooth,
        cameras,
        unavailable,
        source: "CIM / WMI（只读）".to_string(),
    }
}

fn collect() -> Result<DeviceDetails, String> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| format!("无法启动设备信息采集：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .last()
        .ok_or_else(|| "设备信息采集没有返回 JSON".to_string())?;
    let payload: Value =
        serde_json::from_str(json_line).map_err(|error| format!("无法解析设备信息：{error}"))?;
    Ok(parse_details(&payload))
}

#[tauri::command]
pub async fn get_device_details() -> Result<DeviceDetails, String> {
    tauri::async_runtime::spawn_blocking(collect)
        .await
        .map_err(|error| format!("设备信息任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_groups_and_drops_duplicates() {
        let payload = json!({
            "monitors": [
                {"name": "DELL U2723QE", "manufacturer": "DEL", "product": "U2723QE", "serial": "ABC123", "year": 2023},
                {"name": "DELL U2723QE", "manufacturer": "DEL", "product": "U2723QE", "serial": "ABC123", "year": 2023},
                {"name": "", "manufacturer": "", "product": "", "serial": "", "year": 0}
            ],
            "audio": [
                {"name": "扬声器 (Realtek)", "manufacturer": "Realtek", "status": "OK"},
                {"name": "扬声器 (Realtek)", "manufacturer": "Realtek", "status": "OK"}
            ],
            "usb": [
                {"name": "USB Composite Device", "manufacturer": "(标准 USB 主机控制器)", "status": "OK", "vid": "046D", "pid": "C52B"},
                {"name": "USB Composite Device", "manufacturer": "(标准 USB 主机控制器)", "status": "OK", "vid": "046D", "pid": "C52B"}
            ],
            "bluetooth": [
                {"name": "Intel(R) Wireless Bluetooth(R)", "manufacturer": "Intel", "status": "OK", "kind": "适配器"}
            ],
            "cameras": [
                {"name": "Integrated Camera", "manufacturer": "SunplusIT", "status": "OK", "pnpClass": "Camera"}
            ],
            "errors": ["蓝牙设备查询失败：拒绝访问"]
        });
        let details = parse_details(&payload);
        assert_eq!(details.monitors.len(), 1);
        assert_eq!(details.monitors[0].name, "DELL U2723QE");
        assert_eq!(details.monitors[0].year, 2023);
        assert_eq!(details.audio.len(), 1);
        assert_eq!(details.usb.len(), 1);
        assert_eq!(details.usb[0].vid, "046D");
        assert_eq!(details.bluetooth.len(), 1);
        assert_eq!(details.bluetooth[0].kind, "适配器");
        assert_eq!(details.cameras.len(), 1);
        assert_eq!(details.unavailable.len(), 1);
    }

    #[test]
    fn tolerates_missing_groups_and_fields() {
        let details = parse_details(&json!({}));
        assert!(details.monitors.is_empty());
        assert!(details.audio.is_empty());
        assert!(details.usb.is_empty());
        assert!(details.bluetooth.is_empty());
        assert!(details.cameras.is_empty());
        assert!(details.unavailable.is_empty());
    }

    #[test]
    fn falls_back_for_empty_names() {
        let details = parse_details(&json!({
            "monitors": [{"manufacturer": "DEL", "product": "P2419H"}],
            "bluetooth": [{"name": "Buds", "kind": ""}]
        }));
        assert_eq!(details.monitors[0].name, "通用显示器");
        assert_eq!(details.bluetooth[0].kind, "蓝牙设备");
    }

    #[test]
    #[ignore = "依赖真实 WMI 环境"]
    fn collects_real_devices() {
        let details = collect().expect("collect device details");
        assert!(
            !details.audio.is_empty()
                || !details.usb.is_empty()
                || !details.cameras.is_empty()
                || !details.monitors.is_empty(),
            "expected at least one device group from real WMI"
        );
    }
}
