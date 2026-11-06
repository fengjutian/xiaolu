//! Local rule engine.
//!
//! Rules decide severity; the model only explains. When the model is disabled,
//! unavailable or returns invalid output, these findings are still shown in the
//! UI. Every finding references at least one local fact id.

use super::model::{AiFinding, AiRecommendation, DiagnosticFact, Severity};

fn finding(
    id: &str,
    title: &str,
    severity: Severity,
    summary: String,
    confidence: &str,
    evidence_refs: Vec<String>,
    recommendations: Vec<AiRecommendation>,
) -> AiFinding {
    AiFinding {
        id: id.to_string(),
        title: title.to_string(),
        severity: severity.as_str().to_string(),
        summary,
        confidence: confidence.to_string(),
        evidence_refs,
        recommendations,
        limitations: Vec::new(),
    }
}

fn recommendation(label: &str, kind: &str, risk: &str) -> AiRecommendation {
    AiRecommendation {
        label: label.to_string(),
        kind: kind.to_string(),
        tool_id: None,
        risk: risk.to_string(),
        requires_confirmation: false,
    }
}

fn fact<'a>(facts: &'a [DiagnosticFact], id: &str) -> Option<&'a DiagnosticFact> {
    facts.iter().find(|fact| fact.id == id && fact.is_available())
}

fn number(fact: &DiagnosticFact, key: &str) -> Option<f64> {
    fact.value.get(key).and_then(serde_json::Value::as_f64)
}

/// Evaluates deterministic rules against the current facts.
pub fn evaluate(facts: &[DiagnosticFact], _limitations: &[String]) -> Vec<AiFinding> {
    let mut findings = Vec::new();

    if let Some(memory) = fact(facts, "memory.usage") {
        if let Some(percent) = number(memory, "usagePercent") {
            let severity = if percent >= 95.0 {
                Severity::Critical
            } else if percent >= 85.0 {
                Severity::Warning
            } else {
                Severity::Info
            };
            if severity != Severity::Info {
                findings.push(finding(
                    "rule.memory.usage",
                    "内存压力偏高",
                    severity,
                    format!(
                        "当前内存使用率 {:.1}%。持续高占用会触发分页并造成卡顿。",
                        percent
                    ),
                    "high",
                    vec![memory.id.clone()],
                    vec![recommendation("查看占用内存最高的进程", "read_only_check", "none")],
                ));
            }
        }
    }

    if let Some(cpu) = fact(facts, "cpu.usage") {
        if let Some(percent) = number(cpu, "usagePercent") {
            if percent >= 90.0 {
                findings.push(finding(
                    "rule.cpu.usage",
                    "CPU 瞬时占用很高",
                    Severity::Warning,
                    format!("当前 CPU 使用率 {:.1}%。单次采样不代表持续负载，需结合趋势判断。", percent),
                    "medium",
                    vec![cpu.id.clone()],
                    vec![recommendation("查看运行进程中的高占用项", "read_only_check", "none")],
                ));
            }
        }
    }

    if let Some(trend) = fact(facts, "trend.system.24h") {
        let cpu_max = number(trend, "cpuMaxPercent").unwrap_or(0.0);
        let memory_max = number(trend, "memoryMaxPercent").unwrap_or(0.0);
        if cpu_max >= 90.0 || memory_max >= 90.0 {
            findings.push(finding(
                "rule.trend.high",
                "最近 24 小时出现高负载",
                Severity::Warning,
                format!(
                    "过去 24 小时 CPU 峰值 {:.1}%，内存峰值 {:.1}%。",
                    cpu_max, memory_max
                ),
                "high",
                vec![trend.id.clone()],
                vec![recommendation("对照性能概览确认高负载时段", "read_only_check", "none")],
            ));
        }
    }

    for disk in facts
        .iter()
        .filter(|fact| fact.category == "disk" && fact.id.ends_with(".space"))
    {
        if let Some(free) = number(disk, "freePercent") {
            if free < 10.0 {
                let severity = if free < 5.0 {
                    Severity::Critical
                } else {
                    Severity::Warning
                };
                findings.push(finding(
                    &format!("rule.{}", disk.id),
                    "磁盘可用空间不足",
                    severity,
                    format!(
                        "{} 剩余空间约 {:.1}%，空间过低会影响更新、休眠文件和临时文件写入。",
                        disk.value
                            .get("drive")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("系统盘"),
                        free
                    ),
                    "high",
                    vec![disk.id.clone()],
                    vec![recommendation("使用空间分析定位可清理内容", "read_only_check", "low")],
                ));
            }
        }
    }

    for alert in facts
        .iter()
        .filter(|fact| fact.id.starts_with("sensor.alert."))
        .take(2)
    {
        let severity = match alert.severity {
            Severity::Critical => Severity::Critical,
            _ => Severity::Warning,
        };
        findings.push(finding(
            &format!("rule.{}", alert.id),
            "温度传感器触发本地告警",
            severity,
            alert
                .value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| alert.title.clone()),
            "high",
            vec![alert.id.clone()],
            vec![recommendation("检查散热、风扇与后台负载", "manual_action", "low")],
        ));
    }

    for storage in facts.iter().filter(|fact| fact.category == "storage") {
        if storage.id.starts_with("storage.smart.") {
            let predict_failure = storage
                .value
                .get("predictFailure")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if predict_failure {
                findings.push(finding(
                    &format!("rule.{}", storage.id),
                    "SMART 预测磁盘可能故障",
                    Severity::Critical,
                    "SMART 报告预测故障状态。请优先备份重要数据，并联系硬件服务渠道。".to_string(),
                    "high",
                    vec![storage.id.clone()],
                    vec![recommendation("立即备份重要数据", "manual_action", "medium")],
                ));
            }
            continue;
        }
        if storage.severity == Severity::Critical || storage.severity == Severity::Warning {
            findings.push(finding(
                &format!("rule.{}", storage.id),
                "存储健康状态需要关注",
                storage.severity,
                format!(
                    "{} 的健康评估为 {}，请结合 SMART 原始属性判断。",
                    storage
                        .value
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("存储设备"),
                    storage.severity.as_str()
                ),
                "high",
                vec![storage.id.clone()],
                vec![recommendation("备份数据并复查 SMART", "manual_action", "medium")],
            ));
        }
    }

    if let Some(drivers) = fact(facts, "drivers.inventory") {
        let missing = number(drivers, "missing").unwrap_or(0.0);
        let problems = number(drivers, "problems").unwrap_or(0.0);
        if missing > 0.0 || problems > 0.0 {
            findings.push(finding(
                "rule.drivers.problems",
                "存在缺失驱动或设备异常",
                Severity::Warning,
                format!("{:.0} 个设备缺少驱动，{:.0} 个设备状态异常。", missing, problems),
                "high",
                vec![drivers.id.clone()],
                vec![recommendation("打开驱动检测查看问题设备", "read_only_check", "none")],
            ));
        }
    }

    if let Some(diagnostics) = fact(facts, "diagnostics.report") {
        let pending_reboot = diagnostics
            .value
            .get("pendingReboot")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if pending_reboot {
            findings.push(finding(
                "rule.diagnostics.reboot",
                "系统有待重启的更新或组件",
                Severity::Info,
                "检测到待重启状态；完成更新后重启可避免部分组件异常。".to_string(),
                "high",
                vec![diagnostics.id.clone()],
                vec![recommendation("保存工作后重启系统", "manual_action", "none")],
            ));
        }
        let crashes = number(diagnostics, "crashEventCount").unwrap_or(0.0);
        let dumps = number(diagnostics, "minidumpCount").unwrap_or(0.0);
        if crashes > 0.0 || dumps > 0.0 {
            findings.push(finding(
                "rule.diagnostics.crashes",
                "最近一周存在崩溃或异常关机记录",
                Severity::Warning,
                format!("{:.0} 个崩溃/异常关机事件，{:.0} 个转储文件。需结合时间点与驱动变更分析。", crashes, dumps),
                "medium",
                vec![diagnostics.id.clone()],
                vec![recommendation("查看系统诊断中的崩溃事件", "read_only_check", "none")],
            ));
        }
        if let Some(health) = number(diagnostics, "batteryHealthPercent") {
            if health > 0.0 && health < 60.0 {
                findings.push(finding(
                    "rule.diagnostics.battery",
                    "电池容量衰减明显",
                    Severity::Warning,
                    format!("电池当前满充容量约为设计容量的 {:.0}%。", health),
                    "medium",
                    vec![diagnostics.id.clone()],
                    vec![recommendation("检查供电与电池使用时长", "manual_action", "low")],
                ));
            }
        }
    }

    findings.sort_by(|left, right| {
        Severity::parse(&right.severity)
            .cmp(&Severity::parse(&left.severity))
            .then_with(|| left.id.cmp(&right.id))
    });
    findings.truncate(6);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::model::{Availability, DiagnosticFact};
    use serde_json::json;

    fn fact(id: &str, category: &str, value: serde_json::Value) -> DiagnosticFact {
        DiagnosticFact {
            id: id.into(),
            snapshot_id: "snap-test".into(),
            category: category.into(),
            severity: Severity::Info,
            title: id.into(),
            value,
            source: "test".into(),
            captured_at_unix_ms: 0,
            confidence: 0.9,
            availability: Availability::Available,
            evidence_refs: vec![],
            sensitive: false,
        }
    }

    #[test]
    fn flags_memory_and_disk_pressure() {
        let facts = vec![
            fact("memory.usage", "memory", json!({ "usagePercent": 96.0 })),
            fact(
                "disk.c.space",
                "disk",
                json!({ "drive": "C:", "freePercent": 3.2 }),
            ),
        ];
        let findings = evaluate(&facts, &[]);
        assert!(findings.iter().any(|item| item.id == "rule.memory.usage"));
        assert!(findings.iter().any(|item| item.id == "rule.disk.c.space"));
        assert!(findings
            .iter()
            .all(|item| !item.evidence_refs.is_empty()));
    }

    #[test]
    fn ignores_unavailable_facts() {
        let mut unavailable = fact("memory.usage", "memory", json!({ "usagePercent": 99.0 }));
        unavailable.availability = Availability::Unavailable;
        assert!(evaluate(&[unavailable], &[]).is_empty());
    }
}
