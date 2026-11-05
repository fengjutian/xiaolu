use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 连续多少个越限样本才真正升级告警级别。
pub const RISE_DEBOUNCE_SAMPLES: u32 = 3;
/// 回落到阈值以下多少度才解除告警（滞回）。
pub const HYSTERESIS_C: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Normal,
    Warning,
    Critical,
}

impl AlertLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            AlertLevel::Normal => "normal",
            AlertLevel::Warning => "warning",
            AlertLevel::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "warning" => AlertLevel::Warning,
            "critical" => AlertLevel::Critical,
            _ => AlertLevel::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdClass {
    pub warning_c: f64,
    pub critical_c: f64,
}

impl ThresholdClass {
    fn sanitized(&self, fallback: &ThresholdClass) -> ThresholdClass {
        let warning = if self.warning_c.is_finite() {
            self.warning_c.clamp(30.0, 110.0)
        } else {
            fallback.warning_c
        };
        let critical = if self.critical_c.is_finite() {
            self.critical_c.clamp(warning + 1.0, 120.0)
        } else {
            fallback.critical_c.max(warning + 1.0)
        };
        ThresholdClass {
            warning_c: warning,
            critical_c: critical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub cpu: ThresholdClass,
    pub gpu: ThresholdClass,
    pub disk: ThresholdClass,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            cpu: ThresholdClass { warning_c: 85.0, critical_c: 95.0 },
            gpu: ThresholdClass { warning_c: 80.0, critical_c: 90.0 },
            disk: ThresholdClass { warning_c: 55.0, critical_c: 65.0 },
        }
    }
}

impl Thresholds {
    pub fn sanitize(&self) -> Thresholds {
        let fallback = Thresholds::default();
        Thresholds {
            cpu: self.cpu.sanitized(&fallback.cpu),
            gpu: self.gpu.sanitized(&fallback.gpu),
            disk: self.disk.sanitized(&fallback.disk),
        }
    }

    pub fn for_class(&self, hardware_class: &str) -> &ThresholdClass {
        match hardware_class {
            "cpu" => &self.cpu,
            "gpu" => &self.gpu,
            "disk" => &self.disk,
            _ => &self.cpu,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub sensor_id: String,
    pub sensor_name: String,
    pub hardware_class: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertState {
    pub sensor_id: String,
    pub level: AlertLevel,
    pub consecutive: u32,
    pub changed_at_unix_ms: u64,
    pub last_value: f64,
    pub last_notified_unix_ms: u64,
}

impl AlertState {
    fn normal(sensor_id: &str, now: u64, value: f64) -> Self {
        AlertState {
            sensor_id: sensor_id.to_string(),
            level: AlertLevel::Normal,
            consecutive: 0,
            changed_at_unix_ms: now,
            last_value: value,
            last_notified_unix_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub sensor_id: String,
    pub sensor_name: String,
    pub level: AlertLevel,
    pub value: f64,
    pub threshold: f64,
    pub message: String,
    pub since_unix_ms: u64,
    pub last_notified_unix_ms: u64,
}

fn target_level(value: f64, limits: &ThresholdClass) -> AlertLevel {
    if value >= limits.critical_c {
        AlertLevel::Critical
    } else if value >= limits.warning_c {
        AlertLevel::Warning
    } else {
        AlertLevel::Normal
    }
}

/// 纯状态机：输入当前状态、观测值和时间戳，输出新状态与仍然有效的告警。
///
/// 升级需要连续 `RISE_DEBOUNCE_SAMPLES` 个样本；降级立即生效，但必须低于
/// 阈值 `HYSTERESIS_C`，避免在阈值边缘来回抖动。
pub fn evaluate(
    states: &mut BTreeMap<String, AlertState>,
    observations: &[Observation],
    thresholds: &Thresholds,
    now: u64,
) -> Vec<Alert> {
    let mut alerts = Vec::new();
    for observation in observations {
        if !observation.value.is_finite() {
            continue;
        }
        let limits = thresholds.for_class(&observation.hardware_class);
        let previous = states
            .entry(observation.sensor_id.clone())
            .or_insert_with(|| AlertState::normal(&observation.sensor_id, now, observation.value));

        let target = target_level(observation.value, limits);
        let current = previous.level;
        let mut next = current;
        let mut consecutive = previous.consecutive;
        let mut changed = false;

        if target > current {
            consecutive = consecutive.saturating_add(1);
            if consecutive >= RISE_DEBOUNCE_SAMPLES {
                next = target;
                consecutive = 0;
                changed = true;
            }
        } else if target < current {
            // 降级必须越过滞回带。
            let release_below = match current {
                AlertLevel::Critical => limits.critical_c - HYSTERESIS_C,
                AlertLevel::Warning => limits.warning_c - HYSTERESIS_C,
                AlertLevel::Normal => f64::NEG_INFINITY,
            };
            if observation.value < release_below {
                next = target;
                consecutive = 0;
                changed = true;
            }
        } else {
            consecutive = 0;
        }

        if changed {
            previous.changed_at_unix_ms = now;
        }
        previous.level = next;
        previous.consecutive = consecutive;
        previous.last_value = observation.value;

        if next != AlertLevel::Normal {
            let threshold = match next {
                AlertLevel::Critical => limits.critical_c,
                _ => limits.warning_c,
            };
            let label = if observation.hardware_class == "disk" {
                "磁盘"
            } else if observation.hardware_class == "gpu" {
                "GPU"
            } else {
                "CPU"
            };
            alerts.push(Alert {
                id: format!("{}:{}", observation.sensor_id, next.as_str()),
                sensor_id: observation.sensor_id.clone(),
                sensor_name: observation.sensor_name.clone(),
                level: next,
                value: observation.value,
                threshold,
                message: format!(
                    "{}温度 {:.1}°C 已超过{}阈值 {:.0}°C",
                    label,
                    observation.value,
                    if next == AlertLevel::Critical { "危险" } else { "警告" },
                    threshold
                ),
                since_unix_ms: previous.changed_at_unix_ms,
                last_notified_unix_ms: previous.last_notified_unix_ms,
            });
        }
    }
    alerts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(id: &str, class: &str, value: f64) -> Observation {
        Observation {
            sensor_id: id.to_string(),
            sensor_name: format!("{id} name"),
            hardware_class: class.to_string(),
            value,
        }
    }

    #[test]
    fn rises_only_after_debounce() {
        let mut states = BTreeMap::new();
        let thresholds = Thresholds::default();
        for step in 0..RISE_DEBOUNCE_SAMPLES {
            let alerts = evaluate(&mut states, &[observation("cpu.temp", "cpu", 90.0)], &thresholds, step as u64);
            assert!(alerts.is_empty(), "sample {step} must not alert yet");
        }
        let alerts = evaluate(&mut states, &[observation("cpu.temp", "cpu", 90.0)], &thresholds, 99);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, AlertLevel::Warning);
        assert_eq!(alerts[0].since_unix_ms, 99);
    }

    #[test]
    fn applies_hysteresis_before_clearing() {
        let mut states = BTreeMap::new();
        let thresholds = Thresholds::default();
        for step in 0..RISE_DEBOUNCE_SAMPLES {
            evaluate(&mut states, &[observation("cpu.temp", "cpu", 90.0)], &thresholds, step);
        }
        // 90 -> 81 仍高于 warning 阈值，保持告警。
        let alerts = evaluate(&mut states, &[observation("cpu.temp", "cpu", 81.0)], &thresholds, 10);
        assert_eq!(alerts.len(), 1);
        // 低于 85 - 5 = 80 才解除。
        let alerts = evaluate(&mut states, &[observation("cpu.temp", "cpu", 79.0)], &thresholds, 11);
        assert!(alerts.is_empty());
        assert_eq!(states["cpu.temp"].level, AlertLevel::Normal);
    }

    #[test]
    fn jumps_to_critical_and_downgrades_by_hysteresis() {
        let mut states = BTreeMap::new();
        let thresholds = Thresholds::default();
        for step in 0..RISE_DEBOUNCE_SAMPLES {
            evaluate(&mut states, &[observation("gpu.temp", "gpu", 96.0)], &thresholds, step);
        }
        assert_eq!(states["gpu.temp"].level, AlertLevel::Critical);
        // 91 仍高于 critical 阈值 90，但低于 90 + 5，保持 critical。
        let alerts = evaluate(&mut states, &[observation("gpu.temp", "gpu", 91.0)], &thresholds, 10);
        assert_eq!(alerts[0].level, AlertLevel::Critical);
        // 84 低于 90 - 5，降到 warning（仍高于 80）。
        let alerts = evaluate(&mut states, &[observation("gpu.temp", "gpu", 84.0)], &thresholds, 11);
        assert_eq!(alerts[0].level, AlertLevel::Warning);
    }

    #[test]
    fn ignores_non_finite_values() {
        let mut states = BTreeMap::new();
        let alerts = evaluate(&mut states, &[observation("cpu.temp", "cpu", f64::NAN)], &Thresholds::default(), 0);
        assert!(alerts.is_empty());
        assert!(states.is_empty());
    }
}
