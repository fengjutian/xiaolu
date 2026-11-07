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

/// 参与基线统计的最大样本数：只看最近这么多次。
const BASELINE_WINDOW: usize = 10;
/// 判断波动所需的最少样本数。
const BASELINE_MIN_SAMPLES: usize = 3;
/// 可信度「高」的相对标准差上限（%）。
const CONFIDENCE_HIGH_SPREAD: f64 = 5.0;
/// 可信度「中」的相对标准差上限（%）。
const CONFIDENCE_MEDIUM_SPREAD: f64 = 12.0;
/// 波动带下限（%）：即便样本看起来完全一致，也留一点测量噪声余量。
const MIN_BAND_PERCENT: f64 = 2.0;

/// 单项测试的本机基线统计。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkBaseline {
    kind: String,
    label: String,
    schema_version: u32,
    app_version: String,
    /// 参与统计的样本数。
    sample_count: usize,
    /// 样本分数中位数，作为基线（对偶发离群不敏感）。
    median_score: f64,
    /// 样本分数平均值。
    mean_score: f64,
    best_score: f64,
    worst_score: f64,
    /// 相对标准差（%）：同机重复测试的波动幅度。
    spread_percent: f64,
    /// 可信度：高 / 中 / 低 / 样本不足。
    confidence: String,
    /// 可信度判定理由，直接给用户看。
    confidence_note: String,
}

/// 一次新结果与基线的对比。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkComparison {
    kind: String,
    score: f64,
    baseline_score: f64,
    /// 相对基线的偏差（%）；基线为 0 时记 0。
    delta_percent: f64,
    /// 是否落在「基线 ± 波动带」内。
    within_baseline_band: bool,
    band_low: f64,
    band_high: f64,
    /// 面向用户的一句话结论。
    note: String,
}

/// 基线报告：基线 + 本次对比 + 说明。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkBaselineReport {
    baselines: Vec<BenchmarkBaseline>,
    comparisons: Vec<BenchmarkComparison>,
    notes: Vec<String>,
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

fn median_of(values: &[f64]) -> f64 {
    if values.is_empty() { return 0.0; }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

/// 按项目统计本机基线。
///
/// 只统计「同 kind + 同工作负载版本 + 同应用版本」的记录：换了工作负载或应用版本，
/// 分数本来就不能直接比较，混进来会把基线算歪。旧记录没有环境信息，一律不参与。
/// 输入按时间从旧到新排列（与历史文件一致）。
pub fn compute_baselines(records: &[BenchmarkRecord], app_version: &str) -> Vec<BenchmarkBaseline> {
    struct Group {
        label: String,
        scores: Vec<f64>,
    }
    let mut groups: BTreeMap<(String, u32), Group> = BTreeMap::new();
    for record in records {
        let Some(environment) = record.environment.as_ref() else { continue };
        if environment.app_version != app_version { continue; }
        let result = &record.result;
        if !result.score.is_finite() || result.score <= 0.0 { continue; }
        let key = (result.kind.clone(), result.schema_version);
        let group = groups.entry(key).or_insert_with(|| Group {
            label: result.label.clone(),
            scores: Vec::new(),
        });
        group.scores.push(result.score);
    }

    let mut baselines = Vec::new();
    for ((kind, schema_version), group) in groups {
        let Group { label, scores } = group;
        if scores.is_empty() { continue; }
        // 只看最近 BASELINE_WINDOW 次。
        let window = if scores.len() > BASELINE_WINDOW {
            &scores[scores.len() - BASELINE_WINDOW..]
        } else {
            &scores[..]
        };
        let count = window.len();
        let mean = window.iter().sum::<f64>() / count as f64;
        let variance = window.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count as f64;
        let spread = if mean > 0.0 { variance.sqrt() / mean * 100.0 } else { 0.0 };
        let (confidence, confidence_note) = if count < BASELINE_MIN_SAMPLES {
            (
                "样本不足".to_string(),
                format!("同版本样本只有 {count} 次，至少需要 {BASELINE_MIN_SAMPLES} 次才能判断波动。"),
            )
        } else if spread <= CONFIDENCE_HIGH_SPREAD {
            ("高".to_string(), format!("同机波动 ±{:.1}%，结果稳定。", spread))
        } else if spread <= CONFIDENCE_MEDIUM_SPREAD {
            ("中".to_string(), format!("同机波动 ±{:.1}%，建议再跑一次取中位数。", spread))
        } else {
            ("低".to_string(), format!("同机波动 ±{:.1}%，后台负载或电源模式可能影响结果。", spread))
        };
        baselines.push(BenchmarkBaseline {
            kind,
            label,
            schema_version,
            app_version: app_version.to_string(),
            sample_count: count,
            median_score: median_of(window),
            mean_score: mean,
            best_score: window.iter().copied().fold(f64::MIN, f64::max),
            worst_score: window.iter().copied().fold(f64::MAX, f64::min),
            spread_percent: spread,
            confidence,
            confidence_note,
        });
    }
    baselines
}

/// 把一次新结果与基线对比，给出偏差与是否落在波动带内。
pub fn compare_with_baseline(baseline: &BenchmarkBaseline, score: f64) -> BenchmarkComparison {
    let base = baseline.median_score;
    let delta = if base > 0.0 { (score - base) / base * 100.0 } else { 0.0 };
    let band_percent = baseline.spread_percent.max(MIN_BAND_PERCENT);
    let band = base * band_percent / 100.0;
    let band_low = (base - band).max(0.0);
    let band_high = base + band;
    let within = score >= band_low && score <= band_high;
    let note = if baseline.sample_count < BASELINE_MIN_SAMPLES {
        format!("样本不足（{} 次），暂时无法判断本次是否正常。", baseline.sample_count)
    } else if within {
        format!("落在基线 ±{band_percent:.1}% 波动带内，与本机常态一致。")
    } else if delta > 0.0 {
        format!("高于基线 {delta:.1}%，超出波动带；可能是真实提升，也可能是当时负载更低。")
    } else {
        format!("低于基线 {:.1}%，超出波动带；建议确认后台负载后复检。", -delta)
    };
    BenchmarkComparison {
        kind: baseline.kind.clone(),
        score,
        baseline_score: base,
        delta_percent: delta,
        within_baseline_band: within,
        band_low,
        band_high,
        note,
    }
}

/// 组装基线报告：基线 + 本次结果对比 + 面向用户的说明。
pub fn build_baseline_report(
    records: &[BenchmarkRecord],
    app_version: &str,
    current: &[BenchmarkResult],
) -> BenchmarkBaselineReport {
    let baselines = compute_baselines(records, app_version);
    let mut comparisons = Vec::new();
    for result in current {
        if !result.score.is_finite() || result.score <= 0.0 { continue; }
        let Some(baseline) = baselines.iter().find(|baseline| {
            baseline.kind == result.kind && baseline.schema_version == result.schema_version
        }) else { continue; };
        comparisons.push(compare_with_baseline(baseline, result.score));
    }
    let notes = vec![
        "基线只统计本机同项目、同工作负载版本、同应用版本的最近若干次记录；换机器或换版本不通用。".to_string(),
        format!(
            "可信度按同机波动判定：≤{CONFIDENCE_HIGH_SPREAD:.0}% 为高，≤{CONFIDENCE_MEDIUM_SPREAD:.0}% 为中，更大为低；样本少于 {BASELINE_MIN_SAMPLES} 次时无法判断。"
        ),
        "基线会随新记录滚动更新（取最近若干次的中位数）。".to_string(),
        "分数受电源模式、后台负载与温度影响；复检前请关闭其他高负载程序。".to_string(),
    ];
    BenchmarkBaselineReport { baselines, comparisons, notes }
}

/// 取本机跑分基线：按项目统计基线与可信度，并与本次结果对比。
#[tauri::command(async)]
pub fn get_benchmark_baselines(
    app: tauri::AppHandle,
    state: tauri::State<'_, BenchmarkHistoryLock>,
    current: Option<Vec<BenchmarkResult>>,
) -> Result<BenchmarkBaselineReport, String> {
    let _guard = state.0.lock().map_err(|_| "跑分历史正在被占用".to_string())?;
    let history = load_history_file(&app)?;
    Ok(build_baseline_report(
        &history,
        env!("CARGO_PKG_VERSION"),
        current.as_deref().unwrap_or(&[]),
    ))
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

    fn make_record(
        kind: &str,
        score: f64,
        schema: u32,
        app_version: &str,
    ) -> BenchmarkRecord {
        BenchmarkRecord {
            recorded_at_unix_ms: 1,
            result: BenchmarkResult {
                schema_version: schema,
                kind: kind.into(),
                label: "测试项".into(),
                score,
                unit: "分".into(),
                duration_ms: 1,
                details: BTreeMap::new(),
                note: String::new(),
            },
            environment: Some(BenchmarkEnvironment {
                app_version: app_version.into(),
                cpu_brand: "CPU".into(),
                logical_cores: 8,
                memory_bytes: 16 * 1024 * 1024 * 1024,
                os_name: "Windows".into(),
                os_version: "11".into(),
            }),
        }
    }

    fn make_result(kind: &str, score: f64, schema: u32) -> BenchmarkResult {
        BenchmarkResult {
            schema_version: schema,
            kind: kind.into(),
            label: "测试项".into(),
            score,
            unit: "分".into(),
            duration_ms: 1,
            details: BTreeMap::new(),
            note: String::new(),
        }
    }

    #[test]
    fn baseline_uses_median_and_reports_spread() {
        let records = vec![
            make_record("cpu-single", 100.0, 1, "1.0.0"),
            make_record("cpu-single", 102.0, 1, "1.0.0"),
            make_record("cpu-single", 101.0, 1, "1.0.0"),
        ];
        let baselines = compute_baselines(&records, "1.0.0");
        assert_eq!(baselines.len(), 1);
        let baseline = &baselines[0];
        assert_eq!(baseline.sample_count, 3);
        assert_eq!(baseline.median_score, 101.0);
        assert_eq!(baseline.best_score, 102.0);
        assert_eq!(baseline.worst_score, 100.0);
        assert!(baseline.spread_percent > 0.0 && baseline.spread_percent < 2.0);
        assert_eq!(baseline.confidence, "高");
    }

    #[test]
    fn baseline_skips_other_app_versions_and_legacy_records() {
        let mut legacy = make_record("cpu-single", 500.0, 1, "1.0.0");
        legacy.environment = None;
        let records = vec![
            legacy,
            make_record("cpu-single", 500.0, 1, "0.9.0"),
            make_record("cpu-single", 100.0, 1, "1.0.0"),
        ];
        let baselines = compute_baselines(&records, "1.0.0");
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].sample_count, 1);
        assert_eq!(baselines[0].median_score, 100.0);
    }

    #[test]
    fn baseline_reports_insufficient_samples() {
        let records = vec![
            make_record("memory", 100.0, 1, "1.0.0"),
            make_record("memory", 200.0, 1, "1.0.0"),
        ];
        let baselines = compute_baselines(&records, "1.0.0");
        assert_eq!(baselines[0].sample_count, 2);
        assert_eq!(baselines[0].confidence, "样本不足");
        assert!(baselines[0].confidence_note.contains("3"));
    }

    #[test]
    fn baseline_keeps_only_the_latest_window() {
        let mut records = Vec::new();
        for index in 0..20 {
            records.push(make_record("disk", index as f64, 1, "1.0.0"));
        }
        let baselines = compute_baselines(&records, "1.0.0");
        assert_eq!(baselines[0].sample_count, BASELINE_WINDOW);
        // 最近 10 次是 10..=19，中位数为 14.5。
        assert_eq!(baselines[0].median_score, 14.5);
    }

    #[test]
    fn comparison_inside_band_is_reported_as_stable() {
        let records = vec![
            make_record("cpu-single", 100.0, 1, "1.0.0"),
            make_record("cpu-single", 101.0, 1, "1.0.0"),
            make_record("cpu-single", 99.0, 1, "1.0.0"),
        ];
        let baselines = compute_baselines(&records, "1.0.0");
        let comparison = compare_with_baseline(&baselines[0], 100.5);
        assert!(comparison.within_baseline_band);
        assert!(comparison.delta_percent.abs() < 1.0);
        assert!(comparison.note.contains("波动带内"));
    }

    #[test]
    fn comparison_flags_a_result_outside_the_band() {
        let records = vec![
            make_record("cpu-single", 100.0, 1, "1.0.0"),
            make_record("cpu-single", 101.0, 1, "1.0.0"),
            make_record("cpu-single", 99.0, 1, "1.0.0"),
        ];
        let baselines = compute_baselines(&records, "1.0.0");
        let comparison = compare_with_baseline(&baselines[0], 60.0);
        assert!(!comparison.within_baseline_band);
        assert!(comparison.delta_percent < -30.0);
        assert!(comparison.band_low > 50.0);
        assert!(comparison.note.contains("复检"));
    }

    #[test]
    fn report_pairs_current_results_only_with_matching_baseline() {
        let records = vec![
            make_record("cpu-single", 100.0, 1, "1.0.0"),
            make_record("cpu-single", 101.0, 1, "1.0.0"),
            make_record("cpu-single", 99.0, 1, "1.0.0"),
        ];
        let current = vec![
            make_result("cpu-single", 100.0, 1),
            make_result("gpu", 500.0, 3),
        ];
        let report = build_baseline_report(&records, "1.0.0", &current);
        assert_eq!(report.baselines.len(), 1);
        assert_eq!(report.comparisons.len(), 1);
        assert_eq!(report.comparisons[0].kind, "cpu-single");
        assert!(!report.notes.is_empty());
    }
}
