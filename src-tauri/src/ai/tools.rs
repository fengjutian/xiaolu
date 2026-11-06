//! Read-only tool gateway for AI-guided troubleshooting.
//!
//! The model can only name a tool id from a fixed catalogue. The host validates
//! parameters, runs the collector with a timeout, redacts the result, caps its
//! size and writes a metadata-only audit line. There is no path, command line,
//! script, registry or write capability in this gateway.

use super::model::{AiRequestedCheck, AiToolAuditEntry, AiToolRun, ToolCall};
use super::provider::{CancelHandle, CANCELLED};
use super::redact::redact_payload;
use crate::{diagnostics, drivers, event_log, network_diagnostics, scheduled_tasks, sensors, system};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const MAX_CHECKS: usize = 3;
const MAX_RESULT_BYTES: usize = 6000;
static AUDIT_LOCK: Mutex<()> = Mutex::new(());

pub struct ToolSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub timeout_ms: u64,
}

pub const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        id: "event_log.recent",
        label: "最近关键事件",
        description: "查询最近 1-7 天 System/Application 日志中的 critical/error/warning 事件；只返回来源、事件 ID、级别和时间，不返回日志全文。参数：logName=System|Application，level=critical|error|warning|all，hours=1-168。",
        timeout_ms: 30_000,
    },
    ToolSpec {
        id: "drivers.problems",
        label: "驱动问题清单",
        description: "读取设备驱动清单，返回总数、缺失驱动数、异常设备数和最多 3 个问题设备名称；不返回设备 ID。",
        timeout_ms: 20_000,
    },
    ToolSpec {
        id: "network.summary",
        label: "网络诊断摘要",
        description: "读取网关、DNS、代理、Hosts 和活动连接统计；不返回 IP、DNS 服务器地址或远程地址。",
        timeout_ms: 20_000,
    },
    ToolSpec {
        id: "startup.summary",
        label: "启动项摘要",
        description: "读取当前用户与所有用户启动项的名称、范围和启用状态；不返回注册表命令或可执行路径。",
        timeout_ms: 5_000,
    },
    ToolSpec {
        id: "scheduled_tasks.summary",
        label: "计划任务摘要",
        description: "读取计划任务的总数、启用/运行状态和最近失败任务名称；不返回任务路径或参数。",
        timeout_ms: 30_000,
    },
    ToolSpec {
        id: "sensors.storage",
        label: "温度与存储健康",
        description: "读取当前温度、本地告警和 SMART 存储健康摘要；只返回温度值、健康状态、磨损/温度/错误计数，不返回序列号、设备实例名或原始 SMART 属性表。",
        timeout_ms: 25_000,
    },
    ToolSpec {
        id: "diagnostics.summary",
        label: "系统诊断摘要",
        description: "读取待重启、崩溃/转储、服务异常和电池健康摘要；不返回事件日志全文或转储内容。",
        timeout_ms: 40_000,
    },
];

pub fn find_tool(tool_id: &str) -> Option<&'static ToolSpec> {
    TOOL_SPECS.iter().find(|spec| spec.id == tool_id)
}

pub fn catalog_prompt() -> String {
    let mut lines = Vec::new();
    for spec in TOOL_SPECS {
        lines.push(format!("- {}（只读）：{}", spec.id, spec.description));
    }
    lines.join("\n")
}

/// OpenAI function names cannot contain dots, so the catalogue exposes a
/// stable underscore alias while the host keeps the dotted tool id.
pub fn function_name(tool_id: &str) -> String {
    tool_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub fn tool_id_from_function_name(name: &str) -> Option<&'static str> {
    TOOL_SPECS
        .iter()
        .find(|spec| function_name(spec.id) == name)
        .map(|spec| spec.id)
}

fn parameters_schema(tool_id: &str) -> Value {
    match tool_id {
        "event_log.recent" => json!({
            "type": "object",
            "properties": {
                "logName": { "type": "string", "enum": ["System", "Application"] },
                "level": { "type": "string", "enum": ["critical", "error", "warning", "all"] },
                "hours": { "type": "integer", "minimum": 1, "maximum": 168 }
            },
            "additionalProperties": false
        }),
        _ => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

pub fn openai_tools() -> Value {
    Value::Array(
        TOOL_SPECS
            .iter()
            .map(|spec| {
                json!({
                    "type": "function",
                    "function": {
                        "name": function_name(spec.id),
                        "description": spec.description,
                        "parameters": parameters_schema(spec.id),
                    }
                })
            })
            .collect(),
    )
}

pub fn check_from_tool_call(call: &ToolCall) -> AiRequestedCheck {
    let tool_id = tool_id_from_function_name(&call.name)
        .map(str::to_string)
        .unwrap_or_else(|| call.name.clone());
    let parameters = serde_json::from_str::<Value>(&call.arguments)
        .unwrap_or_else(|_| json!({ "__invalidArguments": call.arguments }));
    AiRequestedCheck {
        tool_id,
        reason: "模型原生工具调用".to_string(),
        parameters,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct EventLogParams {
    log_name: String,
    level: String,
    hours: u32,
}

impl Default for EventLogParams {
    fn default() -> Self {
        EventLogParams {
            log_name: "System".into(),
            level: "error".into(),
            hours: 24,
        }
    }
}

impl EventLogParams {
    fn normalized(self) -> EventLogParams {
        let log_name = match self.log_name.trim() {
            "Application" | "application" => "Application",
            _ => "System",
        };
        let level = match self.level.trim().to_ascii_lowercase().as_str() {
            "critical" => "critical",
            "warning" => "warning",
            "all" => "all",
            _ => "error",
        };
        EventLogParams {
            log_name: log_name.to_string(),
            level: level.to_string(),
            hours: self.hours.clamp(1, 168),
        }
    }
}

fn parse_event_params(parameters: &Value) -> Result<EventLogParams, String> {
    if parameters.is_null() {
        return Ok(EventLogParams::default());
    }
    serde_json::from_value::<EventLogParams>(parameters.clone())
        .map(EventLogParams::normalized)
        .map_err(|error| format!("事件日志检查参数不合法：{error}"))
}

fn truncate_text(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

async fn blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("只读检查任务失败：{error}"))?
}

async fn run_limited<F, T>(
    spec: &ToolSpec,
    cancel: &Arc<CancelHandle>,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::select! {
        _ = cancel.cancelled() => Err(CANCELLED.to_string()),
        outcome = tokio::time::timeout(Duration::from_millis(spec.timeout_ms), future) => {
            match outcome {
                Ok(result) => result,
                Err(_) => Err(format!("只读检查超时（{} 秒）", spec.timeout_ms / 1000)),
            }
        }
    }
}

fn cap_summary(value: Value) -> Value {
    let (redacted, _) = redact_payload(&value);
    match serde_json::to_string(&redacted) {
        Ok(encoded) if encoded.len() <= MAX_RESULT_BYTES => redacted,
        Ok(encoded) => json!({
            "truncated": true,
            "originalBytes": encoded.len(),
            "note": "只读检查结果过大，已截断；请基于已有事实和摘要回答。",
        }),
        Err(_) => Value::Null,
    }
}

fn summarize_event_log(result: Value) -> Value {
    let events: Vec<Value> = result
        .get("events")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(10)
                .map(|event| {
                    json!({
                        "timeCreatedUnixMs": event.get("timeCreatedUnixMs").and_then(Value::as_u64),
                        "provider": event.get("provider").and_then(Value::as_str).map(|value| truncate_text(value, 80)),
                        "eventId": event.get("eventId").and_then(Value::as_u64),
                        "level": event.get("level").and_then(Value::as_str).map(|value| truncate_text(value, 20)),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "logName": result.get("logName").and_then(Value::as_str),
        "hours": result.get("hours").and_then(Value::as_u64),
        "total": result.get("total").and_then(Value::as_u64),
        "truncated": result.get("truncated").and_then(Value::as_bool),
        "events": events,
        "note": "仅返回事件元数据，未发送日志全文。",
    })
}

fn summarize_drivers(inventory: Value) -> Value {
    let problems: Vec<String> = inventory
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("missingDriver").and_then(Value::as_bool).unwrap_or(false)
                        || item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(|status| !status.eq_ignore_ascii_case("OK"))
                            .unwrap_or(false)
                })
                .take(3)
                .filter_map(|item| item.get("deviceName").and_then(Value::as_str))
                .map(|name| truncate_text(name, 80))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "total": inventory.get("total").and_then(Value::as_u64),
        "missing": inventory.get("missing").and_then(Value::as_u64),
        "problems": inventory.get("problems").and_then(Value::as_u64),
        "unsigned": inventory.get("unsigned").and_then(Value::as_u64),
        "problemDevices": problems,
    })
}

fn summarize_network(report: Value) -> Value {
    let dns_aliases = report
        .get("dns")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let dns_servers: usize = report
        .get("dns")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("servers").and_then(Value::as_array))
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0);
    let mut states: BTreeMap<String, usize> = BTreeMap::new();
    for connection in report
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let state = connection
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *states.entry(state).or_insert(0) += 1;
    }
    json!({
        "gatewayCount": report.get("defaultGateways").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "dnsAliasCount": dns_aliases,
        "dnsServerCount": dns_servers,
        "proxyEnabled": report.get("proxyEnabled").and_then(Value::as_bool),
        "autoConfigEnabled": report.get("autoConfigUrl").and_then(Value::as_str).map(|value| !value.is_empty()).unwrap_or(false),
        "hostsCustomEntries": report.get("hostsCustomEntries").and_then(Value::as_u64),
        "connectionStates": states,
    })
}

fn summarize_startup(items: Value) -> Value {
    let list = items.as_array().cloned().unwrap_or_default();
    let enabled = list
        .iter()
        .filter(|item| item.get("enabled").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let sample: Vec<Value> = list
        .iter()
        .take(12)
        .map(|item| {
            json!({
                "name": item.get("name").and_then(Value::as_str).map(|value| truncate_text(value, 80)),
                "scope": item.get("scope").and_then(Value::as_str),
                "enabled": item.get("enabled").and_then(Value::as_bool),
            })
        })
        .collect();
    json!({
        "total": list.len(),
        "enabled": enabled,
        "disabled": list.len().saturating_sub(enabled),
        "items": sample,
    })
}

fn summarize_scheduled_tasks(inventory: Value) -> Value {
    let failing: Vec<Value> = inventory
        .get("tasks")
        .and_then(Value::as_array)
        .map(|tasks| {
            tasks
                .iter()
                .filter(|task| {
                    task.get("lastResult")
                        .and_then(Value::as_i64)
                        .map(|result| result != 0)
                        .unwrap_or(false)
                })
                .take(6)
                .map(|task| {
                    json!({
                        "name": task.get("name").and_then(Value::as_str).map(|value| truncate_text(value, 100)),
                        "state": task.get("state").and_then(Value::as_str),
                        "lastResult": task.get("lastResult").and_then(Value::as_i64),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "total": inventory.get("total").and_then(Value::as_u64),
        "enabled": inventory.get("enabled").and_then(Value::as_u64),
        "disabled": inventory.get("disabled").and_then(Value::as_u64),
        "running": inventory.get("running").and_then(Value::as_u64),
        "failingTasks": failing,
    })
}

fn summarize_sensors(snapshot: Value) -> Value {
    let temperatures: Vec<Value> = snapshot
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter(|value| {
                    value.get("category").and_then(Value::as_str) == Some("temperature")
                        && value.get("availability").and_then(Value::as_str) == Some("ok")
                })
                .take(12)
                .map(|value| {
                    json!({
                        "hardwareClass": value.get("hardwareClass").and_then(Value::as_str),
                        "name": value.get("name").and_then(Value::as_str).map(|text| truncate_text(text, 60)),
                        "valueC": value.get("value").and_then(Value::as_f64),
                        "source": value.get("source").and_then(Value::as_str).map(|text| truncate_text(text, 60)),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let storage: Vec<Value> = snapshot
        .get("storage")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(8)
                .map(|item| {
                    json!({
                        "name": item.get("name").and_then(Value::as_str).map(|text| truncate_text(text, 80)),
                        "healthStatus": item.get("healthStatus").and_then(Value::as_str),
                        "temperatureC": item.get("temperatureC").and_then(Value::as_f64),
                        "wearPercent": item.get("wearPercent").and_then(Value::as_f64),
                        "powerOnHours": item.get("powerOnHours").and_then(Value::as_u64),
                        "readErrorsTotal": item.get("readErrorsTotal").and_then(Value::as_u64),
                        "writeErrorsTotal": item.get("writeErrorsTotal").and_then(Value::as_u64),
                        "signals": item.get("assessment").and_then(|assessment| assessment.get("signals")).and_then(Value::as_array).map(|signals| signals.iter().take(6).filter_map(|signal| signal.get("code").and_then(Value::as_str)).collect::<Vec<_>>()).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let predict_failure_count = snapshot
        .get("smartDevices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| device.get("predictFailure").and_then(Value::as_bool) == Some(true))
                .count()
        })
        .unwrap_or(0);

    json!({
        "temperatures": temperatures,
        "alerts": snapshot.get("alerts").and_then(Value::as_array).map(|alerts| alerts.iter().take(6).map(|alert| json!({
            "level": alert.get("level").and_then(Value::as_str),
            "sensorName": alert.get("sensorName").and_then(Value::as_str).map(|text| truncate_text(text, 60)),
            "value": alert.get("value").and_then(Value::as_f64),
            "threshold": alert.get("threshold").and_then(Value::as_f64),
        })).collect::<Vec<_>>()).unwrap_or_default(),
        "storage": storage,
        "predictFailureCount": predict_failure_count,
        "elevated": snapshot.get("elevated").and_then(Value::as_bool),
        "limitationCount": snapshot.get("limitations").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
    })
}

fn summarize_diagnostics(report: Value) -> Value {
    json!({
        "pendingReboot": report.get("pendingReboot").and_then(Value::as_bool),
        "pendingWindowsUpdateReboot": report.get("pendingWindowsUpdateReboot").and_then(Value::as_bool),
        "crashEventCount": report.get("crashEvents").and_then(Value::as_array).map(Vec::len),
        "minidumpCount": report.get("minidumpCount").and_then(Value::as_u64),
        "serviceIssueCount": report.get("serviceIssues").and_then(Value::as_array).map(Vec::len),
        "batteryPresent": report.get("battery").and_then(|battery| battery.get("present")).and_then(Value::as_bool),
        "batteryHealthPercent": report.get("battery").and_then(|battery| battery.get("healthPercent")).and_then(Value::as_f64),
        "batteryCycleCount": report.get("battery").and_then(|battery| battery.get("cycleCount")).and_then(Value::as_u64),
    })
}

async fn run_spec(
    app: &AppHandle,
    spec: &ToolSpec,
    parameters: &Value,
    cancel: &Arc<CancelHandle>,
) -> Result<Value, String> {
    match spec.id {
        "event_log.recent" => {
            let params = parse_event_params(parameters)?;
            let result = run_limited(spec, cancel, async move {
                blocking(move || {
                    event_log::query(&params.log_name, &params.level, params.hours, "", 20)
                })
                .await
            })
            .await?;
            let value = serde_json::to_value(&result)
                .map_err(|error| format!("无法序列化事件日志：{error}"))?;
            Ok(summarize_event_log(value))
        }
        "drivers.problems" => {
            let inventory = run_limited(spec, cancel, async {
                blocking(|| {
                    drivers::get_driver_inventory()
                        .and_then(|inventory| {
                            serde_json::to_value(&inventory)
                                .map_err(|error| format!("无法序列化驱动清单：{error}"))
                        })
                })
                .await
            })
            .await?;
            Ok(summarize_drivers(inventory))
        }
        "network.summary" => {
            let report = run_limited(spec, cancel, async {
                blocking(network_diagnostics::collect).await
            })
            .await?;
            let value = serde_json::to_value(&report)
                .map_err(|error| format!("无法序列化网络诊断：{error}"))?;
            Ok(summarize_network(value))
        }
        "startup.summary" => {
            let items = run_limited(spec, cancel, async {
                blocking(system::startup_items_json).await
            })
            .await?;
            Ok(summarize_startup(items))
        }
        "scheduled_tasks.summary" => {
            let inventory = run_limited(spec, cancel, async {
                blocking(|| {
                    scheduled_tasks::collect().and_then(|inventory| {
                        serde_json::to_value(&inventory)
                            .map_err(|error| format!("无法序列化计划任务：{error}"))
                    })
                })
                .await
            })
            .await?;
            Ok(summarize_scheduled_tasks(inventory))
        }
        "sensors.storage" => {
            let app = app.clone();
            let snapshot = run_limited(spec, cancel, async move {
                blocking(move || sensors::snapshot_blocking(&app)).await
            })
            .await?;
            let value = serde_json::to_value(&snapshot)
                .map_err(|error| format!("无法序列化温度与存储健康：{error}"))?;
            Ok(summarize_sensors(value))
        }
        "diagnostics.summary" => {
            let report = run_limited(spec, cancel, async {
                blocking(diagnostics::collect_report).await
            })
            .await?;
            let value = serde_json::to_value(&report)
                .map_err(|error| format!("无法序列化系统诊断：{error}"))?;
            Ok(summarize_diagnostics(value))
        }
        other => Err(format!("未授权的只读检查：{other}")),
    }
}

fn audit_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join("ai-tool-audit.jsonl"))
}

fn audit(app: &AppHandle, request_id: &str, run: &AiToolRun, parameters: &Value) {
    let Some(path) = audit_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _guard = match AUDIT_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let (redacted_params, _) = redact_payload(parameters);
    let line = json!({
        "savedAtUnixMs": super::now_unix_ms(),
        "requestId": request_id,
        "toolId": run.tool_id,
        "ok": run.ok,
        "durationMs": run.duration_ms,
        "reason": run.reason,
        "parameters": redacted_params,
        "error": run.error,
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

pub fn read_audit(app: &AppHandle, limit: usize) -> Result<Vec<AiToolAuditEntry>, String> {
    let Some(path) = audit_path(app) else {
        return Ok(Vec::new());
    };
    let _guard = match AUDIT_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("无法读取只读检查审计：{error}")),
    };
    let mut entries = Vec::new();
    for line in content.lines().rev() {
        if entries.len() >= limit {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<AiToolAuditEntry>(line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn clear_audit(app: &AppHandle) -> Result<(), String> {
    let Some(path) = audit_path(app) else {
        return Ok(());
    };
    let _guard = match AUDIT_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清除只读检查审计：{error}")),
    }
}

/// Executes at most three host-validated read-only checks sequentially.
pub async fn execute_checks(
    app: &AppHandle,
    request_id: &str,
    checks: &[AiRequestedCheck],
    cancel: Arc<CancelHandle>,
) -> Result<Vec<AiToolRun>, String> {
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for check in checks {
        if !seen.insert(check.tool_id.clone()) {
            continue;
        }
        selected.push(check);
        if selected.len() >= MAX_CHECKS {
            break;
        }
    }

    let mut runs = Vec::new();
    for (index, check) in selected.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_string());
        }
        let Some(spec) = find_tool(&check.tool_id) else {
            let run = AiToolRun {
                tool_id: check.tool_id.clone(),
                label: "未授权的只读检查".into(),
                reason: check.reason.clone(),
                ok: false,
                duration_ms: 0,
                summary: Value::Null,
                error: Some("该工具不在只读白名单中，已拒绝执行。".into()),
            };
            audit(app, request_id, &run, &check.parameters);
            runs.push(run);
            continue;
        };

        super::emit_stream(
            app,
            request_id,
            "status",
            String::new(),
            0,
            format!(
                "只读检查 {}/{}：{}…",
                index + 1,
                selected.len(),
                spec.label
            ),
        );
        let started = Instant::now();
        let outcome = run_spec(app, spec, &check.parameters, &cancel).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        let (ok, summary, error) = match outcome {
            Ok(value) => (true, cap_summary(value), None),
            Err(error) if error == CANCELLED => return Err(error),
            Err(error) => (false, Value::Null, Some(truncate_text(&error, 300))),
        };
        let run = AiToolRun {
            tool_id: spec.id.to_string(),
            label: spec.label.to_string(),
            reason: truncate_text(&check.reason, 240),
            ok,
            duration_ms,
            summary,
            error,
        };
        let status = if run.ok {
            format!("只读检查完成：{}（{} ms）", run.label, run.duration_ms)
        } else {
            format!(
                "只读检查失败：{}（{}）",
                run.label,
                run.error.as_deref().unwrap_or("未知错误")
            )
        };
        super::emit_stream(app, request_id, "status", String::new(), 0, status);
        audit(app, request_id, &run, &check.parameters);
        runs.push(run);
    }
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_ids_are_unique_and_whitelisted() {
        let mut ids = HashSet::new();
        for spec in TOOL_SPECS {
            assert!(ids.insert(spec.id), "duplicate tool id: {}", spec.id);
            assert!(spec.timeout_ms >= 1_000);
            assert!(spec.description.contains("不返回") || spec.description.contains("只返回"));
        }
        assert!(find_tool("event_log.recent").is_some());
        assert!(find_tool("shell.exec").is_none());
    }

    #[test]
    fn event_params_are_clamped_and_whitelisted() {
        let params = parse_event_params(&json!({
            "logName": "Application",
            "level": "warning",
            "hours": 9999
        }))
        .unwrap();
        assert_eq!(params.log_name, "Application");
        assert_eq!(params.level, "warning");
        assert_eq!(params.hours, 168);

        let fallback = parse_event_params(&json!({
            "logName": "Security",
            "level": "information",
            "hours": 0
        }))
        .unwrap();
        assert_eq!(fallback.log_name, "System");
        assert_eq!(fallback.level, "error");
        assert_eq!(fallback.hours, 1);
    }

    #[test]
    fn rejects_unknown_or_oversized_parameters() {
        assert!(parse_event_params(&json!({ "script": "calc.exe" })).is_err());
        assert!(parse_event_params(&json!({ "hours": "many" })).is_err());
    }

    #[test]
    fn caps_and_redacts_tool_summaries() {
        let capped = cap_summary(json!({ "note": "C:\\Users\\alice\\secret.txt" }));
        assert_eq!(capped["note"], "[本地路径已脱敏]");

        let huge = cap_summary(json!({ "blob": "x".repeat(MAX_RESULT_BYTES + 100) }));
        assert_eq!(huge["truncated"], true);
    }

    #[test]
    fn native_tool_schema_maps_every_tool() {
        let schema = openai_tools();
        let functions = schema.as_array().unwrap();
        assert_eq!(functions.len(), TOOL_SPECS.len());
        let mut names = HashSet::new();
        for spec in TOOL_SPECS {
            let name = function_name(spec.id);
            assert!(name.chars().all(|character| character != '.'));
            assert!(names.insert(name.clone()));
            assert_eq!(tool_id_from_function_name(&name), Some(spec.id));
        }
        let event = functions
            .iter()
            .find(|item| item["function"]["name"] == "event_log_recent")
            .unwrap();
        assert_eq!(event["function"]["parameters"]["additionalProperties"], false);
        assert!(event["function"]["parameters"]["properties"]["hours"].is_object());
    }

    #[test]
    fn native_tool_call_is_converted_and_validated() {
        let check = check_from_tool_call(&ToolCall {
            id: "call_1".into(),
            name: "event_log_recent".into(),
            arguments: r#"{"logName":"Application","level":"warning","hours":48}"#.into(),
        });
        assert_eq!(check.tool_id, "event_log.recent");
        assert_eq!(check.parameters["hours"], 48);

        let invalid = check_from_tool_call(&ToolCall {
            id: "call_2".into(),
            name: "event_log_recent".into(),
            arguments: "{not-json".into(),
        });
        assert_eq!(invalid.tool_id, "event_log.recent");
        assert!(invalid.parameters.get("__invalidArguments").is_some());
    }

    #[test]
    fn catalog_lists_every_tool() {
        let catalog = catalog_prompt();
        for spec in TOOL_SPECS {
            assert!(catalog.contains(spec.id));
        }
    }

    #[test]
    fn tool_summaries_drop_sensitive_details() {
        let events = summarize_event_log(json!({
            "logName": "System",
            "hours": 24,
            "total": 1,
            "truncated": false,
            "events": [{
                "provider": "Demo",
                "eventId": 41,
                "level": "错误",
                "timeCreatedUnixMs": 1,
                "message": "C:\\Users\\alice\\secret.txt"
            }]
        }));
        let encoded = serde_json::to_string(&events).unwrap();
        assert!(!encoded.contains("secret.txt"));
        assert!(!encoded.contains("Users"));

        let network = summarize_network(json!({
            "defaultGateways": ["192.168.1.1"],
            "dns": [{ "alias": "以太网", "servers": ["8.8.8.8"] }],
            "proxyEnabled": false,
            "autoConfigUrl": "",
            "hostsCustomEntries": 2,
            "connections": [{ "remoteAddress": "10.0.0.5", "state": "Established" }]
        }));
        let encoded = serde_json::to_string(&network).unwrap();
        assert!(!encoded.contains("192.168.1.1"));
        assert!(!encoded.contains("8.8.8.8"));
        assert!(!encoded.contains("10.0.0.5"));
        assert!(encoded.contains("Established"));

        let startup = summarize_startup(json!([{
            "name": "Demo",
            "scope": "当前用户",
            "enabled": true,
            "command": "C:\\Users\\alice\\secret.exe"
        }]));
        let encoded = serde_json::to_string(&startup).unwrap();
        assert!(!encoded.contains("secret.exe"));
        assert!(!encoded.contains("command"));

        let tasks = summarize_scheduled_tasks(json!({
            "total": 2,
            "enabled": 1,
            "disabled": 1,
            "running": 0,
            "tasks": [{
                "name": "DemoTask",
                "state": "Ready",
                "lastResult": 1,
                "path": "\\Microsoft\\Secret"
            }]
        }));
        let encoded = serde_json::to_string(&tasks).unwrap();
        assert!(!encoded.contains("Microsoft"));
        assert!(encoded.contains("DemoTask"));

        let sensors = summarize_sensors(json!({
            "values": [{
                "category": "temperature",
                "availability": "ok",
                "hardwareClass": "cpu",
                "name": "CPU 温度",
                "value": 81.0,
                "source": "ACPI"
            }],
            "alerts": [],
            "storage": [{
                "name": "NVMe",
                "healthStatus": "Healthy",
                "wearPercent": 4.0,
                "assessment": { "signals": [] }
            }],
            "smartDevices": [{
                "instanceName": "SERIAL-SECRET-123",
                "predictFailure": true
            }],
            "elevated": false,
            "limitations": []
        }));
        let encoded = serde_json::to_string(&sensors).unwrap();
        assert!(!encoded.contains("SERIAL-SECRET-123"));
        assert_eq!(sensors["predictFailureCount"], 1);
        assert!(encoded.contains("81.0"));

        let diagnostics = summarize_diagnostics(json!({
            "pendingReboot": true,
            "pendingWindowsUpdateReboot": false,
            "crashEvents": [{ "message": "C:\\Users\\alice\\secret.dmp" }],
            "minidumpCount": 1,
            "serviceIssues": [{ "name": "Demo" }],
            "battery": { "present": true, "healthPercent": 71.0, "cycleCount": 120 }
        }));
        let encoded = serde_json::to_string(&diagnostics).unwrap();
        assert!(!encoded.contains("secret.dmp"));
        assert_eq!(diagnostics["crashEventCount"], 1);
        assert_eq!(diagnostics["batteryHealthPercent"], 71.0);
    }
}
