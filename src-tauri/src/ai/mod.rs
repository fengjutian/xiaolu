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
    AiAnswer, AiPayloadPreview, AiSettings, AiSettingsView, AiStreamEvent, AiToolAuditEntry,
    AiTurn, DiagnosticSnapshot,
};
pub use store::AiSessionRecord;

use self::answer::{enforce_local_priority, parse_answer, validate, ValidatedAnswer};
use self::model::{AiUsage, ChatMessage, ProviderRequest, AI_PROMPT_VERSION};
use self::provider::{provider_for, AiProvider, CancelHandle, StreamEvent, CANCELLED};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

const MAX_CACHED_SNAPSHOTS: usize = 5;
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
                },
                api_key.to_string(),
                cancel.clone(),
                Box::new(emit),
            )
            .await?;
        merge_usage(usage, response.usage.clone());

        match parse_answer(&response.text).and_then(|raw| validate(raw, &snapshot.facts)) {
            Ok(mut validated) => {
                enforce_local_priority(&snapshot.local_findings, &mut validated);
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
    let (messages, _, _, _) = prompt::build_messages(&snapshot, question.as_deref(), &history);
    let first = call_and_validate(
        &app,
        provider.as_ref(),
        &settings,
        &api_key,
        cancel.clone(),
        &request_id,
        messages,
        &snapshot,
        &mut usage,
    )
    .await?;

    let mut checks = Vec::new();
    let turn = if first.validated.requested_checks.is_empty() {
        first
    } else {
        checks = tools::execute_checks(
            &app,
            &request_id,
            &first.validated.requested_checks,
            cancel.clone(),
        )
        .await?;
        let (messages, _, _, _) = prompt::build_messages_with_results(
            &snapshot,
            question.as_deref(),
            &history,
            &checks,
        );
        let mut second = call_and_validate(
            &app,
            provider.as_ref(),
            &settings,
            &api_key,
            cancel.clone(),
            &request_id,
            messages,
            &snapshot,
            &mut usage,
        )
        .await?;
        if !second.validated.requested_checks.is_empty() {
            second
                .validated
                .limitations
                .push("每轮最多执行一组只读检查，后续检查请求已忽略。".to_string());
        }
        for run in checks.iter().filter(|run| !run.ok) {
            second
                .validated
                .limitations
                .push(format!("只读检查“{}”未完成，相关结论请谨慎对待。", run.label));
        }
        second.validated.limitations.dedup();
        second.validated.limitations.truncate(8);
        second
    };

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
}
