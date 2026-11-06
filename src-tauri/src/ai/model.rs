//! Shared data model for the AI diagnostic layer.
//!
//! The model is deliberately independent from any vendor: facts and snapshots
//! are produced locally, providers only see the redacted payload, and answers
//! are validated against the local facts before they reach the UI.

use serde::{Deserialize, Serialize};

pub const AI_SCHEMA_VERSION: u32 = 1;
pub const AI_PROMPT_VERSION: &str = "ai-mvp-1";
pub const MAX_AI_FINDINGS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "critical" | "danger" | "high" => Severity::Critical,
            "warning" | "warn" | "medium" => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Available,
    Partial,
    Unavailable,
}

impl Availability {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Availability::Available => "available",
            Availability::Partial => "partial",
            Availability::Unavailable => "unavailable",
        }
    }
}

/// A single structured diagnostic fact. Facts are the only system data the
/// model is allowed to see.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFact {
    pub id: String,
    pub snapshot_id: String,
    pub category: String,
    pub severity: Severity,
    pub title: String,
    pub value: serde_json::Value,
    pub source: String,
    pub captured_at_unix_ms: u64,
    pub confidence: f64,
    pub availability: Availability,
    pub evidence_refs: Vec<String>,
    pub sensitive: bool,
}

impl DiagnosticFact {
    pub fn is_available(&self) -> bool {
        !matches!(self.availability, Availability::Unavailable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSourceStatus {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiRecommendation {
    pub label: String,
    pub kind: String,
    pub tool_id: Option<String>,
    pub risk: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiFinding {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub summary: String,
    pub confidence: String,
    pub evidence_refs: Vec<String>,
    pub recommendations: Vec<AiRecommendation>,
    pub limitations: Vec<String>,
}

/// Immutable local snapshot used by the rule engine and as the only input to
/// the model. Follow-up questions must reference `snapshot_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub captured_at_unix_ms: u64,
    pub facts: Vec<DiagnosticFact>,
    pub local_findings: Vec<AiFinding>,
    pub limitations: Vec<String>,
    pub sources: Vec<AiSourceStatus>,
    pub excluded_fact_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub max_output_tokens: u32,
    pub timeout_seconds: u32,
    pub native_tools: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            enabled: false,
            provider: "openai-compatible".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-chat".to_string(),
            max_output_tokens: 1200,
            timeout_seconds: 60,
            native_tools: false,
        }
    }
}

impl AiSettings {
    pub fn sanitize(&self) -> AiSettings {
        let mut base_url = self.base_url.trim().trim_end_matches('/').to_string();
        if let Some(stripped) = base_url.strip_suffix("/chat/completions") {
            base_url = stripped.trim_end_matches('/').to_string();
        }
        let base_url = if base_url.is_empty() {
            AiSettings::default().base_url
        } else {
            base_url
        };
        let model = self.model.trim().to_string();
        AiSettings {
            enabled: self.enabled,
            provider: "openai-compatible".to_string(),
            base_url,
            model: if model.is_empty() {
                AiSettings::default().model
            } else {
                model.chars().take(120).collect()
            },
            max_output_tokens: self.max_output_tokens.clamp(128, 4096),
            timeout_seconds: self.timeout_seconds.clamp(10, 180),
            native_tools: self.native_tools,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub settings: AiSettings,
    pub has_api_key: bool,
    pub key_storage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionSummary {
    pub removed_fields: usize,
    pub applied_rules: Vec<String>,
    pub excluded_facts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPayloadPreview {
    pub snapshot_id: String,
    pub base_url: String,
    pub model: String,
    pub payload: serde_json::Value,
    pub estimated_input_tokens: usize,
    pub redaction: RedactionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiRequestedCheck {
    pub tool_id: String,
    pub reason: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiToolRun {
    pub tool_id: String,
    pub label: String,
    pub reason: String,
    pub ok: bool,
    pub duration_ms: u64,
    pub summary: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiToolAuditEntry {
    pub saved_at_unix_ms: u64,
    pub request_id: String,
    pub tool_id: String,
    pub ok: bool,
    pub duration_ms: u64,
    pub reason: String,
    pub parameters: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiCapabilityReport {
    pub reachable: bool,
    pub structured_output: bool,
    pub native_tools: bool,
    pub model: String,
    pub latency_ms: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAnswer {
    pub request_id: String,
    pub snapshot_id: String,
    pub generated_at_unix_ms: u64,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub summary: String,
    pub findings: Vec<AiFinding>,
    pub limitations: Vec<String>,
    pub local_findings: Vec<AiFinding>,
    pub usage: Option<AiUsage>,
    #[serde(default)]
    pub checks: Vec<AiToolRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTurn {
    pub snapshot_id: String,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStreamEvent {
    pub request_id: String,
    pub stage: String,
    pub delta: String,
    pub char_count: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub timeout_seconds: u32,
    pub tools: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub text: String,
    pub model: String,
    pub usage: Option<AiUsage>,
    pub tool_calls: Vec<ToolCall>,
}
