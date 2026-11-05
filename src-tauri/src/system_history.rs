use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SystemSampleInput {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub download_bytes_per_sec: u64,
    pub upload_bytes_per_sec: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHistoryPoint {
    pub t: i64,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub download_bytes_per_sec: f64,
    pub upload_bytes_per_sec: f64,
    pub disk_read_bytes_per_sec: f64,
    pub disk_write_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHistorySeries {
    pub hours: u32,
    pub bucket_ms: i64,
    pub points: Vec<SystemHistoryPoint>,
    pub sample_count: i64,
}

pub fn default_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("metrics.db")
}

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建数据目录：{error}"))?;
    }
    let connection = Connection::open(path).map_err(|error| format!("无法打开指标数据库：{error}"))?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|error| format!("无法设置数据库超时：{error}"))?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(|error| format!("无法初始化指标数据库：{error}"))?;
    migrate(&connection)?;
    Ok(connection)
}

pub fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS system_samples (
                captured_at_ms        INTEGER PRIMARY KEY,
                cpu_percent           REAL NOT NULL,
                memory_percent        REAL NOT NULL,
                download_bytes_per_sec REAL NOT NULL,
                upload_bytes_per_sec   REAL NOT NULL,
                disk_read_bytes_per_sec REAL NOT NULL,
                disk_write_bytes_per_sec REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_system_samples_time
                ON system_samples (captured_at_ms);
            PRAGMA user_version = 1;
            "#,
        )
        .map_err(|error| format!("无法迁移指标数据库：{error}"))
}

pub fn record(
    connection: &Connection,
    captured_at_ms: i64,
    sample: &SystemSampleInput,
    retention_days: u32,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT OR REPLACE INTO system_samples
             (captured_at_ms, cpu_percent, memory_percent, download_bytes_per_sec,
              upload_bytes_per_sec, disk_read_bytes_per_sec, disk_write_bytes_per_sec)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                captured_at_ms,
                sample.cpu_percent,
                sample.memory_percent,
                sample.download_bytes_per_sec as f64,
                sample.upload_bytes_per_sec as f64,
                sample.disk_read_bytes_per_sec as f64,
                sample.disk_write_bytes_per_sec as f64,
            ],
        )
        .map_err(|error| format!("无法写入指标样本：{error}"))?;
    let cutoff = captured_at_ms - (retention_days.clamp(1, 30) as i64) * 86_400_000;
    connection
        .execute(
            "DELETE FROM system_samples WHERE captured_at_ms < ?1",
            params![cutoff],
        )
        .map_err(|error| format!("无法清理过期指标：{error}"))?;
    Ok(())
}

pub fn query(connection: &Connection, hours: u32) -> Result<SystemHistorySeries, String> {
    let hours = hours.clamp(1, 24 * 30);
    let bucket_ms: i64 = match hours {
        0..=1 => 10_000,
        2..=6 => 60_000,
        7..=24 => 300_000,
        _ => 3_600_000,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let from = now - (hours as i64) * 3_600_000;

    let mut statement = connection
        .prepare(
            "SELECT (captured_at_ms / ?1) * ?1 AS bucket,
                    AVG(cpu_percent), AVG(memory_percent),
                    AVG(download_bytes_per_sec), AVG(upload_bytes_per_sec),
                    AVG(disk_read_bytes_per_sec), AVG(disk_write_bytes_per_sec),
                    COUNT(*)
             FROM system_samples
             WHERE captured_at_ms >= ?2
             GROUP BY bucket
             ORDER BY bucket",
        )
        .map_err(|error| format!("无法查询指标历史：{error}"))?;
    let rows = statement
        .query_map(params![bucket_ms, from], |row| {
            Ok((
                SystemHistoryPoint {
                    t: row.get(0)?,
                    cpu_percent: row.get(1)?,
                    memory_percent: row.get(2)?,
                    download_bytes_per_sec: row.get(3)?,
                    upload_bytes_per_sec: row.get(4)?,
                    disk_read_bytes_per_sec: row.get(5)?,
                    disk_write_bytes_per_sec: row.get(6)?,
                },
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|error| format!("无法读取指标历史：{error}"))?;

    let mut points = Vec::new();
    let mut sample_count = 0i64;
    for row in rows {
        let (point, count) = row.map_err(|error| format!("无法解析指标历史：{error}"))?;
        points.push(point);
        sample_count += count;
    }

    Ok(SystemHistorySeries {
        hours,
        bucket_ms,
        points,
        sample_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
    }

    fn sample(cpu: f64, memory: f64) -> SystemSampleInput {
        SystemSampleInput {
            cpu_percent: cpu,
            memory_percent: memory,
            download_bytes_per_sec: 1000,
            upload_bytes_per_sec: 500,
            disk_read_bytes_per_sec: 2000,
            disk_write_bytes_per_sec: 1000,
        }
    }

    #[test]
    fn records_and_averages_buckets() {
        let connection = memory_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let bucket = now / 10_000 * 10_000;
        record(&connection, bucket + 1_000, &sample(10.0, 40.0), 7).unwrap();
        record(&connection, bucket + 2_000, &sample(30.0, 60.0), 7).unwrap();
        let series = query(&connection, 1).unwrap();
        assert_eq!(series.sample_count, 2);
        assert!(!series.points.is_empty());
        let point = series.points.last().unwrap();
        assert!((point.cpu_percent - 20.0).abs() < 0.001);
        assert!((point.memory_percent - 50.0).abs() < 0.001);
    }

    #[test]
    fn prunes_samples_outside_retention() {
        let connection = memory_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        record(&connection, now - 10 * 86_400_000, &sample(10.0, 40.0), 7).unwrap();
        record(&connection, now, &sample(20.0, 50.0), 7).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM system_samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
