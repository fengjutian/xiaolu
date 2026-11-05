use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, System};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    brand: String,
    logical_cores: usize,
    usage_percent: f32,
    frequency_mhz: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    total_bytes: u64,
    used_bytes: u64,
    usage_percent: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    name: String,
    mount_point: String,
    file_system: String,
    total_bytes: u64,
    available_bytes: u64,
    removable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_percent: f32,
    memory_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    host_name: String,
    os_name: String,
    os_version: String,
    kernel_version: String,
    architecture: String,
    uptime_seconds: u64,
    captured_at_unix_ms: u128,
    cpu: CpuInfo,
    memory: MemoryInfo,
    disks: Vec<DiskInfo>,
    processes: Vec<ProcessInfo>,
}

pub fn collect_snapshot() -> Result<SystemSnapshot, String> {
    let mut system = System::new_all().map_err(|error| format!("无法初始化系统信息：{error}"))?;
    system.refresh_all();

    let cpu_usage = system.global_cpu_usage();
    let cpu = system.cpus().first();
    let total_memory = system.total_memory();
    let used_memory = system.used_memory();
    let usage_percent = if total_memory == 0 { 0.0 } else { used_memory as f64 / total_memory as f64 * 100.0 };

    let disks = Disks::new_with_refreshed_list()
        .map_err(|error| format!("无法读取磁盘信息：{error}"))?
        .iter()
        .map(|disk| DiskInfo {
            name: disk.name().to_string_lossy().into_owned(),
            mount_point: disk.mount_point().to_string_lossy().into_owned(),
            file_system: disk.file_system().to_string_lossy().into_owned(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
            removable: disk.is_removable(),
        })
        .collect();

    let mut processes: Vec<_> = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string_lossy().into_owned(),
            cpu_percent: process.cpu_usage(),
            memory_bytes: process.memory(),
        })
        .collect();
    processes.sort_by(|left, right| {
        right.cpu_percent.total_cmp(&left.cpu_percent)
            .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
    });
    processes.truncate(20);

    Ok(SystemSnapshot {
        host_name: System::host_name().unwrap_or_else(|_| "未知设备".into()),
        os_name: System::name().unwrap_or_else(|_| "Windows".into()),
        os_version: System::long_os_version()
            .or_else(|_| System::os_version())
            .unwrap_or_else(|_| "未知版本".into()),
        kernel_version: System::kernel_version().unwrap_or_else(|_| "未知".into()),
        architecture: System::cpu_arch(),
        uptime_seconds: System::uptime().unwrap_or_default(),
        captured_at_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        cpu: CpuInfo {
            brand: cpu.map(|item| item.brand().to_string()).unwrap_or_else(|| "未知处理器".into()),
            logical_cores: system.cpus().len(),
            usage_percent: cpu_usage,
            frequency_mhz: cpu.map(|item| item.frequency()).unwrap_or_default(),
        },
        memory: MemoryInfo { total_bytes: total_memory, used_bytes: used_memory, usage_percent },
        disks,
        processes,
    })
}

#[tauri::command]
pub fn get_system_snapshot() -> Result<SystemSnapshot, String> {
    collect_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_has_sane_memory_values() {
        let snapshot = collect_snapshot().expect("system snapshot should be available in tests");
        assert!(snapshot.memory.used_bytes <= snapshot.memory.total_bytes);
        assert!((0.0..=100.0).contains(&snapshot.memory.usage_percent));
    }
}
