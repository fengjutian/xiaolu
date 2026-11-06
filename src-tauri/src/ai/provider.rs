//! Vendor-neutral AI provider layer.
//!
//! The MVP ships an OpenAI-compatible chat provider (DeepSeek, OpenAI, vLLM,
//! Ollama's OpenAI endpoint, ...). Additional providers implement the same
//! `AiProvider` trait without changing the analysis pipeline.

use super::model::{AiSettings, AiUsage, ProviderRequest, ProviderResponse};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

pub const CANCELLED: &str = "AI_REQUEST_CANCELLED";

pub struct CancelHandle {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancelHandle {
    pub fn new() -> Self {
        CancelHandle {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

impl Default for CancelHandle {
    fn default() -> Self {
        Self::new()
    }
}

pub enum StreamEvent {
    Status(String),
    Delta { text: String, char_count: usize },
}

pub trait AiProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn complete<'a>(
        &'a self,
        request: ProviderRequest,
        api_key: String,
        cancel: Arc<CancelHandle>,
        on_event: Box<dyn Fn(StreamEvent) + Send + 'a>,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, String>> + Send + 'a>>;
}

pub fn provider_for(settings: &AiSettings) -> Result<Box<dyn AiProvider>, String> {
    match settings.provider.as_str() {
        "openai-compatible" => Ok(Box::new(OpenAiCompatible {
            base_url: settings.base_url.trim_end_matches('/').to_string(),
        })),
        other => Err(format!("暂不支持该模型服务类型：{other}")),
    }
}

pub struct OpenAiCompatible {
    base_url: String,
}

#[derive(Default)]
pub(crate) struct SseChunk {
    done: bool,
    content: String,
    model: Option<String>,
    usage: Option<AiUsage>,
}

fn parse_usage(value: &Value) -> Option<AiUsage> {
    let usage = value.get("usage")?;
    Some(AiUsage {
        prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
    })
}

/// Parses a single `data:` payload from an SSE chat completion stream.
pub(crate) fn parse_sse_data(data: &str) -> SseChunk {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return SseChunk::default();
    }
    if trimmed == "[DONE]" {
        return SseChunk {
            done: true,
            ..SseChunk::default()
        };
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return SseChunk::default();
    };
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    SseChunk {
        done: false,
        content,
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        usage: parse_usage(&value),
    }
}

fn classify_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        return "模型服务请求超时，请检查网络或增大超时时间。".to_string();
    }
    if error.is_connect() {
        return "无法连接模型服务，请检查服务地址、网络和代理设置。".to_string();
    }
    format!("模型服务请求失败：{error}")
}

impl AiProvider for OpenAiCompatible {
    fn id(&self) -> &'static str {
        "openai-compatible"
    }

    fn label(&self) -> &'static str {
        "OpenAI 兼容接口"
    }

    fn complete<'a>(
        &'a self,
        request: ProviderRequest,
        api_key: String,
        cancel: Arc<CancelHandle>,
        on_event: Box<dyn Fn(StreamEvent) + Send + 'a>,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, String>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url);
            let timeout = Duration::from_secs(u64::from(request.timeout_seconds.max(10)));
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .user_agent("SystemLens/0.53 (local diagnostic assistant)")
                .build()
                .map_err(|error| format!("无法初始化网络客户端：{error}"))?;
            let body = json!({
                "model": request.model,
                "messages": request.messages.iter().map(|message| json!({
                    "role": message.role,
                    "content": message.content,
                })).collect::<Vec<_>>(),
                "max_tokens": request.max_tokens,
                "temperature": 0.2,
                "stream": true,
                "response_format": { "type": "json_object" },
            });

            on_event(StreamEvent::Status("正在连接模型服务…".to_string()));
            let send = client
                .post(&url)
                .bearer_auth(api_key)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(&body)
                .send();
            let response = tokio::select! {
                _ = cancel.cancelled() => return Err(CANCELLED.to_string()),
                result = send => result.map_err(classify_error)?,
            };
            let status = response.status();
            if !status.is_success() {
                let detail = response.text().await.unwrap_or_default();
                let detail: String = detail.chars().take(400).collect();
                return Err(format!(
                    "模型服务返回 HTTP {}：{}",
                    status.as_u16(),
                    if detail.trim().is_empty() {
                        "无错误详情"
                    } else {
                        detail.trim()
                    }
                ));
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let server_model = request.model.clone();

            if content_type.contains("application/json") {
                on_event(StreamEvent::Status("已连接，正在解析响应…".to_string()));
                let text = tokio::select! {
                    _ = cancel.cancelled() => return Err(CANCELLED.to_string()),
                    result = response.text() => result.map_err(classify_error)?,
                };
                let value: Value = serde_json::from_str(&text)
                    .map_err(|error| format!("无法解析模型响应：{error}"))?;
                let content = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if content.trim().is_empty() {
                    return Err("模型服务返回了空响应。".to_string());
                }
                on_event(StreamEvent::Delta {
                    text: content.clone(),
                    char_count: content.chars().count(),
                });
                return Ok(ProviderResponse {
                    text: content,
                    model: value
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or(&server_model)
                        .to_string(),
                    usage: parse_usage(&value),
                });
            }

            on_event(StreamEvent::Status("已连接，正在接收响应…".to_string()));
            let mut stream = Box::pin(response.bytes_stream());
            let mut buffer: Vec<u8> = Vec::new();
            let mut full = String::new();
            let mut usage: Option<AiUsage> = None;
            let mut model = server_model;
            let mut finished = false;

            while !finished {
                let next = tokio::select! {
                    _ = cancel.cancelled() => return Err(CANCELLED.to_string()),
                    item = stream.next() => item,
                };
                let Some(chunk) = next else {
                    break;
                };
                let bytes = chunk.map_err(classify_error)?;
                buffer.extend_from_slice(&bytes);
                while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line = buffer.drain(..=position).collect::<Vec<u8>>();
                    let line = String::from_utf8_lossy(&line);
                    let Some(data) = line.trim().strip_prefix("data:") else {
                        continue;
                    };
                    let parsed = parse_sse_data(data);
                    if let Some(next_model) = parsed.model {
                        model = next_model;
                    }
                    if let Some(next_usage) = parsed.usage {
                        usage = Some(next_usage);
                    }
                    if parsed.done {
                        finished = true;
                        break;
                    }
                    if !parsed.content.is_empty() {
                        full.push_str(&parsed.content);
                        on_event(StreamEvent::Delta {
                            text: parsed.content,
                            char_count: full.chars().count(),
                        });
                    }
                }
            }

            if full.trim().is_empty() {
                return Err("模型服务返回了空响应。".to_string());
            }
            Ok(ProviderResponse {
                text: full,
                model,
                usage,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_and_usage() {
        let chunk = parse_sse_data(
            r#"{"model":"deepseek-chat","choices":[{"delta":{"content":"你好"}}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}"#,
        );
        assert_eq!(chunk.content, "你好");
        assert_eq!(chunk.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(chunk.usage.as_ref().unwrap().total_tokens, Some(15));
        assert!(!chunk.done);
    }

    #[test]
    fn detects_done_and_ignores_malformed_data() {
        assert!(parse_sse_data("[DONE]").done);
        assert_eq!(parse_sse_data("{not-json}").content, "");
    }

    #[test]
    fn cancel_handle_is_observable() {
        let handle = CancelHandle::new();
        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }
}
