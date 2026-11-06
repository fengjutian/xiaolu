use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_ELEVATED: &str = "read_elevated";

fn elevated_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("SystemLens")
        .join("elevated")
}

fn helper_path() -> Result<PathBuf, ToolError> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法定位工具路径：{error}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    let candidates = [
        dir.join("lens_helper.exe"),
        dir.join("..").join("lens_helper").join("lens_helper.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            ToolError::new(
                "HELPER_MISSING",
                "缺少提权助手 lens_helper.exe，请重新打包或安装。",
            )
        })
}

fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn run_elevated(mut payload: Value) -> Result<Value, ToolError> {
    let dir = elevated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建提权请求目录：{error}")))?;
    let nonce_source = format!("{}-{}", tool_protocol::now_ms(), std::process::id());
    let nonce_full = tool_protocol::digest_of(&[&nonce_source]);
    let nonce = &nonce_full[..32];
    let request_path = dir.join(format!("req-{nonce}.json"));
    let result_path = dir.join(format!("res-{nonce}.json"));
    if let Value::Object(map) = &mut payload {
        map.insert("version".into(), json!(1));
        map.insert("nonce".into(), json!(nonce));
        map.insert("resultPath".into(), json!(result_path));
    }
    let serialized = serde_json::to_string(&payload)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化提权请求：{error}")))?;
    std::fs::write(&request_path, serialized)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入提权请求：{error}")))?;

    let helper = helper_path()?;
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath {} -ArgumentList ('--request',('\"' + {} + '\"')) -Verb RunAs -PassThru -Wait -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_quote(&helper.to_string_lossy()),
        ps_quote(&request_path.to_string_lossy())
    );
    let status = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;

    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);

    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new(
                "ELEVATION_CANCELLED",
                "已取消管理员授权，未读取受限传感器。",
            ));
        }
        return Err(ToolError::new(
            "ELEVATION_FAILED",
            format!("提权助手执行失败（退出码 {:?}）。", status.code()),
        ));
    }
    let text = result_text
        .ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn render_elevated(data: &Value) -> Value {
    let empty = Vec::new();
    let zones = data
        .get("zones")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let fans = data
        .get("fans")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let probes = data
        .get("probes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let disks = data
        .get("disks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let smart = data
        .get("smart")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

    let mut rows = Vec::new();
    for zone in zones {
        let name = text(zone, "name");
        if let Some(temp) = number(zone, "temp") {
            rows.push(json!({
                "sensor": if name.is_empty() { "ACPI 热区".to_string() } else { name.to_string() },
                "value": format!("{temp:.1} °C"),
                "status": if temp >= 80.0 { "偏高" } else { "正常" },
            }));
        }
    }
    for probe in probes {
        let name = text(probe, "name");
        rows.push(json!({
            "sensor": if name.is_empty() { "温度探针".to_string() } else { name.to_string() },
            "value": number(probe, "temp").map(|temp| format!("{temp:.1} °C")).unwrap_or_else(|| "未报告".into()),
            "status": text(probe, "status"),
        }));
    }
    for fan in fans {
        let name = text(fan, "name");
        rows.push(json!({
            "sensor": if name.is_empty() { "风扇".to_string() } else { name.to_string() },
            "value": number(fan, "rpm").map(|rpm| format!("{rpm:.0} RPM")).unwrap_or_else(|| "未报告".into()),
            "status": text(fan, "status"),
        }));
    }
    for disk in disks {
        let name = text(disk, "name");
        if let Some(temp) = number(disk, "temp") {
            rows.push(json!({
                "sensor": format!("磁盘温度 · {name}"),
                "value": format!("{temp:.0} °C"),
                "status": text(disk, "media"),
            }));
        }
        if let Some(hours) = number(disk, "hours") {
            rows.push(json!({
                "sensor": format!("通电时间 · {name}"),
                "value": format!("{hours:.0} 小时"),
                "status": "SMART",
            }));
        }
        if let Some(wear) = number(disk, "wear") {
            rows.push(json!({
                "sensor": format!("磨损 · {name}"),
                "value": format!("{wear:.0}%"),
                "status": "SMART",
            }));
        }
    }
    for entry in smart {
        let name = text(entry, "name");
        let predict = entry.get("predict").and_then(Value::as_bool).unwrap_or(false);
        rows.push(json!({
            "sensor": format!("SMART 预测 · {name}"),
            "value": if predict { "预测故障" } else { "未预测故障" },
            "status": if predict { "危险" } else { "正常" },
        }));
    }

    let mut notes = vec![
        "受限读取由管理员助手一次性执行，退出后不保留权限。".to_string(),
        "ACPI 热区、风扇与温度探针的覆盖取决于主板/固件；CPU 封装温度通常仍需可选传感器驱动。".to_string(),
        "SMART/磁盘温度在部分 NVMe、RAID、USB 设备上不可用。".to_string(),
    ];
    if rows.is_empty() {
        notes.push("本次没有读到任何受限传感器；设备可能未暴露这些接口。".to_string());
    }

    table(
        json!([
            item("ACPI 热区", zones.len().to_string()),
            item("风扇", fans.len().to_string()),
            item("磁盘", disks.len().to_string()),
            item("SMART", smart.len().to_string()),
        ]),
        json!([
            column("sensor", "传感器"),
            column("value", "数值"),
            column("status", "状态"),
        ]),
        Value::Array(rows),
        json!(notes),
    )
}

fn read_elevated() -> Result<Value, ToolError> {
    let result = run_elevated(json!({"op": "sensor_read"}))?;
    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    Ok(render_elevated(&data))
}

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
    tool_protocol::serve_with_request(
        "ai_cooling",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_ELEVATED],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_ELEVATED => read_elevated(),
            other => Err(ToolError::unsupported(other)),
        },
    );
}
