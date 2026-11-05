use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::toolbox::ToolDescriptor;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAction {
    id: String,
    #[serde(default)]
    write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permissions {
    #[serde(default)]
    elevation: String,
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
    note: Option<String>,
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

fn receive(rx: &mpsc::Receiver<String>, timeout: Duration, label: &str) -> Result<String, String> {
    rx.recv_timeout(timeout)
        .map_err(|_| format!("{label}超时"))
}

fn run_action_blocking(app: &AppHandle, tool_id: &str, action: &str) -> Result<Value, String> {
    let found = discover(app)
        .into_iter()
        .find(|(_, manifest)| manifest.id == tool_id)
        .ok_or_else(|| format!("工具未安装：{tool_id}"))?;
    let (directory, manifest) = found;
    let allowed = manifest
        .actions
        .iter()
        .any(|candidate| candidate.id == action);
    if !allowed {
        return Err(format!("工具 {tool_id} 不支持动作 {action}"));
    }
    if action != "preview" && !manifest.risk.is_empty() && manifest.risk == "read_only" {
        return Err("只读工具不允许写操作".into());
    }

    let executable = directory.join(&manifest.entry);
    let mut child = Command::new(&executable)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
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
        "parameters": {},
        "confirmation": null
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
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || run_action_blocking(&app, &tool_id, &action))
        .await
        .map_err(|error| format!("工具执行任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
