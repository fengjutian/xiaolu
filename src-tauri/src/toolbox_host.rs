use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::toolbox::{ToolActionDescriptor, ToolDescriptor};

static TOKEN_SECRET: OnceLock<String> = OnceLock::new();
static USED_TOKENS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// 工具进度事件名：界面按 jobId 关联到具体的一次执行。
const PROGRESS_EVENT: &str = "toolbox-progress";
/// 取消后留给工具自行收尾的宽限期；超时才强制结束子进程。
const CANCEL_GRACE: Duration = Duration::from_secs(3);
/// 轮询间隔：决定取消与超时的响应速度。
const POLL_INTERVAL: Duration = Duration::from_millis(200);

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

/// 取消标记文件所在目录：工具进程轮询这里的标记文件是否存在。
fn cancel_dir(app: &AppHandle) -> Option<PathBuf> {
    let directory = state_dir(app)?.join("tool-cancel");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

/// 正在运行的工具任务：界面用它请求取消。
#[derive(Debug, Clone)]
struct ToolJob {
    tool_id: String,
    action: String,
    /// 取消标记文件：宿主请求取消时创建它，工具看到后自行收尾。
    marker: PathBuf,
    cancelled: bool,
}

/// 工具任务注册表：只有带 jobId 的执行才会登记，也才支持取消。
#[derive(Default)]
pub struct ToolboxState {
    jobs: Mutex<HashMap<String, ToolJob>>,
}

impl ToolboxState {
    fn register(&self, job_id: &str, job: ToolJob) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job_id.to_string(), job);
        }
    }

    fn finish(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(job_id);
        }
    }

    /// 请求取消：落下标记文件，工具进程下一轮轮询就会看到。
    fn request_cancel(&self, job_id: &str) -> bool {
        let Ok(mut jobs) = self.jobs.lock() else {
            return false;
        };
        let Some(job) = jobs.get_mut(job_id) else {
            return false;
        };
        job.cancelled = true;
        eprintln!(
            "[toolbox] 请求取消任务 {job_id}（{} / {}）",
            job.tool_id, job.action
        );
        std::fs::write(&job.marker, b"cancel").is_ok()
    }

    fn is_cancelled(&self, job_id: &str) -> bool {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(job_id).map(|job| job.cancelled))
            .unwrap_or(false)
    }
}

/// 进度事件负载：工具负责给 phase/done/total/message，宿主补上归属信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub job_id: String,
    pub tool_id: String,
    pub action: String,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

fn progress_payload(job_id: &str, tool_id: &str, action: &str, data: &Value) -> ToolProgress {
    ToolProgress {
        job_id: job_id.to_string(),
        tool_id: tool_id.to_string(),
        action: action.to_string(),
        phase: data
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        done: data.get("done").and_then(Value::as_u64).unwrap_or_default(),
        total: data.get("total").and_then(Value::as_u64).unwrap_or_default(),
        message: data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// 任务 id 会拼进文件路径，必须挡住路径穿越与超长输入。
fn validate_job_id(job_id: &str) -> Result<(), String> {
    if job_id.is_empty() || job_id.len() > 64 {
        return Err("任务 id 无效".into());
    }
    if !job_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err("任务 id 只能包含字母、数字、连字符与下划线".into());
    }
    Ok(())
}

/// 任务结束时的清理：注销任务并删除取消标记文件。
struct JobGuard<'a> {
    app: &'a AppHandle,
    job_id: Option<String>,
    marker: Option<PathBuf>,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if let Some(job_id) = &self.job_id {
            // 清理发生在展开路径上，不能因为状态缺失而 panic。
            if let Some(state) = self.app.try_state::<ToolboxState>() {
                state.finish(job_id);
            }
        }
        if let Some(marker) = &self.marker {
            let _ = std::fs::remove_file(marker);
        }
    }
}

fn tools_dirs(app: &AppHandle) -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_TOOLS_DIR") {
        return vec![PathBuf::from(path)];
    }
    let mut candidates = Vec::new();
    if let Ok(directory) = app.path().app_local_data_dir() {
        let candidate = directory.join("tools");
        if candidate.is_dir() {
            candidates.push(candidate);
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            let candidate = parent.join("tools");
            if candidate.is_dir() {
                candidates.push(candidate);
            }
        }
    }
    // 开发模式下 Tauri 的当前目录可能是仓库根目录，也可能是 src-tauri；
    // 两种位置都纳入候选，避免本地安装目录存在时把开发工具包全部遮蔽。
    candidates.push(PathBuf::from("tools-dist"));
    #[cfg(debug_assertions)]
    if let Some(repository_root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        candidates.push(repository_root.join("tools-dist"));
    }

    let mut unique = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| path.is_dir())
        .filter(|path| unique.insert(path.clone()))
        .collect()
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
    discover_in_roots(tools_dirs(app))
}

fn discover_in_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<(PathBuf, Manifest)> {
    let mut found = HashMap::<String, (PathBuf, Manifest)>::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }
            if found.contains_key(&name) {
                continue;
            }
            if let Some(manifest) = load_manifest(&entry.path(), &name) {
                found.insert(name, (entry.path(), manifest));
            }
        }
    }
    let mut found = found.into_values().collect::<Vec<_>>();
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
    job_id: Option<String>,
) -> Result<Value, String> {
    // 先校验任务 id：它会拼进文件路径，而且校验失败不应该白白消耗掉确认令牌。
    if let Some(id) = job_id.as_deref() {
        validate_job_id(id)?;
    }
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

    // 任务 id 与取消标记必须在启动子进程前准备好：工具一启动就可能去读标记文件。
    let job = match job_id.as_deref() {
        Some(id) => {
            let cancel_directory = cancel_dir(app).ok_or("无法准备任务取消目录")?;
            let marker = cancel_directory.join(format!("{id}.cancel"));
            let _ = std::fs::remove_file(&marker);
            Some((id.to_string(), marker))
        }
        None => None,
    };

    let executable = directory.join(&manifest.entry);
    let mut command = Command::new(&executable);
    command
        .arg("--stdio")
        .env("SYSTEM_LENS_TOKEN_SECRET", token_secret())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    if let Some((_, marker)) = &job {
        command.env("SYSTEM_LENS_CANCEL_FILE", marker);
    }
    if let Some(directory) = state_dir(app) {
        command.env("SYSTEM_LENS_STATE_DIR", directory);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动工具 {}：{error}", executable.display()))?;

    if let Some((id, marker)) = &job {
        app.state::<ToolboxState>().register(
            id,
            ToolJob {
                tool_id: tool_id.to_string(),
                action: action.to_string(),
                marker: marker.clone(),
                cancelled: false,
            },
        );
    }
    let _guard = JobGuard {
        app,
        job_id: job.as_ref().map(|(id, _)| id.clone()),
        marker: job.as_ref().map(|(_, marker)| marker.clone()),
    };

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
    let started = Instant::now();
    let mut received_bytes = 0usize;
    let mut cancel_deadline: Option<Instant> = None;
    loop {
        // 超时与取消每轮都检查：工具持续上报进度时不能只等收不到消息才判定。
        if started.elapsed() >= timeout {
            child.kill().ok();
            return Err("工具响应超时".into());
        }
        if cancel_deadline.is_none() {
            if let Some((id, _)) = &job {
                if app.state::<ToolboxState>().is_cancelled(id) {
                    cancel_deadline = Some(Instant::now() + CANCEL_GRACE);
                }
            }
        }
        if let Some(deadline) = cancel_deadline {
            if Instant::now() >= deadline {
                child.kill().ok();
                return Err("操作已取消".into());
            }
        }
        let line = match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                child.kill().ok();
                return Err("工具进程意外退出".into());
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
            Some("progress") => {
                if let Some((id, _)) = &job {
                    let data = message.get("data").cloned().unwrap_or(Value::Null);
                    let _ = app.emit(
                        PROGRESS_EVENT,
                        progress_payload(id, tool_id, action, &data),
                    );
                }
                continue;
            }
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
    job_id: Option<String>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_action_blocking(&app, &tool_id, &action, confirmation, parameters, job_id)
    })
    .await
    .map_err(|error| format!("工具执行任务失败：{error}"))?
}

/// 请求取消正在执行的工具任务：工具看到标记文件后自行收尾并清理半成品。
#[tauri::command]
pub async fn cancel_tool_action(app: AppHandle, job_id: String) -> Result<bool, String> {
    validate_job_id(&job_id)?;
    Ok(app.state::<ToolboxState>().request_cancel(&job_id))
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
    fn merges_tool_roots_and_prefers_the_first_copy() {
        let test_root = |name: &str| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("{name}-{}", std::process::id()))
        };
        let first = test_root("toolbox-roots-first");
        let second = test_root("toolbox-roots-second");
        let write_tool = |root: &Path, id: &str, name: &str| {
            let directory = root.join(id);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{id}.exe")), b"test").unwrap();
            let manifest = serde_json::json!({
                "schemaVersion": 1,
                "id": id,
                "name": name,
                "version": "1.0.0",
                "apiVersion": 1,
                "description": "test",
                "category": "test",
                "risk": "read_only",
                "entry": format!("{id}.exe"),
                "actions": [{"id": "preview", "write": false}]
            });
            std::fs::write(
                directory.join("tool.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        };
        write_tool(&first, "shared", "本地版本");
        write_tool(&second, "shared", "内置版本");
        write_tool(&second, "bundled", "内置工具");

        let found = discover_in_roots([first.clone(), second.clone()]);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .any(|(_, manifest)| manifest.id == "shared" && manifest.name == "本地版本"));
        assert!(found
            .iter()
            .any(|(_, manifest)| manifest.id == "bundled"));

        std::fs::remove_dir_all(first).ok();
        std::fs::remove_dir_all(second).ok();
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

    #[test]
    fn rejects_unsafe_job_ids() {
        assert!(validate_job_id("job-abc_123").is_ok());
        assert!(validate_job_id("").is_err());
        assert!(validate_job_id("..\\evil").is_err());
        assert!(validate_job_id("job/1").is_err());
        assert!(validate_job_id("job 1").is_err());
        assert!(validate_job_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn cancel_writes_marker_and_marks_job() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        let marker = temp.join("job.cancel");
        let _ = std::fs::remove_file(&marker);

        let state = ToolboxState::default();
        assert!(!state.request_cancel("job-missing"));
        state.register(
            "job-1",
            ToolJob {
                tool_id: "omni_zip".into(),
                action: "apply".into(),
                marker: marker.clone(),
                cancelled: false,
            },
        );
        assert!(!state.is_cancelled("job-1"));
        assert!(state.request_cancel("job-1"));
        assert!(state.is_cancelled("job-1"));
        assert!(marker.is_file(), "取消必须落下标记文件供工具进程发现");

        state.finish("job-1");
        assert!(!state.is_cancelled("job-1"));
        assert!(!state.request_cancel("job-1"));

        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn progress_payload_fills_defaults() {
        let payload = progress_payload(
            "job-1",
            "omni_zip",
            "apply",
            &serde_json::json!({"phase": "compress", "done": 3, "total": 10, "message": "处理中"}),
        );
        assert_eq!(payload.job_id, "job-1");
        assert_eq!(payload.tool_id, "omni_zip");
        assert_eq!(payload.action, "apply");
        assert_eq!(payload.phase, "compress");
        assert_eq!(payload.done, 3);
        assert_eq!(payload.total, 10);
        assert_eq!(payload.message, "处理中");

        let empty = progress_payload("job-2", "omni_zip", "apply", &Value::Null);
        assert_eq!(empty.phase, "");
        assert_eq!(empty.done, 0);
        assert_eq!(empty.total, 0);
    }
}
