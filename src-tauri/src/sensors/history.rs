use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::alerts::{AlertLevel, AlertState, Thresholds};
use super::SensorValue;

const SETTINGS_KEY: &str = "sensor_settings";
const HOURLY_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorSettings {
    pub sampling_interval_ms: u64,
    pub retention_hours: u32,
    pub alerts_enabled: bool,
    pub thresholds: Thresholds,
}

impl Default for SensorSettings {
    fn default() -> Self {
        SensorSettings {
            sampling_interval_ms: 10_000,
            retention_hours: 24,
            alerts_enabled: true,
            thresholds: Thresholds::default(),
        }
    }
}

impl SensorSettings {
    /// 把外部输入收敛到可用范围，避免 UI 传入 0 或极端值破坏采样与数据库。
    pub fn sanitize(&self) -> SensorSettings {
        SensorSettings {
            sampling_interval_ms: self.sampling_interval_ms.clamp(2_000, 60_000),
            retention_hours: self.retention_hours.clamp(1, 24 * 30),
            alerts_enabled: self.alerts_enabled,
            thresholds: self.thresholds.sanitize(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub t: i64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySeries {
    pub sensor_id: String,
    pub bucket_ms: i64,
    pub points: Vec<HistoryPoint>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>,
    pub sample_count: i64,
}

pub fn default_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("sensors.db")
}

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建数据目录：{error}"))?;
    }
    let connection = Connection::open(path).map_err(|error| format!("无法打开传感器数据库：{error}"))?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|error| format!("无法设置数据库超时：{error}"))?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
        .map_err(|error| format!("无法初始化数据库：{error}"))?;
    migrate(&connection)?;
    Ok(connection)
}

pub fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sensor_samples (
                captured_at_ms INTEGER NOT NULL,
                sensor_id      TEXT    NOT NULL,
                sensor_name    TEXT    NOT NULL,
                value          REAL    NOT NULL,
                unit           TEXT    NOT NULL,
                source         TEXT    NOT NULL,
                PRIMARY KEY (captured_at_ms, sensor_id)
            ) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS idx_sensor_samples_time
                ON sensor_samples (captured_at_ms);
            CREATE INDEX IF NOT EXISTS idx_sensor_samples_sensor
                ON sensor_samples (sensor_id, captured_at_ms DESC);
            CREATE TABLE IF NOT EXISTS sensor_hourly (
                hour_ms    INTEGER NOT NULL,
                sensor_id  TEXT    NOT NULL,
                min_value  REAL    NOT NULL,
                max_value  REAL    NOT NULL,
                avg_value  REAL    NOT NULL,
                samples    INTEGER NOT NULL,
                PRIMARY KEY (hour_ms, sensor_id)
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS alert_state (
                sensor_id        TEXT PRIMARY KEY,
                level            TEXT NOT NULL,
                consecutive      INTEGER NOT NULL,
                changed_at_ms    INTEGER NOT NULL,
                last_value       REAL NOT NULL,
                last_notified_ms INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            PRAGMA user_version = 1;
            "#,
        )
        .map_err(|error| format!("无法迁移传感器数据库：{error}"))
}

/// 写入本次快照中可用的数值，同时维护小时聚合。
pub fn insert_snapshot(
    connection: &mut Connection,
    values: &[SensorValue],
    retention_hours: u32,
) -> Result<usize, String> {
    let now = super::now_unix_ms() as i64;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始数据库事务：{error}"))?;
    let mut inserted = 0usize;
    {
        let mut sample = transaction
            .prepare(
                "INSERT OR REPLACE INTO sensor_samples
                 (captured_at_ms, sensor_id, sensor_name, value, unit, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|error| format!("无法准备样本写入：{error}"))?;
        let mut hourly = transaction
            .prepare(
                "INSERT INTO sensor_hourly (hour_ms, sensor_id, min_value, max_value, avg_value, samples)
                 VALUES (?1, ?2, ?3, ?3, ?3, 1)
                 ON CONFLICT(hour_ms, sensor_id) DO UPDATE SET
                     min_value = MIN(sensor_hourly.min_value, excluded.min_value),
                     max_value = MAX(sensor_hourly.max_value, excluded.max_value),
                     avg_value = (sensor_hourly.avg_value * sensor_hourly.samples + excluded.avg_value)
                                 / (sensor_hourly.samples + 1),
                     samples   = sensor_hourly.samples + 1",
            )
            .map_err(|error| format!("无法准备聚合写入：{error}"))?;
        for value in values {
            let Some(number) = value.value.filter(|item| item.is_finite()) else {
                continue;
            };
            if !value.availability.is_ok() {
                continue;
            }
            let captured_at = value.captured_at_unix_ms as i64;
            sample
                .execute(params![
                    captured_at,
                    value.id,
                    value.name,
                    number,
                    value.unit,
                    value.source,
                ])
                .map_err(|error| format!("无法写入传感器样本：{error}"))?;
            let hour = captured_at / 3_600_000 * 3_600_000;
            hourly
                .execute(params![hour, value.id, number])
                .map_err(|error| format!("无法写入小时聚合：{error}"))?;
            inserted += 1;
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("无法提交传感器样本：{error}"))?;
    prune(connection, now, retention_hours)?;
    Ok(inserted)
}

pub fn prune(connection: &Connection, now: i64, retention_hours: u32) -> Result<(), String> {
    let cutoff = now - (retention_hours as i64) * 3_600_000;
    connection
        .execute("DELETE FROM sensor_samples WHERE captured_at_ms < ?1", params![cutoff])
        .map_err(|error| format!("无法清理过期样本：{error}"))?;
    connection
        .execute(
            "DELETE FROM sensor_hourly WHERE hour_ms < ?1",
            params![now - HOURLY_RETENTION_MS],
        )
        .map_err(|error| format!("无法清理过期聚合：{error}"))?;
    Ok(())
}

pub fn load_settings(connection: &Connection) -> Result<SensorSettings, String> {
    let stored: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("无法读取传感器设置：{error}"))?;
    match stored {
        Some(json) => serde_json::from_str::<SensorSettings>(&json)
            .map(|settings| settings.sanitize())
            .map_err(|error| format!("传感器设置格式无效：{error}")),
        None => Ok(SensorSettings::default()),
    }
}

pub fn save_settings(
    connection: &Connection,
    settings: &SensorSettings,
) -> Result<SensorSettings, String> {
    let sanitized = settings.sanitize();
    let json = serde_json::to_string(&sanitized).map_err(|error| format!("无法序列化设置：{error}"))?;
    connection
        .execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SETTINGS_KEY, json],
        )
        .map_err(|error| format!("无法保存传感器设置：{error}"))?;
    Ok(sanitized)
}

pub fn load_alert_states(connection: &Connection) -> Result<BTreeMap<String, AlertState>, String> {
    let mut statement = connection
        .prepare(
            "SELECT sensor_id, level, consecutive, changed_at_ms, last_value, last_notified_ms
             FROM alert_state",
        )
        .map_err(|error| format!("无法读取告警状态：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(AlertState {
                sensor_id: row.get(0)?,
                level: AlertLevel::parse(&row.get::<_, String>(1)?),
                consecutive: row.get::<_, i64>(2)? as u32,
                changed_at_unix_ms: row.get::<_, i64>(3)? as u64,
                last_value: row.get(4)?,
                last_notified_unix_ms: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(|error| format!("无法查询告警状态：{error}"))?;
    let mut states = BTreeMap::new();
    for row in rows {
        let state = row.map_err(|error| format!("无法解析告警状态：{error}"))?;
        states.insert(state.sensor_id.clone(), state);
    }
    Ok(states)
}

pub fn save_alert_states(
    connection: &mut Connection,
    states: &BTreeMap<String, AlertState>,
) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始告警事务：{error}"))?;
    transaction
        .execute("DELETE FROM alert_state", [])
        .map_err(|error| format!("无法重置告警状态：{error}"))?;
    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO alert_state
                 (sensor_id, level, consecutive, changed_at_ms, last_value, last_notified_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|error| format!("无法准备告警写入：{error}"))?;
        for state in states.values() {
            statement
                .execute(params![
                    state.sensor_id,
                    state.level.as_str(),
                    state.consecutive as i64,
                    state.changed_at_unix_ms as i64,
                    state.last_value,
                    state.last_notified_unix_ms as i64,
                ])
                .map_err(|error| format!("无法写入告警状态：{error}"))?;
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("无法提交告警状态：{error}"))
}

/// 按时间范围查询，自动选择合适的聚合粒度，避免前端拿到上万个点。
pub fn query_history(connection: &Connection, sensor_id: &str, hours: u32) -> Result<HistorySeries, String> {
    let hours = hours.clamp(1, 24 * 30);
    let bucket_ms: i64 = match hours {
        0..=1 => 10_000,
        2..=6 => 60_000,
        7..=24 => 300_000,
        25..=72 => 900_000,
        _ => 3_600_000,
    };
    let from = super::now_unix_ms() as i64 - (hours as i64) * 3_600_000;
    let mut statement = connection
        .prepare(
            "SELECT (captured_at_ms / ?1) * ?1 AS bucket,
                    MIN(value), MAX(value), AVG(value), COUNT(*)
             FROM sensor_samples
             WHERE sensor_id = ?2 AND captured_at_ms >= ?3
             GROUP BY bucket
             ORDER BY bucket",
        )
        .map_err(|error| format!("无法查询历史：{error}"))?;
    let points = statement
        .query_map(params![bucket_ms, sensor_id, from], |row| {
            Ok(HistoryPoint {
                t: row.get(0)?,
                min: row.get(1)?,
                max: row.get(2)?,
                avg: row.get(3)?,
                count: row.get(4)?,
            })
        })
        .map_err(|error| format!("无法读取历史：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法解析历史：{error}"))?;

    let (min, max, avg, count): (Option<f64>, Option<f64>, Option<f64>, i64) = connection
        .query_row(
            "SELECT MIN(value), MAX(value), AVG(value), COUNT(*)
             FROM sensor_samples WHERE sensor_id = ?1 AND captured_at_ms >= ?2",
            params![sensor_id, from],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| format!("无法汇总历史：{error}"))?;

    Ok(HistorySeries {
        sensor_id: sensor_id.to_string(),
        bucket_ms,
        points,
        min,
        max,
        avg,
        sample_count: count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::{Availability, Quality};

    fn sample(id: &str, at: u64, value: f64) -> SensorValue {
        SensorValue {
            id: id.to_string(),
            hardware_id: "hw".into(),
            hardware_name: "HW".into(),
            hardware_class: "cpu".into(),
            category: "temperature".into(),
            name: "CPU".into(),
            value: Some(value),
            unit: "°C".into(),
            source: "test".into(),
            captured_at_unix_ms: at,
            availability: Availability::Ok,
            error_code: None,
            quality: Quality::Measured,
        }
    }

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
    }

    fn now_ms() -> u64 {
        crate::sensors::now_unix_ms()
    }

    #[test]
    fn stores_available_values_only() {
        let mut connection = memory_db();
        let mut unavailable = sample("cpu", now_ms(), 40.0);
        unavailable.availability = Availability::Unavailable;
        unavailable.value = None;
        let count = insert_snapshot(&mut connection, &[sample("cpu", now_ms(), 40.0), unavailable], 24).unwrap();
        assert_eq!(count, 1);
        let stored: i64 = connection
            .query_row("SELECT COUNT(*) FROM sensor_samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn aggregates_into_hourly_buckets() {
        let mut connection = memory_db();
        let hour = now_ms() / 3_600_000 * 3_600_000;
        insert_snapshot(&mut connection, &[sample("cpu", hour + 1_000, 40.0)], 24).unwrap();
        insert_snapshot(&mut connection, &[sample("cpu", hour + 61_000, 60.0)], 24).unwrap();
        let (min, max, avg, samples): (f64, f64, f64, i64) = connection
            .query_row(
                "SELECT min_value, max_value, avg_value, samples FROM sensor_hourly",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(min, 40.0);
        assert_eq!(max, 60.0);
        assert_eq!(avg, 50.0);
        assert_eq!(samples, 2);
    }

    #[test]
    fn prunes_samples_older_than_retention() {
        let mut connection = memory_db();
        let now = now_ms();
        insert_snapshot(&mut connection, &[sample("cpu", now - 2 * 3_600_000, 40.0)], 1).unwrap();
        let stored: i64 = connection
            .query_row("SELECT COUNT(*) FROM sensor_samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, 0);
    }

    #[test]
    fn query_history_returns_buckets_and_summary() {
        let mut connection = memory_db();
        let now = now_ms();
        insert_snapshot(&mut connection, &[sample("cpu", now - 120_000, 40.0)], 24).unwrap();
        insert_snapshot(&mut connection, &[sample("cpu", now - 60_000, 50.0)], 24).unwrap();
        let series = query_history(&connection, "cpu", 1).unwrap();
        assert_eq!(series.sensor_id, "cpu");
        assert_eq!(series.sample_count, 2);
        assert_eq!(series.min, Some(40.0));
        assert_eq!(series.max, Some(50.0));
        assert!(!series.points.is_empty());
        assert_eq!(series.bucket_ms, 10_000);
    }

    #[test]
    fn settings_roundtrip_and_clamping() {
        let connection = memory_db();
        let default = load_settings(&connection).unwrap();
        assert_eq!(default.retention_hours, 24);

        let mut desired = default.clone();
        desired.sampling_interval_ms = 10;
        desired.retention_hours = 9_999;
        let saved = save_settings(&connection, &desired).unwrap();
        assert_eq!(saved.sampling_interval_ms, 2_000);
        assert_eq!(saved.retention_hours, 720);

        let loaded = load_settings(&connection).unwrap();
        assert_eq!(loaded.sampling_interval_ms, 2_000);
        assert_eq!(loaded.retention_hours, 720);
    }

    #[test]
    fn alert_states_roundtrip() {
        let mut connection = memory_db();
        let mut states = BTreeMap::new();
        states.insert(
            "cpu".to_string(),
            AlertState {
                sensor_id: "cpu".into(),
                level: AlertLevel::Warning,
                consecutive: 0,
                changed_at_unix_ms: 123,
                last_value: 90.0,
                last_notified_unix_ms: 0,
            },
        );
        save_alert_states(&mut connection, &states).unwrap();
        let loaded = load_alert_states(&connection).unwrap();
        assert_eq!(loaded["cpu"].level, AlertLevel::Warning);
        assert_eq!(loaded["cpu"].changed_at_unix_ms, 123);
    }
}
