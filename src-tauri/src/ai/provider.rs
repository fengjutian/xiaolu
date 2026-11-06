//! Vendor-neutral AI provider layer.
//!
//! The MVP ships an OpenAI-compatible chat provider (DeepSeek, OpenAI, vLLM,
//! Ollama's OpenAI endpoint, ...). Additional providers implement the same
//! `AiProvider` trait without changing the analysis pipeline.

use super::model::{
    AiSettings, AiUsage, ChatMessage, ProviderRequest, ProviderResponse, ToolCall,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
struct SseChunk {
    done: bool,
    content: String,
    model: Option<String>,
    usage: Option<AiUsage>,
    tool_calls: Vec<ToolCallChunk>,
}

#[derive(Default)]
struct ToolCallChunk {
    index: usize,
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn parse_message_tool_calls(message: &Value) -> Vec<ToolCall> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|call| ToolCall {
                    id: call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: call
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    arguments: call
                        .get("function")
                        .and_then(|function| function.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
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
fn parse_sse_data(data: &str) -> SseChunk {
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
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());
    let content = choice
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tool_calls = choice
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .enumerate()
                .map(|(position, call)| {
                    let function = call.get("function");
                    ToolCallChunk {
                        index: call
                            .get("index")
                            .and_then(Value::as_u64)
                            .map(|index| index as usize)
                            .unwrap_or(position),
                        id: call
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        name: function
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        arguments: function
                            .and_then(|function| function.get("arguments"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    SseChunk {
        done: false,
        content,
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        usage: parse_usage(&value),
        tool_calls,
    }
}

fn message_value(message: &ChatMessage) -> Value {
    let mut value = json!({
        "role": message.role,
        "content": message.content,
    });
    if let Some(tool_calls) = &message.tool_calls {
        value["tool_calls"] = Value::Array(
            tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments,
                        }
                    })
                })
                .collect(),
        );
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        value["tool_call_id"] = Value::String(tool_call_id.clone());
    }
    value
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
                .user_agent("SystemLens/0.55 (local diagnostic assistant)")
                .build()
                .map_err(|error| format!("无法初始化网络客户端：{error}"))?;
            let messages: Vec<Value> = request.messages.iter().map(message_value).collect();
            let mut body = json!({
                "model": request.model,
                "messages": messages,
                "max_tokens": request.max_tokens,
                "temperature": 0.2,
                "stream": true,
            });
            if let Some(tools) = &request.tools {
                body["tools"] = tools.clone();
                body["tool_choice"] = Value::String("auto".to_string());
            } else {
                body["response_format"] = json!({ "type": "json_object" });
            }

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
                let message = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("message"));
                let content = message
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let tool_calls = message.map(parse_message_tool_calls).unwrap_or_default();
                if content.trim().is_empty() && tool_calls.is_empty() {
                    return Err("模型服务返回了空响应。".to_string());
                }
                if !content.is_empty() {
                    on_event(StreamEvent::Delta {
                        text: content.clone(),
                        char_count: content.chars().count(),
                    });
                }
                return Ok(ProviderResponse {
                    text: content,
                    model: value
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or(&server_model)
                        .to_string(),
                    usage: parse_usage(&value),
                    tool_calls,
                });
            }

            on_event(StreamEvent::Status("已连接，正在接收响应…".to_string()));
            let mut stream = Box::pin(response.bytes_stream());
            let mut buffer: Vec<u8> = Vec::new();
            let mut full = String::new();
            let mut usage: Option<AiUsage> = None;
            let mut model = server_model;
            let mut finished = false;
            let mut tool_calls: BTreeMap<usize, ToolCall> = BTreeMap::new();

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
                    for call in parsed.tool_calls {
                        let entry = tool_calls.entry(call.index).or_default();
                        if let Some(id) = call.id.filter(|id| !id.is_empty()) {
                            entry.id = id;
                        }
                        if let Some(name) = call.name.filter(|name| !name.is_empty()) {
                            entry.name = name;
                        }
                        entry.arguments.push_str(&call.arguments);
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

            let tool_calls: Vec<ToolCall> = tool_calls.into_values().collect();
            if full.trim().is_empty() && tool_calls.is_empty() {
                return Err("模型服务返回了空响应。".to_string());
            }
            Ok(ProviderResponse {
                text: full,
                model,
                usage,
                tool_calls,
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
    fn parses_tool_call_deltas() {
        let first = parse_sse_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"event_log_recent","arguments":"{\"hours\":24}"}}]}}]}"#,
        );
        assert_eq!(first.tool_calls.len(), 1);
        assert_eq!(first.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(first.tool_calls[0].name.as_deref(), Some("event_log_recent"));
        assert_eq!(first.tool_calls[0].arguments, "{\"hours\":24}");

        let second = parse_sse_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":24}"}}]}}]}"#,
        );
        assert_eq!(second.tool_calls[0].arguments, ":24}");
        assert!(second.tool_calls[0].id.is_none());
    }

    #[test]
    fn serialises_assistant_tool_calls_and_tool_results() {
        let message = ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "event_log_recent".into(),
                arguments: "{}".into(),
            }]),
            tool_call_id: None,
        };
        let value = message_value(&message);
        assert_eq!(value["tool_calls"][0]["function"]["name"], "event_log_recent");
        assert_eq!(value["tool_calls"][0]["type"], "function");

        let result = message_value(&ChatMessage {
            role: "tool".into(),
            content: "{\"ok\":true}".into(),
            tool_calls: None,
            tool_call_id: Some("call_1".into()),
        });
        assert_eq!(result["tool_call_id"], "call_1");
    }

    #[test]
    fn cancel_handle_is_observable() {
        let handle = CancelHandle::new();
        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }
}
