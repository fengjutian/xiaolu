use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::toolbox::{ToolActionDescriptor, ToolDescriptor};

static TOKEN_SECRET: OnceLock<String> = OnceLock::new();
static USED_TOKENS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[cfg(windows)]
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: u32, flags: u32) -> i32;
}

const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAction {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permissions {
    #[serde(default)]
    elevation: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ui {
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default = "default_output")]
    max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            timeout_ms: default_timeout(),
            max_output_bytes: default_output(),
        }
    }
}

fn default_timeout() -> u64 {
    120_000
}

fn default_output() -> usize {
    4 * 1024 * 1024
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    id: String,
    name: String,
    version: String,
    api_version: u32,
    description: String,
    category: String,
    risk: String,
    entry: String,
    #[serde(default)]
    #[allow(dead_code)]
    arguments: Vec<String>,
    #[serde(default)]
    actions: Vec<ManifestAction>,
    #[serde(default)]
    permissions: Permissions,
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    ui: Ui,
    #[serde(default)]
    note: Option<String>,
}

/// 生成一次进程级令牌密钥。宿主每次启动重新生成，工具进程通过环境变量获得。
fn token_secret() -> &'static str {
    TOKEN_SECRET.get_or_init(|| {
        let mut bytes = [0u8; 32];
        #[cfg(windows)]
        {
            // SAFETY: 传入空算法句柄表示使用系统首选随机数生成器。
            let rc = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if rc != 0 {
                bytes.fill(0);
            }
        }
        if bytes.iter().all(|byte| *byte == 0) {
            // 回退：不稳定但仍是每次启动唯一，仅用于无法调用系统 RNG 的环境。
            let mut state = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0)
                ^ ((std::process::id() as u64) << 32);
            state |= 1;
            for byte in bytes.iter_mut() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
        }
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    })
}

fn state_dir(app: &AppHandle) -> Option<PathBuf> {
    let directory = app.path().app_local_data_dir().ok()?;
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

fn tools_dir(app: &AppHandle) -> PathBuf {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_TOOLS_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(directory) = app.path().app_local_data_dir() {
        let candidate = directory.join("tools");
        if candidate.is_dir() {
            return candidate;
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            let candidate = parent.join("tools");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    PathBuf::from("tools-dist")
}

fn load_manifest(directory: &Path, folder_name: &str) -> Option<Manifest> {
    let path = directory.join("tool.json");
    let bytes = std::fs::read(&path).ok()?;
    let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
    if manifest.id != folder_name {
        eprintln!("[toolbox] 目录名与清单 id 不一致：{}", path.display());
        return None;
    }
    if manifest.schema_version != 1 || manifest.api_version != 1 {
        eprintln!("[toolbox] 清单版本不兼容：{}", path.display());
        return None;
    }
    if manifest.entry.contains('\\') || manifest.entry.contains('/') {
        eprintln!("[toolbox] entry 不能包含路径分隔符：{}", path.display());
        return None;
    }
    if !directory.join(&manifest.entry).is_file() {
        eprintln!("[toolbox] 缺少入口程序：{}", directory.join(&manifest.entry).display());
        return None;
    }
    if manifest.actions.is_empty() {
        eprintln!("[toolbox] 清单没有声明任何动作：{}", path.display());
        return None;
    }
    Some(manifest)
}

fn discover(app: &AppHandle) -> Vec<(PathBuf, Manifest)> {
    let root = tools_dir(app);
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return found;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        if let Some(manifest) = load_manifest(&entry.path(), &name) {
            found.push((entry.path(), manifest));
        }
    }
    found.sort_by(|left, right| left.1.name.cmp(&right.1.name));
    found
}

fn descriptor(directory: &Path, manifest: &Manifest) -> ToolDescriptor {
    ToolDescriptor {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        category: manifest.category.clone(),
        description: manifest.description.clone(),
        risk: manifest.risk.clone(),
        requires_admin: !manifest.permissions.elevation.is_empty()
            && manifest.permissions.elevation != "none",
        status: "available".into(),
        note: manifest
            .note
            .clone()
            .unwrap_or_else(|| format!("v{} · 独立工具进程 · {}", manifest.version, directory.display())),
        ui_kind: if manifest.ui.kind.is_empty() {
            "none".into()
        } else {
            manifest.ui.kind.clone()
        },
        actions: manifest
            .actions
            .iter()
            .map(|action| ToolActionDescriptor {
                id: action.id.clone(),
                label: action
                    .label
                    .clone()
                    .unwrap_or_else(|| action.id.clone()),
                write: action.write,
            })
            .collect(),
    }
}

pub fn list_tools(app: &AppHandle) -> Vec<ToolDescriptor> {
    let mut tools = discover(app)
        .iter()
        .map(|(directory, manifest)| descriptor(directory, manifest))
        .collect::<Vec<_>>();
    for planned in crate::toolbox::tools() {
        if !tools.iter().any(|tool| tool.id == planned.id) {
            tools.push(planned);
        }
    }
    tools
}

/// 动作授权：预览始终允许；写动作必须清单声明、非只读，并携带确认令牌。
fn authorize_action(
    manifest: &Manifest,
    action: &str,
    confirmation: Option<&str>,
) -> Result<(), String> {
    let declared = manifest
        .actions
        .iter()
        .find(|candidate| candidate.id == action)
        .ok_or_else(|| format!("工具 {} 不支持动作 {}", manifest.id, action))?;
    if action == "preview" {
        return Ok(());
    }
    if !declared.write {
        return Err(format!("动作 {action} 未声明为写操作"));
    }
    if manifest.risk == "read_only" {
        return Err("只读工具不允许写操作".into());
    }
    match confirmation {
        Some(token) if !token.trim().is_empty() => Ok(()),
        _ => Err("缺少确认令牌：请先执行预览并在确认后重试".into()),
    }
}

/// 记录并拒绝重复使用的确认令牌；令牌过期由工具侧校验。
fn consume_token(confirmation: &str) -> Result<(), String> {
    let tokens = USED_TOKENS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut used = tokens
        .lock()
        .map_err(|_| "确认令牌状态不可用，请重启应用".to_string())?;
    let token = confirmation.trim().to_string();
    if used.contains(&token) {
        return Err("确认令牌已使用，请重新预览".into());
    }
    if used.len() > 512 {
        used.clear();
    }
    used.insert(token);
    Ok(())
}

fn receive(rx: &mpsc::Receiver<String>, timeout: Duration, label: &str) -> Result<String, String> {
    rx.recv_timeout(timeout)
        .map_err(|_| format!("{label}超时"))
}

fn run_action_blocking(
    app: &AppHandle,
    tool_id: &str,
    action: &str,
    confirmation: Option<String>,
    parameters: Option<Value>,
) -> Result<Value, String> {
    let found = discover(app)
        .into_iter()
        .find(|(_, manifest)| manifest.id == tool_id)
        .ok_or_else(|| format!("工具未安装：{tool_id}"))?;
    let (directory, manifest) = found;
    authorize_action(&manifest, action, confirmation.as_deref())?;
    if action != "preview" {
        let token = confirmation.as_deref().unwrap_or_default();
        consume_token(token)?;
    }

    let executable = directory.join(&manifest.entry);
    let mut command = Command::new(&executable);
    command
        .arg("--stdio")
        .env("SYSTEM_LENS_TOKEN_SECRET", token_secret())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    if let Some(directory) = state_dir(app) {
        command.env("SYSTEM_LENS_STATE_DIR", directory);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动工具 {}：{error}", executable.display()))?;

    let mut stdin = child.stdin.take().ok_or("无法写入工具进程")?;
    let stdout = child.stdout.take().ok_or("无法读取工具进程")?;
    let (sender, receiver) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let hello = receive(&receiver, Duration::from_secs(5), "工具握手")?;
    let hello: Value = serde_json::from_str(&hello).map_err(|_| "工具握手格式无效".to_string())?;
    if hello.get("type").and_then(Value::as_str) != Some("hello")
        || hello.get("protocol").and_then(Value::as_u64) != Some(1)
        || hello.get("toolId").and_then(Value::as_str) != Some(tool_id)
    {
        child.kill().ok();
        return Err("工具握手失败：协议或 id 不匹配".into());
    }

    let request = serde_json::json!({
        "type": "request",
        "id": 1,
        "action": action,
        "parameters": parameters.unwrap_or_else(|| serde_json::json!({})),
        "confirmation": confirmation,
    });
    writeln!(stdin, "{request}").map_err(|error| format!("无法发送工具请求：{error}"))?;
    stdin.flush().ok();

    let timeout = Duration::from_millis(manifest.limits.timeout_ms.max(1_000));
    let mut received_bytes = 0usize;
    loop {
        let line = match receive(&receiver, timeout, "工具响应") {
            Ok(line) => line,
            Err(error) => {
                child.kill().ok();
                return Err(error);
            }
        };
        received_bytes += line.len();
        if received_bytes > manifest.limits.max_output_bytes {
            child.kill().ok();
            return Err("工具输出超过上限".into());
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                child.kill().ok();
                return Err("工具输出不是合法 JSON".into());
            }
        };
        match message.get("type").and_then(Value::as_str) {
            Some("progress") => continue,
            Some("result") => {
                child.kill().ok();
                return Ok(message.get("data").cloned().unwrap_or(Value::Null));
            }
            Some("error") => {
                child.kill().ok();
                let code = message.get("code").and_then(Value::as_str).unwrap_or("INTERNAL");
                let text = message
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("工具返回错误");
                return Err(format!("{code}: {text}"));
            }
            _ => continue,
        }
    }
}

#[tauri::command]
pub async fn list_toolbox_tools(app: AppHandle) -> Result<Vec<ToolDescriptor>, String> {
    Ok(list_tools(&app))
}

#[tauri::command]
pub async fn run_tool_action(
    app: AppHandle,
    tool_id: String,
    action: String,
    confirmation: Option<String>,
    parameters: Option<Value>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_action_blocking(&app, &tool_id, &action, confirmation, parameters)
    })
    .await
    .map_err(|error| format!("工具执行任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(risk: &str, actions: Value) -> Manifest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": risk,
            "entry": "demo.exe",
            "actions": actions,
            "permissions": {"elevation": "none"},
            "limits": {"timeoutMs": 1000, "maxOutputBytes": 1024}
        }))
        .expect("manifest")
    }

    #[test]
    fn rejects_entry_with_path_separator() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": "read_only",
            "entry": "..\\evil.exe",
            "actions": [{"id": "preview", "write": false}]
        });
        std::fs::write(temp.join("tool.json"), serde_json::to_vec(&manifest).unwrap()).ok();
        assert!(load_manifest(&temp, "demo").is_none());
        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn parses_valid_manifest() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-valid-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        std::fs::write(temp.join("demo.exe"), b"stub").ok();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": "read_only",
            "entry": "demo.exe",
            "actions": [{"id": "preview", "write": false}],
            "permissions": {"elevation": "none"},
            "limits": {"timeoutMs": 1000, "maxOutputBytes": 1024}
        });
        std::fs::write(temp.join("tool.json"), serde_json::to_vec(&manifest).unwrap()).ok();
        let parsed = load_manifest(&temp, "demo").expect("valid manifest");
        assert_eq!(parsed.id, "demo");
        assert_eq!(parsed.limits.timeout_ms, 1000);
        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn preview_is_allowed_without_token() {
        let manifest = manifest("read_only", serde_json::json!([{"id": "preview", "write": false}]));
        assert!(authorize_action(&manifest, "preview", None).is_ok());
    }

    #[test]
    fn read_only_tools_cannot_write_even_with_token() {
        let manifest = manifest(
            "read_only",
            serde_json::json!([{"id": "apply", "write": true}]),
        );
        let error = authorize_action(&manifest, "apply", Some("token")).unwrap_err();
        assert!(error.contains("只读"));
    }

    #[test]
    fn write_requires_declared_write_action_and_token() {
        let manifest = manifest(
            "system",
            serde_json::json!([
                {"id": "preview", "label": "预览", "write": false},
                {"id": "apply", "label": "切换", "write": true}
            ]),
        );
        assert!(authorize_action(&manifest, "apply", None)
            .unwrap_err()
            .contains("确认令牌"));
        assert!(authorize_action(&manifest, "apply", Some("  "))
            .is_err());
        assert!(authorize_action(&manifest, "apply", Some("token")).is_ok());
        assert!(authorize_action(&manifest, "unknown", Some("token")).is_err());
    }

    #[test]
    fn tokens_are_single_use() {
        let token = format!("unit-test-{}", std::process::id());
        assert!(consume_token(&token).is_ok());
        assert!(consume_token(&token).unwrap_err().contains("已使用"));
    }
}
