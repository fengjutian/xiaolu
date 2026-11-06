use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_SUBKEYS: [&str; 2] = ["Run", "RunOnce"];
const BACKUP_PATH: &str = "Software\\SystemLens\\DisabledStartup";
const OP_STARTUP: &str = "system_startup_set";

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

    let response = match run(&request) {
        Ok(message) => json!({
            "ok": true,
            "nonce": nonce,
            "op": OP_STARTUP,
            "message": message,
            "at": now_ms(),
        }),
        Err(error) => json!({
            "ok": false,
            "nonce": nonce,
            "op": OP_STARTUP,
            "error": error,
            "at": now_ms(),
        }),
    };
    if let Err(error) = write_result(&result_path, &response) {
        eprintln!("lens_helper: {error}");
        std::process::exit(1);
    }
}

fn run(request: &Value) -> Result<String, String> {
    let version = request.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version != 1 {
        return Err("不支持的请求版本。".into());
    }
    let op = request.get("op").and_then(Value::as_str).unwrap_or_default();
    if op != OP_STARTUP {
        return Err(format!("不支持的操作：{op}"));
    }
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
}
