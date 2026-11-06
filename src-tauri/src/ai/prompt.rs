//! Prompt construction and payload assembly.
//!
//! The payload contains only structured facts. Host names, user names, serial
//! numbers, MAC/IP addresses, command lines and full paths never enter it.

use super::model::{
    AiToolRun, AiTurn, ChatMessage, DiagnosticSnapshot, RedactionSummary, AI_PROMPT_VERSION,
};
use super::redact::{filter_facts_for_model, redact_payload};
use super::tools::catalog_prompt;
use serde_json::{json, Value};

pub fn system_prompt() -> String {
    format!(
        r#"你是“小鲁 · System Lens”的本地系统健康解读助手。当前提示词版本：{AI_PROMPT_VERSION}。

你的职责只有解释和编排只读检查，不执行任何命令或系统写操作。严格遵守：
1. 只能使用“诊断数据”中 facts 提供的信息。不得编造温度、SMART 数值、驱动版本、硬件寿命或修复结果。
2. 每个确定性问题必须给出 evidenceRefs，且只能引用 facts 里真实存在的 id；没有证据就不要下结论。
3. 最多输出 3 个 findings，按严重度从高到低排列；数据缺失或互相冲突时写进 limitations，并把 confidence 设为 low。
4. 不能输出 shell、PowerShell、注册表脚本、下载地址或可直接执行的代码。
5. 不覆盖本地规则引擎的严重度判定；本地规则判定为 critical/warning 的事项不得描述为“正常”。
6. 只输出一个 JSON 对象，不要 Markdown、代码围栏或额外文字。
7. 如需更多只读证据，可以在 requestedChecks 中请求宿主执行只读检查（最多 3 项）。只能使用下面列出的工具 ID 和参数，不得请求命令、脚本、文件路径或任何写操作。宿主执行后会把结果再次提供给你；不得伪造工具结果。
8. 工具结果只是一轮补充证据。收到结果后的下一次输出必须是最终解读，不得再次请求检查。

可用只读检查（白名单）：
{catalog}

JSON 结构：
{{
  "summary": "一段不超过 400 字的总体解读",
  "findings": [
    {{
      "id": "英文短 id",
      "title": "问题标题",
      "severity": "info|warning|critical",
      "summary": "解释、影响和数据依据",
      "confidence": "low|medium|high",
      "evidenceRefs": ["facts 中存在的 id"],
      "recommendations": [
        {{"label": "建议", "kind": "read_only_check|manual_action|controlled_write_preview", "risk": "none|low|medium|high", "requiresConfirmation": false}}
      ],
      "limitations": ["该结论的限制"]
    }}
  ],
  "requestedChecks": [
    {{"toolId": "白名单中的工具 ID", "reason": "为什么需要这项检查", "parameters": {{}}}}
  ],
  "limitations": ["整体数据限制"]
}}"#,
        catalog = catalog_prompt()
    )
}

pub fn build_payload(
    snapshot: &DiagnosticSnapshot,
    question: Option<&str>,
    history: &[AiTurn],
) -> (Value, usize) {
    let (mut facts, excluded) = filter_facts_for_model(snapshot.facts.iter());
    facts.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.id.cmp(&right.id))
    });
    facts.truncate(60);

    let fact_values: Vec<Value> = facts
        .iter()
        .map(|fact| {
            json!({
                "id": fact.id,
                "category": fact.category,
                "severity": fact.severity,
                "title": fact.title,
                "value": fact.value,
                "source": fact.source,
                "capturedAtUnixMs": fact.captured_at_unix_ms,
                "confidence": (fact.confidence * 100.0).round() / 100.0,
                "availability": fact.availability,
            })
        })
        .collect();

    let local_findings: Vec<Value> = snapshot
        .local_findings
        .iter()
        .map(|finding| {
            json!({
                "id": finding.id,
                "title": finding.title,
                "severity": finding.severity,
                "summary": finding.summary,
                "confidence": finding.confidence,
                "evidenceRefs": finding.evidence_refs,
            })
        })
        .collect();

    let mut selected: Vec<&AiTurn> = history
        .iter()
        .filter(|turn| turn.snapshot_id == snapshot.snapshot_id)
        .filter(|turn| turn.role == "user" || turn.role == "assistant")
        .collect();
    if selected.len() > 6 {
        selected = selected.split_off(selected.len() - 6);
    }
    let safe_history: Vec<Value> = selected
        .into_iter()
        .map(|turn| {
            json!({
                "role": turn.role,
                "content": turn.content.chars().take(1200).collect::<String>(),
            })
        })
        .collect();

    let payload = json!({
        "schemaVersion": snapshot.schema_version,
        "promptVersion": AI_PROMPT_VERSION,
        "snapshotId": snapshot.snapshot_id,
        "capturedAtUnixMs": snapshot.captured_at_unix_ms,
        "facts": fact_values,
        "localRuleFindings": local_findings,
        "limitations": snapshot.limitations.iter().take(20).collect::<Vec<_>>(),
        "sources": snapshot.sources.iter().map(|source| json!({
            "id": source.id,
            "label": source.label,
            "ok": source.ok,
        })).collect::<Vec<_>>(),
        "question": question.map(|value| value.chars().take(800).collect::<String>()),
        "history": safe_history,
    });
    (payload, excluded)
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn build_messages_inner(
    snapshot: &DiagnosticSnapshot,
    question: Option<&str>,
    history: &[AiTurn],
    tool_results: Option<&[AiToolRun]>,
) -> (Vec<ChatMessage>, Value, usize, RedactionSummary) {
    let (mut payload, excluded) = build_payload(snapshot, question, history);
    if let Some(results) = tool_results {
        if let Ok(value) = serde_json::to_value(results) {
            payload["toolResults"] = value;
        }
    }
    let (redacted, mut redaction) = redact_payload(&payload);
    redaction.excluded_facts = excluded + snapshot.excluded_fact_count;
    let question_text = question
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| format!("\n用户追问：{value}"))
        .unwrap_or_default();
    let instruction = if tool_results.is_some() {
        "下面是宿主已经执行的只读检查结果。不要再请求任何检查，只基于 facts 与 toolResults 输出最终 JSON 解读；工具失败时在 limitations 中说明。"
    } else {
        "请基于下面的诊断事实输出 JSON 健康解读。仅引用 facts 中存在的 id；数据不足时明确说明限制；如确实需要更多证据，可按白名单请求只读检查。"
    };
    let user = format!(
        "{instruction}{question_text}\n\n<diagnostic_data>\n{}\n</diagnostic_data>",
        compact_json(&redacted)
    );
    (
        vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt(),
                ..Default::default()
            },
            ChatMessage {
                role: "user".to_string(),
                content: user,
                ..Default::default()
            },
        ],
        redacted,
        excluded,
        redaction,
    )
}

pub fn build_messages(
    snapshot: &DiagnosticSnapshot,
    question: Option<&str>,
    history: &[AiTurn],
) -> (Vec<ChatMessage>, Value, usize, RedactionSummary) {
    build_messages_inner(snapshot, question, history, None)
}

pub fn build_messages_with_results(
    snapshot: &DiagnosticSnapshot,
    question: Option<&str>,
    history: &[AiTurn],
    results: &[AiToolRun],
) -> (Vec<ChatMessage>, Value, usize, RedactionSummary) {
    build_messages_inner(snapshot, question, history, Some(results))
}

pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    let characters: usize = messages.iter().map(|message| message.content.chars().count()).sum();
    ((characters as f64) / 2.2).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::model::{
        AiSourceStatus, Availability, DiagnosticFact, DiagnosticSnapshot, Severity,
        AI_SCHEMA_VERSION,
    };

    fn snapshot() -> DiagnosticSnapshot {
        let fact = DiagnosticFact {
            id: "memory.usage".into(),
            snapshot_id: "snap-1".into(),
            category: "memory".into(),
            severity: Severity::Warning,
            title: "内存当前使用率".into(),
            value: json!({ "usagePercent": 91.0 }),
            source: "sysinfo".into(),
            captured_at_unix_ms: 1,
            confidence: 0.9,
            availability: Availability::Available,
            evidence_refs: vec![],
            sensitive: false,
        };
        DiagnosticSnapshot {
            schema_version: AI_SCHEMA_VERSION,
            snapshot_id: "snap-1".into(),
            captured_at_unix_ms: 1,
            facts: vec![fact],
            local_findings: vec![],
            limitations: vec!["无温度传感器".into()],
            sources: vec![AiSourceStatus {
                id: "system".into(),
                label: "系统".into(),
                ok: true,
                detail: "ok".into(),
            }],
            excluded_fact_count: 0,
        }
    }

    #[test]
    fn payload_contains_facts_and_question() {
        let (payload, excluded) = build_payload(&snapshot(), Some("为什么变慢？"), &[]);
        assert_eq!(excluded, 0);
        assert_eq!(payload["snapshotId"], "snap-1");
        assert_eq!(payload["facts"][0]["id"], "memory.usage");
        assert_eq!(payload["question"], "为什么变慢？");
        assert!(payload.get("hostName").is_none());
    }

    #[test]
    fn history_from_other_snapshot_is_dropped() {
        let history = vec![
            AiTurn {
                snapshot_id: "snap-old".into(),
                role: "user".into(),
                content: "旧问题".into(),
            },
            AiTurn {
                snapshot_id: "snap-1".into(),
                role: "user".into(),
                content: "新问题".into(),
            },
        ];
        let (payload, _) = build_payload(&snapshot(), None, &history);
        let items = payload["history"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["content"], "新问题");
    }

    #[test]
    fn tool_results_are_embedded_without_raw_paths() {
        let results = vec![AiToolRun {
            tool_id: "event_log.recent".into(),
            label: "最近关键事件".into(),
            reason: "查看崩溃".into(),
            ok: true,
            duration_ms: 42,
            summary: json!({ "total": 2, "path": "C:\\Users\\alice\\x.log" }),
            error: None,
        }];
        let (messages, payload, _, _) = build_messages_with_results(&snapshot(), None, &[], &results);
        assert_eq!(payload["toolResults"][0]["toolId"], "event_log.recent");
        assert_eq!(
            payload["toolResults"][0]["summary"]["path"],
            "[本地路径已脱敏]"
        );
        assert!(messages[1].content.contains("只读检查结果"));
    }
}
