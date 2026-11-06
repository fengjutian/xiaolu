//! Collects structured diagnostic facts from the existing local collectors.
//!
//! Facts contain no host name, user name, serial number, MAC/IP address,
//! command line or full local path. The model never talks to Tauri commands or
//! collectors directly; it only receives this fact list through the provider.

use super::model::{
    AiSourceStatus, Availability, DiagnosticFact, DiagnosticSnapshot, Severity,
    AI_SCHEMA_VERSION,
};
use crate::sensors;
use crate::storage_health::HealthLevel;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn fnv1a64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn build_snapshot_id(captured_at: u64, facts: &[DiagnosticFact]) -> String {
    let mut material = format!("{captured_at}|");
    for fact in facts {
        material.push_str(&fact.id);
        material.push('=');
        material.push_str(&fact.value.to_string());
        material.push(';');
    }
    format!("snap-{captured_at:x}-{:08x}", fnv1a64(&material) as u32)
}

fn truncate_text(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

struct FactBuilder {
    snapshot_id: String,
    captured_at: u64,
    facts: Vec<DiagnosticFact>,
}

impl FactBuilder {
    fn new(snapshot_id: String, captured_at: u64) -> Self {
        FactBuilder {
            snapshot_id,
            captured_at,
            facts: Vec::new(),
        }
    }

    fn push(
        &mut self,
        id: impl Into<String>,
        category: &str,
        severity: Severity,
        title: impl Into<String>,
        value: Value,
        source: impl Into<String>,
        confidence: f64,
        availability: Availability,
    ) {
        self.facts.push(DiagnosticFact {
            id: id.into(),
            snapshot_id: self.snapshot_id.clone(),
            category: category.to_string(),
            severity,
            title: title.into(),
            value,
            source: source.into(),
            captured_at_unix_ms: self.captured_at,
            confidence: confidence.clamp(0.0, 1.0),
            availability,
            evidence_refs: Vec::new(),
            sensitive: false,
        });
    }
}

fn number_at(value: &Value, path: &[&str]) -> Option<f64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_f64()
}

fn u64_at(value: &Value, path: &[&str]) -> Option<u64> {
    number_at(value, path).map(|number| number.max(0.0) as u64)
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str()
}

fn health_severity(level: HealthLevel) -> Severity {
    match level {
        HealthLevel::Critical => Severity::Critical,
        HealthLevel::Caution => Severity::Warning,
        _ => Severity::Info,
    }
}

fn disk_letter(mount_point: &str) -> String {
    let trimmed = mount_point.trim().trim_end_matches(['\\', '/']);
    let mut characters = trimmed.chars();
    if let (Some(drive), Some(':')) = (characters.next(), characters.next()) {
        if drive.is_ascii_alphabetic() {
            return format!("{}:", drive.to_ascii_uppercase());
        }
    }
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        truncate_text(trimmed, 12)
    }
}

fn push_system_facts(
    builder: &mut FactBuilder,
    app: &AppHandle,
    sources: &mut Vec<AiSourceStatus>,
    limitations: &mut Vec<String>,
) {
    let collector = app.state::<std::sync::Mutex<crate::system::SystemCollector>>();
    let collected = (|| -> Result<(Value, Value), String> {
        let mut guard = collector
            .lock()
            .map_err(|_| "系统采集器被占用".to_string())?;
        let snapshot = guard.snapshot()?;
        let processes = guard.processes();
        Ok((
            serde_json::to_value(&snapshot).map_err(|error| error.to_string())?,
            serde_json::to_value(&processes).map_err(|error| error.to_string())?,
        ))
    })();

    match collected {
        Ok((system, processes)) => {
            let os_name = string_at(&system, &["osName"]).unwrap_or("Windows");
            let os_version = string_at(&system, &["osVersion"]).unwrap_or("未知版本");
            let kernel = string_at(&system, &["kernelVersion"]).unwrap_or("未知");
            let arch = string_at(&system, &["architecture"]).unwrap_or("未知");
            builder.push(
                "os.version",
                "system",
                Severity::Info,
                "操作系统版本",
                json!({
                    "osName": os_name,
                    "osVersion": os_version,
                    "kernelVersion": kernel,
                    "architecture": arch,
                }),
                "Windows / sysinfo",
                0.95,
                Availability::Available,
            );
            let uptime = u64_at(&system, &["uptimeSeconds"]).unwrap_or(0);
            builder.push(
                "system.uptime",
                "system",
                Severity::Info,
                "系统运行时长",
                json!({
                    "uptimeSeconds": uptime,
                    "uptimeHours": (uptime as f64 / 3600.0 * 10.0).round() / 10.0,
                }),
                "Windows / sysinfo",
                0.95,
                Availability::Available,
            );

            let cpu_usage = number_at(&system, &["cpu", "usagePercent"]).unwrap_or(0.0);
            let cpu_severity = if cpu_usage >= 95.0 {
                Severity::Critical
            } else if cpu_usage >= 85.0 {
                Severity::Warning
            } else {
                Severity::Info
            };
            builder.push(
                "cpu.usage",
                "cpu",
                cpu_severity,
                "CPU 当前使用率",
                json!({
                    "usagePercent": (cpu_usage * 10.0).round() / 10.0,
                    "logicalCores": u64_at(&system, &["cpu", "logicalCores"]).unwrap_or(0),
                    "frequencyMhz": u64_at(&system, &["cpu", "frequencyMhz"]).unwrap_or(0),
                    "brand": string_at(&system, &["cpu", "brand"]).unwrap_or("未知处理器"),
                }),
                "sysinfo",
                0.9,
                Availability::Available,
            );

            let memory_usage = number_at(&system, &["memory", "usagePercent"]).unwrap_or(0.0);
            let memory_severity = if memory_usage >= 95.0 {
                Severity::Critical
            } else if memory_usage >= 85.0 {
                Severity::Warning
            } else {
                Severity::Info
            };
            builder.push(
                "memory.usage",
                "memory",
                memory_severity,
                "内存当前使用率",
                json!({
                    "usagePercent": (memory_usage * 10.0).round() / 10.0,
                    "totalBytes": u64_at(&system, &["memory", "totalBytes"]).unwrap_or(0),
                    "usedBytes": u64_at(&system, &["memory", "usedBytes"]).unwrap_or(0),
                }),
                "sysinfo",
                0.9,
                Availability::Available,
            );

            if let Some(disks) = system.get("disks").and_then(Value::as_array) {
                for disk in disks {
                    let mount = string_at(disk, &["mountPoint"]).unwrap_or("");
                    let letter = disk_letter(mount);
                    let total = u64_at(disk, &["totalBytes"]).unwrap_or(0);
                    let available = u64_at(disk, &["availableBytes"]).unwrap_or(0);
                    let free_percent = if total > 0 {
                        available as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    };
                    let severity = if free_percent < 5.0 {
                        Severity::Critical
                    } else if free_percent < 10.0 {
                        Severity::Warning
                    } else {
                        Severity::Info
                    };
                    builder.push(
                        format!("disk.{}.space", letter.to_ascii_lowercase()),
                        "disk",
                        severity,
                        format!("{letter} 盘空间"),
                        json!({
                            "drive": letter,
                            "fileSystem": string_at(disk, &["fileSystem"]).unwrap_or("未知"),
                            "totalBytes": total,
                            "availableBytes": available,
                            "freePercent": (free_percent * 10.0).round() / 10.0,
                            "removable": disk.get("removable").and_then(Value::as_bool).unwrap_or(false),
                        }),
                        "Windows 卷信息 / sysinfo",
                        0.9,
                        Availability::Available,
                    );
                }
            }

            if let Some(network) = system.get("network") {
                builder.push(
                    "network.throughput",
                    "network",
                    Severity::Info,
                    "网络吞吐",
                    json!({
                        "downloadBytesPerSec": u64_at(network, &["downloadBytesPerSec"]).unwrap_or(0),
                        "uploadBytesPerSec": u64_at(network, &["uploadBytesPerSec"]).unwrap_or(0),
                        "interfaces": u64_at(network, &["interfaces"]).unwrap_or(0),
                    }),
                    "sysinfo",
                    0.85,
                    Availability::Available,
                );
            }

            if let Some(items) = processes.as_array() {
                let top: Vec<Value> = items
                    .iter()
                    .take(5)
                    .map(|process| {
                        json!({
                            "name": truncate_text(string_at(process, &["name"]).unwrap_or("未知进程"), 80),
                            "cpuPercent": number_at(process, &["cpuPercent"]).unwrap_or(0.0),
                            "memoryBytes": u64_at(process, &["memoryBytes"]).unwrap_or(0),
                        })
                    })
                    .collect();
                if !top.is_empty() {
                    builder.push(
                        "process.top",
                        "process",
                        Severity::Info,
                        "资源占用最高的进程",
                        json!({ "items": top }),
                        "sysinfo",
                        0.85,
                        Availability::Available,
                    );
                }
            }
            sources.push(AiSourceStatus {
                id: "system".into(),
                label: "系统与硬件".into(),
                ok: true,
                detail: "系统快照、磁盘与进程".into(),
            });
        }
        Err(error) => {
            limitations.push(format!("系统快照不可用：{error}"));
            sources.push(AiSourceStatus {
                id: "system".into(),
                label: "系统与硬件".into(),
                ok: false,
                detail: error,
            });
        }
    }
}

fn push_sensor_facts(
    builder: &mut FactBuilder,
    app: &AppHandle,
    sources: &mut Vec<AiSourceStatus>,
    limitations: &mut Vec<String>,
) {
    let snapshot = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sensors::snapshot_blocking(app)
    }));
    let snapshot = match snapshot {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) => {
            limitations.push(format!("温度与健康采集不可用：{error}"));
            sources.push(AiSourceStatus {
                id: "sensors".into(),
                label: "温度与存储健康".into(),
                ok: false,
                detail: error,
            });
            builder.push(
                "sensor.snapshot",
                "sensor",
                Severity::Info,
                "温度与存储健康采集",
                json!({ "available": false }),
                "ACPI / SMART / 厂商接口",
                0.4,
                Availability::Unavailable,
            );
            return;
        }
        Err(_) => {
            limitations.push("温度与健康采集发生异常，温度结论不可用。".to_string());
            sources.push(AiSourceStatus {
                id: "sensors".into(),
                label: "温度与存储健康".into(),
                ok: false,
                detail: "采集任务异常".into(),
            });
            return;
        }
    };

    let temperatures: Vec<_> = snapshot
        .values
        .iter()
        .filter(|value| value.category == "temperature" && value.availability.is_ok())
        .collect();
    if temperatures.is_empty() {
        limitations.push("未检测到可用温度传感器，不能对温度作确定性判断。".to_string());
        builder.push(
            "sensor.temperature",
            "sensor",
            Severity::Info,
            "CPU / GPU / 磁盘温度",
            json!({ "available": false, "reason": "未检测到可用温度传感器" }),
            "ACPI / 厂商接口",
            0.4,
            Availability::Unavailable,
        );
    } else {
        let values: Vec<Value> = temperatures
            .iter()
            .take(12)
            .map(|value| {
                json!({
                    "id": value.id,
                    "hardware": truncate_text(&value.hardware_name, 80),
                    "hardwareClass": value.hardware_class,
                    "name": truncate_text(&value.name, 80),
                    "valueC": value.value,
                    "unit": value.unit,
                    "source": value.source,
                    "quality": value.quality,
                })
            })
            .collect();
        builder.push(
            "sensor.temperature",
            "sensor",
            Severity::Info,
            "CPU / GPU / 磁盘温度",
            json!({ "available": true, "values": values }),
            "ACPI / 厂商接口",
            0.85,
            Availability::Available,
        );
    }

    for alert in snapshot.alerts.iter().take(6) {
        let severity = match alert.level {
            sensors::alerts::AlertLevel::Critical => Severity::Critical,
            sensors::alerts::AlertLevel::Warning => Severity::Warning,
            sensors::alerts::AlertLevel::Normal => Severity::Info,
        };
        builder.push(
            format!("sensor.alert.{}", truncate_text(&alert.id, 60)),
            "sensor",
            severity,
            format!("温度告警：{}", truncate_text(&alert.sensor_name, 60)),
            json!({
                "level": alert.level,
                "value": alert.value,
                "threshold": alert.threshold,
                "message": truncate_text(&alert.message, 200),
                "sinceUnixMs": alert.since_unix_ms,
            }),
            "本地温度规则引擎",
            0.9,
            Availability::Available,
        );
    }

    for (index, storage) in snapshot.storage.iter().take(8).enumerate() {
        builder.push(
            format!("storage.health.{index}"),
            "storage",
            health_severity(storage.assessment.level),
            format!("存储健康：{}", truncate_text(&storage.name, 80)),
            json!({
                "name": truncate_text(&storage.name, 80),
                "healthStatus": truncate_text(&storage.health_status, 80),
                "operationalStatus": truncate_text(&storage.operational_status, 80),
                "temperatureC": storage.temperature_c,
                "wearPercent": storage.wear_percent,
                "powerOnHours": storage.power_on_hours,
                "readErrorsTotal": storage.read_errors_total,
                "writeErrorsTotal": storage.write_errors_total,
                "signals": storage.assessment.signals.iter().take(6).map(|signal| json!({
                    "code": signal.code,
                    "severity": signal.severity,
                    "message": truncate_text(&signal.message, 160),
                })).collect::<Vec<_>>(),
            }),
            format!("SMART / {}", truncate_text(&storage.source, 60)),
            0.9,
            Availability::Available,
        );
    }

    for (index, device) in snapshot.smart_devices.iter().take(8).enumerate() {
        builder.push(
            format!("storage.smart.{index}"),
            "storage",
            if device.predict_failure == Some(true) {
                Severity::Critical
            } else if device.assessment.level == HealthLevel::Caution {
                Severity::Warning
            } else {
                Severity::Info
            },
            format!("SMART 预测：{}", truncate_text(&device.instance_name, 80)),
            json!({
                "instance": truncate_text(&device.instance_name, 80),
                "predictFailure": device.predict_failure,
                "healthLevel": device.assessment.level,
                "attributeCount": device.attributes.len(),
                "source": truncate_text(&device.source, 60),
            }),
            "ATA SMART",
            0.85,
            Availability::Available,
        );
    }

    for limitation in snapshot.limitations.iter().take(6) {
        limitations.push(format!(
            "{}：{}",
            truncate_text(&limitation.code, 40),
            truncate_text(&limitation.message, 160)
        ));
    }
    if snapshot.alerts.is_empty() && !temperatures.is_empty() {
        limitations.push("当前没有温度告警，但温度阈值由本地规则决定，模型不覆盖该判定。".to_string());
    }

    sources.push(AiSourceStatus {
        id: "sensors".into(),
        label: "温度与存储健康".into(),
        ok: true,
        detail: format!(
            "{} 个温度值 · {} 个存储设备",
            temperatures.len(),
            snapshot.storage.len()
        ),
    });
}

fn push_driver_facts(
    builder: &mut FactBuilder,
    sources: &mut Vec<AiSourceStatus>,
    limitations: &mut Vec<String>,
) {
    match crate::drivers::get_driver_inventory()
        .and_then(|inventory| serde_json::to_value(&inventory).map_err(|error| error.to_string()))
    {
        Ok(inventory) => {
            let total = u64_at(&inventory, &["total"]).unwrap_or(0);
            let missing = u64_at(&inventory, &["missing"]).unwrap_or(0);
            let problems = u64_at(&inventory, &["problems"]).unwrap_or(0);
            let unsigned = u64_at(&inventory, &["unsigned"]).unwrap_or(0);
            let severity = if missing > 0 || problems > 0 {
                Severity::Warning
            } else {
                Severity::Info
            };
            let problem_names: Vec<String> = inventory
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter(|item| {
                            item.get("missingDriver").and_then(Value::as_bool).unwrap_or(false)
                                || item
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .map(|status| !status.eq_ignore_ascii_case("OK"))
                                    .unwrap_or(false)
                        })
                        .take(3)
                        .filter_map(|item| item.get("deviceName").and_then(Value::as_str))
                        .map(|name| truncate_text(name, 80))
                        .collect()
                })
                .unwrap_or_default();
            builder.push(
                "drivers.inventory",
                "drivers",
                severity,
                "设备驱动状态",
                json!({
                    "total": total,
                    "missing": missing,
                    "problems": problems,
                    "unsigned": unsigned,
                    "problemDevices": problem_names,
                    "source": string_at(&inventory, &["source"]).unwrap_or("SetupAPI / PnP"),
                }),
                "SetupAPI / PnP",
                0.9,
                Availability::Available,
            );
            sources.push(AiSourceStatus {
                id: "drivers".into(),
                label: "设备与驱动".into(),
                ok: true,
                detail: format!("{total} 个设备，{missing} 个缺失驱动"),
            });
        }
        Err(error) => {
            limitations.push(format!("驱动清单不可用：{error}"));
            sources.push(AiSourceStatus {
                id: "drivers".into(),
                label: "设备与驱动".into(),
                ok: false,
                detail: error,
            });
            builder.push(
                "drivers.inventory",
                "drivers",
                Severity::Info,
                "设备驱动状态",
                json!({ "available": false }),
                "SetupAPI / PnP",
                0.4,
                Availability::Unavailable,
            );
        }
    }
}

fn push_diagnostic_facts(
    builder: &mut FactBuilder,
    sources: &mut Vec<AiSourceStatus>,
    limitations: &mut Vec<String>,
) {
    match crate::diagnostics::collect_report()
        .and_then(|report| serde_json::to_value(&report).map_err(|error| error.to_string()))
    {
        Ok(report) => {
            let pending_reboot = report
                .get("pendingReboot")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let crash_events = report
                .get("crashEvents")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let service_issues = report
                .get("serviceIssues")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let minidumps = u64_at(&report, &["minidumpCount"]).unwrap_or(0);
            let severity = if crash_events > 0 || minidumps > 0 {
                Severity::Warning
            } else {
                Severity::Info
            };
            builder.push(
                "diagnostics.report",
                "diagnostics",
                severity,
                "系统诊断摘要",
                json!({
                    "pendingReboot": pending_reboot,
                    "pendingWindowsUpdateReboot": report.get("pendingWindowsUpdateReboot").and_then(Value::as_bool).unwrap_or(false),
                    "crashEventCount": crash_events,
                    "minidumpCount": minidumps,
                    "serviceIssueCount": service_issues,
                    "batteryPresent": report.get("battery").and_then(|battery| battery.get("present")).and_then(Value::as_bool).unwrap_or(false),
                    "batteryHealthPercent": number_at(&report, &["battery", "healthPercent"]),
                    "batteryCycleCount": u64_at(&report, &["battery", "cycleCount"]),
                    "source": string_at(&report, &["source"]).unwrap_or("CIM / WMI / Event Log"),
                }),
                "Windows CIM / WMI / 事件日志",
                0.85,
                Availability::Available,
            );
            sources.push(AiSourceStatus {
                id: "diagnostics".into(),
                label: "系统诊断".into(),
                ok: true,
                detail: format!("{crash_events} 个崩溃事件，{minidumps} 个转储"),
            });
        }
        Err(error) => {
            limitations.push(format!("系统诊断不可用：{error}"));
            sources.push(AiSourceStatus {
                id: "diagnostics".into(),
                label: "系统诊断".into(),
                ok: false,
                detail: error,
            });
            builder.push(
                "diagnostics.report",
                "diagnostics",
                Severity::Info,
                "系统诊断摘要",
                json!({ "available": false }),
                "Windows CIM / WMI",
                0.4,
                Availability::Unavailable,
            );
        }
    }
}

fn push_history_facts(
    builder: &mut FactBuilder,
    app: &AppHandle,
    sources: &mut Vec<AiSourceStatus>,
    limitations: &mut Vec<String>,
) {
    let result = (|| -> Result<crate::system_history::SystemHistorySeries, String> {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法定位数据目录：{error}"))?;
        let connection = crate::system_history::open(&crate::system_history::default_db_path(
            &directory,
        ))?;
        crate::system_history::query(&connection, 24)
    })();

    match result {
        Ok(series) => {
            if series.points.is_empty() {
                limitations.push("最近 24 小时没有指标样本，趋势结论不可用。".to_string());
                builder.push(
                    "trend.system.24h",
                    "trend",
                    Severity::Info,
                    "最近 24 小时系统趋势",
                    json!({ "available": false }),
                    "本地指标数据库",
                    0.5,
                    Availability::Unavailable,
                );
            } else {
                let mut cpu_sum = 0.0;
                let mut cpu_max = 0.0_f64;
                let mut memory_sum = 0.0;
                let mut memory_max = 0.0_f64;
                for point in &series.points {
                    cpu_sum += point.cpu_percent;
                    cpu_max = cpu_max.max(point.cpu_percent);
                    memory_sum += point.memory_percent;
                    memory_max = memory_max.max(point.memory_percent);
                }
                let count = series.points.len() as f64;
                let cpu_avg = cpu_sum / count;
                let memory_avg = memory_sum / count;
                let severity = if cpu_max >= 95.0 || memory_max >= 95.0 {
                    Severity::Warning
                } else {
                    Severity::Info
                };
                builder.push(
                    "trend.system.24h",
                    "trend",
                    severity,
                    "最近 24 小时系统趋势",
                    json!({
                        "available": true,
                        "hours": series.hours,
                        "sampleCount": series.sample_count,
                        "points": series.points.len(),
                        "cpuAvgPercent": (cpu_avg * 10.0).round() / 10.0,
                        "cpuMaxPercent": (cpu_max * 10.0).round() / 10.0,
                        "memoryAvgPercent": (memory_avg * 10.0).round() / 10.0,
                        "memoryMaxPercent": (memory_max * 10.0).round() / 10.0,
                    }),
                    "本地指标数据库",
                    0.8,
                    Availability::Available,
                );
            }
            sources.push(AiSourceStatus {
                id: "history".into(),
                label: "24 小时趋势".into(),
                ok: true,
                detail: format!("{} 个趋势点", series.points.len()),
            });
        }
        Err(error) => {
            limitations.push(format!("24 小时趋势不可用：{error}"));
            sources.push(AiSourceStatus {
                id: "history".into(),
                label: "24 小时趋势".into(),
                ok: false,
                detail: error,
            });
            builder.push(
                "trend.system.24h",
                "trend",
                Severity::Info,
                "最近 24 小时系统趋势",
                json!({ "available": false }),
                "本地指标数据库",
                0.4,
                Availability::Unavailable,
            );
        }
    }
}

/// Collects every available source, isolating per-source failures so a partial
/// snapshot is still useful. Runs on a blocking thread.
pub fn collect_snapshot(app: &AppHandle) -> DiagnosticSnapshot {
    let captured_at = now_unix_ms();
    let provisional_id = format!("snap-{captured_at:x}-pending");
    let mut builder = FactBuilder::new(provisional_id, captured_at);
    let mut sources = Vec::new();
    let mut limitations = Vec::new();

    push_system_facts(&mut builder, app, &mut sources, &mut limitations);
    push_sensor_facts(&mut builder, app, &mut sources, &mut limitations);
    push_driver_facts(&mut builder, &mut sources, &mut limitations);
    push_diagnostic_facts(&mut builder, &mut sources, &mut limitations);
    push_history_facts(&mut builder, app, &mut sources, &mut limitations);

    let snapshot_id = build_snapshot_id(captured_at, &builder.facts);
    for fact in builder.facts.iter_mut() {
        fact.snapshot_id = snapshot_id.clone();
    }

    let local_findings = super::rules::evaluate(&builder.facts, &limitations);
    DiagnosticSnapshot {
        schema_version: AI_SCHEMA_VERSION,
        snapshot_id,
        captured_at_unix_ms: captured_at,
        facts: builder.facts,
        local_findings,
        limitations,
        sources,
        excluded_fact_count: 0,
    }
}
