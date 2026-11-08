use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_ROWS: usize = 300;
const ACTION_REMOVE: &str = "remove_appx";
const ACTION_EXPORT: &str = "export_list";
const ACTION_RESTORE_POINT: &str = "create_restore_point";

/// 只允许移除这些已知可重装的内置应用族；其余一律只读。
const REMOVABLE: [&str; 18] = [
    "Microsoft.XboxApp",
    "Microsoft.XboxGamingOverlay",
    "Microsoft.XboxGameOverlay",
    "Microsoft.XboxSpeechToTextOverlay",
    "Microsoft.ZuneMusic",
    "Microsoft.ZuneVideo",
    "Microsoft.WindowsMaps",
    "Microsoft.BingWeather",
    "Microsoft.BingNews",
    "Microsoft.MicrosoftSolitaireCollection",
    "Microsoft.SkypeApp",
    "Microsoft.GetHelp",
    "Microsoft.Getstarted",
    "Microsoft.MicrosoftOfficeHub",
    "Microsoft.OneConnect",
    "Microsoft.People",
    "Microsoft.WindowsFeedbackHub",
    "Microsoft.YourPhone",
];

fn is_removable(name: &str) -> bool {
    REMOVABLE.iter().any(|family| *family == name)
}

fn valid_package_full_name(name: &str) -> bool {
    name.len() >= 5
        && name.len() <= 300
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

fn scan_packages() -> Result<Vec<Value>, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$items=@()
Get-AppxPackage -ErrorAction Stop | ForEach-Object {
  $items += @{
    name = [string]$_.Name
    fullName = [string]$_.PackageFullName
    familyName = [string]$_.PackageFamilyName
    version = [string]$_.Version
    location = if($_.InstallLocation){[string]$_.InstallLocation}else{''}
  }
}
@{items=@($items)} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动应用清单查询：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析应用清单：{error}")))?;
    Ok(payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn scan() -> Result<Value, ToolError> {
    let packages = scan_packages()?;

    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut with_location = 0usize;
    let mut builtin = 0usize;
    let mut removable = 0usize;
    for package in packages.iter().take(MAX_ROWS) {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("未知应用");
        let full_name = package
            .get("fullName")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = package
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("");
        let location = package
            .get("location")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !location.is_empty() {
            with_location += 1;
        }
        let builtin_flag = name.starts_with("Microsoft.") || name.starts_with("MicrosoftWindows.");
        if builtin_flag {
            builtin += 1;
        }
        let can_remove = is_removable(name);
        if can_remove {
            removable += 1;
        }
        rows.push(json!({
            "name": name,
            "version": version,
            "type": if builtin_flag { "系统内置" } else { "第三方" },
            "location": location,
            "removable": if can_remove { "可移除（白名单）" } else { "只读" },
        }));
        items.push(json!({
            "name": name,
            "fullName": full_name,
            "version": version,
            "type": if builtin_flag { "系统内置" } else { "第三方" },
            "removable": can_remove,
        }));
    }

    let mut data = table(
        json!([
            item("已安装应用包", packages.len().to_string()),
            item("有安装目录", with_location.to_string()),
            item("系统内置候选", builtin.to_string()),
            item("白名单可移除", removable.to_string()),
        ]),
        json!([
            column("name", "应用包"),
            column("version", "版本"),
            column("type", "类型"),
            column("location", "安装目录"),
            column("removable", "移除"),
        ]),
        Value::Array(rows),
        json!([
            "只读清单 + 白名单移除：仅允许移除 REMOVABLE 列表中的内置应用。",
            "移除只影响当前用户，可从 Microsoft Store 重新安装；不处理系统组件或驱动。",
            "卸载不可逆（需重装），执行前会二次确认。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("appxItems".into(), Value::Array(items));
        map.insert("restorePointEnabled".into(), json!(restore_point_enabled()));
        if let Ok(token) = tool_protocol::issue_token(ACTION_RESTORE_POINT, &restore_point_digest()) {
            map.insert("restorePointToken".into(), Value::String(token));
        }
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_REMOVE, &digest_of(&["appx_remove"])) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn restore_point_enabled() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore",
            KEY_READ,
        )
        .ok()
        .and_then(|key| key.get_value::<u32, _>("DisableSR").ok())
        .unwrap_or(0)
        == 0
}

fn restore_point_digest() -> String {
    digest_of(&["restore_point"])
}

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
    let dir = exe.parent().ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    [dir.join("lens_helper.exe"), dir.join("..").join("lens_helper").join("lens_helper.exe")]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| ToolError::new("HELPER_MISSING", "缺少提权助手 lens_helper.exe。"))
}

fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn run_elevated(mut payload: Value) -> Result<Value, ToolError> {
    let dir = elevated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建提权请求目录：{error}")))?;
    let nonce_source = format!("{}-{}", tool_protocol::now_ms(), std::process::id());
    let nonce_full = digest_of(&[&nonce_source]);
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
    // 请求文件用「独占创建 + 其它进程只读」打开：内容写完后就无法被替换，
    // 也挡住了预先放好的同名文件（含硬链接）。句柄要保持到助手退出。
    let request_file = tool_protocol::write_request_exclusive(&request_path, &serialized)
        .map_err(|error| ToolError::new("INTERNAL", error))?;
    let helper = helper_path()?;
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath {} -ArgumentList ('--request',('\"' + {} + '\"')) -Verb RunAs -PassThru -Wait -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_quote(&helper.to_string_lossy()),
        ps_quote(&request_path.to_string_lossy())
    );
    let status = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;
    // 先释放请求文件的独占句柄，随后才能删除它。
    drop(request_file);
    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);
    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new("ELEVATION_CANCELLED", "已取消管理员授权，未创建还原点。"));
        }
        return Err(ToolError::new("ELEVATION_FAILED", format!("提权助手执行失败（退出码 {:?}）。", status.code())));
    }
    let text = result_text.ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value.get("error").and_then(Value::as_str).unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

fn create_restore_point(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new("CONFIRMATION_REQUIRED", "创建还原点需要确认令牌，请先预览。")
    })?;
    tool_protocol::verify_token(token, ACTION_RESTORE_POINT, &restore_point_digest())?;
    if !restore_point_enabled() {
        return Err(ToolError::new(
            "CONFLICT",
            "系统保护已关闭，无法创建还原点；请先在系统属性中启用系统保护。",
        ));
    }
    run_elevated(json!({"op": "restore_point_create"}))?;
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "sys_clean",
        "action": ACTION_RESTORE_POINT,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([item("结果", "已创建系统还原点")]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{"item": "系统还原点", "value": "已创建"}]),
        json!([
            "还原点由 Windows 系统保护创建，可在系统属性中查看。",
            "Checkpoint-Computer 可能受 24 小时频率限制；失败时会返回明确原因。",
        ]),
    ))
}

fn remove_appx(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "移除应用需要确认令牌，请先预览。",
        )
    })?;
    tool_protocol::verify_token(token, ACTION_REMOVE, &digest_of(&["appx_remove"]))?;
    let full_name = request
        .parameters
        .get("fullName")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 fullName。"))?;
    if !valid_package_full_name(full_name) {
        return Err(ToolError::new("INVALID_INPUT", "应用包全名格式无效。"));
    }

    let packages = scan_packages()?;
    let package = packages
        .iter()
        .find(|package| package.get("fullName").and_then(Value::as_str) == Some(full_name))
        .ok_or_else(|| ToolError::new("CONFLICT", "应用包不存在，请重新预览。"))?;
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_removable(name) {
        return Err(ToolError::new(
            "FORBIDDEN",
            "该应用不在可移除白名单中。",
        ));
    }

    let script = format!(
        "$ErrorActionPreference='Stop'; Remove-AppxPackage -Package '{}' -ErrorAction Stop; if(Get-AppxPackage -Package '{}'){{ exit 1 }}",
        full_name, full_name
    );
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动卸载：{error}")))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ToolError::new(
            "INTERNAL",
            format!("卸载失败：{text}"),
        ));
    }

    let remaining = scan_packages()?;
    if remaining
        .iter()
        .any(|package| package.get("fullName").and_then(Value::as_str) == Some(full_name))
    {
        return Err(ToolError::new("INTERNAL", "卸载后仍能查询到该应用包。"));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "sys_clean",
        "action": ACTION_REMOVE,
        "name": name,
        "fullName": full_name,
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("已移除", name),
            item("应用包", full_name),
        ]),
        json!([
            column("name", "应用包"),
            column("status", "结果"),
        ]),
        json!([{
            "name": name,
            "status": "已移除（可重装）",
        }]),
        json!([
            "只影响当前用户，可从 Microsoft Store 重新安装。",
            "已写入本地操作日志；如需恢复请重新安装该应用。",
        ]),
    ))
}

fn export_list() -> Result<Value, ToolError> {
    let packages = scan_packages()?;
    let removals = packages
        .iter()
        .filter(|package| {
            package
                .get("name")
                .and_then(Value::as_str)
                .map(is_removable)
                .unwrap_or(false)
        })
        .map(|package| {
            json!({
                "name": package.get("name").cloned().unwrap_or(Value::Null),
                "fullName": package.get("fullName").cloned().unwrap_or(Value::Null),
                "version": package.get("version").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();

    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("SystemLens")
        .join("sys_clean");
    std::fs::create_dir_all(&base)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建导出目录：{error}")))?;
    let path = base.join(format!("appx-removals-{}.json", tool_protocol::now_ms()));
    let payload = json!({
        "exportedAt": tool_protocol::now_ms(),
        "count": removals.len(),
        "packages": removals,
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&payload)
            .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化清单：{error}")))?,
    )
    .map_err(|error| ToolError::new("INTERNAL", format!("无法写入清单：{error}")))?;
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "sys_clean",
        "action": ACTION_EXPORT,
        "path": path.to_string_lossy(),
        "count": payload.get("count").cloned().unwrap_or(Value::Null),
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("导出数量", removals.len().to_string()),
            item("文件", path.to_string_lossy()),
        ]),
        json!([
            column("item", "项目"),
            column("value", "结果"),
        ]),
        json!([{
            "item": "清单文件",
            "value": path.to_string_lossy(),
        }]),
        json!([
            "清单只包含白名单可移除应用，不包含系统关键组件。",
            "导出为 JSON，可用于审计或重装后对照。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "sys_clean",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_REMOVE, ACTION_EXPORT, ACTION_RESTORE_POINT],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_REMOVE => remove_appx(request),
            ACTION_EXPORT => export_list(),
            ACTION_RESTORE_POINT => create_restore_point(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_matches_exact_families() {
        assert!(is_removable("Microsoft.XboxApp"));
        assert!(is_removable("Microsoft.BingNews"));
        assert!(!is_removable("Microsoft.WindowsStore"));
        assert!(!is_removable("Microsoft.XboxApp.Dev"));
        assert!(!is_removable(""));
    }

    #[test]
    fn package_full_name_is_restricted() {
        assert!(valid_package_full_name(
            "Microsoft.XboxApp_48.89.18001.0_x64__8wekyb3d8bbwe"
        ));
        assert!(!valid_package_full_name("bad name"));
        assert!(!valid_package_full_name("bad'name"));
        assert!(!valid_package_full_name(""));
    }
}
