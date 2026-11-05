use serde_json::Value;
use std::process::Command;

use super::{
    now_unix_ms, stable_hash, Availability, Limitation, Quality, SensorSourceStatus, SensorValue,
    SmartAttribute, SmartDevice, StorageHealth,
};

pub struct Parsed {
    pub values: Vec<SensorValue>,
    pub storage: Vec<StorageHealth>,
    pub smart_devices: Vec<SmartDevice>,
    pub limitations: Vec<Limitation>,
    pub sources: Vec<SensorSourceStatus>,
    pub elevated: bool,
}

#[cfg(windows)]
pub fn collect() -> Result<Parsed, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| format!("无法启动传感器采集：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("无法解析传感器数据：{error}"))?;
    Ok(parse_payload(&payload, now_unix_ms()))
}

#[cfg(not(windows))]
pub fn collect() -> Result<Parsed, String> {
    let mut parsed = empty_parsed();
    parsed.limitations.push(Limitation {
        code: "SENSOR_UNSUPPORTED".into(),
        message: "传感器采集当前仅支持 Windows。".into(),
        remediation: "在 Windows 10/11 上运行以获取传感器数据。".into(),
    });
    Ok(parsed)
}

fn empty_parsed() -> Parsed {
    Parsed {
        values: Vec::new(),
        storage: Vec::new(),
        smart_devices: Vec::new(),
        limitations: Vec::new(),
        sources: Vec::new(),
        elevated: false,
    }
}

#[cfg(windows)]
fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$admin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
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
$smartStatus=@{}
Get-CimInstance -Namespace root/wmi -ClassName MSStorageDriver_FailurePredictStatus -ErrorAction SilentlyContinue | ForEach-Object {$smartStatus[$_.InstanceName]=[bool]$_.PredictFailure}
$smart=@()
Get-CimInstance -Namespace root/wmi -ClassName MSStorageDriver_FailurePredictData -ErrorAction SilentlyContinue | ForEach-Object {
  $bytes=$_.VendorSpecific;$attributes=@()
  if($bytes){
    for($offset=2;$offset+11 -lt $bytes.Count;$offset+=12){
      $id=[int]$bytes[$offset]
      if($id -gt 0){
        [uint64]$raw=0
        for($j=0;$j -lt 6;$j++){$raw=$raw -bor ([uint64]$bytes[$offset+5+$j] -shl (8*$j))}
        $attributes+=@{id=$id;current=[int]$bytes[$offset+3];worst=[int]$bytes[$offset+4];raw=$raw}
      }
    }
  }
  $prediction=$null;if($smartStatus.ContainsKey($_.InstanceName)){$prediction=$smartStatus[$_.InstanceName]}
  $smart+=@{instanceName=[string]$_.InstanceName;predictFailure=$prediction;attributes=@($attributes)}
}
@{admin=$admin;nvidiaFound=[bool]$nvidia;gpu=@($gpu);thermal=@($thermal);storage=@($storage);smart=@($smart)}|ConvertTo-Json -Depth 7 -Compress
"#
}

/// PowerShell 的 `ConvertTo-Json` 对单元素集合可能输出对象而不是数组，两种都接受。
fn items(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(values)) => values.iter().collect(),
        Some(value @ Value::Object(_)) => vec![value],
        _ => Vec::new(),
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|item| item.is_finite())
}

fn unsigned(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

pub fn text(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

#[allow(clippy::too_many_arguments)]
fn reading(
    hardware_name: &str,
    hardware_class: &str,
    category: &str,
    suffix: &str,
    label: &str,
    value: f64,
    unit: &str,
    source: &str,
    captured_at: u64,
) -> SensorValue {
    let hardware_id = format!("{}-{}", hardware_class, stable_hash(hardware_name));
    SensorValue {
        id: format!("{hardware_id}-{suffix}"),
        hardware_id,
        hardware_name: hardware_name.to_string(),
        hardware_class: hardware_class.to_string(),
        category: category.to_string(),
        name: label.to_string(),
        value: Some(value),
        unit: unit.to_string(),
        source: source.to_string(),
        captured_at_unix_ms: captured_at,
        availability: Availability::Ok,
        error_code: None,
        quality: Quality::Measured,
    }
}

fn smart_name(id: u8) -> String {
    match id {
        1 => "原始读取错误率",
        3 => "主轴启动时间",
        4 => "启动/停止次数",
        5 => "重映射扇区数",
        7 => "寻道错误率",
        9 => "通电时间",
        10 => "主轴重试次数",
        12 => "通电次数",
        171 => "程序失败计数",
        172 => "擦除失败计数",
        173 => "磨损均衡计数",
        174 => "意外断电次数",
        175 => "断电保护失败",
        177 => "磨损范围",
        179 => "已用保留块",
        181 => "程序失败总数",
        182 => "擦除失败总数",
        183 => "SATA 降速错误",
        184 => "端到端错误",
        187 => "不可校正错误",
        188 => "命令超时",
        189 => "高飞写入",
        190 => "气流温度",
        191 => "冲击错误",
        192 => "断电回缩次数",
        193 => "磁头加载次数",
        194 => "温度",
        196 => "重映射事件数",
        197 => "待处理扇区数",
        198 => "离线不可校正扇区",
        199 => "接口 CRC 错误",
        200 => "多区域错误率",
        202 => "寿命剩余",
        206 => "飞行高度",
        210 => "振动写入",
        231 => "SSD 剩余寿命",
        232 => "可用保留空间",
        233 => "介质磨损指标",
        241 => "主机写入总量",
        242 => "主机读取总量",
        _ => "厂商属性",
    }
    .to_string()
}

/// 把一次 PowerShell 采集结果映射为统一模型，便于用固定样本做回归测试。
pub fn parse_payload(payload: &Value, captured_at: u64) -> Parsed {
    let elevated = payload.get("admin").and_then(Value::as_bool).unwrap_or(false);
    let nvidia_found = payload.get("nvidiaFound").and_then(Value::as_bool).unwrap_or(false);
    let mut parsed = empty_parsed();
    parsed.elevated = elevated;

    for (index, gpu) in items(payload.get("gpu")).into_iter().enumerate() {
        let name = text(gpu.get("name"), &format!("NVIDIA GPU {index}"));
        for (category, suffix, field, unit, label) in [
            ("temperature", "temp", "temperature", "°C", "温度"),
            ("fan", "fan", "fan", "%", "风扇"),
            ("power", "power", "power", "W", "功耗"),
            ("clock", "clock", "clock", "MHz", "核心频率"),
        ] {
            if let Some(value) = gpu.get(field).and_then(number) {
                parsed.values.push(reading(
                    &name,
                    "gpu",
                    category,
                    suffix,
                    &format!("{name} {label}"),
                    value,
                    unit,
                    "nvidia-smi",
                    captured_at,
                ));
            }
        }
    }

    for (index, zone) in items(payload.get("thermal")).into_iter().enumerate() {
        let name = text(zone.get("name"), &format!("ACPI 热区 {index}"));
        let is_cpu = name.to_lowercase().contains("cpu") || name.to_lowercase().contains("pkg");
        if let Some(value) = zone
            .get("temperature")
            .and_then(number)
            .filter(|item| (-20.0..=130.0).contains(item))
        {
            parsed.values.push(reading(
                &name,
                if is_cpu { "cpu" } else { "system" },
                "temperature",
                "temp",
                &format!("{name} 温度"),
                value,
                "°C",
                "windows-acpi",
                captured_at,
            ));
        }
    }

    for disk in items(payload.get("storage")) {
        let name = text(disk.get("name"), "未知磁盘");
        let health = StorageHealth {
            name: name.clone(),
            health_status: text(disk.get("health"), "Unknown"),
            operational_status: text(disk.get("operational"), "Unknown"),
            temperature_c: disk.get("temperature").and_then(number),
            wear_percent: disk.get("wear").and_then(number),
            power_on_hours: disk.get("hours").and_then(unsigned),
            read_errors_total: disk.get("readErrors").and_then(unsigned),
            write_errors_total: disk.get("writeErrors").and_then(unsigned),
            source: "Windows Storage Reliability Counter".into(),
        };
        if let Some(temperature) = health.temperature_c {
            parsed.values.push(reading(
                &name,
                "disk",
                "temperature",
                "temp",
                &format!("{name} 温度"),
                temperature,
                "°C",
                "windows-storage",
                captured_at,
            ));
        }
        parsed.storage.push(health);
    }

    for device in items(payload.get("smart")) {
        let instance_name = text(device.get("instanceName"), "未知 SMART 设备");
        let attributes = items(device.get("attributes"))
            .into_iter()
            .filter_map(|attribute| {
                let id = attribute.get("id")?.as_u64()? as u8;
                Some(SmartAttribute {
                    id,
                    name: smart_name(id),
                    current: attribute.get("current").and_then(Value::as_u64).unwrap_or_default() as u8,
                    worst: attribute.get("worst").and_then(Value::as_u64).unwrap_or_default() as u8,
                    raw: attribute.get("raw").and_then(unsigned).unwrap_or_default(),
                })
            })
            .collect::<Vec<_>>();
        parsed.smart_devices.push(SmartDevice {
            instance_name,
            predict_failure: device.get("predictFailure").and_then(Value::as_bool),
            attributes,
            source: "WMI MSStorageDriver SMART".into(),
        });
    }

    let has_temperature = parsed
        .values
        .iter()
        .any(|item| item.category == "temperature");
    let has_gpu = parsed
        .values
        .iter()
        .any(|item| item.hardware_class == "gpu");

    parsed.sources.push(SensorSourceStatus {
        source: "nvidia-smi".into(),
        ok: has_gpu,
        last_error: if has_gpu || nvidia_found {
            None
        } else {
            Some("未检测到 nvidia-smi.exe，NVIDIA 显卡以外的 GPU 需要可选传感器组件。".into())
        },
        last_success_at_unix_ms: if has_gpu { Some(captured_at) } else { None },
    });
    parsed.sources.push(SensorSourceStatus {
        source: "windows-acpi".into(),
        ok: parsed
            .values
            .iter()
            .any(|item| item.source == "windows-acpi"),
        last_error: None,
        last_success_at_unix_ms: parsed
            .values
            .iter()
            .find(|item| item.source == "windows-acpi")
            .map(|_| captured_at),
    });
    parsed.sources.push(SensorSourceStatus {
        source: "windows-storage".into(),
        ok: !parsed.storage.is_empty(),
        last_error: None,
        last_success_at_unix_ms: if parsed.storage.is_empty() { None } else { Some(captured_at) },
    });
    parsed.sources.push(SensorSourceStatus {
        source: "wmi-smart".into(),
        ok: !parsed.smart_devices.is_empty(),
        last_error: if parsed.smart_devices.is_empty() && !elevated {
            Some("ATA SMART 需要管理员权限；普通权限下只能看到健康摘要。".into())
        } else {
            None
        },
        last_success_at_unix_ms: if parsed.smart_devices.is_empty() { None } else { Some(captured_at) },
    });

    if !has_temperature {
        parsed.limitations.push(Limitation {
            code: "SENSOR_UNAVAILABLE".into(),
            message: "当前硬件没有通过 Windows 标准接口公开 CPU/主板温度。".into(),
            remediation: "CPU/主板温度需要可选的传感器组件；本版本默认不安装内核驱动。".into(),
        });
    }
    if parsed.smart_devices.is_empty() {
        parsed.limitations.push(Limitation {
            code: if elevated { "SENSOR_UNAVAILABLE" } else { "SENSOR_DENIED" }.into(),
            message: if elevated {
                "存储控制器未向 Windows WMI 公开 ATA SMART 原始属性。".into()
            } else {
                "普通权限下无法读取 ATA SMART 原始属性。".into()
            },
            remediation: if elevated {
                "NVMe/RAID/USB 设备可能只提供健康摘要，属正常现象。".into()
            } else {
                "如需原始 SMART，可以管理员身份重新启动应用后重试。".into()
            },
        });
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_single_element_arrays_and_objects() {
        let payload = json!({
            "admin": false,
            "nvidiaFound": true,
            "gpu": {"name": "NVIDIA GeForce RTX 4070", "temperature": "61", "fan": "32", "power": "120.5", "clock": "2610"},
            "thermal": [],
            "storage": {"name": "NVMe SSD", "health": "Healthy", "operational": "OK", "temperature": 39, "wear": 3, "hours": 1200, "readErrors": 0, "writeErrors": 0},
            "smart": []
        });
        let parsed = parse_payload(&payload, 1_700_000_000_000);
        assert_eq!(parsed.values.len(), 5);
        let gpu_temp = parsed.values.iter().find(|item| item.id.ends_with("-temp") && item.hardware_class == "gpu").unwrap();
        assert_eq!(gpu_temp.value, Some(61.0));
        assert!(gpu_temp.id.starts_with("gpu-"));
        assert_eq!(gpu_temp.hardware_name, "NVIDIA GeForce RTX 4070");
        assert!(parsed.storage[0].temperature_c == Some(39.0));
        assert!(parsed.limitations.iter().any(|item| item.code == "SENSOR_DENIED"));
    }

    #[test]
    fn reports_unavailable_when_no_temperature() {
        let payload = json!({
            "admin": true,
            "nvidiaFound": false,
            "gpu": [],
            "thermal": [],
            "storage": [],
            "smart": []
        });
        let parsed = parse_payload(&payload, 1);
        assert!(parsed.values.is_empty());
        assert!(parsed.limitations.iter().any(|item| item.code == "SENSOR_UNAVAILABLE"));
        assert!(parsed.sources.iter().any(|item| item.source == "nvidia-smi" && !item.ok));
        assert!(parsed.elevated);
    }

    #[test]
    fn ids_are_stable_across_calls() {
        let payload = json!({
            "admin": false,
            "nvidiaFound": true,
            "gpu": [{"name": "NVIDIA GeForce RTX 4070", "temperature": 61}],
            "thermal": [],
            "storage": [],
            "smart": []
        });
        let first = parse_payload(&payload, 10);
        let second = parse_payload(&payload, 20);
        assert_eq!(first.values[0].id, second.values[0].id);
        assert_ne!(first.values[0].captured_at_unix_ms, second.values[0].captured_at_unix_ms);
    }

    /// 手动冒烟测试：真实执行一次 PowerShell 采集。
    /// 运行方式：cargo test live_collect_smoke -- --ignored --nocapture
    #[cfg(windows)]
    #[test]
    #[ignore = "需要在本机真实执行 PowerShell 采集"]
    fn live_collect_smoke() {
        let parsed = collect().expect("live collect should succeed");
        for value in &parsed.values {
            if let Some(number) = value.value {
                assert!(number.is_finite(), "non-finite value for {}", value.id);
            }
        }
        println!(
            "values={} storage={} smart={} limitations={}",
            parsed.values.len(),
            parsed.storage.len(),
            parsed.smart_devices.len(),
            parsed.limitations.len()
        );
        assert!(!parsed.sources.is_empty());
        assert!(parsed.sources.iter().all(|source| !source.source.is_empty()));
    }
}
