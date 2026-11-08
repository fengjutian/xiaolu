use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::toolbox::{ToolActionDescriptor, ToolDescriptor};

static TOKEN_SECRET: OnceLock<String> = OnceLock::new();
static USED_TOKENS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
/// 令牌重放记录保留时长：远大于令牌本身的 5 分钟有效期，过期记录直接淘汰。
const TOKEN_MEMORY: Duration = Duration::from_secs(15 * 60);

/// 工具进度事件名：界面按 jobId 关联到具体的一次执行。
const PROGRESS_EVENT: &str = "toolbox-progress";
/// 取消后留给工具自行收尾的宽限期；超时才强制结束子进程。
const CANCEL_GRACE: Duration = Duration::from_secs(3);
/// 轮询间隔：决定取消与超时的响应速度。
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// 每个工具保留的耗时样本数：够算 P95，又不至于让统计文件无限增长。
const STAT_SAMPLE_LIMIT: usize = 200;
/// §8 验收线：握手成功率目标（%）。
pub const TARGET_HANDSHAKE_SUCCESS_PERCENT: f64 = 99.0;
/// §8 验收线：超时率上限（%）。
pub const TARGET_TIMEOUT_PERCENT: f64 = 1.0;
/// 判定所需的最少样本数：两三次执行算出来的比率没有统计意义，不拿来判达标。
pub const MIN_VERDICT_SAMPLES: u64 = 5;

#[cfg(windows)]
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: u32, flags: u32) -> i32;
}

const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAction {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permissions {
    #[serde(default)]
    elevation: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ui {
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default = "default_output")]
    max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            timeout_ms: default_timeout(),
            max_output_bytes: default_output(),
        }
    }
}

fn default_timeout() -> u64 {
    120_000
}

fn default_output() -> usize {
    4 * 1024 * 1024
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    id: String,
    name: String,
    version: String,
    api_version: u32,
    description: String,
    category: String,
    risk: String,
    entry: String,
    #[serde(default)]
    #[allow(dead_code)]
    arguments: Vec<String>,
    #[serde(default)]
    actions: Vec<ManifestAction>,
    #[serde(default)]
    permissions: Permissions,
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    ui: Ui,
    #[serde(default)]
    note: Option<String>,
}

/// 生成一次进程级令牌密钥。宿主每次启动重新生成，工具进程通过环境变量获得。
fn token_secret() -> &'static str {
    TOKEN_SECRET.get_or_init(|| {
        let mut bytes = [0u8; 32];
        #[cfg(windows)]
        {
            // SAFETY: 传入空算法句柄表示使用系统首选随机数生成器。
            let rc = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if rc != 0 {
                bytes.fill(0);
            }
        }
        if bytes.iter().all(|byte| *byte == 0) {
            // 回退：不稳定但仍是每次启动唯一，仅用于无法调用系统 RNG 的环境。
            let mut state = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0)
                ^ ((std::process::id() as u64) << 32);
            state |= 1;
            for byte in bytes.iter_mut() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
        }
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    })
}

fn state_dir(app: &AppHandle) -> Option<PathBuf> {
    let directory = app.path().app_local_data_dir().ok()?;
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

/// 取消标记文件所在目录：工具进程轮询这里的标记文件是否存在。
fn cancel_dir(app: &AppHandle) -> Option<PathBuf> {
    let directory = state_dir(app)?.join("tool-cancel");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

/// 正在运行的工具任务：界面用它请求取消。
#[derive(Debug, Clone)]
struct ToolJob {
    tool_id: String,
    action: String,
    /// 取消标记文件：宿主请求取消时创建它，工具看到后自行收尾。
    marker: PathBuf,
    cancelled: bool,
}

/// 工具任务注册表：只有带 jobId 的执行才会登记，也才支持取消。
#[derive(Default)]
pub struct ToolboxState {
    jobs: Mutex<HashMap<String, ToolJob>>,
    /// 工具执行统计：握手成功率、超时率与耗时 P95 的原始样本。
    stats: Mutex<HashMap<String, ToolRunStat>>,
}

impl ToolboxState {
    fn register(&self, job_id: &str, job: ToolJob) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job_id.to_string(), job);
        }
    }

    fn finish(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(job_id);
        }
    }

    /// 请求取消：落下标记文件，工具进程下一轮轮询就会看到。
    fn request_cancel(&self, job_id: &str) -> bool {
        let Ok(mut jobs) = self.jobs.lock() else {
            return false;
        };
        let Some(job) = jobs.get_mut(job_id) else {
            return false;
        };
        job.cancelled = true;
        eprintln!(
            "[toolbox] 请求取消任务 {job_id}（{} / {}）",
            job.tool_id, job.action
        );
        std::fs::write(&job.marker, b"cancel").is_ok()
    }

    fn is_cancelled(&self, job_id: &str) -> bool {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(job_id).map(|job| job.cancelled))
            .unwrap_or(false)
    }

    /// 记录一次工具执行的结果与耗时，并立即落盘。
    fn record_run(
        &self,
        app: &AppHandle,
        tool_id: &str,
        outcome: ToolRunOutcome,
        trace: &RunTrace,
        duration_ms: u64,
    ) {
        let Ok(mut stats) = self.stats.lock() else {
            return;
        };
        if stats.is_empty() {
            // 进程刚启动时内存里没有数据，先把历史统计读回来再累加。
            if let Some(loaded) = load_stats(app) {
                *stats = loaded;
            }
        }
        stats
            .entry(tool_id.to_string())
            .or_default()
            .record(outcome, trace, duration_ms);
        save_stats(app, &stats);
    }

    /// 取出统计快照：内存里有就用内存的（更新），否则回落到磁盘上的历史。
    fn stats_snapshot(&self, app: &AppHandle) -> HashMap<String, ToolRunStat> {
        if let Ok(stats) = self.stats.lock() {
            if !stats.is_empty() {
                return stats.clone();
            }
        }
        load_stats(app).unwrap_or_default()
    }
}

/// 一次工具执行的过程状态：握手是否发起、是否成功。
#[derive(Debug, Default, Clone, Copy)]
struct RunTrace {
    /// 是否进入过握手阶段。
    handshake_attempted: bool,
    /// 握手是否成功。
    handshaked: bool,
}

/// 一次工具执行的结果分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolRunOutcome {
    /// 握手成功并拿到结果。
    Ok,
    /// 握手失败：协议不匹配、握手超时或握手格式非法。
    HandshakeFailed,
    /// 执行超时。
    TimedOut,
    /// 用户取消。
    Cancelled,
    /// 其它失败：工具报错、进程异常退出、输出超限等。
    Failed,
}

/// 单个工具的累计执行统计（落盘形式）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunStat {
    /// 总发起次数（含各类失败）。
    pub attempts: u64,
    /// 进入握手阶段的次数。
    pub handshake_attempts: u64,
    /// 握手成功次数。
    pub handshakes: u64,
    /// 成功拿到结果的次数。
    pub completed: u64,
    /// 超时次数。
    pub timeouts: u64,
    /// 取消次数。
    pub cancelled: u64,
    /// 其它失败次数。
    pub failed: u64,
    /// 每次执行的耗时（毫秒），只保留最近若干次。
    pub durations_ms: Vec<u64>,
}

impl ToolRunStat {
    fn record(&mut self, outcome: ToolRunOutcome, trace: &RunTrace, duration_ms: u64) {
        self.attempts += 1;
        if trace.handshake_attempted {
            self.handshake_attempts += 1;
        }
        if trace.handshaked {
            self.handshakes += 1;
        }
        match outcome {
            ToolRunOutcome::Ok => self.completed += 1,
            ToolRunOutcome::HandshakeFailed => self.failed += 1,
            ToolRunOutcome::TimedOut => self.timeouts += 1,
            ToolRunOutcome::Cancelled => self.cancelled += 1,
            ToolRunOutcome::Failed => self.failed += 1,
        }
        self.durations_ms.push(duration_ms);
        if self.durations_ms.len() > STAT_SAMPLE_LIMIT {
            let overflow = self.durations_ms.len() - STAT_SAMPLE_LIMIT;
            self.durations_ms.drain(0..overflow);
        }
    }
}

/// 逐工具统计明细。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunStatEntry {
    pub tool_id: String,
    pub attempts: u64,
    pub handshake_attempts: u64,
    pub handshakes: u64,
    pub completed: u64,
    pub timeouts: u64,
    pub cancelled: u64,
    pub failed: u64,
    /// 握手成功率（%）；没有握手样本时为 null。
    pub handshake_success_percent: Option<f64>,
    /// 超时率（%）；没有执行样本时为 null。
    pub timeout_percent: Option<f64>,
    /// 耗时 P95（毫秒）。
    pub duration_p95_ms: Option<u64>,
    /// 该工具的达标状态：ok / degraded / insufficient。
    pub status: String,
    /// 未达标的具体原因（给用户看的原话，不是错误码）。
    pub issues: Vec<String>,
}

/// 达标判定：把 §8 的两条验收线变成一个可读的结论。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolboxStatsVerdict {
    /// 总体状态：pass 达标 / warn 未达标 / unknown 样本不足无法判定。
    pub status: String,
    /// 握手成功率是否达标；样本不足时为 null。
    pub handshake_ok: Option<bool>,
    /// 超时率是否达标；样本不足时为 null。
    pub timeout_ok: Option<bool>,
    /// 未达标的工具 id。
    pub degraded_tools: Vec<String>,
    /// 样本不足、暂不判定的工具 id。
    pub insufficient_tools: Vec<String>,
    /// 一句话结论。
    pub summary: String,
}

/// 工具执行统计汇总。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolboxStatsReport {
    /// 总发起次数。
    pub attempts: u64,
    /// 进入握手阶段的次数。
    pub handshake_attempts: u64,
    /// 握手成功次数。
    pub handshakes: u64,
    /// 握手成功率（%）；没有样本时为 null —— 没有数据不等于 100%。
    pub handshake_success_percent: Option<f64>,
    /// 超时次数。
    pub timeouts: u64,
    /// 超时率（%）；没有样本时为 null。
    pub timeout_percent: Option<f64>,
    /// 耗时 P95（毫秒）。
    pub duration_p95_ms: Option<u64>,
    /// 参与统计的工具数。
    pub tool_count: usize,
    /// 逐工具明细，按执行次数从多到少。
    pub tools: Vec<ToolRunStatEntry>,
    /// 对照 §8 验收线的达标判定。
    pub verdict: ToolboxStatsVerdict,
}

/// 比率（%）：分母为 0 时返回 null，不拿 0 冒充「没有失败」。
fn ratio_percent(part: u64, total: u64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    Some(part as f64 / total as f64 * 100.0)
}

/// 百分位（最近邻法）：排序后取第 ceil(p × n) 个样本。
fn percentile(values: &[u64], p: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

/// 判定握手成功率是否达标：样本不足时返回 None（既不是达标也不是未达标）。
fn check_handshake_target(handshakes: u64, handshake_attempts: u64) -> Option<bool> {
    if handshake_attempts < MIN_VERDICT_SAMPLES {
        return None;
    }
    let rate = ratio_percent(handshakes, handshake_attempts)?;
    Some(rate >= TARGET_HANDSHAKE_SUCCESS_PERCENT)
}

/// 判定超时率是否达标：样本不足时返回 None。
fn check_timeout_target(timeouts: u64, attempts: u64) -> Option<bool> {
    if attempts < MIN_VERDICT_SAMPLES {
        return None;
    }
    let rate = ratio_percent(timeouts, attempts)?;
    Some(rate < TARGET_TIMEOUT_PERCENT)
}

/// 逐工具判定：返回状态（ok / degraded / insufficient）与给用户看的原因。
fn evaluate_tool(stat: &ToolRunStat) -> (String, Vec<String>) {
    let mut issues = Vec::new();
    // 跑够次数却没有一次进入握手阶段：工具根本没能启动起来，比「握手率低」更严重。
    if stat.attempts >= MIN_VERDICT_SAMPLES && stat.handshake_attempts == 0 {
        issues.push(format!(
            "{} 次执行没有一次进入握手阶段",
            stat.attempts
        ));
    }
    if check_handshake_target(stat.handshakes, stat.handshake_attempts) == Some(false) {
        let rate = ratio_percent(stat.handshakes, stat.handshake_attempts).unwrap_or(0.0);
        issues.push(format!(
            "握手成功率 {:.1}% 低于目标 {:.0}%",
            rate, TARGET_HANDSHAKE_SUCCESS_PERCENT
        ));
    }
    if check_timeout_target(stat.timeouts, stat.attempts) == Some(false) {
        let rate = ratio_percent(stat.timeouts, stat.attempts).unwrap_or(0.0);
        issues.push(format!(
            "超时率 {:.1}% 高于上限 {:.0}%",
            rate, TARGET_TIMEOUT_PERCENT
        ));
    }
    if !issues.is_empty() {
        return ("degraded".to_string(), issues);
    }
    let handshake_ok = check_handshake_target(stat.handshakes, stat.handshake_attempts);
    let timeout_ok = check_timeout_target(stat.timeouts, stat.attempts);
    if handshake_ok.is_none() && timeout_ok.is_none() {
        return (
            "insufficient".to_string(),
            vec![format!(
                "执行 {} 次，不足 {} 次，暂不判定",
                stat.attempts, MIN_VERDICT_SAMPLES
            )],
        );
    }
    ("ok".to_string(), Vec::new())
}

/// 汇总达标判定：两条 §8 验收线 + 逐工具归因。
fn build_verdict(
    handshake_ok: Option<bool>,
    timeout_ok: Option<bool>,
    handshake_percent: Option<f64>,
    timeout_percent: Option<f64>,
    degraded_tools: &[String],
    insufficient_tools: &[String],
) -> ToolboxStatsVerdict {
    let mut parts: Vec<String> = Vec::new();
    match (handshake_ok, handshake_percent) {
        (Some(true), Some(rate)) => parts.push(format!(
            "握手成功率 {:.2}% 达标（目标 ≥{:.0}%）",
            rate, TARGET_HANDSHAKE_SUCCESS_PERCENT
        )),
        (Some(false), Some(rate)) => parts.push(format!(
            "握手成功率 {:.2}% 未达标（目标 ≥{:.0}%）",
            rate, TARGET_HANDSHAKE_SUCCESS_PERCENT
        )),
        _ => parts.push("握手成功率样本不足，未判定".to_string()),
    }
    match (timeout_ok, timeout_percent) {
        (Some(true), Some(rate)) => parts.push(format!(
            "超时率 {:.2}% 达标（上限 <{:.0}%）",
            rate, TARGET_TIMEOUT_PERCENT
        )),
        (Some(false), Some(rate)) => parts.push(format!(
            "超时率 {:.2}% 未达标（上限 <{:.0}%）",
            rate, TARGET_TIMEOUT_PERCENT
        )),
        _ => parts.push("超时率样本不足，未判定".to_string()),
    }
    let failed = handshake_ok == Some(false) || timeout_ok == Some(false) || !degraded_tools.is_empty();
    // 只有两条验收线都判过且都达标、也没有工具掉链子，才敢说「达标」。
    let status = if failed {
        "warn"
    } else if handshake_ok == Some(true) && timeout_ok == Some(true) {
        "pass"
    } else {
        "unknown"
    };
    let mut summary = parts.join("；");
    if !degraded_tools.is_empty() {
        summary.push_str(&format!("；未达标工具：{}", degraded_tools.join("、")));
    }
    if !insufficient_tools.is_empty() {
        summary.push_str(&format!(
            "；{} 个工具样本不足暂不判定",
            insufficient_tools.len()
        ));
    }
    ToolboxStatsVerdict {
        status: status.to_string(),
        handshake_ok,
        timeout_ok,
        degraded_tools: degraded_tools.to_vec(),
        insufficient_tools: insufficient_tools.to_vec(),
        summary,
    }
}

/// 把逐工具统计汇总成一份报告。
pub fn build_stats_report(stats: &HashMap<String, ToolRunStat>) -> ToolboxStatsReport {
    let mut attempts = 0u64;
    let mut handshake_attempts = 0u64;
    let mut handshakes = 0u64;
    let mut timeouts = 0u64;
    let mut durations: Vec<u64> = Vec::new();
    let mut tools = Vec::new();
    let mut degraded_tools: Vec<String> = Vec::new();
    let mut insufficient_tools: Vec<String> = Vec::new();
    for (tool_id, stat) in stats {
        attempts += stat.attempts;
        handshake_attempts += stat.handshake_attempts;
        handshakes += stat.handshakes;
        timeouts += stat.timeouts;
        durations.extend(stat.durations_ms.iter().copied());
        let (status, issues) = evaluate_tool(stat);
        match status.as_str() {
            "degraded" => degraded_tools.push(tool_id.clone()),
            "insufficient" => insufficient_tools.push(tool_id.clone()),
            _ => {}
        }
        tools.push(ToolRunStatEntry {
            tool_id: tool_id.clone(),
            attempts: stat.attempts,
            handshake_attempts: stat.handshake_attempts,
            handshakes: stat.handshakes,
            completed: stat.completed,
            timeouts: stat.timeouts,
            cancelled: stat.cancelled,
            failed: stat.failed,
            handshake_success_percent: ratio_percent(stat.handshakes, stat.handshake_attempts),
            timeout_percent: ratio_percent(stat.timeouts, stat.attempts),
            duration_p95_ms: percentile(&stat.durations_ms, 0.95),
            status,
            issues,
        });
    }
    tools.sort_by(|left, right| {
        right
            .attempts
            .cmp(&left.attempts)
            .then_with(|| left.tool_id.cmp(&right.tool_id))
    });
    // HashMap 遍历顺序不固定，归因列表要排序，否则界面每次刷新顺序都在跳。
    degraded_tools.sort();
    insufficient_tools.sort();
    let handshake_success_percent = ratio_percent(handshakes, handshake_attempts);
    let timeout_percent = ratio_percent(timeouts, attempts);
    let verdict = build_verdict(
        check_handshake_target(handshakes, handshake_attempts),
        check_timeout_target(timeouts, attempts),
        handshake_success_percent,
        timeout_percent,
        &degraded_tools,
        &insufficient_tools,
    );
    ToolboxStatsReport {
        attempts,
        handshake_attempts,
        handshakes,
        handshake_success_percent,
        timeouts,
        timeout_percent,
        duration_p95_ms: percentile(&durations, 0.95),
        tool_count: tools.len(),
        tools,
        verdict,
    }
}

/// 把错误消息归类：握手失败、超时、取消要分开统计，不能混进「其它失败」。
fn classify_failure(message: &str) -> ToolRunOutcome {
    if message.contains("握手") {
        ToolRunOutcome::HandshakeFailed
    } else if message.contains("已取消") {
        ToolRunOutcome::Cancelled
    } else if message.contains("超时") {
        ToolRunOutcome::TimedOut
    } else {
        ToolRunOutcome::Failed
    }
}

/// 工具执行统计文件：跨进程重启保留，否则「握手成功率」只能看当次运行。
fn stats_path(app: &AppHandle) -> Option<PathBuf> {
    Some(state_dir(app)?.join("toolbox-stats.json"))
}

fn load_stats(app: &AppHandle) -> Option<HashMap<String, ToolRunStat>> {
    let path = stats_path(app)?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_stats(app: &AppHandle, stats: &HashMap<String, ToolRunStat>) {
    let Some(path) = stats_path(app) else {
        return;
    };
    if let Ok(bytes) = serde_json::to_vec_pretty(stats) {
        let _ = std::fs::write(path, bytes);
    }
}

/// 进度事件负载：工具负责给 phase/done/total/message，宿主补上归属信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub job_id: String,
    pub tool_id: String,
    pub action: String,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

fn progress_payload(job_id: &str, tool_id: &str, action: &str, data: &Value) -> ToolProgress {
    ToolProgress {
        job_id: job_id.to_string(),
        tool_id: tool_id.to_string(),
        action: action.to_string(),
        phase: data
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        done: data.get("done").and_then(Value::as_u64).unwrap_or_default(),
        total: data.get("total").and_then(Value::as_u64).unwrap_or_default(),
        message: data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// 任务 id 会拼进文件路径，必须挡住路径穿越与超长输入。
fn validate_job_id(job_id: &str) -> Result<(), String> {
    if job_id.is_empty() || job_id.len() > 64 {
        return Err("任务 id 无效".into());
    }
    if !job_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err("任务 id 只能包含字母、数字、连字符与下划线".into());
    }
    Ok(())
}

/// 任务结束时的清理：注销任务并删除取消标记文件。
struct JobGuard<'a> {
    app: &'a AppHandle,
    job_id: Option<String>,
    marker: Option<PathBuf>,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if let Some(job_id) = &self.job_id {
            // 清理发生在展开路径上，不能因为状态缺失而 panic。
            if let Some(state) = self.app.try_state::<ToolboxState>() {
                state.finish(job_id);
            }
        }
        if let Some(marker) = &self.marker {
            let _ = std::fs::remove_file(marker);
        }
    }
}

fn tools_dirs(app: &AppHandle) -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_TOOLS_DIR") {
        return vec![PathBuf::from(path)];
    }
    let mut candidates = Vec::new();
    if let Ok(directory) = app.path().app_local_data_dir() {
        let candidate = directory.join("tools");
        if candidate.is_dir() {
            candidates.push(candidate);
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            let candidate = parent.join("tools");
            if candidate.is_dir() {
                candidates.push(candidate);
            }
        }
    }
    // 开发模式下 Tauri 的当前目录可能是仓库根目录，也可能是 src-tauri；
    // 两种位置都纳入候选，避免本地安装目录存在时把开发工具包全部遮蔽。
    candidates.push(PathBuf::from("tools-dist"));
    #[cfg(debug_assertions)]
    if let Some(repository_root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        candidates.push(repository_root.join("tools-dist"));
    }

    let mut unique = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| path.is_dir())
        .filter(|path| unique.insert(path.clone()))
        .collect()
}

fn load_manifest(directory: &Path, folder_name: &str) -> Option<Manifest> {
    let path = directory.join("tool.json");
    let bytes = std::fs::read(&path).ok()?;
    let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
    if manifest.id != folder_name {
        eprintln!("[toolbox] 目录名与清单 id 不一致：{}", path.display());
        return None;
    }
    if manifest.schema_version != 1 || manifest.api_version != 1 {
        eprintln!("[toolbox] 清单版本不兼容：{}", path.display());
        return None;
    }
    if manifest.entry.contains('\\') || manifest.entry.contains('/') {
        eprintln!("[toolbox] entry 不能包含路径分隔符：{}", path.display());
        return None;
    }
    if !directory.join(&manifest.entry).is_file() {
        eprintln!("[toolbox] 缺少入口程序：{}", directory.join(&manifest.entry).display());
        return None;
    }
    if manifest.actions.is_empty() {
        eprintln!("[toolbox] 清单没有声明任何动作：{}", path.display());
        return None;
    }
    Some(manifest)
}

fn discover(app: &AppHandle) -> Vec<(PathBuf, Manifest)> {
    discover_in_roots(tools_dirs(app))
}

fn discover_in_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<(PathBuf, Manifest)> {
    let mut found = HashMap::<String, (PathBuf, Manifest)>::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }
            if found.contains_key(&name) {
                continue;
            }
            if let Some(manifest) = load_manifest(&entry.path(), &name) {
                found.insert(name, (entry.path(), manifest));
            }
        }
    }
    let mut found = found.into_values().collect::<Vec<_>>();
    found.sort_by(|left, right| left.1.name.cmp(&right.1.name));
    found
}

fn descriptor(directory: &Path, manifest: &Manifest) -> ToolDescriptor {
    ToolDescriptor {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        category: manifest.category.clone(),
        description: manifest.description.clone(),
        risk: manifest.risk.clone(),
        requires_admin: !manifest.permissions.elevation.is_empty()
            && manifest.permissions.elevation != "none",
        status: "available".into(),
        note: manifest
            .note
            .clone()
            .unwrap_or_else(|| format!("v{} · 独立工具进程 · {}", manifest.version, directory.display())),
        ui_kind: if manifest.ui.kind.is_empty() {
            "none".into()
        } else {
            manifest.ui.kind.clone()
        },
        actions: manifest
            .actions
            .iter()
            .map(|action| ToolActionDescriptor {
                id: action.id.clone(),
                label: action
                    .label
                    .clone()
                    .unwrap_or_else(|| action.id.clone()),
                write: action.write,
            })
            .collect(),
    }
}

pub fn list_tools(app: &AppHandle) -> Vec<ToolDescriptor> {
    let mut tools = discover(app)
        .iter()
        .map(|(directory, manifest)| descriptor(directory, manifest))
        .collect::<Vec<_>>();
    for planned in crate::toolbox::tools() {
        if !tools.iter().any(|tool| tool.id == planned.id) {
            tools.push(planned);
        }
    }
    tools
}

/// 动作授权：预览始终允许；写动作必须清单声明、非只读，并携带确认令牌。
fn authorize_action(
    manifest: &Manifest,
    action: &str,
    confirmation: Option<&str>,
) -> Result<(), String> {
    let declared = manifest
        .actions
        .iter()
        .find(|candidate| candidate.id == action)
        .ok_or_else(|| format!("工具 {} 不支持动作 {}", manifest.id, action))?;
    // 只读动作（list / info / pages / import_rules / open_official / save_rollback 等）
    // 只要清单声明过就直接放行，不需要确认令牌：它们不改系统状态。
    if !declared.write {
        return Ok(());
    }
    if manifest.risk == "read_only" {
        return Err("只读工具不允许写操作".into());
    }
    match confirmation {
        Some(token) if !token.trim().is_empty() => Ok(()),
        _ => Err("缺少确认令牌：请先执行预览并在确认后重试".into()),
    }
}

/// 确认令牌的结构：`hex(claims).hex(hmac-sha256)`。
///
/// 宿主不做签名校验（那是工具侧的事），但至少要挡住「随便一串字符串」——
/// 否则任意非空串都能进重放表，把真实令牌的记录挤掉。
fn looks_like_token(token: &str) -> bool {
    let Some((payload, signature)) = token.split_once('.') else {
        return false;
    };
    let hex = |text: &str| {
        !text.is_empty() && text.len() % 2 == 0 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    hex(payload) && payload.len() >= 32 && signature.len() == 64 && hex(signature)
}

/// 记录并拒绝重复使用的确认令牌；令牌过期由工具侧校验。
fn consume_token(confirmation: &str) -> Result<(), String> {
    let token = confirmation.trim();
    if !looks_like_token(token) {
        return Err("确认令牌格式无效，请重新预览".into());
    }
    let tokens = USED_TOKENS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut used = tokens
        .lock()
        .map_err(|_| "确认令牌状态不可用，请重启应用".to_string())?;
    // 按时间淘汰过期记录，而不是整体 clear()：整体清空会把真实令牌的一次性
    // 保证一并抹掉，攻击者只要先塞满表就能重放。
    used.retain(|_, recorded| recorded.elapsed() < TOKEN_MEMORY);
    if used.contains_key(token) {
        return Err("确认令牌已使用，请重新预览".into());
    }
    used.insert(token.to_string(), Instant::now());
    Ok(())
}

fn receive(rx: &mpsc::Receiver<String>, timeout: Duration, label: &str) -> Result<String, String> {
    rx.recv_timeout(timeout)
        .map_err(|_| format!("{label}超时"))
}

fn run_action_blocking(
    app: &AppHandle,
    tool_id: &str,
    action: &str,
    confirmation: Option<String>,
    parameters: Option<Value>,
    job_id: Option<String>,
) -> Result<Value, String> {
    // 统计必须包住整条执行路径：握手失败、超时、取消都要各自计数。
    let started = Instant::now();
    let mut trace = RunTrace::default();
    let result = run_action_inner(
        app,
        tool_id,
        action,
        confirmation,
        parameters,
        job_id,
        &mut trace,
    );
    let outcome = match &result {
        Ok(_) => ToolRunOutcome::Ok,
        Err(message) => classify_failure(message),
    };
    app.state::<ToolboxState>().record_run(
        app,
        tool_id,
        outcome,
        &trace,
        started.elapsed().as_millis() as u64,
    );
    result
}

fn run_action_inner(
    app: &AppHandle,
    tool_id: &str,
    action: &str,
    confirmation: Option<String>,
    parameters: Option<Value>,
    job_id: Option<String>,
    trace: &mut RunTrace,
) -> Result<Value, String> {
    // 先校验任务 id：它会拼进文件路径，而且校验失败不应该白白消耗掉确认令牌。
    if let Some(id) = job_id.as_deref() {
        validate_job_id(id)?;
    }
    let found = discover(app)
        .into_iter()
        .find(|(_, manifest)| manifest.id == tool_id)
        .ok_or_else(|| format!("工具未安装：{tool_id}"))?;
    let (directory, manifest) = found;
    authorize_action(&manifest, action, confirmation.as_deref())?;
    if action != "preview" {
        let token = confirmation.as_deref().unwrap_or_default();
        consume_token(token)?;
    }

    // 任务 id 与取消标记必须在启动子进程前准备好：工具一启动就可能去读标记文件。
    let job = match job_id.as_deref() {
        Some(id) => {
            let cancel_directory = cancel_dir(app).ok_or("无法准备任务取消目录")?;
            let marker = cancel_directory.join(format!("{id}.cancel"));
            let _ = std::fs::remove_file(&marker);
            Some((id.to_string(), marker))
        }
        None => None,
    };

    let executable = directory.join(&manifest.entry);
    let mut command = Command::new(&executable);
    command
        .arg("--stdio")
        .env("SYSTEM_LENS_TOKEN_SECRET", token_secret())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    if let Some((_, marker)) = &job {
        command.env("SYSTEM_LENS_CANCEL_FILE", marker);
    }
    if let Some(directory) = state_dir(app) {
        command.env("SYSTEM_LENS_STATE_DIR", directory);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动工具 {}：{error}", executable.display()))?;

    if let Some((id, marker)) = &job {
        app.state::<ToolboxState>().register(
            id,
            ToolJob {
                tool_id: tool_id.to_string(),
                action: action.to_string(),
                marker: marker.clone(),
                cancelled: false,
            },
        );
    }
    let _guard = JobGuard {
        app,
        job_id: job.as_ref().map(|(id, _)| id.clone()),
        marker: job.as_ref().map(|(_, marker)| marker.clone()),
    };

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            child.kill().ok();
            return Err("无法写入工具进程".into());
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            child.kill().ok();
            return Err("无法读取工具进程".into());
        }
    };
    // 有界队列 + 单行上限：工具洪水输出时宿主内存不会无界增长。
    const MAX_LINE_BYTES: usize = 512 * 1024;
    let (sender, receiver) = mpsc::sync_channel::<String>(64);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader
                .by_ref()
                .take(MAX_LINE_BYTES as u64)
                .read_until(b'\n', &mut buffer)
            {
                Ok(0) => break,
                Ok(_) => {
                    if buffer.len() >= MAX_LINE_BYTES && !buffer.ends_with(b"\n") {
                        let _ = sender.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "OUTPUT_TOO_LARGE",
                                "message": "工具单行输出超过上限，已中止",
                            })
                            .to_string(),
                        );
                        break;
                    }
                    let line = String::from_utf8_lossy(&buffer).trim_end().to_string();
                    if !line.is_empty() && sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    trace.handshake_attempted = true;
    let hello = match receive(&receiver, Duration::from_secs(5), "工具握手") {
        Ok(value) => value,
        Err(error) => {
            // 握手失败的工具往往根本不读 stdin，不会因为管道关闭自行退出，
            // 这里必须显式结束进程，否则会一直残留到应用退出。
            child.kill().ok();
            return Err(error);
        }
    };
    let hello: Value = match serde_json::from_str(&hello) {
        Ok(value) => value,
        Err(_) => {
            child.kill().ok();
            return Err("工具握手格式无效".into());
        }
    };
    if hello.get("type").and_then(Value::as_str) != Some("hello")
        || hello.get("protocol").and_then(Value::as_u64) != Some(1)
        || hello.get("toolId").and_then(Value::as_str) != Some(tool_id)
    {
        child.kill().ok();
        return Err("工具握手失败：协议或 id 不匹配".into());
    }
    trace.handshaked = true;

    let request = serde_json::json!({
        "type": "request",
        "id": 1,
        "action": action,
        "parameters": parameters.unwrap_or_else(|| serde_json::json!({})),
        "confirmation": confirmation,
    });
    if let Err(error) = writeln!(stdin, "{request}") {
        child.kill().ok();
        return Err(format!("无法发送工具请求：{error}"));
    }
    stdin.flush().ok();

    let timeout = Duration::from_millis(manifest.limits.timeout_ms.max(1_000));
    let started = Instant::now();
    let mut received_bytes = 0usize;
    let mut cancel_deadline: Option<Instant> = None;
    loop {
        // 超时与取消每轮都检查：工具持续上报进度时不能只等收不到消息才判定。
        if started.elapsed() >= timeout {
            child.kill().ok();
            return Err("工具响应超时".into());
        }
        if cancel_deadline.is_none() {
            if let Some((id, _)) = &job {
                if app.state::<ToolboxState>().is_cancelled(id) {
                    cancel_deadline = Some(Instant::now() + CANCEL_GRACE);
                }
            }
        }
        if let Some(deadline) = cancel_deadline {
            if Instant::now() >= deadline {
                child.kill().ok();
                return Err("操作已取消".into());
            }
        }
        let line = match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                child.kill().ok();
                return Err("工具进程意外退出".into());
            }
        };
        received_bytes += line.len();
        if received_bytes > manifest.limits.max_output_bytes {
            child.kill().ok();
            return Err("工具输出超过上限".into());
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                child.kill().ok();
                return Err("工具输出不是合法 JSON".into());
            }
        };
        match message.get("type").and_then(Value::as_str) {
            Some("progress") => {
                if let Some((id, _)) = &job {
                    let data = message.get("data").cloned().unwrap_or(Value::Null);
                    let _ = app.emit(
                        PROGRESS_EVENT,
                        progress_payload(id, tool_id, action, &data),
                    );
                }
                continue;
            }
            Some("result") => {
                child.kill().ok();
                return Ok(message.get("data").cloned().unwrap_or(Value::Null));
            }
            Some("error") => {
                child.kill().ok();
                let code = message.get("code").and_then(Value::as_str).unwrap_or("INTERNAL");
                let text = message
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("工具返回错误");
                return Err(format!("{code}: {text}"));
            }
            _ => continue,
        }
    }
}

#[tauri::command]
pub async fn list_toolbox_tools(app: AppHandle) -> Result<Vec<ToolDescriptor>, String> {
    Ok(list_tools(&app))
}

#[tauri::command]
pub async fn run_tool_action(
    app: AppHandle,
    tool_id: String,
    action: String,
    confirmation: Option<String>,
    parameters: Option<Value>,
    job_id: Option<String>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_action_blocking(&app, &tool_id, &action, confirmation, parameters, job_id)
    })
    .await
    .map_err(|error| format!("工具执行任务失败：{error}"))?
}

/// 请求取消正在执行的工具任务：工具看到标记文件后自行收尾并清理半成品。
#[tauri::command]
pub async fn cancel_tool_action(app: AppHandle, job_id: String) -> Result<bool, String> {
    validate_job_id(&job_id)?;
    Ok(app.state::<ToolboxState>().request_cancel(&job_id))
}

/// 工具执行统计：握手成功率、超时率与耗时 P95。
///
/// 这两项此前在验收指标里一直写着「未统计」——现在由宿主在每次执行后记账，
/// 界面可以直接读数，不用靠主观判断。
#[tauri::command]
pub async fn get_toolbox_stats(app: AppHandle) -> Result<ToolboxStatsReport, String> {
    let stats = app.state::<ToolboxState>().stats_snapshot(&app);
    Ok(build_stats_report(&stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(risk: &str, actions: Value) -> Manifest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": risk,
            "entry": "demo.exe",
            "actions": actions,
            "permissions": {"elevation": "none"},
            "limits": {"timeoutMs": 1000, "maxOutputBytes": 1024}
        }))
        .expect("manifest")
    }

    #[test]
    fn rejects_entry_with_path_separator() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": "read_only",
            "entry": "..\\evil.exe",
            "actions": [{"id": "preview", "write": false}]
        });
        std::fs::write(temp.join("tool.json"), serde_json::to_vec(&manifest).unwrap()).ok();
        assert!(load_manifest(&temp, "demo").is_none());
        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn parses_valid_manifest() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-valid-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        std::fs::write(temp.join("demo.exe"), b"stub").ok();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "apiVersion": 1,
            "description": "demo",
            "category": "test",
            "risk": "read_only",
            "entry": "demo.exe",
            "actions": [{"id": "preview", "write": false}],
            "permissions": {"elevation": "none"},
            "limits": {"timeoutMs": 1000, "maxOutputBytes": 1024}
        });
        std::fs::write(temp.join("tool.json"), serde_json::to_vec(&manifest).unwrap()).ok();
        let parsed = load_manifest(&temp, "demo").expect("valid manifest");
        assert_eq!(parsed.id, "demo");
        assert_eq!(parsed.limits.timeout_ms, 1000);
        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn merges_tool_roots_and_prefers_the_first_copy() {
        let test_root = |name: &str| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("{name}-{}", std::process::id()))
        };
        let first = test_root("toolbox-roots-first");
        let second = test_root("toolbox-roots-second");
        let write_tool = |root: &Path, id: &str, name: &str| {
            let directory = root.join(id);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{id}.exe")), b"test").unwrap();
            let manifest = serde_json::json!({
                "schemaVersion": 1,
                "id": id,
                "name": name,
                "version": "1.0.0",
                "apiVersion": 1,
                "description": "test",
                "category": "test",
                "risk": "read_only",
                "entry": format!("{id}.exe"),
                "actions": [{"id": "preview", "write": false}]
            });
            std::fs::write(
                directory.join("tool.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        };
        write_tool(&first, "shared", "本地版本");
        write_tool(&second, "shared", "内置版本");
        write_tool(&second, "bundled", "内置工具");

        let found = discover_in_roots([first.clone(), second.clone()]);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .any(|(_, manifest)| manifest.id == "shared" && manifest.name == "本地版本"));
        assert!(found
            .iter()
            .any(|(_, manifest)| manifest.id == "bundled"));

        std::fs::remove_dir_all(first).ok();
        std::fs::remove_dir_all(second).ok();
    }

    #[test]
    fn preview_is_allowed_without_token() {
        let manifest = manifest("read_only", serde_json::json!([{"id": "preview", "write": false}]));
        assert!(authorize_action(&manifest, "preview", None).is_ok());
    }

    #[test]
    fn read_only_tools_cannot_write_even_with_token() {
        let manifest = manifest(
            "read_only",
            serde_json::json!([{"id": "apply", "write": true}]),
        );
        let error = authorize_action(&manifest, "apply", Some("token")).unwrap_err();
        assert!(error.contains("只读"));
    }

    #[test]
    fn write_requires_declared_write_action_and_token() {
        let manifest = manifest(
            "system",
            serde_json::json!([
                {"id": "preview", "label": "预览", "write": false},
                {"id": "apply", "label": "切换", "write": true}
            ]),
        );
        assert!(authorize_action(&manifest, "apply", None)
            .unwrap_err()
            .contains("确认令牌"));
        assert!(authorize_action(&manifest, "apply", Some("  "))
            .is_err());
        assert!(authorize_action(&manifest, "apply", Some("token")).is_ok());
        assert!(authorize_action(&manifest, "unknown", Some("token")).is_err());
    }

    #[test]
    fn tokens_are_single_use() {
        let token = format!("{}.{}", "ab".repeat(40), "cd".repeat(32));
        assert!(consume_token(&token).is_ok());
        assert!(consume_token(&token).unwrap_err().contains("已使用"));
    }

    #[test]
    fn rejects_tokens_that_are_not_structurally_valid() {
        // 任意非空字符串不应该进重放表：否则可以把真实令牌的记录挤掉。
        assert!(consume_token("token").is_err());
        assert!(consume_token(&format!("{}.{}", "zz", "cd".repeat(32))).is_err());
        assert!(consume_token(&format!("{}.", "ab".repeat(40))).is_err());
    }

    #[test]
    fn declared_read_only_actions_run_without_a_token() {
        // list / info / pages / import_rules 这类动作原本会被当成写动作拒绝。
        let manifest = manifest(
            "write",
            serde_json::json!([
                {"id": "preview", "write": false},
                {"id": "list", "write": false},
                {"id": "apply", "write": true}
            ]),
        );
        assert!(authorize_action(&manifest, "list", None).is_ok());
        assert!(authorize_action(&manifest, "apply", None).is_err());
    }

    #[test]
    fn read_only_risk_still_blocks_write_actions() {
        let manifest = manifest(
            "read_only",
            serde_json::json!([
                {"id": "preview", "write": false},
                {"id": "list", "write": false},
                {"id": "apply", "write": true}
            ]),
        );
        assert!(authorize_action(&manifest, "list", None).is_ok());
        assert!(authorize_action(&manifest, "apply", Some(&format!("{}.{}", "ab".repeat(40), "cd".repeat(32)))).is_err());
    }

    #[test]
    fn rejects_unsafe_job_ids() {
        assert!(validate_job_id("job-abc_123").is_ok());
        assert!(validate_job_id("").is_err());
        assert!(validate_job_id("..\\evil").is_err());
        assert!(validate_job_id("job/1").is_err());
        assert!(validate_job_id("job 1").is_err());
        assert!(validate_job_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn cancel_writes_marker_and_marks_job() {
        let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("toolbox-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&temp).ok();
        let marker = temp.join("job.cancel");
        let _ = std::fs::remove_file(&marker);

        let state = ToolboxState::default();
        assert!(!state.request_cancel("job-missing"));
        state.register(
            "job-1",
            ToolJob {
                tool_id: "omni_zip".into(),
                action: "apply".into(),
                marker: marker.clone(),
                cancelled: false,
            },
        );
        assert!(!state.is_cancelled("job-1"));
        assert!(state.request_cancel("job-1"));
        assert!(state.is_cancelled("job-1"));
        assert!(marker.is_file(), "取消必须落下标记文件供工具进程发现");

        state.finish("job-1");
        assert!(!state.is_cancelled("job-1"));
        assert!(!state.request_cancel("job-1"));

        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn progress_payload_fills_defaults() {
        let payload = progress_payload(
            "job-1",
            "omni_zip",
            "apply",
            &serde_json::json!({"phase": "compress", "done": 3, "total": 10, "message": "处理中"}),
        );
        assert_eq!(payload.job_id, "job-1");
        assert_eq!(payload.tool_id, "omni_zip");
        assert_eq!(payload.action, "apply");
        assert_eq!(payload.phase, "compress");
        assert_eq!(payload.done, 3);
        assert_eq!(payload.total, 10);
        assert_eq!(payload.message, "处理中");

        let empty = progress_payload("job-2", "omni_zip", "apply", &Value::Null);
        assert_eq!(empty.phase, "");
        assert_eq!(empty.done, 0);
        assert_eq!(empty.total, 0);
    }

    fn traced(handshake_attempted: bool, handshaked: bool) -> RunTrace {
        RunTrace {
            handshake_attempted,
            handshaked,
        }
    }

    #[test]
    fn run_stat_counts_every_outcome() {
        let mut stat = ToolRunStat::default();
        stat.record(ToolRunOutcome::Ok, &traced(true, true), 100);
        stat.record(ToolRunOutcome::TimedOut, &traced(true, true), 200);
        stat.record(ToolRunOutcome::Cancelled, &traced(true, true), 50);
        stat.record(ToolRunOutcome::HandshakeFailed, &traced(true, false), 10);
        // 启动就失败：连握手都没进入。
        stat.record(ToolRunOutcome::Failed, &traced(false, false), 5);
        assert_eq!(stat.attempts, 5);
        assert_eq!(stat.handshake_attempts, 4);
        assert_eq!(stat.handshakes, 3);
        assert_eq!(stat.completed, 1);
        assert_eq!(stat.timeouts, 1);
        assert_eq!(stat.cancelled, 1);
        assert_eq!(stat.failed, 2);
        assert_eq!(stat.durations_ms.len(), 5);
    }

    #[test]
    fn run_stat_keeps_only_the_latest_samples() {
        let mut stat = ToolRunStat::default();
        for index in 0..(STAT_SAMPLE_LIMIT + 25) {
            stat.record(ToolRunOutcome::Ok, &traced(true, true), index as u64);
        }
        assert_eq!(stat.durations_ms.len(), STAT_SAMPLE_LIMIT);
        // 丢的是最旧的样本，最新的仍在。
        assert_eq!(stat.durations_ms.last(), Some(&((STAT_SAMPLE_LIMIT + 24) as u64)));
    }

    #[test]
    fn failure_classification_separates_the_causes() {
        assert_eq!(
            classify_failure("工具握手失败：协议或 id 不匹配"),
            ToolRunOutcome::HandshakeFailed
        );
        assert_eq!(
            classify_failure("工具握手超时"),
            ToolRunOutcome::HandshakeFailed
        );
        assert_eq!(classify_failure("工具响应超时"), ToolRunOutcome::TimedOut);
        assert_eq!(classify_failure("操作已取消"), ToolRunOutcome::Cancelled);
        assert_eq!(
            classify_failure("TOOL_ERROR: 磁盘写满"),
            ToolRunOutcome::Failed
        );
    }

    #[test]
    fn percentile_uses_the_nearest_rank() {
        let values: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&values, 0.95), Some(95));
        assert_eq!(percentile(&values, 0.5), Some(50));
        assert_eq!(percentile(&[7], 0.95), Some(7));
        assert_eq!(percentile(&[], 0.95), None);
    }

    #[test]
    fn stats_report_aggregates_handshake_timeout_and_p95() {
        let mut stats = HashMap::new();
        let mut first = ToolRunStat::default();
        for _ in 0..99 {
            first.record(ToolRunOutcome::Ok, &traced(true, true), 100);
        }
        first.record(ToolRunOutcome::HandshakeFailed, &traced(true, false), 20);
        stats.insert("alpha".to_string(), first);

        let mut second = ToolRunStat::default();
        second.record(ToolRunOutcome::TimedOut, &traced(true, true), 5_000);
        stats.insert("beta".to_string(), second);

        let report = build_stats_report(&stats);
        assert_eq!(report.attempts, 101);
        assert_eq!(report.handshake_attempts, 101);
        assert_eq!(report.handshakes, 100);
        assert_eq!(report.timeouts, 1);
        assert_eq!(report.tool_count, 2);
        // 100 / 101 次握手成功
        let handshake = report.handshake_success_percent.expect("rate");
        assert!((handshake - 99.0099).abs() < 0.01);
        // 1 / 101 次超时
        let timeout = report.timeout_percent.expect("rate");
        assert!((timeout - 0.9901).abs() < 0.01);
        assert!(report.duration_p95_ms.is_some());
        // 执行次数多的排在前面
        assert_eq!(report.tools[0].tool_id, "alpha");
        assert_eq!(report.tools[1].tool_id, "beta");
        assert_eq!(report.tools[1].timeout_percent, Some(100.0));
    }

    #[test]
    fn stats_report_has_no_rate_without_samples() {
        let report = build_stats_report(&HashMap::new());
        assert_eq!(report.attempts, 0);
        // 没有数据不等于「100% 成功」：必须报 null，不能编造。
        assert_eq!(report.handshake_success_percent, None);
        assert_eq!(report.timeout_percent, None);
        assert_eq!(report.duration_p95_ms, None);
        assert_eq!(report.tool_count, 0);
        assert!(report.tools.is_empty());
    }

    #[test]
    fn tool_run_stat_survives_a_json_round_trip() {
        let mut stat = ToolRunStat::default();
        stat.record(ToolRunOutcome::Ok, &traced(true, true), 321);
        let json = serde_json::to_string(&stat).expect("serialize");
        assert!(json.contains("\"attempts\":1"));
        let restored: ToolRunStat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.attempts, 1);
        assert_eq!(restored.handshakes, 1);
        assert_eq!(restored.durations_ms, vec![321]);
    }

    #[test]
    fn verdict_needs_enough_samples_before_it_judges() {
        // 4 次执行不够判定：既不判达标，也不判未达标。
        assert_eq!(check_handshake_target(4, 4), None);
        assert_eq!(check_timeout_target(0, 4), None);
        // 5 次起才给结论。
        assert_eq!(check_handshake_target(5, 5), Some(true));
        assert_eq!(check_timeout_target(0, 5), Some(true));
        // 99% 是目标线，低于就要报不达标。
        assert_eq!(check_handshake_target(99, 100), Some(true));
        assert_eq!(check_handshake_target(98, 100), Some(false));
        // 超时率上限是 1%（严格小于）。
        assert_eq!(check_timeout_target(0, 100), Some(true));
        assert_eq!(check_timeout_target(1, 100), Some(false));
    }

    #[test]
    fn verdict_passes_when_both_targets_are_met() {
        let mut stats = HashMap::new();
        let mut stat = ToolRunStat::default();
        for _ in 0..100 {
            stat.record(ToolRunOutcome::Ok, &traced(true, true), 100);
        }
        stats.insert("alpha".to_string(), stat);
        let report = build_stats_report(&stats);
        assert_eq!(report.verdict.status, "pass");
        assert_eq!(report.verdict.handshake_ok, Some(true));
        assert_eq!(report.verdict.timeout_ok, Some(true));
        assert!(report.verdict.degraded_tools.is_empty());
        assert_eq!(report.tools[0].status, "ok");
        assert!(report.tools[0].issues.is_empty());
        assert!(report.verdict.summary.contains("达标"));
    }

    #[test]
    fn verdict_warns_and_names_the_degraded_tool() {
        let mut stats = HashMap::new();
        // 健康工具
        let mut healthy = ToolRunStat::default();
        for _ in 0..50 {
            healthy.record(ToolRunOutcome::Ok, &traced(true, true), 100);
        }
        stats.insert("alpha".to_string(), healthy);
        // 一半都超时的工具
        let mut flaky = ToolRunStat::default();
        for _ in 0..5 {
            flaky.record(ToolRunOutcome::TimedOut, &traced(true, true), 9_000);
        }
        stats.insert("beta".to_string(), flaky);

        let report = build_stats_report(&stats);
        assert_eq!(report.verdict.status, "warn");
        assert_eq!(report.verdict.timeout_ok, Some(false));
        assert_eq!(report.verdict.degraded_tools, vec!["beta".to_string()]);
        let beta = report
            .tools
            .iter()
            .find(|entry| entry.tool_id == "beta")
            .expect("beta entry");
        assert_eq!(beta.status, "degraded");
        assert!(beta.issues.iter().any(|issue| issue.contains("超时率")));
        // 归因要指名道姓，不能只说「有问题」。
        assert!(report.verdict.summary.contains("beta"));
    }

    #[test]
    fn verdict_flags_a_tool_that_never_handshook() {
        let mut stats = HashMap::new();
        let mut stat = ToolRunStat::default();
        for _ in 0..6 {
            stat.record(ToolRunOutcome::Failed, &traced(false, false), 10);
        }
        stats.insert("gamma".to_string(), stat);
        let report = build_stats_report(&stats);
        assert_eq!(report.verdict.status, "warn");
        assert_eq!(report.verdict.degraded_tools, vec!["gamma".to_string()]);
        assert_eq!(report.tools[0].status, "degraded");
        assert!(report.tools[0].issues[0].contains("握手阶段"));
    }

    #[test]
    fn verdict_stays_unknown_without_enough_samples() {
        let mut stats = HashMap::new();
        let mut stat = ToolRunStat::default();
        for _ in 0..3 {
            stat.record(ToolRunOutcome::Ok, &traced(true, true), 100);
        }
        stats.insert("delta".to_string(), stat);
        let report = build_stats_report(&stats);
        // 样本不足不是「达标」，也不是「坏了」。
        assert_eq!(report.verdict.status, "unknown");
        assert_eq!(report.verdict.handshake_ok, None);
        assert_eq!(report.verdict.timeout_ok, None);
        assert_eq!(report.verdict.insufficient_tools, vec!["delta".to_string()]);
        assert_eq!(report.tools[0].status, "insufficient");
        assert!(report.tools[0].issues[0].contains("暂不判定"));
    }

    #[test]
    fn empty_report_is_unknown_not_pass() {
        let report = build_stats_report(&HashMap::new());
        assert_eq!(report.verdict.status, "unknown");
        assert_eq!(report.verdict.handshake_ok, None);
        assert_eq!(report.verdict.timeout_ok, None);
        assert!(report.verdict.degraded_tools.is_empty());
        assert!(report.verdict.summary.contains("样本不足"));
    }
}
