use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Unknown,
    Healthy,
    Caution,
    Critical,
}

impl HealthLevel {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            HealthLevel::Unknown => "unknown",
            HealthLevel::Healthy => "healthy",
            HealthLevel::Caution => "caution",
            HealthLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSignal {
    pub code: String,
    pub severity: HealthLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthAssessment {
    pub level: HealthLevel,
    pub signals: Vec<HealthSignal>,
}

pub struct DiskHealthInput<'a> {
    pub health_status: &'a str,
    pub temperature_c: Option<f64>,
    pub wear_percent: Option<f64>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
    pub predict_failure: Option<bool>,
    /// (SMART attribute id, raw value)
    pub attributes: &'a [(u8, u64)],
}

fn attr(input: &DiskHealthInput<'_>, id: u8) -> Option<u64> {
    input
        .attributes
        .iter()
        .find(|(attribute_id, _)| *attribute_id == id)
        .map(|(_, raw)| *raw)
}

fn push(signals: &mut Vec<HealthSignal>, severity: HealthLevel, code: &str, message: impl Into<String>) {
    signals.push(HealthSignal {
        code: code.into(),
        severity,
        message: message.into(),
    });
}

/// 基于公开 SMART 属性与可靠性计数器做规则分级，不伪造剩余寿命或健康百分比。
///
/// 规则均为“阈值 + 明确原因”，未识别到任何信号时返回健康或未知，而不是猜测。
pub fn assess(input: &DiskHealthInput<'_>) -> HealthAssessment {
    let mut signals = Vec::new();

    if input.predict_failure == Some(true) {
        push(
            &mut signals,
            HealthLevel::Critical,
            "SMART_PREDICT_FAILURE",
            "SMART 已预测磁盘故障，请立即备份数据。",
        );
    }

    let health = input.health_status.to_lowercase();
    if health.contains("unhealthy") || health.contains("critical") || health.contains("failed") {
        push(
            &mut signals,
            HealthLevel::Critical,
            "WINDOWS_HEALTH_UNHEALTHY",
            "Windows 存储健康状态报告为不健康。",
        );
    } else if health.contains("warning") {
        push(
            &mut signals,
            HealthLevel::Caution,
            "WINDOWS_HEALTH_WARNING",
            "Windows 存储健康状态报告为警告。",
        );
    }

    let reallocated = [attr(input, 5), attr(input, 196)]
        .into_iter()
        .flatten()
        .max();
    if let Some(raw) = reallocated.filter(|value| *value > 0) {
        let severity = if raw > 10 { HealthLevel::Critical } else { HealthLevel::Caution };
        push(
            &mut signals,
            severity,
            "SMART_REALLOCATED_SECTORS",
            format!("存在 {raw} 个重映射扇区，介质可能正在退化。"),
        );
    }

    for (id, label, code) in [
        (187u8, "不可校正错误", "SMART_UNCORRECTABLE"),
        (197u8, "待处理扇区", "SMART_PENDING_SECTORS"),
        (198u8, "离线不可校正扇区", "SMART_OFFLINE_UNCORRECTABLE"),
    ] {
        if let Some(raw) = attr(input, id).filter(|value| *value > 0) {
            let severity = if raw > 10 { HealthLevel::Critical } else { HealthLevel::Caution };
            push(
                &mut signals,
                severity,
                code,
                format!("{label}计数为 {raw}。"),
            );
        }
    }

    if let Some(raw) = attr(input, 199).filter(|value| *value > 0) {
        push(
            &mut signals,
            HealthLevel::Caution,
            "SMART_CRC_ERRORS",
            format!("接口 CRC 错误计数为 {raw}，建议检查数据线或接口。"),
        );
    }

    let temperature = input.temperature_c.or_else(|| {
        [attr(input, 194), attr(input, 190)]
            .into_iter()
            .flatten()
            .max()
            .filter(|value| (1..=120).contains(value))
            .map(|value| value as f64)
    });
    if let Some(temperature) = temperature.filter(|value| value.is_finite()) {
        if temperature >= 65.0 {
            push(
                &mut signals,
                HealthLevel::Critical,
                "DISK_TEMPERATURE_CRITICAL",
                format!("磁盘温度 {temperature:.0}°C，已超过安全范围。"),
            );
        } else if temperature >= 55.0 {
            push(
                &mut signals,
                HealthLevel::Caution,
                "DISK_TEMPERATURE_HIGH",
                format!("磁盘温度 {temperature:.0}°C，持续高温会加速磨损。"),
            );
        }
    }

    if let Some(wear) = input.wear_percent.filter(|value| value.is_finite()) {
        if wear >= 90.0 {
            push(
                &mut signals,
                HealthLevel::Critical,
                "NVME_WEAR_CRITICAL",
                format!("NVMe 寿命消耗已达 {wear:.0}%。"),
            );
        } else if wear >= 80.0 {
            push(
                &mut signals,
                HealthLevel::Caution,
                "NVME_WEAR_HIGH",
                format!("NVMe 寿命消耗已达 {wear:.0}%。"),
            );
        }
    }

    let life_remaining = [attr(input, 231), attr(input, 202)]
        .into_iter()
        .flatten()
        .filter(|value| *value <= 100)
        .min();
    if let Some(remaining) = life_remaining {
        if remaining < 10 {
            push(
                &mut signals,
                HealthLevel::Critical,
                "SSD_LIFE_CRITICAL",
                format!("SSD 剩余寿命约 {remaining}%。"),
            );
        } else if remaining < 20 {
            push(
                &mut signals,
                HealthLevel::Caution,
                "SSD_LIFE_LOW",
                format!("SSD 剩余寿命约 {remaining}%。"),
            );
        }
    }

    let read_errors = input.read_errors.unwrap_or_default();
    let write_errors = input.write_errors.unwrap_or_default();
    if read_errors > 0 || write_errors > 0 {
        push(
            &mut signals,
            HealthLevel::Caution,
            "STORAGE_IO_ERRORS",
            format!("存储可靠性计数器报告读取错误 {read_errors}、写入错误 {write_errors}。"),
        );
    }

    let level = signals
        .iter()
        .map(|signal| signal.severity)
        .max()
        .unwrap_or_else(|| {
            if health.contains("healthy")
                || input.temperature_c.is_some()
                || input.wear_percent.is_some()
                || !input.attributes.is_empty()
            {
                HealthLevel::Healthy
            } else {
                HealthLevel::Unknown
            }
        });

    HealthAssessment { level, signals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(attributes: &[(u8, u64)]) -> DiskHealthInput<'_> {
        DiskHealthInput {
            health_status: "Healthy",
            temperature_c: Some(40.0),
            wear_percent: Some(5.0),
            read_errors: Some(0),
            write_errors: Some(0),
            predict_failure: Some(false),
            attributes,
        }
    }

    #[test]
    fn clean_disk_is_healthy() {
        let assessment = assess(&input(&[(5, 0), (194, 40), (231, 95)]));
        assert_eq!(assessment.level, HealthLevel::Healthy);
        assert!(assessment.signals.is_empty());
    }

    #[test]
    fn reallocated_sectors_escalate() {
        let caution = assess(&input(&[(5, 3)]));
        assert_eq!(caution.level, HealthLevel::Caution);
        let critical = assess(&input(&[(5, 40)]));
        assert_eq!(critical.level, HealthLevel::Critical);
    }

    #[test]
    fn pending_and_uncorrectable_sectors_are_flagged() {
        let pending = assess(&input(&[(197, 2)]));
        assert_eq!(pending.level, HealthLevel::Caution);
        assert!(pending.signals.iter().any(|signal| signal.code == "SMART_PENDING_SECTORS"));
        let uncorrectable = assess(&input(&[(198, 30)]));
        assert_eq!(uncorrectable.level, HealthLevel::Critical);
    }

    #[test]
    fn temperature_thresholds_apply() {
        let mut data = input(&[]);
        data.temperature_c = Some(58.0);
        assert_eq!(assess(&data).level, HealthLevel::Caution);
        data.temperature_c = Some(70.0);
        assert_eq!(assess(&data).level, HealthLevel::Critical);
    }

    #[test]
    fn wear_and_life_remaining_apply() {
        let mut data = input(&[]);
        data.wear_percent = Some(85.0);
        assert_eq!(assess(&data).level, HealthLevel::Caution);
        data.wear_percent = Some(95.0);
        assert_eq!(assess(&data).level, HealthLevel::Critical);

        let life = assess(&input(&[(231, 12)]));
        assert_eq!(life.level, HealthLevel::Caution);
        let life_critical = assess(&input(&[(231, 5)]));
        assert_eq!(life_critical.level, HealthLevel::Critical);
    }

    #[test]
    fn predicted_failure_always_wins() {
        let mut data = input(&[]);
        data.predict_failure = Some(true);
        assert_eq!(assess(&data).level, HealthLevel::Critical);
    }

    #[test]
    fn unknown_when_no_data() {
        let assessment = assess(&DiskHealthInput {
            health_status: "Unknown",
            temperature_c: None,
            wear_percent: None,
            read_errors: None,
            write_errors: None,
            predict_failure: None,
            attributes: &[],
        });
        assert_eq!(assessment.level, HealthLevel::Unknown);
    }

    #[test]
    fn io_errors_are_caution() {
        let mut data = input(&[]);
        data.read_errors = Some(3);
        data.write_errors = Some(1);
        assert_eq!(assess(&data).level, HealthLevel::Caution);
        assert!(assess(&data).signals.iter().any(|signal| signal.code == "STORAGE_IO_ERRORS"));
    }
}
