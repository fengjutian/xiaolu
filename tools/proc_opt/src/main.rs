use serde_json::{json, Value};
use std::ffi::c_void;
use std::time::Duration;
use tool_protocol::{column, digest_of, human_bytes, item, table, Request, ToolError};

const TOP_N: usize = 20;
const ITEM_LIMIT: usize = 24;
const GIB: u64 = 1024 * 1024 * 1024;
const ACTION_END: &str = "end_process";
const ACTION_PRIORITY: &str = "set_priority";

const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_SET_INFORMATION: u32 = 0x0200;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn TerminateProcess(handle: *mut c_void, code: u32) -> i32;
    fn GetPriorityClass(handle: *mut c_void) -> u32;
    fn SetPriorityClass(handle: *mut c_void, class: u32) -> i32;
    fn IsProcessCritical(handle: *mut c_void, critical: *mut i32) -> i32;
    fn GetLastError() -> u32;
}

fn win_error(context: &str) -> String {
    let code = unsafe { GetLastError() };
    if code == 5 {
        format!("{context}：拒绝访问，可能需要管理员权限。")
    } else {
        format!("{context}：Windows 错误码 {code}。")
    }
}

fn priority_label(class: u32) -> &'static str {
    match class {
        IDLE_PRIORITY_CLASS => "空闲",
        BELOW_NORMAL_PRIORITY_CLASS => "低于正常",
        NORMAL_PRIORITY_CLASS => "正常",
        ABOVE_NORMAL_PRIORITY_CLASS => "高于正常",
        HIGH_PRIORITY_CLASS => "高",
        _ => "未知",
    }
}

fn priority_key(class: u32) -> &'static str {
    match class {
        IDLE_PRIORITY_CLASS => "idle",
        BELOW_NORMAL_PRIORITY_CLASS => "below_normal",
        NORMAL_PRIORITY_CLASS => "normal",
        ABOVE_NORMAL_PRIORITY_CLASS => "above_normal",
        HIGH_PRIORITY_CLASS => "high",
        _ => "unknown",
    }
}

fn priority_class(key: &str) -> Option<u32> {
    match key {
        "idle" => Some(IDLE_PRIORITY_CLASS),
        "below_normal" => Some(BELOW_NORMAL_PRIORITY_CLASS),
        "normal" => Some(NORMAL_PRIORITY_CLASS),
        "above_normal" => Some(ABOVE_NORMAL_PRIORITY_CLASS),
        "high" => Some(HIGH_PRIORITY_CLASS),
        _ => None,
    }
}

fn is_protected_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "system"
            | "registry"
            | "smss.exe"
            | "csrss.exe"
            | "wininit.exe"
            | "winlogon.exe"
            | "services.exe"
            | "lsass.exe"
            | "svchost.exe"
    )
}

fn suggestion(name: &str, cpu: f32, memory: u64) -> &'static str {
    if memory >= 2 * GIB {
        "内存占用较高，可检查是否有必要常驻"
    } else if cpu >= 30.0 {
        "CPU 占用较高，可观察是否持续"
    } else if name.to_lowercase().contains("msedge") || name.to_lowercase().contains("chrome") {
        "浏览器多标签/扩展可能占用较多内存"
    } else {
        ""
    }
}

struct ProcInfo {
    name: String,
    pid: u32,
    start_time: u64,
    cpu: f32,
    memory: u64,
}

fn collect() -> Result<Vec<ProcInfo>, String> {
    let mut system = sysinfo::System::new_all().map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_millis(300));
    system.refresh_all();
    Ok(system
        .processes()
        .iter()
        .map(|(pid, process)| ProcInfo {
            name: process.name().to_string_lossy().into_owned(),
            pid: pid.as_u32(),
            start_time: process.start_time(),
            cpu: process.cpu_usage(),
            memory: process.memory(),
        })
        .collect())
}

fn priority_of(pid: u32) -> (String, String) {
    // SAFETY: 句柄由 OpenProcess 返回并在使用后关闭。
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return ("unknown".into(), "未知".into());
        }
        let class = GetPriorityClass(handle);
        CloseHandle(handle);
        (priority_key(class).into(), priority_label(class).into())
    }
}

fn is_critical(handle: *mut c_void) -> bool {
    let mut critical: i32 = 0;
    // SAFETY: handle 来自 OpenProcess，IsProcessCritical 写入 i32。
    let ok = unsafe { IsProcessCritical(handle, &mut critical) };
    ok != 0 && critical != 0
}

fn process_alive(pid: u32) -> bool {
    // SAFETY: 仅用于探测进程是否存在，句柄立即关闭。
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        CloseHandle(handle);
        true
    }
}

fn identity_digest(pid: u32, name: &str, start_time: u64) -> String {
    digest_of(&[&format!("{pid}|{name}|{start_time}")])
}

fn current_identity(pid: u32) -> Option<(String, u64)> {
    let mut system = sysinfo::System::new_all().ok()?;
    system.refresh_all();
    let process = system.process(sysinfo::Pid::from_u32(pid))?;
    Some((
        process.name().to_string_lossy().into_owned(),
        process.start_time(),
    ))
}

fn verify_target(
    request: &Request,
    action: &str,
) -> Result<(u32, String, u64), ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "该操作需要逐进程确认令牌，请重新预览。",
        )
    })?;
    let pid = request
        .parameters
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 pid。"))?;
    let name = request
        .parameters
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 name。"))?;
    let start_time = request
        .parameters
        .get("startTime")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 startTime。"))?;
    tool_protocol::verify_token(token, action, &identity_digest(pid, name, start_time))?;
    let (current_name, current_start) = current_identity(pid)
        .ok_or_else(|| ToolError::new("CONFLICT", "进程已退出或不存在。"))?;
    if current_name != name || current_start != start_time {
        return Err(ToolError::new(
            "CONFLICT",
            "进程身份已变化（PID 可能被复用），请重新预览。",
        ));
    }
    Ok((pid, name.to_string(), start_time))
}

fn end_process(request: &Request) -> Result<Value, ToolError> {
    let (pid, name, _) = verify_target(request, ACTION_END)?;
    if pid <= 4 || pid == std::process::id() || is_protected_name(&name) {
        return Err(ToolError::new(
            "FORBIDDEN",
            "这是系统关键进程或本工具自身，禁止结束。",
        ));
    }
    // SAFETY: 句柄由 OpenProcess 返回；失败时返回错误，成功时关闭。
    unsafe {
        let handle = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return Err(ToolError::new("PERMISSION_DENIED", win_error("打开进程")));
        }
        if is_critical(handle) {
            CloseHandle(handle);
            return Err(ToolError::new(
                "FORBIDDEN",
                " Windows 将进程标记为关键进程，结束它可能导致系统重启。",
            ));
        }
        if TerminateProcess(handle, 1) == 0 {
            let message = win_error("结束进程");
            CloseHandle(handle);
            return Err(ToolError::new("INTERNAL", message));
        }
        CloseHandle(handle);
    }
    std::thread::sleep(Duration::from_millis(300));
    if process_alive(pid) {
        return Err(ToolError::new("INTERNAL", "结束进程后仍能打开该进程。"));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "proc_opt",
        "action": ACTION_END,
        "pid": pid,
        "name": name,
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([
            item("已结束", name.clone()),
            item("PID", pid.to_string()),
        ]),
        json!([column("name", "进程"), column("status", "结果")]),
        json!([{"name": name, "status": "已结束"}]),
        json!([
            "进程已被强制结束；未保存的数据可能丢失。",
            "该操作不可撤销；系统关键进程有单独保护。",
        ]),
    ))
}

fn set_priority(request: &Request) -> Result<Value, ToolError> {
    let (pid, name, _) = verify_target(request, ACTION_PRIORITY)?;
    let key = request
        .parameters
        .get("priority")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 priority。"))?;
    let target = priority_class(key)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "不支持的优先级。"))?;
    if pid <= 4 || is_protected_name(&name) {
        return Err(ToolError::new(
            "FORBIDDEN",
            "系统关键进程不允许修改优先级。",
        ));
    }
    // SAFETY: 句柄由 OpenProcess 返回；所有路径都会关闭句柄。
    unsafe {
        let handle = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return Err(ToolError::new("PERMISSION_DENIED", win_error("打开进程")));
        }
        let before = GetPriorityClass(handle);
        if before == target {
            CloseHandle(handle);
            return Ok(table(
                json!([item("进程", name.clone()), item("优先级", priority_label(before))]),
                json!([column("name", "进程"), column("status", "结果")]),
                json!([{"name": name, "status": "已是目标优先级"}]),
                json!(["未做修改。"]),
            ));
        }
        if SetPriorityClass(handle, target) == 0 {
            let message = win_error("设置优先级");
            CloseHandle(handle);
            return Err(ToolError::new("INTERNAL", message));
        }
        let after = GetPriorityClass(handle);
        CloseHandle(handle);
        if after != target {
            return Err(ToolError::new("INTERNAL", "写入后回读优先级不一致。"));
        }
        let _ = tool_protocol::append_audit_log(json!({
            "tool": "proc_opt",
            "action": ACTION_PRIORITY,
            "pid": pid,
            "name": name,
            "before": priority_label(before),
            "after": priority_label(after),
            "at": tool_protocol::now_ms(),
        }));
        Ok(table(
            json!([
                item("进程", name.clone()),
                item("原优先级", priority_label(before)),
                item("新优先级", priority_label(after)),
            ]),
            json!([
                column("name", "进程"),
                column("before", "原优先级"),
                column("after", "新优先级"),
            ]),
            json!([{
                "name": name,
                "before": priority_label(before),
                "after": priority_label(after),
            }]),
            json!([
                "优先级只影响本次运行，进程重启后恢复默认。",
                "如需恢复，重新选择原优先级即可。",
            ]),
        ))
    }
}

fn scan() -> Result<Value, ToolError> {
    let processes = collect().map_err(|error| {
        ToolError::new("INTERNAL", format!("无法初始化进程采集：{error}"))
    })?;
    let total_memory = processes.iter().map(|process| process.memory).sum::<u64>();
    let mut memory_sorted = processes.iter().collect::<Vec<_>>();
    memory_sorted.sort_by(|left, right| right.memory.cmp(&left.memory));
    let mut cpu_sorted = processes.iter().collect::<Vec<_>>();
    cpu_sorted.sort_by(|left, right| {
        right
            .cpu
            .partial_cmp(&left.cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let rows = memory_sorted
        .iter()
        .take(TOP_N)
        .map(|process| {
            json!({
                "name": process.name,
                "pid": process.pid,
                "cpu": format!("{:.1}%", process.cpu),
                "memory": human_bytes(process.memory),
                "suggestion": suggestion(&process.name, process.cpu, process.memory),
            })
        })
        .collect::<Vec<_>>();

    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for process in memory_sorted.iter().take(16).chain(cpu_sorted.iter().take(10)) {
        if items.len() >= ITEM_LIMIT {
            break;
        }
        if !seen.insert(process.pid) {
            continue;
        }
        let (priority_key_value, priority_name) = priority_of(process.pid);
        let digest = identity_digest(process.pid, &process.name, process.start_time);
        let end_token = tool_protocol::issue_token(ACTION_END, &digest)?;
        let priority_token = tool_protocol::issue_token(ACTION_PRIORITY, &digest)?;
        let protected = process.pid <= 4
            || process.pid == std::process::id()
            || is_protected_name(&process.name);
        items.push(json!({
            "name": process.name,
            "pid": process.pid,
            "startTime": process.start_time,
            "cpu": format!("{:.1}%", process.cpu),
            "memory": human_bytes(process.memory),
            "priority": priority_key_value,
            "priorityLabel": priority_name,
            "canEnd": !protected,
            "endToken": end_token,
            "priorityToken": priority_token,
        }));
    }

    let top = memory_sorted
        .first()
        .map(|process| format!("{}（{}）", process.name, human_bytes(process.memory)))
        .unwrap_or_else(|| "无".into());

    let mut data = table(
        json!([
            item("进程总数", processes.len().to_string()),
            item("进程内存合计", human_bytes(total_memory)),
            item("内存最高", top),
        ]),
        json!([
            column("name", "进程"),
            column("pid", "PID"),
            column("cpu", "CPU"),
            column("memory", "内存"),
            column("suggestion", "提示"),
        ]),
        Value::Array(rows),
        json!([
            "列出内存占用最高的 20 个进程；结束/改优先级需逐进程确认。",
            "系统关键进程、PID ≤ 4、本工具自身在界面上被禁用。",
            "结束进程会丢失未保存数据；优先级修改只在本次运行有效。",
            "CPU 为短时间采样值，可能随系统负载波动。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("processItems".into(), Value::Array(items));
    }
    Ok(data)
}

fn main() {
    tool_protocol::serve_with_request(
        "proc_opt",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_END, ACTION_PRIORITY],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_END => end_process(request),
            ACTION_PRIORITY => set_priority(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_keys_roundtrip() {
        for key in ["idle", "below_normal", "normal", "above_normal", "high"] {
            let class = priority_class(key).expect("known priority");
            assert_eq!(priority_key(class), key);
        }
        assert!(priority_class("realtime").is_none());
    }

    #[test]
    fn protects_critical_process_names() {
        assert!(is_protected_name("lsass.exe"));
        assert!(is_protected_name("SERVICES.EXE"));
        assert!(is_protected_name("System"));
        assert!(!is_protected_name("notepad.exe"));
    }

    #[test]
    fn identity_digest_binds_pid_name_and_start_time() {
        let base = identity_digest(1234, "app.exe", 100);
        assert_eq!(base, identity_digest(1234, "app.exe", 100));
        assert_ne!(base, identity_digest(1234, "app.exe", 101));
        assert_ne!(base, identity_digest(1235, "app.exe", 100));
        assert_ne!(base, identity_digest(1234, "other.exe", 100));
    }
}
