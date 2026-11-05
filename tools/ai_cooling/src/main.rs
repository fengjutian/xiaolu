use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$zones=@()
Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction SilentlyContinue | ForEach-Object {
  $c=([double]$_.CurrentTemperature/10)-273.15
  if($c -gt -50 -and $c -lt 200){ $zones += @{name=[string]$_.InstanceName; temp=[math]::Round($c,1)} }
}
$loads=@(Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -ExpandProperty LoadPercentage)
$load=if($loads.Count -gt 0){ [double](($loads | Measure-Object -Average).Average) } else { $null }
@{zones=@($zones); load=$load} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取温度信息：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析温度信息：{error}")))?;
    let zones = payload
        .get("zones")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let load = payload.get("load").and_then(Value::as_f64);

    let mut rows = Vec::new();
    let mut max_temp: Option<f64> = None;
    for zone in &zones {
        let name = zone.get("name").and_then(Value::as_str).unwrap_or("热区");
        let temp = zone.get("temp").and_then(Value::as_f64).unwrap_or(0.0);
        max_temp = Some(max_temp.map_or(temp, |current| current.max(temp)));
        let suggestion = if temp >= 85.0 {
            "温度偏高，检查散热或降低负载"
        } else if temp >= 75.0 {
            "温度较高，关注持续负载"
        } else {
            "正常范围"
        };
        rows.push(json!({
            "sensor": if name.is_empty() { "ACPI 热区" } else { name },
            "value": format!("{temp:.1} °C"),
            "suggestion": suggestion,
        }));
    }
    if let Some(load) = load {
        rows.push(json!({
            "sensor": "CPU 平均负载",
            "value": format!("{load:.0}%"),
            "suggestion": if load >= 80.0 { "负载偏高，可检查高占用进程" } else { "正常范围" },
        }));
    }

    Ok(table(
        json!([
            item("ACPI 热区", zones.len().to_string()),
            item("最高温度", max_temp.map(|temp| format!("{temp:.1} °C")).unwrap_or_else(|| "不可用".into())),
            item("CPU 负载", load.map(|value| format!("{value:.0}%")).unwrap_or_else(|| "未知".into())),
        ]),
        json!([
            column("sensor", "传感器"),
            column("value", "数值"),
            column("suggestion", "建议"),
        ]),
        Value::Array(rows),
        json!([
            "只读建议：只读取 ACPI 热区与 CPU 负载，不控制风扇、不修改电源设置。",
            "部分设备/虚拟机不暴露 ACPI 热区，此时只能依据 CPU 负载给出建议。",
            "温度阈值仅为通用参考，不同机型的安全范围不同。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "ai_cooling",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}
