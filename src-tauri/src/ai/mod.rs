//! AI diagnostic layer: local facts, rules, provider orchestration and Tauri
//! commands. This module never executes model-generated commands and never
//! exposes write tools.

mod answer;
mod facts;
mod model;
mod prompt;
mod provider;
mod redact;
mod rules;
mod settings;
mod store;
mod tools;

#[cfg(test)]
mod evaluation;

pub use model::{
    AiAnswer, AiCapabilityReport, AiPayloadPreview, AiSettings, AiSettingsView, AiStreamEvent,
    AiToolAuditEntry, AiTurn, DiagnosticSnapshot,
};
pub use store::AiSessionRecord;

use self::answer::{enforce_local_priority, parse_answer, validate, ValidatedAnswer};
use self::model::{
    AiToolRun, AiUsage, ChatMessage, ProviderRequest, ToolCall, AI_PROMPT_VERSION,
};
use self::provider::{provider_for, AiProvider, CancelHandle, StreamEvent, CANCELLED};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

const MAX_CACHED_SNAPSHOTS: usize = 5;
const MAX_TOOL_ROUNDS: usize = 2;
const MAX_TOOL_CHECKS: usize = 6;
const STREAM_EVENT: &str = "ai-stream";

#[derive(Default)]
pub struct AiState {
    snapshots: Mutex<VecDeque<DiagnosticSnapshot>>,
    cancels: Mutex<HashMap<String, Arc<CancelHandle>>>,
}

impl AiState {
    fn store_snapshot(&self, snapshot: DiagnosticSnapshot) {
        let mut snapshots = match self.snapshots.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        snapshots.retain(|item| item.snapshot_id != snapshot.snapshot_id);
        snapshots.push_front(snapshot);
        snapshots.truncate(MAX_CACHED_SNAPSHOTS);
    }

    fn snapshot(&self, snapshot_id: &str) -> Option<DiagnosticSnapshot> {
        let snapshots = match self.snapshots.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        snapshots
            .iter()
            .find(|item| item.snapshot_id == snapshot_id)
            .cloned()
    }

    fn register_cancel(&self, request_id: &str) -> Arc<CancelHandle> {
        let handle = Arc::new(CancelHandle::new());
        let mut cancels = match self.cancels.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        cancels.insert(request_id.to_string(), handle.clone());
        handle
    }

    fn release_cancel(&self, request_id: &str) {
        let mut cancels = match self.cancels.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        cancels.remove(request_id);
    }

    fn cancel(&self, request_id: &str) {
        let cancels = match self.cancels.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(handle) = cancels.get(request_id) {
            handle.cancel();
        }
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_request_id(request_id: &str) -> Result<(), String> {
    if request_id.len() < 6 || request_id.len() > 64 {
        return Err("请求标识长度无效。".to_string());
    }
    if !request_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("请求标识包含非法字符。".to_string());
    }
    Ok(())
}

fn settings_view(app: &AppHandle) -> AiSettingsView {
    AiSettingsView {
        settings: settings::load_settings(app),
        has_api_key: settings::has_api_key(app),
        key_storage: settings::key_storage_label().to_string(),
    }
}

#[tauri::command]
pub async fn get_ai_settings(app: AppHandle) -> Result<AiSettingsView, String> {
    Ok(settings_view(&app))
}

#[tauri::command]
pub async fn save_ai_settings(
    app: AppHandle,
    settings: AiSettings,
    api_key: Option<String>,
) -> Result<AiSettingsView, String> {
    settings::save_settings(&app, &settings)?;
    if let Some(key) = api_key {
        let key = key.trim();
        if key.is_empty() {
            settings::clear_api_key(&app)?;
        } else {
            settings::save_api_key(&app, key)?;
        }
    }
    Ok(settings_view(&app))
}

#[tauri::command]
pub async fn clear_ai_api_key(app: AppHandle) -> Result<AiSettingsView, String> {
    settings::clear_api_key(&app)?;
    Ok(settings_view(&app))
}

/// Collects a fresh immutable snapshot and caches it for follow-up questions.
#[tauri::command]
pub async fn build_ai_snapshot(app: AppHandle) -> Result<DiagnosticSnapshot, String> {
    let app_for_task = app.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || facts::collect_snapshot(&app_for_task))
        .await
        .map_err(|error| format!("诊断快照任务失败：{error}"))?;
    app.state::<AiState>().store_snapshot(snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn preview_ai_payload(
    app: AppHandle,
    snapshot_id: String,
    question: Option<String>,
) -> Result<AiPayloadPreview, String> {
    let ai_state = app.state::<AiState>();
    let snapshot = ai_state
        .snapshot(&snapshot_id)
        .ok_or_else(|| "快照已过期，请重新生成本机健康快照。".to_string())?;
    let settings = settings::load_settings(&app);
    let (messages, redacted_payload, _excluded, redaction) =
        prompt::build_messages(&snapshot, question.as_deref(), &[]);
    Ok(AiPayloadPreview {
        snapshot_id: snapshot.snapshot_id.clone(),
        base_url: settings.base_url,
        model: settings.model,
        payload: redacted_payload,
        estimated_input_tokens: prompt::estimate_tokens(&messages),
        redaction,
    })
}

fn emit_stream(
    app: &AppHandle,
    request_id: &str,
    stage: &str,
    delta: String,
    char_count: usize,
    message: String,
) {
    let _ = app.emit(
        STREAM_EVENT,
        AiStreamEvent {
            request_id: request_id.to_string(),
            stage: stage.to_string(),
            delta,
            char_count,
            message,
        },
    );
}

fn merge_usage(current: &mut Option<AiUsage>, next: Option<AiUsage>) {
    if let Some(next) = next {
        let entry = current.get_or_insert(AiUsage {
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        if next.prompt_tokens.is_some() {
            entry.prompt_tokens = next.prompt_tokens;
        }
        if next.completion_tokens.is_some() {
            entry.completion_tokens = next.completion_tokens;
        }
        if next.total_tokens.is_some() {
            entry.total_tokens = next.total_tokens;
        }
    }
}

fn fallback_answer(
    snapshot: &DiagnosticSnapshot,
    request_id: &str,
    settings: &AiSettings,
    reason: String,
) -> AiAnswer {
    let mut limitations = vec![reason.clone()];
    limitations.extend(snapshot.limitations.iter().take(5).cloned());
    limitations.truncate(8);
    AiAnswer {
        request_id: request_id.to_string(),
        snapshot_id: snapshot.snapshot_id.clone(),
        generated_at_unix_ms: now_unix_ms(),
        provider: settings.provider.clone(),
        model: settings.model.clone(),
        prompt_version: AI_PROMPT_VERSION.to_string(),
        degraded: true,
        degraded_reason: Some(reason),
        summary: "AI 解读暂不可用，已退回本地规则诊断。原始指标、规则结论和报告仍可正常使用。"
            .to_string(),
        findings: Vec::new(),
        limitations,
        local_findings: snapshot.local_findings.clone(),
        usage: None,
        checks: Vec::new(),
    }
}

struct ModelTurn {
    validated: ValidatedAnswer,
    model: String,
}

fn validate_text(text: &str, snapshot: &DiagnosticSnapshot) -> Result<ValidatedAnswer, String> {
    let mut validated = validate(parse_answer(text)?, &snapshot.facts)?;
    enforce_local_priority(&snapshot.local_findings, &mut validated);
    Ok(validated)
}

#[allow(clippy::too_many_arguments)]
async fn call_and_validate(
    app: &AppHandle,
    provider: &dyn AiProvider,
    settings: &AiSettings,
    api_key: &str,
    cancel: Arc<CancelHandle>,
    request_id: &str,
    messages: Vec<ChatMessage>,
    snapshot: &DiagnosticSnapshot,
    usage: &mut Option<AiUsage>,
) -> Result<ModelTurn, String> {
    let mut last_error = String::new();
    for attempt in 0..2 {
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_string());
        }
        if attempt > 0 {
            emit_stream(
                app,
                request_id,
                "status",
                String::new(),
                0,
                "模型输出格式不正确，正在重试一次…".to_string(),
            );
        }
        let mut attempt_messages = messages.clone();
        if attempt > 0 {
            attempt_messages.push(ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "上一次输出无法解析或未通过证据校验（{last_error}）。请只返回一个符合约定的 JSON 对象，并确保 evidenceRefs 只使用诊断数据中存在的 id。"
                ),
                ..Default::default()
            });
        }

        let event_app = app.clone();
        let event_request = request_id.to_string();
        let emit = move |event: StreamEvent| match event {
            StreamEvent::Status(message) => {
                emit_stream(&event_app, &event_request, "status", String::new(), 0, message);
            }
            StreamEvent::Delta { text, char_count } => {
                emit_stream(&event_app, &event_request, "delta", text, char_count, String::new());
            }
        };

        let response = provider
            .complete(
                ProviderRequest {
                    model: settings.model.clone(),
                    messages: attempt_messages,
                    max_tokens: settings.max_output_tokens,
                    timeout_seconds: settings.timeout_seconds,
                    tools: None,
                },
                api_key.to_string(),
                cancel.clone(),
                Box::new(emit),
            )
            .await?;
        merge_usage(usage, response.usage.clone());

        match validate_text(&response.text, snapshot) {
            Ok(validated) => {
                return Ok(ModelTurn {
                    validated,
                    model: response.model,
                });
            }
            Err(error) => last_error = error,
        }
    }
    Err(format!(
        "模型连续两次输出都无法通过校验：{last_error}"
    ))
}

/// Native tool-calling phase: at most two rounds and six checks. The final JSON
/// answer is always produced in a separate call with tools disabled, so a model
/// cannot keep calling tools indefinitely.
#[allow(clippy::too_many_arguments)]
async fn collect_native_checks(
    app: &AppHandle,
    provider: &dyn AiProvider,
    settings: &AiSettings,
    api_key: &str,
    cancel: Arc<CancelHandle>,
    request_id: &str,
    snapshot: &DiagnosticSnapshot,
    question: Option<&str>,
    history: &[AiTurn],
    usage: &mut Option<AiUsage>,
) -> Result<Vec<AiToolRun>, String> {
    let (mut messages, _, _, _) = prompt::build_messages(snapshot, question, history);
    let mut checks = Vec::new();
    let mut rounds = 0;
    while rounds < MAX_TOOL_ROUNDS && checks.len() < MAX_TOOL_CHECKS {
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_string());
        }
        emit_stream(
            app,
            request_id,
            "status",
            String::new(),
            0,
            format!("正在规划只读检查（第 {} 轮）…", rounds + 1),
        );
        let event_app = app.clone();
        let event_request = request_id.to_string();
        let emit = move |event: StreamEvent| match event {
            StreamEvent::Status(message) => {
                emit_stream(&event_app, &event_request, "status", String::new(), 0, message);
            }
            StreamEvent::Delta { text, char_count } => {
                emit_stream(&event_app, &event_request, "delta", text, char_count, String::new());
            }
        };
        let response = provider
            .complete(
                ProviderRequest {
                    model: settings.model.clone(),
                    messages: messages.clone(),
                    max_tokens: settings.max_output_tokens,
                    timeout_seconds: settings.timeout_seconds,
                    tools: Some(tools::openai_tools()),
                },
                api_key.to_string(),
                cancel.clone(),
                Box::new(emit),
            )
            .await?;
        merge_usage(usage, response.usage.clone());
        if response.tool_calls.is_empty() {
            break;
        }
        let remaining = MAX_TOOL_CHECKS.saturating_sub(checks.len());
        let calls: Vec<ToolCall> = response
            .tool_calls
            .into_iter()
            .take(remaining.min(3))
            .collect();
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: response.text.clone(),
            tool_calls: Some(calls.clone()),
            ..Default::default()
        });
        for call in &calls {
            let check = tools::check_from_tool_call(call);
            let runs = tools::execute_checks(app, request_id, &[check], cancel.clone()).await?;
            let run = match runs.into_iter().next() {
                Some(run) => run,
                None => continue,
            };
            let content = serde_json::to_string(&serde_json::json!({
                "toolId": run.tool_id,
                "ok": run.ok,
                "summary": run.summary,
                "error": run.error,
            }))
            .unwrap_or_else(|_| "{\"ok\":false}".to_string());
            messages.push(ChatMessage {
                role: "tool".to_string(),
                content,
                tool_call_id: Some(call.id.clone()),
                ..Default::default()
            });
            checks.push(run);
        }
        rounds += 1;
    }
    Ok(checks)
}

async fn run_model_analysis(
    app: AppHandle,
    settings: AiSettings,
    api_key: String,
    snapshot: DiagnosticSnapshot,
    question: Option<String>,
    history: Vec<AiTurn>,
    request_id: String,
    cancel: Arc<CancelHandle>,
) -> Result<AiAnswer, String> {
    let provider = provider_for(&settings)?;
    let mut usage: Option<AiUsage> = None;
    let mut native_fallback = false;
    let mut checks = if settings.native_tools {
        match collect_native_checks(
            &app,
            provider.as_ref(),
            &settings,
            &api_key,
            cancel.clone(),
            &request_id,
            &snapshot,
            question.as_deref(),
            &history,
            &mut usage,
        )
        .await
        {
            Ok(checks) => checks,
            Err(error) if error == CANCELLED => return Err(error),
            Err(_) => {
                native_fallback = true;
                emit_stream(
                    &app,
                    &request_id,
                    "status",
                    String::new(),
                    0,
                    "原生工具调用不可用，已回退到受控只读检查协议。".to_string(),
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let mut messages = if checks.is_empty() {
        prompt::build_messages(&snapshot, question.as_deref(), &history).0
    } else {
        prompt::build_messages_with_results(&snapshot, question.as_deref(), &history, &checks).0
    };

    let mut legacy_rounds = 0;
    let mut turn = loop {
        let mut turn = call_and_validate(
            &app,
            provider.as_ref(),
            &settings,
            &api_key,
            cancel.clone(),
            &request_id,
            messages.clone(),
            &snapshot,
            &mut usage,
        )
        .await?;

        let requested = std::mem::take(&mut turn.validated.requested_checks);
        if requested.is_empty() {
            break turn;
        }
        if legacy_rounds >= MAX_TOOL_ROUNDS || checks.len() + requested.len() > MAX_TOOL_CHECKS {
            turn.validated
                .limitations
                .push("已达到只读检查上限，后续检查请求已忽略。".to_string());
            turn.validated.limitations.dedup();
            turn.validated.limitations.truncate(8);
            break turn;
        }

        let remaining = MAX_TOOL_CHECKS.saturating_sub(checks.len());
        let requested: Vec<_> = requested.into_iter().take(remaining).collect();
        let runs = tools::execute_checks(&app, &request_id, &requested, cancel.clone()).await?;
        checks.extend(runs);
        messages =
            prompt::build_messages_with_results(&snapshot, question.as_deref(), &history, &checks).0;
        legacy_rounds += 1;
    };

    for run in checks.iter().filter(|run| !run.ok) {
        turn.validated
            .limitations
            .push(format!("只读检查“{}”未完成，相关结论请谨慎对待。", run.label));
    }
    if native_fallback {
        turn.validated
            .limitations
            .push("原生工具调用失败，本次已回退到受控只读检查协议。".to_string());
    }
    turn.validated.limitations.dedup();
    turn.validated.limitations.truncate(8);

    Ok(AiAnswer {
        request_id,
        snapshot_id: snapshot.snapshot_id.clone(),
        generated_at_unix_ms: now_unix_ms(),
        provider: provider.id().to_string(),
        model: turn.model,
        prompt_version: AI_PROMPT_VERSION.to_string(),
        degraded: false,
        degraded_reason: None,
        summary: turn.validated.summary,
        findings: turn.validated.findings,
        limitations: turn.validated.limitations,
        local_findings: snapshot.local_findings.clone(),
        usage,
        checks,
    })
}

#[tauri::command]
pub async fn ai_analyze(
    app: AppHandle,
    request_id: String,
    snapshot_id: String,
    question: Option<String>,
    mut history: Vec<AiTurn>,
) -> Result<AiAnswer, String> {
    validate_request_id(&request_id)?;
    let settings = settings::load_settings(&app);
    if !settings.enabled {
        return Err("AI 解读未启用。请先在 AI 解读页面启用并配置模型服务。".to_string());
    }
    settings::validate_base_url(&settings.base_url)?;
    let api_key = settings::load_api_key(&app)?
        .ok_or_else(|| "尚未保存 API 密钥，请先在 AI 解读页面配置。".to_string())?;
    let snapshot = app
        .state::<AiState>()
        .snapshot(&snapshot_id)
        .ok_or_else(|| "快照已过期，请重新生成本机健康快照后再解读。".to_string())?;

    history.retain(|turn| turn.snapshot_id == snapshot_id);
    let question = question
        .map(|value| value.trim().chars().take(800).collect::<String>())
        .filter(|value| !value.is_empty());
    let question_for_store = question.clone();

    let cancel = app.state::<AiState>().register_cancel(&request_id);
    let result = run_model_analysis(
        app.clone(),
        settings.clone(),
        api_key,
        snapshot.clone(),
        question,
        history,
        request_id.clone(),
        cancel,
    )
    .await;
    app.state::<AiState>().release_cancel(&request_id);

    match result {
        Ok(answer) => {
            let _ = store::append(&app, question_for_store, &answer);
            Ok(answer)
        }
        Err(error) if error == CANCELLED => Err("AI 请求已取消。".to_string()),
        Err(error) => {
            let answer = fallback_answer(&snapshot, &request_id, &settings, error);
            let _ = store::append(&app, question_for_store, &answer);
            Ok(answer)
        }
    }
}

#[tauri::command]
pub async fn get_ai_sessions(app: AppHandle) -> Result<Vec<AiSessionRecord>, String> {
    Ok(store::list(&app))
}

#[tauri::command]
pub async fn delete_ai_session(app: AppHandle, session_id: String) -> Result<(), String> {
    store::delete(&app, &session_id)
}

#[tauri::command]
pub async fn clear_ai_sessions(app: AppHandle) -> Result<(), String> {
    store::clear(&app)
}

#[tauri::command]
pub async fn get_ai_tool_audit(
    app: AppHandle,
    limit: Option<u32>,
) -> Result<Vec<AiToolAuditEntry>, String> {
    tools::read_audit(&app, limit.unwrap_or(50).clamp(1, 200) as usize)
}

#[tauri::command]
pub async fn clear_ai_tool_audit(app: AppHandle) -> Result<(), String> {
    tools::clear_audit(&app)
}

fn is_json_object_text(text: &str) -> bool {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim()
        .trim_end_matches("```")
        .trim();
    serde_json::from_str::<serde_json::Value>(unfenced)
        .map(|value| value.is_object())
        .unwrap_or(false)
}

async fn probe_request(
    provider: &dyn AiProvider,
    settings: &AiSettings,
    api_key: &str,
    messages: Vec<ChatMessage>,
    tools: Option<serde_json::Value>,
) -> Result<self::model::ProviderResponse, String> {
    let cancel = Arc::new(CancelHandle::new());
    provider
        .complete(
            ProviderRequest {
                model: settings.model.clone(),
                messages,
                max_tokens: 64,
                timeout_seconds: settings.timeout_seconds.min(30),
                tools,
            },
            api_key.to_string(),
            cancel,
            Box::new(|_| {}),
        )
        .await
}

#[tauri::command]
pub async fn probe_ai_capabilities(app: AppHandle) -> Result<AiCapabilityReport, String> {
    let settings = settings::load_settings(&app);
    settings::validate_base_url(&settings.base_url)?;
    let api_key = settings::load_api_key(&app)?
        .ok_or_else(|| "尚未保存 API 密钥，请先配置模型服务。".to_string())?;
    let provider = provider_for(&settings)?;
    let started = std::time::Instant::now();
    let mut model = settings.model.clone();
    let mut reachable = false;
    let mut notes = Vec::new();

    let native_messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "你是模型能力探测助手。".to_string(),
            ..Default::default()
        },
        ChatMessage {
            role: "user".to_string(),
            content: "请调用 probe_ai_ping 工具，参数为 {\"ok\": true}。不要输出普通文本。"
                .to_string(),
            ..Default::default()
        },
    ];
    let native_tools = serde_json::json!([{
        "type": "function",
        "function": {
            "name": "probe_ai_ping",
            "description": "能力探测：原样返回 ok。",
            "parameters": {
                "type": "object",
                "properties": { "ok": { "type": "boolean" } },
                "required": ["ok"],
                "additionalProperties": false
            }
        }
    }]);
    let native_tools_supported =
        match probe_request(provider.as_ref(), &settings, &api_key, native_messages, Some(native_tools))
            .await
        {
            Ok(response) => {
                reachable = true;
                if !response.model.is_empty() {
                    model = response.model;
                }
                response
                    .tool_calls
                    .iter()
                    .any(|call| call.name == "probe_ai_ping")
            }
            Err(error) => {
                notes.push(format!("原生工具调用未通过：{}", error.chars().take(180).collect::<String>()));
                false
            }
        };

    let structured_messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "你是模型能力探测助手。".to_string(),
            ..Default::default()
        },
        ChatMessage {
            role: "user".to_string(),
            content: "只输出一个 JSON 对象：{\"ok\":true}。不要输出 Markdown 或额外文字。"
                .to_string(),
            ..Default::default()
        },
    ];
    let structured_output =
        match probe_request(provider.as_ref(), &settings, &api_key, structured_messages, None).await {
            Ok(response) => {
                reachable = true;
                if !response.model.is_empty() {
                    model = response.model;
                }
                is_json_object_text(&response.text)
            }
            Err(error) => {
                notes.push(format!(
                    "结构化输出未通过：{}",
                    error.chars().take(180).collect::<String>()
                ));
                false
            }
        };

    if !reachable {
        return Err(format!(
            "模型服务不可达：{}",
            if notes.is_empty() {
                "未获得有效响应".to_string()
            } else {
                notes.join("；")
            }
        ));
    }
    if !native_tools_supported {
        notes.push("原生 tool_calls 不可用或未返回工具调用，将使用受控 requestedChecks 计划协议。".to_string());
    }
    Ok(AiCapabilityReport {
        reachable,
        structured_output,
        native_tools: native_tools_supported,
        model,
        latency_ms: started.elapsed().as_millis() as u64,
        note: notes.join(" "),
    })
}

#[tauri::command]
pub async fn cancel_ai_request(app: AppHandle, request_id: String) -> Result<(), String> {
    validate_request_id(&request_id)?;
    app.state::<AiState>().cancel(&request_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_request_ids() {
        assert!(validate_request_id("req-123456").is_ok());
        assert!(validate_request_id("bad id").is_err());
        assert!(validate_request_id("../etc").is_err());
        assert!(validate_request_id("short").is_err());
    }

    #[test]
    fn detects_structured_probe_output() {
        assert!(is_json_object_text("{\"ok\":true}"));
        assert!(is_json_object_text("```json\n{\"ok\":true}\n```"));
        assert!(!is_json_object_text("OK"));
        assert!(!is_json_object_text("[1,2,3]"));
    }
}
