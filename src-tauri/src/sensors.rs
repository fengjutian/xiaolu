use serde::Serialize;
use std::process::Command;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorReading {
    id: String,
    name: String,
    category: String,
    value: f64,
    unit: String,
    source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageHealth {
    name: String,
    health_status: String,
    operational_status: String,
    temperature_c: Option<f64>,
    wear_percent: Option<f64>,
    power_on_hours: Option<u64>,
    read_errors_total: Option<u64>,
    write_errors_total: Option<u64>,
    source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorSnapshot {
    captured_at_unix_ms: u128,
    readings: Vec<SensorReading>,
    storage: Vec<StorageHealth>,
    limitations: Vec<String>,
}

#[cfg(windows)]
fn powershell_json(script: &str) -> Result<serde_json::Value, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("无法启动传感器采集：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析传感器数据：{error}"))
}

fn number(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.trim().parse().ok())
}

fn unsigned(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.trim().parse().ok())
}

fn text(value: Option<&serde_json::Value>, fallback: &str) -> String {
    value.and_then(serde_json::Value::as_str).unwrap_or(fallback).to_string()
}

#[cfg(windows)]
pub fn collect() -> Result<SensorSnapshot, String> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$gpu=@()
$nvidia=Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue
if($nvidia){
  $lines=& $nvidia.Source --query-gpu=name,temperature.gpu,fan.speed,power.draw,clocks.current.graphics --format=csv,noheader,nounits 2>$null
  foreach($line in $lines){$p=$line -split ',\s*';if($p.Count -ge 5){$gpu+=@{name=$p[0];temperature=$p[1];fan=$p[2];power=$p[3];clock=$p[4]}}}
}
$thermal=@()
Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction SilentlyContinue | ForEach-Object {
  if($_.CurrentTemperature -gt 0){$thermal+=@{name=$_.InstanceName;temperature=(($_.CurrentTemperature/10)-273.15)}}
}
$storage=@()
Get-PhysicalDisk -ErrorAction SilentlyContinue | ForEach-Object {
  $disk=$_;$counter=$null
  try{$counter=$disk|Get-StorageReliabilityCounter -ErrorAction Stop}catch{}
  $storage+=@{name=$disk.FriendlyName;health=[string]$disk.HealthStatus;operational=([string]::Join(', ',@($disk.OperationalStatus)));temperature=if($counter){$counter.Temperature}else{$null};wear=if($counter){$counter.Wear}else{$null};hours=if($counter){$counter.PowerOnHours}else{$null};readErrors=if($counter){$counter.ReadErrorsTotal}else{$null};writeErrors=if($counter){$counter.WriteErrorsTotal}else{$null}}
}
@{gpu=$gpu;thermal=$thermal;storage=$storage}|ConvertTo-Json -Depth 5 -Compress
"#;
    let value = powershell_json(script)?;
    let mut readings = Vec::new();
    for (index, gpu) in value.get("gpu").and_then(serde_json::Value::as_array).into_iter().flatten().enumerate() {
        let name = text(gpu.get("name"), "NVIDIA GPU");
        for (suffix, label, field, unit) in [
            ("temperature", format!("{name} 温度"), "temperature", "°C"),
            ("fan", format!("{name} 风扇"), "fan", "%"),
            ("power", format!("{name} 功耗"), "power", "W"),
            ("clock", format!("{name} 核心频率"), "clock", "MHz"),
        ] {
            if let Some(value) = gpu.get(field).and_then(number) {
                readings.push(SensorReading { id: format!("nvidia-{index}-{suffix}"), name: label, category: suffix.into(), value, unit: unit.into(), source: "NVIDIA SMI".into() });
            }
        }
    }
    for (index, zone) in value.get("thermal").and_then(serde_json::Value::as_array).into_iter().flatten().enumerate() {
        if let Some(value) = zone.get("temperature").and_then(number).filter(|x| (-20.0..=130.0).contains(x)) {
            readings.push(SensorReading { id: format!("acpi-{index}"), name: text(zone.get("name"), "ACPI 热区"), category: "temperature".into(), value, unit: "°C".into(), source: "Windows ACPI".into() });
        }
    }
    let storage = value.get("storage").and_then(serde_json::Value::as_array).into_iter().flatten().map(|disk| StorageHealth {
        name: text(disk.get("name"), "未知磁盘"), health_status: text(disk.get("health"), "Unknown"), operational_status: text(disk.get("operational"), "Unknown"),
        temperature_c: disk.get("temperature").and_then(number), wear_percent: disk.get("wear").and_then(number),
        power_on_hours: disk.get("hours").and_then(unsigned), read_errors_total: disk.get("readErrors").and_then(unsigned),
        write_errors_total: disk.get("writeErrors").and_then(unsigned), source: "Windows Storage Reliability Counter".into(),
    }).collect();
    let mut limitations = Vec::new();
    if !readings.iter().any(|item| item.category == "temperature") { limitations.push("当前硬件未通过 Windows 标准接口公开温度；CPU 温度需要后续可选传感器 helper。".into()); }
    Ok(SensorSnapshot { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), readings, storage, limitations })
}

#[cfg(not(windows))]
pub fn collect() -> Result<SensorSnapshot, String> {
    Ok(SensorSnapshot { captured_at_unix_ms: 0, readings: vec![], storage: vec![], limitations: vec!["传感器采集当前仅支持 Windows。".into()] })
}

#[tauri::command]
pub fn get_sensor_snapshot() -> Result<SensorSnapshot, String> { collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sensor_snapshot_is_well_formed() {
        let snapshot = collect().expect("sensor collection should not fail");
        assert!(snapshot.readings.iter().all(|item| item.value.is_finite()));
    }
}
