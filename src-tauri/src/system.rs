use serde::Serialize;
use std::{process::Command, time::{Instant, SystemTime, UNIX_EPOCH}};
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo { brand: String, logical_cores: usize, usage_percent: f32, frequency_mhz: u64 }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo { total_bytes: u64, used_bytes: u64, usage_percent: f64 }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    name: String, mount_point: String, file_system: String, total_bytes: u64,
    available_bytes: u64, removable: bool, read_bytes_per_sec: u64, written_bytes_per_sec: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo { pid: u32, name: String, cpu_percent: f32, memory_bytes: u64 }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    download_bytes_per_sec: u64, upload_bytes_per_sec: u64,
    total_received_bytes: u64, total_transmitted_bytes: u64, interfaces: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    host_name: String, os_name: String, os_version: String, kernel_version: String,
    architecture: String, uptime_seconds: u64, captured_at_unix_ms: u128,
    cpu: CpuInfo, memory: MemoryInfo, network: NetworkInfo, disks: Vec<DiskInfo>, processes: Vec<ProcessInfo>,
}

pub struct SystemCollector {
    system: System,
    networks: Networks,
    disks: Disks,
    last_sample: Instant,
    first_snapshot: bool,
}

impl SystemCollector {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            system: System::new_all().map_err(|e| format!("无法初始化系统信息：{e}"))?,
            networks: Networks::new_with_refreshed_list().map_err(|e| format!("无法读取网络信息：{e}"))?,
            disks: Disks::new_with_refreshed_list().map_err(|e| format!("无法读取磁盘信息：{e}"))?,
            last_sample: Instant::now(),
            first_snapshot: true,
        })
    }

    pub fn snapshot(&mut self) -> Result<SystemSnapshot, String> {
        // `new_all` and the `new_with_refreshed_list` constructors already did
        // a full collection. Repeating it for the first IPC request keeps the
        // splash screen visible for no benefit, so reuse that initial sample.
        if self.first_snapshot {
            self.first_snapshot = false;
        } else {
            self.system.refresh_cpu_usage();
            self.system.refresh_memory();
            self.networks.refresh(true);
            self.disks.refresh(true);
        }
        let elapsed = self.last_sample.elapsed().as_secs_f64().max(0.2);
        self.last_sample = Instant::now();
        let cpu_usage = self.system.global_cpu_usage();
        let cpu = self.system.cpus().first();
        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        let usage_percent = if total_memory == 0 { 0.0 } else { used_memory as f64 / total_memory as f64 * 100.0 };
        let (mut received, mut transmitted, mut total_received, mut total_transmitted) = (0, 0, 0, 0);
        for (_, data) in &self.networks {
            received += data.received(); transmitted += data.transmitted();
            total_received += data.total_received(); total_transmitted += data.total_transmitted();
        }
        let disks = self.disks.iter().map(|disk| {
            let io = disk.usage();
            DiskInfo {
                name: disk.name().to_string_lossy().into_owned(), mount_point: disk.mount_point().to_string_lossy().into_owned(),
                file_system: disk.file_system().to_string_lossy().into_owned(), total_bytes: disk.total_space(),
                available_bytes: disk.available_space(), removable: disk.is_removable(),
                read_bytes_per_sec: (io.read_bytes as f64 / elapsed) as u64,
                written_bytes_per_sec: (io.written_bytes as f64 / elapsed) as u64,
            }
        }).collect();
        Ok(SystemSnapshot {
            host_name: System::host_name().unwrap_or_else(|_| "未知设备".into()), os_name: System::name().unwrap_or_else(|_| "Windows".into()),
            os_version: System::long_os_version().or_else(|_| System::os_version()).unwrap_or_else(|_| "未知版本".into()),
            kernel_version: System::kernel_version().unwrap_or_else(|_| "未知".into()), architecture: System::cpu_arch(),
            uptime_seconds: System::uptime().unwrap_or_default(), captured_at_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            cpu: CpuInfo { brand: cpu.map(|x| x.brand().to_string()).unwrap_or_else(|| "未知处理器".into()), logical_cores: self.system.cpus().len(), usage_percent: cpu_usage, frequency_mhz: cpu.map(|x| x.frequency()).unwrap_or_default() },
            memory: MemoryInfo { total_bytes: total_memory, used_bytes: used_memory, usage_percent },
            network: NetworkInfo { download_bytes_per_sec: (received as f64 / elapsed) as u64, upload_bytes_per_sec: (transmitted as f64 / elapsed) as u64, total_received_bytes: total_received, total_transmitted_bytes: total_transmitted, interfaces: self.networks.len() },
            disks, processes: Vec::new(),
        })
    }

    pub fn processes(&mut self) -> Vec<ProcessInfo> {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let mut processes: Vec<_> = self.system.processes().iter().map(|(pid, process)| ProcessInfo {
            pid: pid.as_u32(), name: process.name().to_string_lossy().into_owned(),
            cpu_percent: process.cpu_usage(), memory_bytes: process.memory(),
        }).collect();
        processes.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent).then_with(|| b.memory_bytes.cmp(&a.memory_bytes)));
        processes.truncate(30);
        processes
    }
}

#[tauri::command(async)]
pub fn get_process_inventory(state: tauri::State<'_, std::sync::Mutex<SystemCollector>>) -> Result<Vec<ProcessInfo>, String> {
    Ok(state.lock().map_err(|_| "系统采集器被占用".to_string())?.processes())
}

fn record_metrics(app: &tauri::AppHandle, snapshot: &SystemSnapshot) {
    let Ok(directory) = app.path().app_data_dir() else {
        return;
    };
    let path = crate::system_history::default_db_path(&directory);
    let Ok(connection) = crate::system_history::open(&path) else {
        return;
    };
    let input = crate::system_history::SystemSampleInput {
        cpu_percent: snapshot.cpu.usage_percent as f64,
        memory_percent: snapshot.memory.usage_percent,
        download_bytes_per_sec: snapshot.network.download_bytes_per_sec,
        upload_bytes_per_sec: snapshot.network.upload_bytes_per_sec,
        disk_read_bytes_per_sec: snapshot.disks.iter().map(|disk| disk.read_bytes_per_sec).sum(),
        disk_write_bytes_per_sec: snapshot.disks.iter().map(|disk| disk.written_bytes_per_sec).sum(),
    };
    // 指标入库失败不应影响实时视图。
    let retention_days = crate::app_settings::load(app).metrics_retention_days;
    let _ = crate::system_history::record(
        &connection,
        snapshot.captured_at_unix_ms as i64,
        &input,
        retention_days,
    );
}

#[tauri::command(async)]
pub fn get_system_snapshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Mutex<SystemCollector>>,
) -> Result<SystemSnapshot, String> {
    let snapshot = state.lock().map_err(|_| "系统采集器被占用".to_string())?.snapshot()?;
    record_metrics(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn get_system_history(
    app: tauri::AppHandle,
    hours: u32,
) -> Result<crate::system_history::SystemHistorySeries, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
        let connection =
            crate::system_history::open(&crate::system_history::default_db_path(&directory))?;
        crate::system_history::query(&connection, hours)
    })
    .await
    .map_err(|error| format!("指标历史任务失败：{error}"))?
}

#[tauri::command(async)]
pub fn get_hardware_details() -> Result<serde_json::Value, String> {
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let script = r#"[Console]::OutputEncoding=[Text.Encoding]::UTF8;$o=[ordered]@{computer=Get-CimInstance Win32_ComputerSystem|Select-Object Manufacturer,Model,TotalPhysicalMemory;board=Get-CimInstance Win32_BaseBoard|Select-Object Manufacturer,Product,SerialNumber;bios=Get-CimInstance Win32_BIOS|Select-Object Manufacturer,SMBIOSBIOSVersion,SerialNumber,ReleaseDate;gpus=@(Get-CimInstance Win32_VideoController|Select-Object Name,AdapterRAM,DriverVersion);memory=@(Get-CimInstance Win32_PhysicalMemory|Select-Object Manufacturer,PartNumber,Capacity,Speed,DeviceLocator);drives=@(Get-CimInstance Win32_DiskDrive|Select-Object Model,Size,InterfaceType,SerialNumber,MediaType);network=@(Get-CimInstance Win32_NetworkAdapterConfiguration|Where-Object IPEnabled|Select-Object Description,MACAddress,IPAddress)};$o|ConvertTo-Json -Depth 5 -Compress"#;
        let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW).args(["-NoProfile", "-NonInteractive", "-Command", script]).output().map_err(|e| format!("无法启动硬件检测：{e}"))?;
        if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).into_owned()); }
        serde_json::from_slice(&output.stdout).map_err(|e| format!("无法解析硬件信息：{e}"))
    }
    #[cfg(not(windows))] { Ok(serde_json::json!({"message":"硬件详情当前仅支持 Windows"})) }
}

#[cfg(windows)]
mod startup {
    use super::*;
    use winreg::{enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE}, RegKey};
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const DISABLED: &str = r"Software\SystemLens\DisabledStartup";
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StartupItem { name: String, command: String, scope: String, enabled: bool, editable: bool }
    fn read_run(root: winreg::HKEY, scope: &str, editable: bool, enabled: bool, path: &str) -> Vec<StartupItem> {
        let root = RegKey::predef(root);
        let Ok(key) = root.open_subkey_with_flags(path, KEY_READ) else { return vec![] };
        key.enum_values().filter_map(Result::ok).filter_map(|(name, _)| key.get_value::<String, _>(&name).ok().map(|command| StartupItem { name, command, scope: scope.into(), enabled, editable })).collect()
    }
    pub fn list() -> Vec<StartupItem> {
        let mut items = read_run(HKEY_CURRENT_USER, "当前用户", true, true, RUN);
        items.extend(read_run(HKEY_LOCAL_MACHINE, "所有用户", false, true, RUN));
        items.extend(read_run(HKEY_CURRENT_USER, "当前用户", true, false, DISABLED));
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())); items
    }
    pub fn set_enabled(name: String, enabled: bool) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let (source_path, target_path) = if enabled { (DISABLED, RUN) } else { (RUN, DISABLED) };
        let source = root.open_subkey_with_flags(source_path, KEY_READ | KEY_WRITE).map_err(|e| format!("无法打开启动项：{e}"))?;
        let command: String = source.get_value(&name).map_err(|e| format!("无法读取启动项：{e}"))?;
        let (target, _) = root.create_subkey(target_path).map_err(|e| format!("无法保存启动项：{e}"))?;
        target.set_value(&name, &command).map_err(|e| format!("无法保存启动项：{e}"))?;
        if let Err(e) = source.delete_value(&name) { let _ = target.delete_value(&name); return Err(format!("无法更新启动项：{e}")); }
        Ok(())
    }
}

#[cfg(windows)]
#[tauri::command(async)]
pub fn get_startup_items() -> Vec<startup::StartupItem> { startup::list() }

/// Read-only startup summary for the AI tool gateway: names and state only,
/// no registry commands or executable paths.
#[cfg(windows)]
pub(crate) fn startup_items_json() -> Result<serde_json::Value, String> {
    serde_json::to_value(startup::list()).map_err(|error| format!("无法序列化启动项：{error}"))
}

#[cfg(not(windows))]
pub(crate) fn startup_items_json() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!([]))
}

#[cfg(windows)]
#[tauri::command(async)]
pub fn set_startup_enabled(name: String, enabled: bool) -> Result<(), String> { startup::set_enabled(name, enabled) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_has_sane_memory_values() {
        let mut collector = SystemCollector::new().expect("collector");
        let snapshot = collector.snapshot().expect("snapshot");
        assert!(snapshot.memory.used_bytes <= snapshot.memory.total_bytes);
        assert!((0.0..=100.0).contains(&snapshot.memory.usage_percent));
        assert!(snapshot.processes.is_empty(), "实时快照不应携带进程清单");
    }

    #[test]
    fn process_inventory_is_collected_separately() {
        let mut collector = SystemCollector::new().expect("collector");
        let processes = collector.processes();
        assert!(processes.iter().all(|process| process.pid > 0));
        assert!(processes.len() <= 30);
    }
}
