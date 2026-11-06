//! Parsing and validation of model output.
//!
//! The UI never parses Markdown. The model must return JSON, and every finding
//! is verified against the local facts. Findings without a valid evidence
//! reference are discarded instead of being shown as deterministic advice.

use super::model::{
    AiFinding, AiRecommendation, AiRequestedCheck, DiagnosticFact, MAX_AI_FINDINGS, Severity,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

const MAX_REQUESTED_CHECKS: usize = 3;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RawAnswer {
    pub summary: String,
    pub findings: Vec<AiFinding>,
    pub limitations: Vec<String>,
    pub requested_checks: Vec<AiRequestedCheck>,
}

#[derive(Debug, Default)]
pub struct ValidatedAnswer {
    pub summary: String,
    pub findings: Vec<AiFinding>,
    pub limitations: Vec<String>,
    pub requested_checks: Vec<AiRequestedCheck>,
}

fn clamp_text(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

fn safe_id(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        .take(64)
        .collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

fn normalize_confidence(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" | "高" => "high",
        "medium" | "中" => "medium",
        _ => "low",
    }
    .to_string()
}

fn normalize_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual_action" => "manual_action",
        "controlled_write_preview" => "controlled_write_preview",
        _ => "read_only_check",
    }
}

fn normalize_risk(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        _ => "none",
    }
}

fn normalize_recommendations(items: Vec<AiRecommendation>) -> Vec<AiRecommendation> {
    items
        .into_iter()
        .filter_map(|item| {
            let label = clamp_text(&item.label, 200);
            if label.is_empty() {
                return None;
            }
            let kind = normalize_kind(&item.kind);
            Some(AiRecommendation {
                label,
                kind: kind.to_string(),
                tool_id: item
                    .tool_id
                    .map(|value| clamp_text(&value, 64))
                    .filter(|value| {
                        !value.is_empty()
                            && value
                                .chars()
                                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
                    }),
                risk: normalize_risk(&item.risk).to_string(),
                requires_confirmation: item.requires_confirmation
                    || kind == "controlled_write_preview",
            })
        })
        .take(4)
        .collect()
}

fn normalize_checks(items: Vec<AiRequestedCheck>) -> Vec<AiRequestedCheck> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter_map(|item| {
            let tool_id = clamp_text(&item.tool_id, 80).to_ascii_lowercase();
            if tool_id.is_empty()
                || !tool_id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
                || !seen.insert(tool_id.clone())
            {
                return None;
            }
            let parameters = match serde_json::to_string(&item.parameters) {
                Ok(encoded) if encoded.len() <= 2000 => item.parameters,
                _ => Value::Null,
            };
            Some(AiRequestedCheck {
                tool_id,
                reason: clamp_text(&item.reason, 240),
                parameters,
            })
        })
        .take(MAX_REQUESTED_CHECKS)
        .collect()
}

/// Ensures a model answer cannot quietly contradict a local warning/critical
/// rule. The UI shows local findings separately, but the answer must say that
/// the local rule takes precedence when it did not reference the same evidence.
pub fn enforce_local_priority(local_findings: &[AiFinding], validated: &mut ValidatedAnswer) {
    let ai_refs: HashSet<&str> = validated
        .findings
        .iter()
        .flat_map(|finding| finding.evidence_refs.iter().map(String::as_str))
        .collect();
    for local in local_findings {
        if Severity::parse(&local.severity) < Severity::Warning || local.evidence_refs.is_empty() {
            continue;
        }
        if local
            .evidence_refs
            .iter()
            .any(|reference| ai_refs.contains(reference.as_str()))
        {
            continue;
        }
        validated.limitations.push(format!(
            "本地规则已将“{}”判定为{}；AI 结论未引用对应证据，请以本地规则为准。",
            local.title,
            Severity::parse(&local.severity).as_str()
        ));
    }
    validated.limitations.dedup();
    validated.limitations.truncate(8);
}

/// Extracts a JSON object from a model response, tolerating code fences.
pub fn parse_answer(text: &str) -> Result<RawAnswer, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("模型返回了空响应。".to_string());
    }
    let unfenced = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        rest.strip_suffix("```").unwrap_or(rest).trim()
    } else {
        trimmed
    };
    if let Ok(answer) = serde_json::from_str::<RawAnswer>(unfenced) {
        return Ok(answer);
    }
    let start = unfenced.find('{');
    let end = unfenced.rfind('}');
    if let (Some(start), Some(end)) = (start, end) {
        if end > start {
            return serde_json::from_str::<RawAnswer>(&unfenced[start..=end])
                .map_err(|error| format!("模型输出不是合法的诊断 JSON：{error}"));
        }
    }
    Err("模型输出不是合法的诊断 JSON。".to_string())
}

/// Validates findings against the immutable snapshot facts.
pub fn validate(raw: RawAnswer, facts: &[DiagnosticFact]) -> Result<ValidatedAnswer, String> {
    let valid_ids: HashSet<&str> = facts.iter().map(|fact| fact.id.as_str()).collect();
    let mut limitations = Vec::new();
    let mut findings = Vec::new();
    let requested_checks = normalize_checks(raw.requested_checks);

    for (index, finding) in raw.findings.into_iter().take(8).enumerate() {
        let title = clamp_text(&finding.title, 120);
        let summary = clamp_text(&finding.summary, 1200);
        if title.is_empty() && summary.is_empty() {
            continue;
        }
        let mut refs: Vec<String> = finding
            .evidence_refs
            .into_iter()
            .map(|reference| clamp_text(&reference, 80))
            .filter(|reference| valid_ids.contains(reference.as_str()))
            .collect();
        refs.dedup();
        refs.truncate(6);
        if refs.is_empty() {
            limitations.push(format!(
                "模型结论“{}”缺少有效证据引用，已隐藏。",
                if title.is_empty() { "未命名结论" } else { &title }
            ));
            continue;
        }
        findings.push(AiFinding {
            id: safe_id(&finding.id, &format!("ai.finding.{index}")),
            title: if title.is_empty() {
                "AI 诊断结论".to_string()
            } else {
                title
            },
            severity: Severity::parse(&finding.severity).as_str().to_string(),
            summary,
            confidence: normalize_confidence(&finding.confidence),
            evidence_refs: refs,
            recommendations: normalize_recommendations(finding.recommendations),
            limitations: finding
                .limitations
                .into_iter()
                .map(|item| clamp_text(&item, 240))
                .filter(|item| !item.is_empty())
                .take(5)
                .collect(),
        });
    }

    if findings.len() > MAX_AI_FINDINGS {
        findings.truncate(MAX_AI_FINDINGS);
        limitations.push(format!("仅展示最值得关注的 {MAX_AI_FINDINGS} 个问题。"));
    }
    findings.sort_by(|left, right| {
        Severity::parse(&right.severity)
            .cmp(&Severity::parse(&left.severity))
            .then_with(|| left.id.cmp(&right.id))
    });

    let summary = clamp_text(&raw.summary, 2000);
    if summary.is_empty() && findings.is_empty() && requested_checks.is_empty() {
        return Err("模型未返回可用的诊断结论。".to_string());
    }
    for limitation in raw.limitations {
        let limitation = clamp_text(&limitation, 240);
        if !limitation.is_empty() {
            limitations.push(limitation);
        }
    }
    limitations.dedup();
    limitations.truncate(8);

    Ok(ValidatedAnswer {
        summary,
        findings,
        limitations,
        requested_checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::model::Availability;
    use serde_json::json;

    fn facts() -> Vec<DiagnosticFact> {
        vec![DiagnosticFact {
            id: "memory.usage".into(),
            snapshot_id: "snap-1".into(),
            category: "memory".into(),
            severity: Severity::Warning,
            title: "内存".into(),
            value: json!({ "usagePercent": 92.0 }),
            source: "sysinfo".into(),
            captured_at_unix_ms: 1,
            confidence: 0.9,
            availability: Availability::Available,
            evidence_refs: vec![],
            sensitive: false,
        }]
    }

    #[test]
    fn parses_fenced_json() {
        let raw = parse_answer("```json\n{\"summary\":\"ok\",\"findings\":[]}\n```").unwrap();
        assert_eq!(raw.summary, "ok");
    }

    #[test]
    fn drops_findings_without_valid_evidence() {
        let raw = parse_answer(
            r#"{"summary":"总体","findings":[
                {"id":"bad","title":"无证据","severity":"critical","summary":"x","confidence":"high","evidenceRefs":["missing"]},
                {"id":"good","title":"内存高","severity":"warning","summary":"y","confidence":"high","evidenceRefs":["memory.usage"]}
            ]}"#,
        )
        .unwrap();
        let validated = validate(raw, &facts()).unwrap();
        assert_eq!(validated.findings.len(), 1);
        assert_eq!(validated.findings[0].id, "good");
        assert!(validated.limitations.iter().any(|item| item.contains("缺少有效证据")));
    }

    #[test]
    fn caps_findings_at_three() {
        let findings: Vec<String> = (0..5)
            .map(|index| {
                format!(
                    r#"{{"id":"f{index}","title":"t{index}","severity":"warning","summary":"s","confidence":"high","evidenceRefs":["memory.usage"]}}"#
                )
            })
            .collect();
        let text = format!(
            r#"{{"summary":"总体","findings":[{}]}}"#,
            findings.join(",")
        );
        let raw = parse_answer(&text).unwrap();
        let validated = validate(raw, &facts()).unwrap();
        assert_eq!(validated.findings.len(), 3);
    }

    #[test]
    fn forces_confirmation_for_write_preview() {
        let raw = parse_answer(
            r#"{"summary":"总体","findings":[{"id":"f","title":"t","severity":"info","summary":"s","confidence":"low","evidenceRefs":["memory.usage"],"recommendations":[{"label":"清理","kind":"controlled_write_preview","risk":"low","requiresConfirmation":false}]}]}"#,
        )
        .unwrap();
        let validated = validate(raw, &facts()).unwrap();
        assert!(validated.findings[0].recommendations[0].requires_confirmation);
    }

    #[test]
    fn allows_plan_only_response_and_normalizes_checks() {
        let raw = parse_answer(
            r#"{"summary":"","findings":[],"requestedChecks":[
                {"toolId":"event_log.recent","reason":"查看崩溃","parameters":{"hours":24}},
                {"toolId":"EVENT_LOG.RECENT","reason":"重复","parameters":{}},
                {"toolId":"bad tool!","reason":"x","parameters":{}},
                {"toolId":"drivers.problems","reason":"驱动","parameters":{}},
                {"toolId":"network.summary","reason":"网络","parameters":{}},
                {"toolId":"startup.summary","reason":"第四项","parameters":{}}
            ]}"#,
        )
        .unwrap();
        let validated = validate(raw, &facts()).unwrap();
        assert_eq!(validated.requested_checks.len(), 3);
        assert_eq!(validated.requested_checks[0].tool_id, "event_log.recent");
        assert_eq!(validated.requested_checks[1].tool_id, "drivers.problems");
    }

    #[test]
    fn marks_unreferenced_local_warning_precedence() {
        let raw = parse_answer(
            r#"{"summary":"总体正常","findings":[{"id":"f","title":"t","severity":"info","summary":"s","confidence":"low","evidenceRefs":["memory.usage"]}]}"#,
        )
        .unwrap();
        let mut validated = validate(raw, &facts()).unwrap();
        let local = vec![AiFinding {
            id: "rule.storage".into(),
            title: "存储健康".into(),
            severity: "critical".into(),
            summary: "SMART 异常".into(),
            confidence: "high".into(),
            evidence_refs: vec!["storage.smart.0".into()],
            recommendations: vec![],
            limitations: vec![],
        }];
        enforce_local_priority(&local, &mut validated);
        assert!(validated
            .limitations
            .iter()
            .any(|item| item.contains("本地规则已将")));
    }
}
