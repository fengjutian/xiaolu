use serde_json::{json, Value};
use std::time::Duration;
use tool_protocol::{column, human_bytes, item, table};

const TOP_N: usize = 15;
const GIB: u64 = 1024 * 1024 * 1024;

#[repr(C)]
#[derive(Default)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
}

#[repr(C)]
#[derive(Default)]
struct PerformanceInfo {
    cb: u32,
    commit_total: usize,
    commit_limit: usize,
    commit_peak: usize,
    physical_total: usize,
    physical_available: usize,
    system_cache: usize,
    kernel_total: usize,
    kernel_paged: usize,
    kernel_nonpaged: usize,
    page_size: usize,
    handle_count: u32,
    process_count: u32,
    thread_count: u32,
}

#[link(name = "psapi")]
extern "system" {
    fn GetPerformanceInfo(info: *mut PerformanceInfo, cb: u32) -> i32;
}

fn memory_status() -> Option<MemoryStatusEx> {
    let mut status = MemoryStatusEx::default();
    status.length = std::mem::size_of::<MemoryStatusEx>() as u32;
    // SAFETY: `status` is a correctly sized, writable MEMORYSTATUSEX buffer.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status)
}

fn performance_info() -> Option<PerformanceInfo> {
    let mut info = PerformanceInfo::default();
    info.cb = std::mem::size_of::<PerformanceInfo>() as u32;
    // SAFETY: `info` matches PERFORMANCE_INFORMATION, including its `cb` size field.
    let ok = unsafe { GetPerformanceInfo(&mut info, info.cb) };
    (ok != 0 && info.page_size > 0).then_some(info)
}

fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// 依据内存负载、提交量占比与可用物理内存给出只读建议。
fn verdict(load_percent: f64, commit_percent: f64, available_gib: f64) -> &'static str {
    if load_percent >= 90.0 || commit_percent >= 90.0 {
        "内存压力高：先关闭不必要程序，再检查是否有进程持续增长"
    } else if load_percent >= 75.0 || commit_percent >= 80.0 || available_gib < 1.0 {
        "内存使用偏高：关注常驻程序与浏览器标签数量"
    } else if load_percent <= 30.0 && commit_percent <= 50.0 {
        "内存充足：无需清理工作集"
    } else {
        "内存使用正常：保持观察即可"
    }
}

fn process_suggestion(memory: u64, virtual_memory: u64, name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if memory >= 2 * GIB {
        "工作集超过 2 GB，确认是否需要常驻"
    } else if virtual_memory >= 4 * GIB {
        "提交内存超过 4 GB，可能缓存较多或存在泄漏"
    } else if lower.contains("msedge") || lower.contains("chrome") || lower.contains("firefox") {
        "浏览器多标签/扩展会持续占用内存"
    } else {
        ""
    }
}

fn scan() -> Value {
    let mut system = match sysinfo::System::new_all() {
        Ok(system) => system,
        Err(error) => {
            return table(
                json!([item("状态", "不可用")]),
                json!([]),
                json!([]),
                json!([format!("无法初始化内存采集：{error}")]),
            )
        }
    };
    std::thread::sleep(Duration::from_millis(300));
    system.refresh_all();

    let mut processes = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            (
                process.name().to_string_lossy().into_owned(),
                pid.as_u32(),
                process.memory(),
                process.virtual_memory(),
            )
        })
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| right.2.cmp(&left.2));

    let status = memory_status();
    let performance = performance_info();

    let total_phys = status
        .as_ref()
        .map(|value| value.total_phys)
        .unwrap_or_else(|| system.total_memory());
    let avail_phys = status
        .as_ref()
        .map(|value| value.avail_phys)
        .unwrap_or_else(|| system.available_memory());
    let used_phys = total_phys.saturating_sub(avail_phys);
    let load_percent = status
        .as_ref()
        .map(|value| value.memory_load as f64)
        .unwrap_or_else(|| percent(used_phys, total_phys));

    let mut summary = vec![
        item(
            "物理内存",
            format!("{} / {}", human_bytes(used_phys), human_bytes(total_phys)),
        ),
        item("内存负载", format!("{load_percent:.0}%")),
        item("可用物理内存", human_bytes(avail_phys)),
    ];
    let mut notes = vec![
        json!("只读诊断：不强制清空工作集、不结束进程、不修改页面文件设置。"),
        json!("Windows 的“可用内存”包含待机缓存，不等于可以立即释放的物理内存。"),
        json!("“一键释放内存”通常只是清空工作集制造空闲假象，之后会因缺页再次读取，本工具不提供该行为。"),
    ];

    let (commit_percent, commit_total, commit_limit) = match performance.as_ref() {
        Some(info) => {
            let page_size = info.page_size as u64;
            let commit_total = info.commit_total as u64 * page_size;
            let commit_limit = info.commit_limit as u64 * page_size;
            let system_cache = info.system_cache as u64 * page_size;
            let kernel_paged = info.kernel_paged as u64 * page_size;
            let kernel_nonpaged = info.kernel_nonpaged as u64 * page_size;

            summary.push(item(
                "提交量",
                format!("{} / {}", human_bytes(commit_total), human_bytes(commit_limit)),
            ));
            summary.push(item(
                "提交占比",
                format!("{:.0}%", percent(commit_total, commit_limit)),
            ));
            summary.push(item("系统缓存", human_bytes(system_cache)));
            summary.push(item(
                "内核分页/非分页池",
                format!("{} / {}", human_bytes(kernel_paged), human_bytes(kernel_nonpaged)),
            ));
            summary.push(item(
                "进程 / 线程 / 句柄",
                format!(
                    "{} / {} / {}",
                    info.process_count, info.thread_count, info.handle_count
                ),
            ));
            (
                percent(commit_total, commit_limit),
                commit_total,
                commit_limit,
            )
        }
        None => {
            notes.push(json!("提交量与内核池信息不可用：GetPerformanceInfo 调用失败。"));
            (0.0, 0, 0)
        }
    };

    if let Some(status) = status.as_ref() {
        summary.push(item(
            "提交上限（含页面文件）",
            human_bytes(status.total_page_file),
        ));
    }

    let available_gib = avail_phys as f64 / GIB as f64;
    let advice = verdict(load_percent, commit_percent, available_gib);
    summary.push(item("诊断", advice));

    let mut rows = Vec::new();
    for (name, pid, memory, virtual_memory) in processes.iter().take(TOP_N) {
        rows.push(json!({
            "name": name,
            "pid": pid,
            "memory": human_bytes(*memory),
            "commit": human_bytes(*virtual_memory),
            "suggestion": process_suggestion(*memory, *virtual_memory, name),
        }));
    }

    let _ = commit_total;
    let _ = commit_limit;

    table(
        Value::Array(summary),
        json!([
            column("name", "进程"),
            column("pid", "PID"),
            column("memory", "工作集"),
            column("commit", "提交内存"),
            column("suggestion", "提示"),
        ]),
        Value::Array(rows),
        Value::Array(notes),
    )
}

fn main() {
    tool_protocol::serve("memopt_pro", env!("CARGO_PKG_VERSION"), &["preview"], |_| {
        Ok(scan())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_escalates_with_pressure() {
        assert!(verdict(95.0, 20.0, 8.0).starts_with("内存压力高"));
        assert!(verdict(20.0, 92.0, 8.0).starts_with("内存压力高"));
        assert!(verdict(78.0, 40.0, 8.0).starts_with("内存使用偏高"));
        assert!(verdict(20.0, 40.0, 0.5).starts_with("内存使用偏高"));
        assert_eq!(verdict(20.0, 30.0, 8.0), "内存充足：无需清理工作集");
        assert_eq!(verdict(50.0, 60.0, 4.0), "内存使用正常：保持观察即可");
    }

    #[test]
    fn process_suggestion_uses_thresholds() {
        assert!(!process_suggestion(3 * GIB, 0, "worker.exe").is_empty());
        assert!(!process_suggestion(0, 5 * GIB, "cache.exe").is_empty());
        assert!(process_suggestion(1024, 2048, "msedge.exe").contains("浏览器"));
        assert!(process_suggestion(1024, 2048, "notepad.exe").is_empty());
    }

    #[test]
    fn percent_handles_zero_total() {
        assert_eq!(percent(10, 0), 0.0);
        assert!((percent(1, 4) - 25.0).abs() < f64::EPSILON);
    }
}
