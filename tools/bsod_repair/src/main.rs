use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_DUMP: &str = "set_dump_mode";

fn dump_value(mode: &str) -> Option<u32> {
    match mode {
        "none" => Some(0),
        "complete" => Some(1),
        "kernel" => Some(2),
        "small" => Some(3),
        "automatic" => Some(7),
        _ => None,
    }
}

fn dump_label(value: u32) -> &'static str {
    match value {
        0 => "不转储",
        1 => "完整内存转储",
        2 => "内核内存转储",
        3 => "小内存转储",
        7 => "自动内存转储",
        _ => "未知",
    }
}

fn dump_mode_key(value: u32) -> &'static str {
    match value {
        0 => "none",
        1 => "complete",
        2 => "kernel",
        3 => "small",
        7 => "automatic",
        _ => "unknown",
    }
}

fn current_dump_value() -> Option<u32> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.open_subkey_with_flags(
        "SYSTEM\\CurrentControlSet\\Control\\CrashControl",
        KEY_READ,
    )
    .ok()?
    .get_value("CrashDumpEnabled")
    .ok()
}

fn dump_digest() -> String {
    digest_of(&["crash_dump"])
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
                "已取消管理员授权，未修改转储设置。",
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

fn scan() -> Result<Value, ToolError> {
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$dumps=@()
Get-ChildItem "$env:SystemRoot\Minidump" -Filter *.dmp -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 20 | ForEach-Object {
    $dumps += @{time=$_.LastWriteTime.ToString('yyyy-MM-dd HH:mm'); name=$_.Name; size=$_.Length}
  }
$events=@()
Get-WinEvent -FilterHashtable @{LogName='System'; Id=1001,41; StartTime=(Get-Date).AddDays(-30)} -ErrorAction SilentlyContinue |
  Select-Object -First 20 | ForEach-Object {
    $message=($_.Message -split "\r?\n")[0]
    $events += @{time=$_.TimeCreated.ToString('yyyy-MM-dd HH:mm'); id=$_.Id; message=$message}
  }
@{dumps=@($dumps); events=@($events)} | ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动蓝屏记录查询：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析蓝屏记录：{error}")))?;
    let dumps = payload
        .get("dumps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let events = payload
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rows = Vec::new();
    let mut latest = String::new();
    for dump in &dumps {
        let time = dump.get("time").and_then(Value::as_str).unwrap_or("");
        let size = dump.get("size").and_then(Value::as_u64).unwrap_or(0);
        let name = dump.get("name").and_then(Value::as_str).unwrap_or("");
        if latest.is_empty() {
            latest = time.to_string();
        }
        rows.push(json!({
            "kind": "转储文件",
            "time": time,
            "detail": name,
            "size": human_bytes(size),
        }));
    }
    for event in &events {
        let time = event.get("time").and_then(Value::as_str).unwrap_or("");
        let id = event.get("id").and_then(Value::as_u64).unwrap_or(0);
        let message = event.get("message").and_then(Value::as_str).unwrap_or("");
        rows.push(json!({
            "kind": if id == 41 { "异常关机" } else { "蓝屏事件" },
            "time": time,
            "detail": message,
            "size": "-",
        }));
    }

    let dump_value_current = current_dump_value();
    let mut data = table(
        json!([
            item("转储文件", dumps.len().to_string()),
            item("30 天蓝屏/异常关机", events.len().to_string()),
            item("最近一次", if latest.is_empty() { "无记录".to_string() } else { latest }),
            item("转储模式", dump_value_current.map(dump_label).unwrap_or("未知")),
        ]),
        json!([
            column("kind", "类型"),
            column("time", "时间"),
            column("detail", "详情"),
            column("size", "大小"),
        ]),
        Value::Array(rows),
        json!([
            "分析部分只读：读取 Minidump 目录与系统事件日志。",
            "可在下方切换崩溃转储模式；写入需要管理员授权。",
            "事件 41 表示异常关机，1001 为 Windows 错误报告中的蓝屏记录。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert(
            "dumpMode".into(),
            json!({
                "value": dump_value_current,
                "mode": dump_value_current.map(dump_mode_key).unwrap_or("unknown"),
                "label": dump_value_current.map(dump_label).unwrap_or("未知"),
            }),
        );
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_DUMP, &dump_digest()) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn set_dump_mode(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "修改转储模式需要确认令牌，请先预览。",
        )
    })?;
    tool_protocol::verify_token(token, ACTION_DUMP, &dump_digest())?;
    let mode = request
        .parameters
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 mode。"))?;
    let value = dump_value(mode)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "不支持的转储模式。"))?;
    let before = current_dump_value();

    run_elevated(json!({"op": "crash_dump_set", "mode": mode}))?;

    let after = current_dump_value();
    if after != Some(value) {
        return Err(ToolError::new(
            "INTERNAL",
            "提权后回读转储模式不一致。",
        ));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "bsod_repair",
        "action": ACTION_DUMP,
        "before": before,
        "after": value,
        "at": tool_protocol::now_ms(),
    }));

    let mut data = table(
        json!([
            item("原模式", before.map(dump_label).unwrap_or("未知")),
            item("新模式", dump_label(value)),
        ]),
        json!([
            column("item", "项目"),
            column("value", "结果"),
        ]),
        json!([{
            "item": "崩溃转储模式",
            "value": dump_label(value),
        }]),
        json!([
            "转储模式由独立提权助手修改，并已回读确认。",
            "小内存转储写入 %SystemRoot%\\Minidump；完整/内核转储写入 %SystemRoot%\\MEMORY.DMP。",
            "再次操作需要重新预览获取新令牌。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert(
            "dumpMode".into(),
            json!({"value": value, "mode": mode, "label": dump_label(value)}),
        );
    }
    Ok(data)
}

fn main() {
    tool_protocol::serve_with_request(
        "bsod_repair",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_DUMP],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_DUMP => set_dump_mode(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_modes_roundtrip() {
        for mode in ["none", "complete", "kernel", "small", "automatic"] {
            let value = dump_value(mode).expect("known mode");
            assert_eq!(dump_mode_key(value), mode);
        }
        assert!(dump_value("invalid").is_none());
        assert_eq!(dump_label(7), "自动内存转储");
    }
}
