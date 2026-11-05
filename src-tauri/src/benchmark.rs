use serde::Serialize;
use std::{collections::BTreeMap, fs::{File, OpenOptions}, hint::black_box, io::{BufReader, BufWriter, Read, Write}, path::PathBuf, time::Instant};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
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

fn integer_workload(iterations: u64, mut value: u64) -> u64 {
    for index in 0..iterations {
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        value = value.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(index);
    }
    black_box(value)
}

fn cpu_single() -> BenchmarkResult {
    const ITERATIONS: u64 = 20_000_000;
    let started = Instant::now();
    let checksum = integer_workload(ITERATIONS, 0x1234_5678_9ABC_DEF0);
    let elapsed = started.elapsed();
    let throughput = ITERATIONS as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "cpu-single".into(), label: "CPU 单核".into(), score: throughput * 100.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("millionIterationsPerSecond".into(), throughput), ("checksum".into(), checksum as f64)]), note: "固定整数混合工作负载；分数仅用于 System Lens 同版本比较。".into() }
}

fn cpu_multi() -> Result<BenchmarkResult, String> {
    const ITERATIONS_PER_THREAD: u64 = 5_000_000;
    let threads = std::thread::available_parallelism().map(|value| value.get()).unwrap_or(1).min(32);
    let started = Instant::now();
    let handles: Vec<_> = (0..threads).map(|index| std::thread::spawn(move || integer_workload(ITERATIONS_PER_THREAD, index as u64 + 1))).collect();
    let mut checksum = 0u64;
    for handle in handles { checksum ^= handle.join().map_err(|_| "CPU 测试线程异常退出".to_string())?; }
    let elapsed = started.elapsed();
    let iterations = ITERATIONS_PER_THREAD * threads as u64;
    let throughput = iterations as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "cpu-multi".into(), label: "CPU 多核".into(), score: throughput * 100.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("threads".into(), threads as f64), ("millionIterationsPerSecond".into(), throughput), ("checksum".into(), checksum as f64)]), note: "最多使用 32 个逻辑线程；分数仅用于 System Lens 同版本比较。".into() })
}

fn memory_bandwidth() -> BenchmarkResult {
    const SIZE: usize = 64 * 1024 * 1024;
    const ROUNDS: usize = 8;
    let source = vec![0xA5u8; SIZE];
    let mut destination = vec![0u8; SIZE];
    let started = Instant::now();
    for _ in 0..ROUNDS { destination.copy_from_slice(&source); black_box(destination[SIZE / 2]); }
    let elapsed = started.elapsed();
    let bytes = (SIZE * ROUNDS) as f64;
    let gib_per_second = bytes / elapsed.as_secs_f64() / 1024f64.powi(3);
    BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "memory".into(), label: "内存带宽".into(), score: gib_per_second * 1000.0, unit: "分".into(), duration_ms: elapsed.as_millis(), details: BTreeMap::from([("gibPerSecond".into(), gib_per_second), ("workingSetMiB".into(), (SIZE * 2 / 1024 / 1024) as f64)]), note: "64 MiB 缓冲区复制 8 轮；包含读写总流量。".into() }
}

struct TempBenchmarkFile(PathBuf);
impl Drop for TempBenchmarkFile { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }

fn disk_sequential() -> Result<BenchmarkResult, String> {
    const SIZE: usize = 128 * 1024 * 1024;
    const BLOCK: usize = 1024 * 1024;
    let path = std::env::temp_dir().join(format!("system-lens-benchmark-{}-{}.tmp", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    let guard = TempBenchmarkFile(path.clone());
    let block = vec![0x5Au8; BLOCK];
    let write_started = Instant::now();
    {
        let file = OpenOptions::new().create_new(true).write(true).open(&path).map_err(|error| format!("无法创建磁盘测试文件：{error}"))?;
        let mut writer = BufWriter::with_capacity(BLOCK, file);
        for _ in 0..(SIZE / BLOCK) { writer.write_all(&block).map_err(|error| format!("磁盘写入测试失败：{error}"))?; }
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
            let count = reader.read(&mut buffer).map_err(|error| format!("磁盘读取测试失败：{error}"))?;
            if count == 0 { break; }
            checksum = checksum.wrapping_add(buffer[..count].iter().map(|value| *value as u64).sum::<u64>());
        }
    }
    let read_elapsed = read_started.elapsed();
    drop(guard);
    let mib = SIZE as f64 / 1024.0 / 1024.0;
    let write_mib = mib / write_elapsed.as_secs_f64();
    let read_mib = mib / read_elapsed.as_secs_f64();
    Ok(BenchmarkResult { schema_version: SCHEMA_VERSION, kind: "disk".into(), label: "磁盘顺序读写".into(), score: (write_mib + read_mib) / 2.0, unit: "分".into(), duration_ms: (write_elapsed + read_elapsed).as_millis(), details: BTreeMap::from([("writeMiBPerSecond".into(), write_mib), ("readMiBPerSecond".into(), read_mib), ("testFileMiB".into(), 128.0), ("checksum".into(), checksum as f64)]), note: "在系统临时目录使用 128 MiB 文件；完成后自动删除。".into() })
}

#[tauri::command]
pub async fn run_benchmark(kind: String) -> Result<BenchmarkResult, String> {
    tauri::async_runtime::spawn_blocking(move || match kind.as_str() {
        "cpu-single" => Ok(cpu_single()), "cpu-multi" => cpu_multi(), "memory" => Ok(memory_bandwidth()), "disk" => disk_sequential(),
        _ => Err("未知的性能测试类型".into()),
    }).await.map_err(|error| format!("性能测试任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_cpu_result_is_finite() {
        let result = cpu_single();
        assert!(result.score.is_finite() && result.score > 0.0);
        assert_eq!(result.schema_version, 1);
    }
}
