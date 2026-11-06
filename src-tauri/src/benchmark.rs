use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    hint::black_box,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    time::Instant,
};

const SCHEMA_VERSION: u32 = 1;
const CANCELLED: &str = "BENCHMARK_CANCELLED";
const DISK_DEFAULT_MIB: u64 = 1024;
const DISK_MIN_MIB: u64 = 128;
const DISK_MAX_MIB: u64 = 4096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    schema_version: u32,
    kind: String,
    label: String,
    score: f64,
    unit: String,
    duration_ms: u128,
    details: BTreeMap<String, f64>,
    note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkEnvironment {
    app_version: String,
    cpu_brand: String,
    logical_cores: usize,
    memory_bytes: u64,
    os_name: String,
    os_version: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRecord {
    recorded_at_unix_ms: u128,
    result: BenchmarkResult,
    /// 旧版本记录没有环境字段，反序列化时按 None 处理。
    #[serde(default)]
    environment: Option<BenchmarkEnvironment>,
}

pub struct BenchmarkHistoryLock(pub std::sync::Mutex<()>);

#[derive(Default)]
pub struct BenchmarkCancel(pub Arc<AtomicBool>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkPreflight {
    max_temperature_c: Option<f64>,
    on_battery: Option<bool>,
    warnings: Vec<String>,
    source: String,
}

fn cancelled() -> String {
    CANCELLED.to_string()
}

fn checkpoint(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::Relaxed) {
        Err(cancelled())
    } else {
        Ok(())
    }
}

fn history_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let directory = app.path().app_data_dir().map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    std::fs::create_dir_all(&directory).map_err(|error| format!("无法创建应用数据目录：{error}"))?;
    Ok(directory.join("benchmark-history.json"))
}

fn load_history_file(app: &tauri::AppHandle) -> Result<Vec<BenchmarkRecord>, String> {
    let path = history_path(app)?;
    if !path.exists() { return Ok(Vec::new()); }
    let bytes = std::fs::read(path).map_err(|error| format!("无法读取跑分历史：{error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("无法解析跑分历史：{error}"))
}

fn save_result(app: &tauri::AppHandle, result: &BenchmarkResult) -> Result<(), String> {
    let path = history_path(app)?;
    let mut history = load_history_file(app)?;
    history.push(BenchmarkRecord {
        recorded_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),
        result: result.clone(),
        environment: Some(capture_environment()),
    });
    if history.len() > 200 { history.drain(0..history.len() - 200); }
    let bytes = serde_json::to_vec_pretty(&history).map_err(|error| format!("无法序列化跑分历史：{error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("无法保存跑分历史：{error}"))
}

/// 记录跑分时的硬件与系统环境，便于同版本、同设备之间比较。
fn capture_environment() -> BenchmarkEnvironment {
    let system = match sysinfo::System::new_all() {
        Ok(system) => system,
        Err(_) => {
            return BenchmarkEnvironment {
                app_version: env!("CARGO_PKG_VERSION").into(),
                cpu_brand: "未知处理器".into(),
                logical_cores: 0,
                memory_bytes: 0,
                os_name: "Windows".into(),
                os_version: String::new(),
            }
        }
    };
    let cpu_brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|brand| !brand.is_empty())
        .unwrap_or_else(|| "未知处理器".into());
    BenchmarkEnvironment {
        app_version: env!("CARGO_PKG_VERSION").into(),
        cpu_brand,
        logical_cores: system.cpus().len(),
        memory_bytes: system.total_memory(),
        os_name: sysinfo::System::name().unwrap_or_else(|_| "Windows".into()),
        os_version: sysinfo::System::os_version().unwrap_or_default(),
    }
}

fn integer_workload(iterations: u64, mut value: u64, cancel: &AtomicBool) -> Result<u64, String> {
    const CHECK_EVERY: u64 = 100_000;
    for index in 0..iterations {
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        value = value.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(index);
        if index % CHECK_EVERY == 0 {
            checkpoint(cancel)?;
        }
    }
    Ok(black_box(value))
}

fn cpu_single(cancel: &AtomicBool) -> Result<BenchmarkResult, String> {
    const ITERATIONS: u64 = 20_000_000;
    checkpoint(cancel)?;
    let started = Instant::now();
    let checksum = integer_workload(ITERATIONS, 0x1234_5678_9ABC_DEF0, cancel)?;
    let elapsed = started.elapsed();
    let throughput = ITERATIONS as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "cpu-single".into(), label: "CPU 单核".into(), score: throughput * 100.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("millionIterationsPerSecond".into(), throughput), ("checksum".into(), checksum as f64)]), note: "固定整数混合工作负载；分数仅用于 System Lens 同版本比较。".into() })
}

fn cpu_multi(cancel: &Arc<AtomicBool>) -> Result<BenchmarkResult, String> {
    const ITERATIONS_PER_THREAD: u64 = 5_000_000;
    let threads = std::thread::available_parallelism().map(|value| value.get()).unwrap_or(1).min(32);
    checkpoint(cancel)?;
    let started = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|index| {
            let flag = Arc::clone(cancel);
            std::thread::spawn(move || integer_workload(ITERATIONS_PER_THREAD, index as u64 + 1, &flag))
        })
        .collect();
    let mut checksum = 0u64;
    for handle in handles {
        checksum ^= handle
            .join()
            .map_err(|_| "CPU 测试线程异常退出".to_string())??;
    }
    checkpoint(cancel)?;
    let elapsed = started.elapsed();
    let iterations = ITERATIONS_PER_THREAD * threads as u64;
    let throughput = iterations as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "cpu-multi".into(), label: "CPU 多核".into(), score: throughput * 100.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("threads".into(), threads as f64), ("millionIterationsPerSecond".into(), throughput), ("checksum".into(), checksum as f64)]), note: "最多使用 32 个逻辑线程；分数仅用于 System Lens 同版本比较。".into() })
}

fn memory_bandwidth(cancel: &AtomicBool) -> Result<BenchmarkResult, String> {
    const SIZE: usize = 64 * 1024 * 1024;
    const ROUNDS: usize = 8;
    let source = vec![0xA5u8; SIZE];
    let mut destination = vec![0u8; SIZE];
    let chunk = 4 * 1024 * 1024;
    checkpoint(cancel)?;
    let started = Instant::now();
    for _ in 0..ROUNDS {
        let mut offset = 0;
        while offset < SIZE {
            checkpoint(cancel)?;
            let end = (offset + chunk).min(SIZE);
            destination[offset..end].copy_from_slice(&source[offset..end]);
            offset = end;
        }
        black_box(destination[SIZE / 2]);
    }
    let elapsed = started.elapsed();
    let bytes = (SIZE * ROUNDS) as f64;
    let gib_per_second = bytes / elapsed.as_secs_f64() / 1024f64.powi(3);
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "memory".into(), label: "内存带宽".into(), score: gib_per_second * 1000.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("gibPerSecond".into(), gib_per_second), ("workingSetMiB".into(), (SIZE * 2 / 1024 / 1024) as f64)]), note: "64 MiB 缓冲区复制 8 轮；包含读写总流量。".into() })
}

struct TempBenchmarkFile(PathBuf);
impl Drop for TempBenchmarkFile { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }

fn disk_sequential(size_mib: u64, cancel: &AtomicBool) -> Result<BenchmarkResult, String> {
    const BLOCK: usize = 1024 * 1024;
    let size_mib = size_mib.clamp(DISK_MIN_MIB, DISK_MAX_MIB);
    let path = std::env::temp_dir().join(format!("system-lens-benchmark-{}-{}.tmp", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    let guard = TempBenchmarkFile(path.clone());
    let block = vec![0x5Au8; BLOCK];
    checkpoint(cancel)?;
    let write_started = Instant::now();
    {
        let file = OpenOptions::new().create_new(true).write(true).open(&path).map_err(|error| format!("无法创建磁盘测试文件：{error}"))?;
        let mut writer = BufWriter::with_capacity(BLOCK, file);
        for _ in 0..size_mib {
            checkpoint(cancel)?;
            writer.write_all(&block).map_err(|error| format!("磁盘写入测试失败：{error}"))?;
        }
        writer.flush().map_err(|error| format!("磁盘写入刷新失败：{error}"))?;
        writer.get_ref().sync_data().map_err(|error| format!("磁盘同步失败：{error}"))?;
    }
    let write_elapsed = write_started.elapsed();
    let read_started = Instant::now();
    let mut checksum = 0u64;
    {
        let file = File::open(&path).map_err(|error| format!("无法读取磁盘测试文件：{error}"))?;
        let mut reader = BufReader::with_capacity(BLOCK, file);
        let mut buffer = vec![0u8; BLOCK];
        loop {
            checkpoint(cancel)?;
            let count = reader.read(&mut buffer).map_err(|error| format!("磁盘读取测试失败：{error}"))?;
            if count == 0 { break; }
            checksum = checksum.wrapping_add(buffer[..count].iter().map(|value| *value as u64).sum::<u64>());
        }
    }
    let read_elapsed = read_started.elapsed();
    drop(guard);
    let mib = size_mib as f64;
    let write_mib = mib / write_elapsed.as_secs_f64();
    let read_mib = mib / read_elapsed.as_secs_f64();
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "disk".into(), label: "磁盘顺序读写".into(), score: (write_mib + read_mib) / 2.0, unit: "分".into(), duration_ms: (write_elapsed + read_elapsed).as_millis(), details: BTreeMap::from([("writeMiBPerSecond".into(), write_mib), ("readMiBPerSecond".into(), read_mib), ("testFileMiB".into(), mib), ("checksum".into(), checksum as f64)]), note: format!("在系统临时目录使用 {size_mib} MiB 文件；完成后自动删除。") })
}

#[cfg(windows)]
fn battery_on_discharge() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = "$battery=Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1; if($battery){[int]$battery.BatteryStatus}else{-1}";
    let output = std::process::Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    let status = String::from_utf8_lossy(&output.stdout).trim().parse::<i32>().ok()?;
    match status {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

#[cfg(not(windows))]
fn battery_on_discharge() -> Option<bool> {
    None
}

fn preflight() -> BenchmarkPreflight {
    let mut max_temperature_c = None;
    let mut warnings = Vec::new();
    if let Ok(parsed) = crate::sensors::collect::collect() {
        max_temperature_c = parsed
            .values
            .iter()
            .filter(|value| value.category == "temperature" && value.availability.is_ok())
            .filter_map(|value| value.value)
            .filter(|value| value.is_finite() && *value > 0.0)
            .max_by(|left, right| left.total_cmp(right));
    }
    if let Some(temperature) = max_temperature_c {
        if temperature >= 90.0 {
            warnings.push(format!("当前温度 {temperature:.0}°C 已处于高位，建议先降温再跑分。"));
        } else if temperature >= 80.0 {
            warnings.push(format!("当前温度 {temperature:.0}°C 偏高，跑分可能触发降频。"));
        }
    }
    let on_battery = battery_on_discharge();
    if on_battery == Some(true) {
        warnings.push("当前使用电池供电，CPU/GPU 可能降频，分数不具可比性。".into());
    }
    BenchmarkPreflight {
        max_temperature_c,
        on_battery,
        warnings,
        source: "传感器快照 + Win32_Battery".into(),
    }
}

#[tauri::command]
pub async fn benchmark_preflight() -> Result<BenchmarkPreflight, String> {
    tauri::async_runtime::spawn_blocking(preflight)
        .await
        .map_err(|error| format!("跑分预检任务失败：{error}"))
}

#[tauri::command]
pub async fn run_benchmark(
    app: tauri::AppHandle,
    cancel: tauri::State<'_, BenchmarkCancel>,
    state: tauri::State<'_, BenchmarkHistoryLock>,
    kind: String,
    disk_size_mib: Option<u64>,
) -> Result<BenchmarkResult, String> {
    let flag = Arc::clone(&cancel.0);
    flag.store(false, Ordering::Relaxed);
    let size = disk_size_mib.unwrap_or(DISK_DEFAULT_MIB);
    let worker_flag = Arc::clone(&flag);
    let result = tauri::async_runtime::spawn_blocking(move || match kind.as_str() {
        "cpu-single" => cpu_single(&worker_flag),
        "cpu-multi" => cpu_multi(&worker_flag),
        "memory" => memory_bandwidth(&worker_flag),
        "disk" => disk_sequential(size, &worker_flag),
        _ => Err("未知的性能测试类型".into()),
    }).await.map_err(|error| format!("性能测试任务失败：{error}"))??;
    let _guard = state.0.lock().map_err(|_| "跑分历史正在被占用".to_string())?;
    save_result(&app, &result)?;
    Ok(result)
}

#[tauri::command]
pub fn cancel_benchmark(cancel: tauri::State<'_, BenchmarkCancel>) {
    cancel.0.store(true, Ordering::Relaxed);
}

#[tauri::command(async)]
pub fn get_benchmark_history(app: tauri::AppHandle, state: tauri::State<'_, BenchmarkHistoryLock>) -> Result<Vec<BenchmarkRecord>, String> {
    let _guard = state.0.lock().map_err(|_| "跑分历史正在被占用".to_string())?;
    let mut history = load_history_file(&app)?;
    history.reverse();
    Ok(history)
}

#[tauri::command(async)]
pub fn save_external_benchmark(app: tauri::AppHandle, state: tauri::State<'_, BenchmarkHistoryLock>, result: BenchmarkResult) -> Result<(), String> {
    if result.kind != "gpu" || !matches!(result.schema_version, 2 | 3) || !result.score.is_finite() || result.score <= 0.0 {
        return Err("GPU 跑分结果格式无效".into());
    }
    let _guard = state.0.lock().map_err(|_| "跑分历史正在被占用".to_string())?;
    save_result(&app, &result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cpu_result_is_finite() {
        let flag = AtomicBool::new(false);
        let result = cpu_single(&flag).unwrap();
        assert!(result.score.is_finite() && result.score > 0.0);
        assert_eq!(result.schema_version, 1);
    }

    #[test]
    fn cancel_flag_stops_workloads() {
        let flag = AtomicBool::new(true);
        assert_eq!(cpu_single(&flag).unwrap_err(), CANCELLED);
        assert_eq!(memory_bandwidth(&flag).unwrap_err(), CANCELLED);
        assert_eq!(disk_sequential(128, &flag).unwrap_err(), CANCELLED);
    }

    #[test]
    fn disk_size_is_clamped() {
        // 只验证常量边界，不实际写 4 GiB。
        assert_eq!(0u64.clamp(DISK_MIN_MIB, DISK_MAX_MIB), DISK_MIN_MIB);
        assert_eq!(99_999u64.clamp(DISK_MIN_MIB, DISK_MAX_MIB), DISK_MAX_MIB);
        assert_eq!(DISK_DEFAULT_MIB, 1024);
    }

    #[test]
    fn temporary_benchmark_file_is_removed_on_drop() {
        let path = std::env::temp_dir().join(format!(
            "system-lens-benchmark-drop-test-{}.tmp",
            std::process::id()
        ));
        std::fs::write(&path, b"cleanup-check").unwrap();
        {
            let _guard = TempBenchmarkFile(path.clone());
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    #[ignore = "会在临时目录真实写入 128 MiB"]
    fn live_disk_benchmark_small() {
        let flag = AtomicBool::new(false);
        let result = disk_sequential(128, &flag).unwrap();
        assert!(result.score > 0.0);
        assert_eq!(result.details.get("testFileMiB"), Some(&128.0));
        assert!(result.note.contains("128 MiB"));
    }

    #[test]
    #[ignore = "需要在本机执行传感器与电池预检"]
    fn live_preflight() {
        let preflight = preflight();
        println!(
            "temperature={:?} on_battery={:?} warnings={:?}",
            preflight.max_temperature_c, preflight.on_battery, preflight.warnings
        );
        assert!(preflight
            .max_temperature_c
            .map(|value| value.is_finite())
            .unwrap_or(true));
    }

    #[test]
    fn environment_has_cpu_and_version() {
        let environment = capture_environment();
        assert_eq!(environment.app_version, env!("CARGO_PKG_VERSION"));
        assert!(environment.logical_cores > 0);
        assert!(!environment.cpu_brand.is_empty());
        assert!(!environment.os_name.is_empty());
    }

    #[test]
    fn legacy_record_without_environment_parses() {
        let json = r#"{"recordedAtUnixMs":1,"result":{"schemaVersion":1,"kind":"cpu-single","label":"x","score":1.0,"unit":"分","durationMs":1,"details":{},"note":""}}"#;
        let record: BenchmarkRecord = serde_json::from_str(json).unwrap();
        assert!(record.environment.is_none());
    }

    #[test]
    fn record_serializes_environment() {
        let record = BenchmarkRecord {
            recorded_at_unix_ms: 1,
            result: BenchmarkResult {
                schema_version: SCHEMA_VERSION,
                kind: "cpu-single".into(),
                label: "CPU 单核".into(),
                score: 1.0,
                unit: "分".into(),
                duration_ms: 1,
                details: BTreeMap::new(),
                note: String::new(),
            },
            environment: Some(capture_environment()),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"environment\""));
        assert!(json.contains("\"cpuBrand\""));
        assert!(json.contains("\"appVersion\""));
    }
}
