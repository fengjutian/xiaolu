use serde_json::{json, Value};
use std::time::Duration;
use tool_protocol::{column, human_bytes, item, table};

const TOP_N: usize = 20;
const GIB: u64 = 1024 * 1024 * 1024;

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

fn scan() -> Value {
    let mut system = match sysinfo::System::new_all() {
        Ok(system) => system,
        Err(error) => {
            return table(
                json!([item("状态", "不可用")]),
                json!([]),
                json!([]),
                json!([format!("无法初始化进程采集：{error}")]),
            )
        }
    };
    std::thread::sleep(Duration::from_millis(300));
    system.refresh_all();

    let total_memory = system.total_memory();
    let used_memory = system.used_memory();
    let mut processes = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            (
                process.name().to_string_lossy().into_owned(),
                pid.as_u32(),
                process.cpu_usage(),
                process.memory(),
            )
        })
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| right.3.cmp(&left.3));

    let rows = processes
        .iter()
        .take(TOP_N)
        .map(|(name, pid, cpu, memory)| {
            json!({
                "name": name,
                "pid": pid,
                "cpu": format!("{cpu:.1}%"),
                "memory": human_bytes(*memory),
                "suggestion": suggestion(name, *cpu, *memory),
            })
        })
        .collect::<Vec<_>>();

    let top = processes
        .first()
        .map(|entry| format!("{}（{}）", entry.0, human_bytes(entry.3)))
        .unwrap_or_else(|| "无".into());

    table(
        json!([
            item("进程总数", processes.len().to_string()),
            item("内存占用", format!("{} / {}", human_bytes(used_memory), human_bytes(total_memory))),
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
            "只读分析：只列出占用较高的进程，不会结束任何进程。",
            "CPU 为短时间采样值，可能随系统负载波动。",
            "结束进程/服务需后续版本逐项确认后提供。",
        ]),
    )
}

fn main() {
    tool_protocol::serve("proc_opt", env!("CARGO_PKG_VERSION"), &["preview"], |_| {
        Ok(scan())
    });
}
