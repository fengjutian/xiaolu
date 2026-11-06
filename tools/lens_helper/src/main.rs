use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN_SUBKEYS: [&str; 2] = ["Run", "RunOnce"];
const BACKUP_PATH: &str = "Software\\SystemLens\\DisabledStartup";
const OP_STARTUP: &str = "system_startup_set";
const OP_SERVICE: &str = "service_set_start";
const OP_SENSOR: &str = "sensor_read";
const OP_TASK: &str = "task_set_enabled";
const SERVICE_PATH: &str = "SYSTEM\\CurrentControlSet\\Services";
const START_TYPES: [u32; 3] = [2, 3, 4];

fn valid_service_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 256
        && name != "."
        && name != ".."
        && !name.contains("::")
        && !name.contains('\\')
        && !name.contains('/')
        && !name
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

fn valid_start_type(value: u64) -> Option<u32> {
    let value = u32::try_from(value).ok()?;
    START_TYPES.contains(&value).then_some(value)
}

/// 任务路径必须是绝对路径（以 `\` 开头），不允许空段、`..`、引号或控制字符。
fn valid_task_path(path: &str) -> bool {
    if !path.starts_with('\\')
        || path.len() > 512
        || path.ends_with('\\')
        || path.contains("..")
        || path.contains('"')
        || path.chars().any(|character| character.is_control())
    {
        return false;
    }
    path.split('\\')
        .skip(1)
        .all(|segment| !segment.is_empty())
}

fn elevated_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("SystemLens").join("elevated"))
}

fn is_hex_nonce(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 256
        && !name.contains("::")
        && !name.chars().any(|character| character == '\0' || character.is_control())
}

fn valid_key(key: &str) -> bool {
    RUN_SUBKEYS.contains(&key)
}

fn same_dir(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
        _ => false,
    }
}

/// 结果文件必须是 `%LOCALAPPDATA%\SystemLens\elevated\res-<nonce>.json`。
fn result_path(request: &Value, dir: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(request.get("resultPath")?.as_str()?);
    let name = path.file_name()?.to_str()?;
    let nonce = request.get("nonce")?.as_str()?;
    if name != format!("res-{nonce}.json") {
        return None;
    }
    if !path
        .parent()
        .map(|parent| same_dir(parent, dir))
        .unwrap_or(false)
    {
        return None;
    }
    Some(path)
}

fn registry_error(context: &str, error: std::io::Error) -> String {
    if error.raw_os_error() == Some(5) {
        format!("{context}：拒绝访问（助手未获得管理员权限）。")
    } else {
        format!("{context}：{error}")
    }
}

/// 只在 HKLM Run/RunOnce 与停用备份之间移动值，不执行任何命令。
fn set_startup(key: &str, name: &str, enable: bool) -> Result<String, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let run_path = format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\{key}");
    let backup_name = format!("{key}::{name}");

    if enable {
        let backup = hklm
            .open_subkey_with_flags(BACKUP_PATH, KEY_READ | KEY_WRITE)
            .map_err(|error| registry_error("打开停用备份", error))?;
        let command: String = backup
            .get_value(&backup_name)
            .map_err(|error| registry_error("读取停用备份", error))?;
        let (run, _) = hklm
            .create_subkey(&run_path)
            .map_err(|error| registry_error("打开系统启动项", error))?;
        run.set_value(name, &command)
            .map_err(|error| registry_error("恢复系统启动项", error))?;
        if let Err(error) = backup.delete_value(&backup_name) {
            let _ = run.delete_value(name);
            return Err(registry_error("清理停用备份", error));
        }
    } else {
        let run = hklm
            .open_subkey_with_flags(&run_path, KEY_READ | KEY_WRITE)
            .map_err(|error| registry_error("打开系统启动项", error))?;
        let command: String = run
            .get_value(name)
            .map_err(|error| registry_error("读取系统启动项", error))?;
        let (backup, _) = hklm
            .create_subkey(BACKUP_PATH)
            .map_err(|error| registry_error("创建停用备份", error))?;
        backup
            .set_value(&backup_name, &command)
            .map_err(|error| registry_error("写入停用备份", error))?;
        if let Err(error) = run.delete_value(name) {
            let _ = backup.delete_value(&backup_name);
            return Err(registry_error("停用系统启动项", error));
        }
    }

    let run_has = hklm
        .open_subkey_with_flags(&run_path, KEY_READ)
        .ok()
        .and_then(|run| run.get_value::<String, _>(name).ok())
        .is_some();
    let backup_has = hklm
        .open_subkey_with_flags(BACKUP_PATH, KEY_READ)
        .ok()
        .and_then(|backup| backup.get_value::<String, _>(&backup_name).ok())
        .is_some();
    let expected = if enable { run_has && !backup_has } else { !run_has && backup_has };
    if !expected {
        return Err("写入后回读状态不一致。".into());
    }
    Ok(if enable {
        "已启用系统启动项".into()
    } else {
        "已停用系统启动项".into()
    })
}

fn set_service_start(service: &str, start_type: u32, expect: u32) -> Result<String, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = format!("{SERVICE_PATH}\\{service}");
    let key = hklm
        .open_subkey_with_flags(&path, KEY_READ | KEY_WRITE)
        .map_err(|error| registry_error("打开服务注册表", error))?;
    let current: u32 = key
        .get_value("Start")
        .map_err(|error| registry_error("读取服务启动类型", error))?;
    if current != expect {
        return Err(format!(
            "服务启动类型已变化（当前 {current}，预期 {expect}）。"
        ));
    }
    if current == start_type {
        return Ok("服务启动类型已是目标值".into());
    }
    key.set_value("Start", &start_type)
        .map_err(|error| registry_error("写入服务启动类型", error))?;
    let after: u32 = key
        .get_value("Start")
        .map_err(|error| registry_error("回读服务启动类型", error))?;
    if after != start_type {
        return Err("写入后回读不一致。".into());
    }
    Ok("服务启动类型已修改，重启后完全生效。".into())
}

fn write_result(path: &Path, payload: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化结果：{error}"))?;
    std::fs::write(path, text).map_err(|error| format!("无法写入结果：{error}"))?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut request_path: Option<PathBuf> = None;
    while let Some(argument) = args.next() {
        if argument == "--request" {
            request_path = args.next().map(PathBuf::from);
        }
    }
    let Some(request_path) = request_path else {
        eprintln!("lens_helper: 缺少 --request 参数");
        std::process::exit(2);
    };

    let request_text = match std::fs::read_to_string(&request_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("lens_helper: 无法读取请求：{error}");
            std::process::exit(2);
        }
    };
    let request: Value = match serde_json::from_str(&request_text) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("lens_helper: 请求不是合法 JSON：{error}");
            std::process::exit(2);
        }
    };

    let nonce = request
        .get("nonce")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !is_hex_nonce(&nonce) {
        eprintln!("lens_helper: nonce 格式无效");
        std::process::exit(2);
    }

    let Some(dir) = elevated_dir() else {
        eprintln!("lens_helper: 无法确定请求目录");
        std::process::exit(2);
    };
    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!("lens_helper: 无法创建请求目录");
        std::process::exit(2);
    }

    let expected_request_name = format!("req-{nonce}.json");
    if request_path.file_name().and_then(|name| name.to_str())
        != Some(expected_request_name.as_str())
        || !request_path
            .parent()
            .map(|parent| same_dir(parent, &dir))
            .unwrap_or(false)
    {
        eprintln!("lens_helper: 请求文件位置不受信任");
        std::process::exit(2);
    }

    let Some(result_path) = result_path(&request, &dir) else {
        eprintln!("lens_helper: 结果文件位置不受信任");
        std::process::exit(2);
    };

    let op = request
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let response = match run(&request) {
        Ok(outcome) => {
            let Outcome { message, data } = outcome;
            let mut value = json!({
                "ok": true,
                "nonce": nonce,
                "op": op,
                "message": message,
                "at": now_ms(),
            });
            if let (Value::Object(map), Some(data)) = (&mut value, data) {
                map.insert("data".into(), data);
            }
            value
        }
        Err(error) => json!({
            "ok": false,
            "nonce": nonce,
            "op": op,
            "error": error,
            "at": now_ms(),
        }),
    };
    if let Err(error) = write_result(&result_path, &response) {
        eprintln!("lens_helper: {error}");
        std::process::exit(1);
    }
}

struct Outcome {
    message: String,
    data: Option<Value>,
}

impl Outcome {
    fn message(message: impl Into<String>) -> Self {
        Outcome {
            message: message.into(),
            data: None,
        }
    }
}

fn run(request: &Value) -> Result<Outcome, String> {
    let version = request.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version != 1 {
        return Err("不支持的请求版本。".into());
    }
    let op = request.get("op").and_then(Value::as_str).unwrap_or_default();
    match op {
        OP_STARTUP => run_startup(request).map(Outcome::message),
        OP_SERVICE => run_service(request).map(Outcome::message),
        OP_SENSOR => read_sensors(),
        OP_TASK => run_task(request).map(Outcome::message),
        _ => Err(format!("不支持的操作：{op}")),
    }
}

fn run_startup(request: &Value) -> Result<String, String> {
    let key = request.get("key").and_then(Value::as_str).unwrap_or_default();
    if !valid_key(key) {
        return Err("只支持 HKLM Run / RunOnce。".into());
    }
    let name = request.get("name").and_then(Value::as_str).unwrap_or_default();
    if !valid_name(name) {
        return Err("启动项名称无效。".into());
    }
    let enable = request
        .get("enable")
        .and_then(Value::as_bool)
        .ok_or_else(|| "缺少 enable。".to_string())?;
    set_startup(key, name, enable)
}

fn run_service(request: &Value) -> Result<String, String> {
    let service = request
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid_service_name(service) {
        return Err("服务名称无效。".into());
    }
    let start_type = request
        .get("startType")
        .and_then(Value::as_u64)
        .and_then(valid_start_type)
        .ok_or_else(|| "startType 必须是 2（自动）、3（手动）或 4（禁用）。".to_string())?;
    let expect = request
        .get("expectCurrent")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 expectCurrent。".to_string())?;
    let expect = u32::try_from(expect).map_err(|_| "expectCurrent 超出范围。".to_string())?;
    set_service_start(service, start_type, expect)
}

fn run_task(request: &Value) -> Result<String, String> {
    let task_path = request
        .get("taskPath")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid_task_path(task_path) {
        return Err("计划任务路径无效。".into());
    }
    let enable = request
        .get("enable")
        .and_then(Value::as_bool)
        .ok_or_else(|| "缺少 enable。".to_string())?;
    set_task_enabled(task_path, enable)
}

/// 只调用系统 `schtasks.exe`，参数以数组传入，不经过 cmd/shell。
fn set_task_enabled(task_path: &str, enable: bool) -> Result<String, String> {
    let mut command = Command::new("schtasks.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command.args([
        "/Change",
        "/TN",
        task_path,
        if enable { "/ENABLE" } else { "/DISABLE" },
    ]);
    let output = command
        .output()
        .map_err(|error| format!("无法启动 schtasks：{error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("schtasks 执行失败：{detail}"));
    }
    Ok(if enable {
        "计划任务已启用".into()
    } else {
        "计划任务已禁用".into()
    })
}

fn read_sensors() -> Result<Outcome, String> {    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$zones=@()
try { Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction Stop | ForEach-Object {
  $c=([double]$_.CurrentTemperature/10)-273.15
  if($c -gt -50 -and $c -lt 200){ $zones += @{name=[string]$_.InstanceName; temp=[math]::Round($c,1)} }
} } catch {}
$fans=@()
try { Get-CimInstance Win32_Fan -ErrorAction Stop | ForEach-Object {
  $fans += @{name=[string]$_.DeviceID; rpm=if($null -ne $_.DesiredSpeed){[double]$_.DesiredSpeed}else{$null}; status=[string]$_.Status}
} } catch {}
$probes=@()
try { Get-CimInstance Win32_TemperatureProbe -ErrorAction Stop | ForEach-Object {
  $probes += @{name=[string]$_.DeviceID; temp=if($null -ne $_.CurrentReading){[double]$_.CurrentReading}else{$null}; status=[string]$_.Status}
} } catch {}
$disks=@()
try { Get-PhysicalDisk -ErrorAction Stop | ForEach-Object {
  $d=$_; $rel=$null
  try { $rel=$d | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
  $disks += @{name=[string]$d.FriendlyName; media=[string]$d.MediaType; temp=if($rel -and $null -ne $rel.Temperature){[double]$rel.Temperature}else{$null}; wear=if($rel -and $null -ne $rel.Wear){[double]$rel.Wear}else{$null}; hours=if($rel -and $null -ne $rel.PowerOnHours){[double]$rel.PowerOnHours}else{$null}}
} } catch {}
$smart=@()
try { Get-CimInstance -Namespace root/wmi -ClassName MSStorageDriver_FailurePredictStatus -ErrorAction Stop | ForEach-Object { $smart += @{name=[string]$_.InstanceName; predict=[bool]$_.PredictFailure} } } catch {}
@{zones=@($zones);fans=@($fans);probes=@($probes);disks=@($disks);smart=@($smart)} | ConvertTo-Json -Depth 6 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("无法启动传感器读取：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "传感器读取失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{feff}');
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("无法解析传感器结果：{error}"))?;
    Ok(Outcome {
        message: "受限传感器读取完成".into(),
        data: Some(value),
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_requires_32_hex_characters() {
        assert!(is_hex_nonce("0123456789abcdef0123456789abcdef"));
        assert!(!is_hex_nonce("0123"));
        assert!(!is_hex_nonce("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn name_validation_rejects_separators_and_controls() {
        assert!(valid_name("OneDrive"));
        assert!(valid_name("中文 名称"));
        assert!(!valid_name(""));
        assert!(!valid_name("a::b"));
        assert!(!valid_name("line\nbreak"));
        assert!(!valid_name(&"x".repeat(257)));
    }

    #[test]
    fn key_validation_is_limited_to_run_keys() {
        assert!(valid_key("Run"));
        assert!(valid_key("RunOnce"));
        assert!(!valid_key("Services"));
        assert!(!valid_key("run"));
    }

    #[test]
    fn result_path_must_stay_in_elevated_directory() {
        let dir = std::env::temp_dir().join(format!("lens-helper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let nonce = "0123456789abcdef0123456789abcdef";
        let inside = dir.join(format!("res-{nonce}.json"));
        let good = json!({"nonce": nonce, "resultPath": inside});
        assert!(result_path(&good, &dir).is_some());

        let outside = std::env::temp_dir().join(format!("res-{nonce}.json"));
        let bad = json!({"nonce": nonce, "resultPath": outside});
        assert!(result_path(&bad, &dir).is_none());

        let wrong_name = json!({"nonce": nonce, "resultPath": dir.join("res-other.json")});
        assert!(result_path(&wrong_name, &dir).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn service_name_rejects_path_separators() {
        assert!(valid_service_name("Spooler"));
        assert!(valid_service_name("Vendor Agent"));
        assert!(!valid_service_name(""));
        assert!(!valid_service_name("..\\evil"));
        assert!(!valid_service_name("a/b"));
        assert!(!valid_service_name("line\nbreak"));
    }

    #[test]
    fn start_types_are_limited_to_auto_manual_disabled() {
        assert_eq!(valid_start_type(2), Some(2));
        assert_eq!(valid_start_type(3), Some(3));
        assert_eq!(valid_start_type(4), Some(4));
        assert_eq!(valid_start_type(0), None);
        assert_eq!(valid_start_type(5), None);
    }

    #[test]
    fn task_path_validation_rejects_traversal_and_empty_segments() {
        assert!(valid_task_path("\\Vendor\\Update"));
        assert!(valid_task_path("\\SingleTask"));
        assert!(valid_task_path("\\Folder With Space\\Task"));
        assert!(!valid_task_path("Vendor\\Update"));
        assert!(!valid_task_path("\\Vendor\\\\Update"));
        assert!(!valid_task_path("\\Vendor\\..\\Update"));
        assert!(!valid_task_path("\\Vendor\\Update\\"));
        assert!(!valid_task_path("\\Vendor\\\"Update"));
        assert!(!valid_task_path("\\Vendor\\line\nbreak"));
    }
}
