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
const OP_DEFRAG: &str = "defrag_volume";
const OP_HOSTS_APPLY: &str = "hosts_apply";
const OP_HOSTS_RESTORE: &str = "hosts_restore";
const OP_CRASH_DUMP: &str = "crash_dump_set";
const OP_WINOPT: &str = "winopt_set";
const OP_WINOPT_BATCH: &str = "winopt_set_batch";
const OP_EXPAND: &str = "expand_volume";
const OP_RESTORE_POINT: &str = "restore_point_create";
const OP_PM_RESTORE: &str = "pm_restore";
const OP_POWER_LIMIT: &str = "power_limit_set";
const OP_POWER_SETTINGS: &str = "power_settings_write";
const OP_POWER_PLAN: &str = "power_plan_activate";
const HOSTS_BEGIN: &str = "# BEGIN SystemLens AdBlock";
const HOSTS_END: &str = "# END SystemLens AdBlock";
const SERVICE_PATH: &str = "SYSTEM\\CurrentControlSet\\Services";
const START_TYPES: [u32; 3] = [2, 3, 4];

/// `powercfg` 只接受下面这些别名：别名会直接拼进命令行，
/// 白名单之外的任何输入一律拒绝，避免把外部字符串带进提权进程。
const POWER_ALIASES: [&str; 9] = [
    "SUB_PROCESSOR",
    "SUB_VIDEO",
    "SUB_SLEEP",
    "SUB_DISK",
    "PROCTHROTTLEMIN",
    "PROCTHROTTLEMAX",
    "VIDEOIDLE",
    "STANDBYIDLE",
    "DISKIDLE",
];

fn valid_power_alias(alias: &str) -> bool {
    POWER_ALIASES.contains(&alias)
}

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

/// 重解析点（符号链接/目录联接）：提权进程不读不写这类路径。
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
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
    // 必须独占创建：结果文件位于用户可写目录，若攻击者预先放一个同名文件
    // （甚至是指向系统文件的硬链接），普通 open 会让提权进程写穿到别处。
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // 上一次异常退出可能留下同名结果；先删除再独占创建。
            std::fs::remove_file(path).map_err(|error| format!("无法清理旧结果：{error}"))?;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| format!("无法写入结果：{error}"))?
        }
        Err(error) => return Err(format!("无法写入结果：{error}")),
    };
    use std::io::Write;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("无法写入结果：{error}"))
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
    if is_reparse_point(&request_path) {
        eprintln!("lens_helper: 请求文件不能是符号链接");
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
        OP_DEFRAG => run_defrag(request),
        OP_HOSTS_APPLY => run_hosts_apply(request),
        OP_HOSTS_RESTORE => run_hosts_restore(),
        OP_CRASH_DUMP => run_crash_dump(request),
        OP_WINOPT => run_winopt(request),
        OP_WINOPT_BATCH => run_winopt_batch(request),
        OP_EXPAND => run_expand(request),
        OP_RESTORE_POINT => run_restore_point(),
        OP_PM_RESTORE => run_pm_restore(request),
        OP_POWER_LIMIT => run_power_limit(request),
        OP_POWER_SETTINGS => run_power_settings(request),
        OP_POWER_PLAN => run_power_plan_activate(request),
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

fn run_power_limit(request: &Value) -> Result<Outcome, String> {
    let percent = request
        .get("percent")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 percent。".to_string())?;
    if !(30..=100).contains(&percent) {
        return Err("percent 必须在 30–100 之间。".into());
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMAX {percent}; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; powercfg /setdcvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMAX {percent}; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; powercfg /setactive SCHEME_CURRENT; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; 'ok'"
    );
    run_powershell(&script).map_err(|error| format!("设置处理器上限失败：{error}"))?;
    Ok(Outcome {
        message: format!("处理器最大状态已设为 {percent}%"),
        data: Some(json!({"percent": percent})),
    })
}

/// 在提权进程里执行 powercfg 命令，返回失败原因。
///
/// PowerShell 的错误输出是系统控制台编码（GBK），直接读会变成乱码，
/// 所以脚本统一回传 ASCII 错误码 `RC=<code>`，这里再翻译成中文原因。
fn run_powershell(script: &str) -> Result<(), String> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("无法启动 powercfg：{error}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let code = stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix("RC=").map(str::to_string));
        return Err(match code {
            Some(code) => format!(
                "powercfg 返回错误码 {code}（常见原因：权限不足，或该设置不被当前计划支持）"
            ),
            None => format!("命令执行失败（退出码 {:?}）", output.status.code()),
        });
    }
    Ok(())
}

/// 写入电源计划的 AC/DC 取值。
///
/// 电源计划的取值存放在 HKLM，普通权限写入只会拿到 ERROR_ACCESS_DENIED，
/// 所以这个操作只能在提权助手里做。整批设置一次写完、一次激活，
/// 用户只会被问一次 UAC。
fn run_power_settings(request: &Value) -> Result<Outcome, String> {
    let items = request
        .get("settings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err("缺少 settings。".into());
    }
    if items.len() > 16 {
        return Err("单次最多写入 16 个电源设置项。".into());
    }
    let mut written = 0usize;
    let mut results: Vec<Value> = Vec::new();
    for item in &items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let subgroup = item.get("subgroup").and_then(Value::as_str).unwrap_or_default();
        let setting = item.get("setting").and_then(Value::as_str).unwrap_or_default();
        if !valid_item_id(id) {
            results.push(json!({"id": id, "ok": false, "error": "电源设置项标识无效。"}));
            continue;
        }
        if !valid_power_alias(subgroup) || !valid_power_alias(setting) {
            results.push(json!({
                "id": id,
                "ok": false,
                "error": format!("不支持的电源设置别名：{subgroup}/{setting}"),
            }));
            continue;
        }
        let ac = item.get("ac").and_then(Value::as_u64).unwrap_or(0);
        let dc = item.get("dc").and_then(Value::as_u64).unwrap_or(0);
        // powercfg 的时间单位是秒。
        let script = format!(
            "$ErrorActionPreference='Stop'; powercfg /setacvalueindex SCHEME_CURRENT {subgroup} {setting} {ac}; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; powercfg /setdcvalueindex SCHEME_CURRENT {subgroup} {setting} {dc}; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; 'ok'"
        );
        match run_powershell(&script) {
            Ok(()) => {
                written += 1;
                results.push(json!({"id": id, "ok": true, "error": null}));
            }
            Err(error) => results.push(json!({"id": id, "ok": false, "error": error})),
        }
    }
    // 写完必须重新激活计划，改动才会生效；激活失败要单独报出来。
    let mut activation_error: Option<String> = None;
    if written > 0 {
        if let Err(error) = run_powershell(
            "$ErrorActionPreference='Stop'; powercfg /setactive SCHEME_CURRENT; if($LASTEXITCODE -ne 0){ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }; 'ok'",
        ) {
            activation_error = Some(error);
        }
    }
    Ok(Outcome {
        message: format!("已写入 {written} 个电源设置项"),
        data: Some(json!({
            "written": written,
            "items": results,
            "activationError": activation_error,
        })),
    })
}

/// 切换当前使用的电源计划。计划 GUID 会直接拼进 powercfg 命令行，
/// 因此只接受标准 GUID 文本。
fn run_power_plan_activate(request: &Value) -> Result<Outcome, String> {
    let guid = request.get("guid").and_then(Value::as_str).unwrap_or_default();
    if !valid_guid_text(guid) {
        return Err("电源计划 GUID 无效。".into());
    }
    run_powershell(&format!(
        "$ErrorActionPreference='Stop'; powercfg /setactive {guid}; if($LASTEXITCODE -ne 0){{ Write-Output \"RC=$LASTEXITCODE\"; exit 1 }}; 'ok'"
    ))
    .map_err(|error| format!("切换电源计划失败：{error}"))?;
    Ok(Outcome {
        message: format!("已切换到电源计划 {guid}"),
        data: Some(json!({"guid": guid})),
    })
}

/// 标准 GUID 文本：8-4-4-4-12 位十六进制。
fn valid_guid_text(text: &str) -> bool {
    let parts = text.split('-').collect::<Vec<_>>();
    if parts.len() != 5 {
        return false;
    }
    let lengths = [8usize, 4, 4, 4, 12];
    for (part, expected) in parts.iter().zip(lengths.iter()) {
        if part.len() != *expected || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    true
}

/// 电源设置项的标识符：只允许小写字母、数字与连字符。
fn valid_item_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn run_pm_restore(request: &Value) -> Result<Outcome, String> {
    let services = request.get("services").and_then(Value::as_array).cloned().unwrap_or_default();
    let tasks = request.get("tasks").and_then(Value::as_array).cloned().unwrap_or_default();
    if services.len() > 200 || tasks.len() > 200 {
        return Err("单次最多恢复 200 个服务和 200 个任务。".into());
    }
    let mut service_count = 0usize;
    let mut task_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for item in &services {
        let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
        let start = item.get("start").and_then(Value::as_u64).unwrap_or(0);
        if !valid_service_name(name) {
            errors.push(format!("服务名无效：{name}"));
            continue;
        }
        let Some(start) = valid_start_type(start) else {
            errors.push(format!("服务启动类型无效：{name}"));
            continue;
        };
        let current = hklm
            .open_subkey_with_flags(format!("SYSTEM\\CurrentControlSet\\Services\\{name}"), KEY_READ)
            .ok()
            .and_then(|key| key.get_value::<u32, _>("Start").ok())
            .unwrap_or(start);
        match set_service_start(name, start, current) {
            Ok(_) => service_count += 1,
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    for item in &tasks {
        let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
        let enabled = item.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        if !valid_task_path(path) {
            errors.push(format!("任务路径无效：{path}"));
            continue;
        }
        match set_task_enabled(path, enabled) {
            Ok(_) => task_count += 1,
            Err(error) => errors.push(format!("{path}: {error}")),
        }
    }
    Ok(Outcome {
        message: format!("已恢复 {service_count} 个服务、{task_count} 个任务"),
        data: Some(json!({
            "services": service_count,
            "tasks": task_count,
            "errors": errors,
        })),
    })
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
        let detail = if detail.contains('\u{fffd}') || detail.is_empty() {
            format!("退出码 {:?}", output.status.code())
        } else {
            detail
        };
        return Err(format!("schtasks 执行失败：{detail}"));
    }
    Ok(if enable {
        "计划任务已启用".into()
    } else {
        "计划任务已禁用".into()
    })
}

fn valid_drive(drive: &str) -> bool {
    drive.len() == 1
        && drive
            .chars()
            .next()
            .map(|character| character.is_ascii_alphabetic())
            .unwrap_or(false)
}

fn valid_defrag_mode(mode: &str) -> bool {
    matches!(mode, "analyze" | "optimize" | "trim")
}

fn run_defrag(request: &Value) -> Result<Outcome, String> {
    let drive = request
        .get("drive")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid_drive(drive) {
        return Err("盘符必须是单个字母。".into());
    }
    let mode = request
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid_defrag_mode(mode) {
        return Err("不支持的操作模式。".into());
    }
    let flag = match mode {
        "analyze" => "/A",
        "optimize" => "/O",
        _ => "/L",
    };
    let mut command = Command::new("defrag.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command.arg(format!("{drive}:")).arg(flag);
    let output = command
        .output()
        .map_err(|error| format!("无法启动 defrag.exe：{error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let text = if stdout.trim().is_empty() { stderr } else { stdout };
    let code = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Err(format!("defrag 执行失败（退出码 {code}）：{}", text.trim()));
    }
    Ok(Outcome {
        message: "磁盘优化命令已完成".into(),
        data: Some(json!({
            "drive": drive,
            "mode": mode,
            "exitCode": code,
            "output": text,
        })),
    })
}

fn hosts_path() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts")
}

fn valid_domain(domain: &str) -> bool {
    let domain = domain.trim();
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

fn strip_managed_block(text: &str) -> (String, usize) {
    let mut output = String::new();
    let mut inside = false;
    let mut removed = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == HOSTS_BEGIN {
            inside = true;
            continue;
        }
        if trimmed == HOSTS_END {
            inside = false;
            continue;
        }
        if inside {
            removed += 1;
            continue;
        }
        output.push_str(line);
        output.push_str("\r\n");
    }
    (output, removed)
}

fn flush_dns() {
    let _ = Command::new("ipconfig.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .arg("/flushdns")
        .status();
}

fn backup_hosts(original: &str) -> Result<(), String> {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Ok(());
    };
    let dir = local.join("SystemLens").join("adblock");
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建备份目录：{error}"))?;
    let backup = dir.join("hosts.backup");
    if !backup.exists() {
        std::fs::write(&backup, original)
            .map_err(|error| format!("无法写入 hosts 备份：{error}"))?;
    }
    Ok(())
}

fn run_hosts_apply(request: &Value) -> Result<Outcome, String> {
    let domains = request
        .get("domains")
        .and_then(Value::as_array)
        .ok_or_else(|| "缺少 domains。".to_string())?;
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for domain in domains {
        let domain = domain
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !valid_domain(&domain) {
            return Err(format!("域名格式无效：{domain}"));
        }
        if seen.insert(domain.clone()) {
            normalized.push(domain);
        }
    }
    if normalized.is_empty() {
        return Err("至少需要一个域名。".into());
    }
    if normalized.len() > 500 {
        return Err("单次最多写入 500 个域名。".into());
    }

    let path = hosts_path();
    let original =
        std::fs::read_to_string(&path).map_err(|error| format!("无法读取 hosts：{error}"))?;
    backup_hosts(&original)?;
    let (mut content, _) = strip_managed_block(&original);
    if !content.is_empty() && !content.ends_with("\r\n") {
        content.push_str("\r\n");
    }
    content.push_str(HOSTS_BEGIN);
    content.push_str("\r\n");
    for domain in &normalized {
        content.push_str(&format!("0.0.0.0 {domain}\r\n"));
    }
    content.push_str(HOSTS_END);
    content.push_str("\r\n");
    std::fs::write(&path, content).map_err(|error| format!("无法写入 hosts：{error}"))?;
    flush_dns();
    Ok(Outcome {
        message: "Hosts 规则已写入".into(),
        data: Some(json!({
            "domains": normalized.len(),
            "path": path.to_string_lossy(),
        })),
    })
}

fn run_hosts_restore() -> Result<Outcome, String> {
    let path = hosts_path();
    let original =
        std::fs::read_to_string(&path).map_err(|error| format!("无法读取 hosts：{error}"))?;
    let (content, removed) = strip_managed_block(&original);
    if removed == 0 {
        return Ok(Outcome {
            message: "没有需要移除的 SystemLens 规则".into(),
            data: Some(json!({"removed": 0})),
        });
    }
    std::fs::write(&path, content).map_err(|error| format!("无法写入 hosts：{error}"))?;
    flush_dns();
    Ok(Outcome {
        message: "Hosts 规则已移除".into(),
        data: Some(json!({"removed": removed})),
    })
}

fn winopt_registry_spec(setting: &str) -> Option<(&'static str, &'static str, u32, u32)> {
    match setting {
        "telemetry" => Some(("SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry", 0, 3)),
        "fastStartup" => Some(("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power", "HiberbootEnabled", 0, 1)),
        "longPaths" => Some(("SYSTEM\\CurrentControlSet\\Control\\FileSystem", "LongPathsEnabled", 0, 1)),
        "systemRestore" => Some(("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore", "DisableSR", 0, 1)),
        _ => None,
    }
}

fn read_hibernate_enabled() -> Option<u32> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags("SYSTEM\\CurrentControlSet\\Control\\Power", KEY_READ)
        .ok()?
        .get_value("HibernateEnabled")
        .ok()
}

fn run_winopt(request: &Value) -> Result<Outcome, String> {
    let setting = request.get("setting").and_then(Value::as_str).unwrap_or_default();
    let value = request.get("value").and_then(Value::as_u64).ok_or_else(|| "缺少 value。".to_string())?;
    let value = u32::try_from(value).map_err(|_| "value 超出范围。".to_string())?;
    let (before, after) = winopt_apply_one(setting, value)?;
    Ok(Outcome {
        message: "系统设置已更新".into(),
        data: Some(json!({"setting": setting, "before": before, "after": after})),
    })
}

/// 批量应用管理员级设置：一次提权、一次 UAC，逐项隔离失败。
///
/// 单项授权意味着用户要为一组设置反复点 UAC；这里把整批放进同一个提权进程，
/// 逐项返回写入前后值，失败只影响该项。
fn run_winopt_batch(request: &Value) -> Result<Outcome, String> {
    let items = request
        .get("settings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err("缺少 settings。".into());
    }
    if items.len() > 16 {
        return Err("单次最多修改 16 项系统设置。".into());
    }
    let mut applied = 0usize;
    let mut results: Vec<Value> = Vec::new();
    for item in &items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let value = match item.get("value").and_then(Value::as_u64) {
            Some(value) => match u32::try_from(value) {
                Ok(value) => value,
                Err(_) => {
                    results.push(json!({"id": id, "ok": false, "before": null, "after": null, "error": "value 超出范围。"}));
                    continue;
                }
            },
            None => {
                results.push(json!({"id": id, "ok": false, "before": null, "after": null, "error": "缺少 value。"}));
                continue;
            }
        };
        match winopt_apply_one(id, value) {
            Ok((before, after)) => {
                applied += 1;
                results.push(json!({
                    "id": id,
                    "ok": true,
                    "before": before,
                    "after": after,
                    "error": null,
                }));
            }
            Err(error) => results.push(json!({
                "id": id,
                "ok": false,
                "before": null,
                "after": null,
                "error": error,
            })),
        }
    }
    Ok(Outcome {
        message: format!("已应用 {applied} 项系统设置"),
        data: Some(json!({"applied": applied, "items": results})),
    })
}

/// 应用一项管理员级设置，返回写入前后的值。
fn winopt_apply_one(setting: &str, value: u32) -> Result<(Value, Value), String> {
    if setting == "searchService" {
        if ![2u32, 3, 4].contains(&value) {
            return Err("searchService 只接受 2/3/4。".into());
        }
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let key = hklm
            .open_subkey_with_flags("SYSTEM\\CurrentControlSet\\Services\\WSearch", KEY_READ | KEY_WRITE)
            .map_err(|error| registry_error("打开 Windows 搜索服务", error))?;
        let before: u32 = key.get_value("Start").unwrap_or(3);
        key.set_value("Start", &value)
            .map_err(|error| registry_error("写入 Windows 搜索服务", error))?;
        let after: u32 = key.get_value("Start").map_err(|error| registry_error("回读 Windows 搜索服务", error))?;
        if after != value {
            return Err("写入后回读服务启动类型不一致。".into());
        }
        return Ok((json!(before), json!(after)));
    }

    if setting == "hibernate" {
        if value > 1 {
            return Err("hibernate 只接受 0/1。".into());
        }
        let flag = if value == 1 { "on" } else { "off" };
        let before = read_hibernate_enabled().map(|value| json!(value));
        let output = Command::new("powercfg.exe")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/hibernate", flag])
            .output()
            .map_err(|error| format!("无法启动 powercfg：{error}"))?;
        if !output.status.success() {
            let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("powercfg 执行失败：{text}"));
        }
        let after = read_hibernate_enabled().unwrap_or(0);
        return Ok((before.unwrap_or(Value::Null), json!(after)));
    }

    let (path, name, min, max) = winopt_registry_spec(setting)
        .ok_or_else(|| format!("不支持的设置：{setting}"))?;
    if value < min || value > max {
        return Err(format!("{setting} 的取值必须在 {min}–{max} 之间。"));
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey_with_flags(path, KEY_READ | KEY_WRITE)
        .map_err(|error| registry_error("打开系统设置注册表", error))?;
    let before: u32 = key.get_value(name).unwrap_or(0);
    key.set_value(name, &value)
        .map_err(|error| registry_error("写入系统设置", error))?;
    let after: u32 = key.get_value(name).map_err(|error| registry_error("回读系统设置", error))?;
    if after != value {
        return Err("写入后回读系统设置不一致。".into());
    }
    Ok((json!(before), json!(after)))
}

fn crash_dump_value(mode: &str) -> Option<u32> {
    match mode {
        "none" => Some(0),
        "complete" => Some(1),
        "kernel" => Some(2),
        "small" => Some(3),
        "automatic" => Some(7),
        _ => None,
    }
}

fn run_crash_dump(request: &Value) -> Result<Outcome, String> {
    let mode = request
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value = crash_dump_value(mode)
        .ok_or_else(|| "不支持的转储模式。".to_string())?;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey_with_flags(
            "SYSTEM\\CurrentControlSet\\Control\\CrashControl",
            KEY_READ | KEY_WRITE,
        )
        .map_err(|error| registry_error("打开崩溃控制注册表", error))?;
    let before: u32 = key
        .get_value("CrashDumpEnabled")
        .map_err(|error| registry_error("读取转储模式", error))?;
    key.set_value("CrashDumpEnabled", &value)
        .map_err(|error| registry_error("写入转储模式", error))?;
    let after: u32 = key
        .get_value("CrashDumpEnabled")
        .map_err(|error| registry_error("回读转储模式", error))?;
    if after != value {
        return Err("写入后回读转储模式不一致。".into());
    }
    Ok(Outcome {
        message: "崩溃转储模式已更新".into(),
        data: Some(json!({
            "mode": mode,
            "before": before,
            "after": after,
        })),
    })
}

fn run_restore_point() -> Result<Outcome, String> {
    let script = "$ErrorActionPreference='Stop'; Checkpoint-Computer -Description 'SystemLens manual restore point' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'ok'";
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("无法启动还原点命令：{error}"))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let text = if text.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { text };
        return Err(format!("创建还原点失败：{text}"));
    }
    Ok(Outcome {
        message: "系统还原点已创建".into(),
        data: Some(json!({"created": true})),
    })
}

fn run_expand(request: &Value) -> Result<Outcome, String> {
    let drive = request.get("drive").and_then(Value::as_str).unwrap_or_default();
    if !valid_drive(drive) {
        return Err("盘符必须是单个字母。".into());
    }
    let target = request.get("targetBytes").and_then(Value::as_u64).ok_or_else(|| "缺少 targetBytes。".to_string())?;
    if target < 1024 * 1024 * 1024 {
        return Err("目标容量过小。".into());
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; $d='{drive}'; $target=[uint64]{target}; \
$vol=Get-Volume -DriveLetter $d -ErrorAction Stop; \
if($vol.FileSystem -ne 'NTFS'){{ throw '只支持 NTFS 卷' }}; \
$part=Get-Partition -DriveLetter $d -ErrorAction Stop; \
$disk=Get-Disk -Number $part.DiskNumber -ErrorAction Stop; \
$end=[uint64]$part.Offset + [uint64]$part.Size; \
$next=Get-Partition -DiskNumber $part.DiskNumber | Where-Object {{ [uint64]$_.Offset -gt $end }} | Sort-Object Offset | Select-Object -First 1; \
$maxEnd=if($next){{[uint64]$next.Offset}}else{{[uint64]$disk.Size}}; \
$maxSize=$maxEnd - [uint64]$part.Offset; \
if($target -le [uint64]$vol.Size){{ throw '目标容量必须大于当前容量' }}; \
if($target -gt $maxSize){{ throw '目标超过右侧相邻未分配空间' }}; \
try {{ $bl=Get-BitLockerVolume -MountPoint ($d+':') -ErrorAction Stop; if($bl.ProtectionStatus -eq 'On'){{ throw 'BitLocker 已启用，拒绝扩容' }} }} catch {{ if($_.Exception.Message -like '*BitLocker*'){{ throw }} }}; \
Resize-Partition -DriveLetter $d -Size $target -ErrorAction Stop; \
$after=(Get-Volume -DriveLetter $d).Size; \
if([uint64]$after -lt $target){{ throw '扩容后容量未达到目标' }}; \
@{{before=[uint64]$vol.Size; after=[uint64]$after}} | ConvertTo-Json -Compress"
    );
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| format!("无法启动扩容命令：{error}"))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("扩容失败：{text}"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{feff}');
    let value: Value = serde_json::from_str(text).map_err(|error| format!("无法解析扩容结果：{error}"))?;
    Ok(Outcome {
        message: "分区扩容完成".into(),
        data: Some(value),
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
    fn power_alias_whitelist_rejects_arbitrary_input() {
        // 别名会直接拼进 powercfg 命令行，白名单之外一律拒绝。
        assert!(valid_power_alias("SUB_PROCESSOR"));
        assert!(valid_power_alias("PROCTHROTTLEMAX"));
        assert!(!valid_power_alias("SUB_PROCESSOR & calc"));
        assert!(!valid_power_alias(""));
        assert!(!valid_power_alias("sub_processor"));
    }

    #[test]
    fn guid_text_must_be_standard_format() {
        assert!(valid_guid_text("381b4222-f694-41f0-9685-ff5bb260df2e"));
        assert!(!valid_guid_text("381b4222-f694-41f0-9685"));
        assert!(!valid_guid_text("381b4222-f694-41f0-9685-ff5bb260df2e; calc"));
        assert!(!valid_guid_text("zzzzzzzz-f694-41f0-9685-ff5bb260df2e"));
    }

    #[test]
    fn item_ids_are_restricted_to_safe_characters() {
        assert!(valid_item_id("processor-min"));
        assert!(!valid_item_id("Processor Min"));
        assert!(!valid_item_id("a;calc"));
        assert!(!valid_item_id(""));
    }

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
        // Scratch space lives next to the test binary: %TEMP% is not writable in
        // every environment (sandboxes / CI), and a failing create_dir_all here
        // would look like a product bug instead of an environment problem.
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|parent| parent.join(format!("lens-helper-{}", std::process::id())))
            })
            .expect("测试二进制路径");
        std::fs::create_dir_all(&dir).unwrap();
        let nonce = "0123456789abcdef0123456789abcdef";
        let inside = dir.join(format!("res-{nonce}.json"));
        let good = json!({"nonce": nonce, "resultPath": inside});
        assert!(result_path(&good, &dir).is_some());

        let outside = dir.join("..").join(format!("res-{nonce}.json"));
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

    #[test]
    fn defrag_drive_and_mode_validation() {
        assert!(valid_drive("C"));
        assert!(valid_drive("d"));
        assert!(!valid_drive("C:"));
        assert!(!valid_drive(""));

        assert!(valid_defrag_mode("analyze"));
        assert!(valid_defrag_mode("optimize"));
        assert!(valid_defrag_mode("trim"));
        assert!(!valid_defrag_mode("delete"));
    }

    #[test]
    fn domains_are_validated() {
        assert!(valid_domain("ads.example.com"));
        assert!(valid_domain("example.com"));
        assert!(!valid_domain(""));
        assert!(!valid_domain("-bad.com"));
        assert!(!valid_domain("bad-.com"));
        assert!(!valid_domain("bad..com"));
        assert!(!valid_domain("bad domain.com"));
    }

    #[test]
    fn strips_only_managed_hosts_block() {
        let text = "127.0.0.1 localhost\r\n# BEGIN SystemLens AdBlock\r\n0.0.0.0 ads.com\r\n# END SystemLens AdBlock\r\n1.1.1.1 foo.com\r\n";
        let (output, removed) = strip_managed_block(text);
        assert_eq!(removed, 1);
        assert!(output.contains("localhost"));
        assert!(output.contains("foo.com"));
        assert!(!output.contains("ads.com"));
    }

    #[test]
    fn crash_dump_modes_are_limited() {
        assert_eq!(crash_dump_value("none"), Some(0));
        assert_eq!(crash_dump_value("complete"), Some(1));
        assert_eq!(crash_dump_value("kernel"), Some(2));
        assert_eq!(crash_dump_value("small"), Some(3));
        assert_eq!(crash_dump_value("automatic"), Some(7));
        assert_eq!(crash_dump_value("invalid"), None);
    }
}
