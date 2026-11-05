pub mod alerts;
pub mod collect;
pub mod history;

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use alerts::{Alert, Observation};
use history::{HistorySeries, SensorSettings};

pub const SCHEMA_VERSION: u32 = 2;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 稳定的 32 位 FNV-1a 哈希：同名硬件在重启后得到同一个 ID。
pub fn stable_hash(input: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in input.trim().to_lowercase().as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Ok,
    Unavailable,
    Denied,
    Unsupported,
    Stale,
}

impl Availability {
    pub fn is_ok(self) -> bool {
        matches!(self, Availability::Ok)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Measured,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorValue {
    pub id: String,
    pub hardware_id: String,
    pub hardware_name: String,
    pub hardware_class: String,
    pub category: String,
    pub name: String,
    pub value: Option<f64>,
    pub unit: String,
    pub source: String,
    pub captured_at_unix_ms: u64,
    pub availability: Availability,
    pub error_code: Option<String>,
    pub quality: Quality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limitation {
    pub code: String,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorSourceStatus {
    pub source: String,
    pub ok: bool,
    pub last_error: Option<String>,
    pub last_success_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageHealth {
    pub name: String,
    pub health_status: String,
    pub operational_status: String,
    pub temperature_c: Option<f64>,
    pub wear_percent: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub read_errors_total: Option<u64>,
    pub write_errors_total: Option<u64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartAttribute {
    pub id: u8,
    pub name: String,
    pub current: u8,
    pub worst: u8,
    pub raw: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartDevice {
    pub instance_name: String,
    pub predict_failure: Option<bool>,
    pub attributes: Vec<SmartAttribute>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorSnapshot {
    pub schema_version: u32,
    pub captured_at_unix_ms: u64,
    pub values: Vec<SensorValue>,
    pub storage: Vec<StorageHealth>,
    pub smart_devices: Vec<SmartDevice>,
    pub limitations: Vec<Limitation>,
    pub sources: Vec<SensorSourceStatus>,
    pub alerts: Vec<Alert>,
    pub elevated: bool,
}

fn db_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    Ok(history::default_db_path(&directory))
}

fn observations(values: &[SensorValue]) -> Vec<Observation> {
    values
        .iter()
        .filter(|value| value.availability.is_ok() && value.category == "temperature")
        .filter_map(|value| {
            value.value.map(|number| Observation {
                sensor_id: value.id.clone(),
                sensor_name: value.name.clone(),
                hardware_class: value.hardware_class.clone(),
                value: number,
            })
        })
        .collect()
}

fn tray_summary(values: &[SensorValue], alerts: &[Alert]) -> String {
    if let Some(critical) = alerts
        .iter()
        .find(|alert| alert.level == alerts::AlertLevel::Critical)
    {
        return format!("小鲁 · 危险：{} {:.0}°C", critical.sensor_name, critical.value);
    }
    let hottest = values
        .iter()
        .filter(|value| value.availability.is_ok() && value.category == "temperature")
        .filter_map(|value| value.value.map(|number| (value, number)))
        .max_by(|left, right| left.1.total_cmp(&right.1));
    match hottest {
        Some((value, number)) => format!(
            "小鲁 · {} {:.0}°C · {} 个传感器{}",
            value.hardware_name,
            number,
            values.iter().filter(|item| item.availability.is_ok()).count(),
            if alerts.is_empty() { "" } else { " · 有告警" }
        ),
        None => "小鲁 · 暂无温度传感器".to_string(),
    }
}

fn update_tray(app: &AppHandle, values: &[SensorValue], alerts: &[Alert]) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tray_summary(values, alerts)));
    }
}

fn snapshot_blocking(app: &AppHandle) -> Result<SensorSnapshot, String> {
    let parsed = collect::collect()?;
    let captured_at = now_unix_ms();
    let path = db_path(app)?;
    let mut connection = history::open(&path)?;
    let settings = history::load_settings(&connection)?;

    let mut active_alerts = Vec::new();
    if settings.alerts_enabled {
        let mut states = history::load_alert_states(&connection)?;
        active_alerts = alerts::evaluate(
            &mut states,
            &observations(&parsed.values),
            &settings.thresholds,
            captured_at,
        );
        history::save_alert_states(&mut connection, &states)?;
    }

    history::insert_snapshot(&mut connection, &parsed.values, settings.retention_hours)?;
    update_tray(app, &parsed.values, &active_alerts);

    Ok(SensorSnapshot {
        schema_version: SCHEMA_VERSION,
        captured_at_unix_ms: captured_at,
        values: parsed.values,
        storage: parsed.storage,
        smart_devices: parsed.smart_devices,
        limitations: parsed.limitations,
        sources: parsed.sources,
        alerts: active_alerts,
        elevated: parsed.elevated,
    })
}

#[tauri::command]
pub async fn get_sensor_snapshot(app: AppHandle) -> Result<SensorSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || snapshot_blocking(&app))
        .await
        .map_err(|error| format!("传感器采集任务失败：{error}"))?
}

#[tauri::command]
pub async fn get_sensor_history(
    app: AppHandle,
    sensor_id: String,
    hours: u32,
) -> Result<HistorySeries, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let connection = history::open(&db_path(&app)?)?;
        history::query_history(&connection, &sensor_id, hours)
    })
    .await
    .map_err(|error| format!("历史查询任务失败：{error}"))?
}

#[tauri::command]
pub async fn get_sensor_settings(app: AppHandle) -> Result<SensorSettings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let connection = history::open(&db_path(&app)?)?;
        history::load_settings(&connection)
    })
    .await
    .map_err(|error| format!("设置读取任务失败：{error}"))?
}

#[tauri::command]
pub async fn save_sensor_settings(
    app: AppHandle,
    settings: SensorSettings,
) -> Result<SensorSettings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let connection = history::open(&db_path(&app)?)?;
        history::save_settings(&connection, &settings)
    })
    .await
    .map_err(|error| format!("设置保存任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_case_and_whitespace_insensitive() {
        assert_eq!(stable_hash(" NVIDIA RTX 4070 "), stable_hash("nvidia rtx 4070"));
        assert_ne!(stable_hash("a"), stable_hash("b"));
    }

    #[test]
    fn tray_summary_prefers_critical_alert() {
        let value = SensorValue {
            id: "cpu-1-temp".into(),
            hardware_id: "cpu-1".into(),
            hardware_name: "Intel Core".into(),
            hardware_class: "cpu".into(),
            category: "temperature".into(),
            name: "CPU 温度".into(),
            value: Some(96.0),
            unit: "°C".into(),
            source: "test".into(),
            captured_at_unix_ms: 0,
            availability: Availability::Ok,
            error_code: None,
            quality: Quality::Measured,
        };
        let alert = Alert {
            id: "cpu-1-temp:critical".into(),
            sensor_id: value.id.clone(),
            sensor_name: value.name.clone(),
            level: alerts::AlertLevel::Critical,
            value: 96.0,
            threshold: 95.0,
            message: "hot".into(),
            since_unix_ms: 0,
            last_notified_unix_ms: 0,
        };
        let summary = tray_summary(&[value], &[alert]);
        assert!(summary.contains("危险"));
    }
}
