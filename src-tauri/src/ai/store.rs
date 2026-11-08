//! Local-only AI session records.
//!
//! Sessions are written to the app data directory so users can review and
//! delete them later. They contain the model answer, not the raw facts; the
//! API key and diagnostic payload are never persisted here.

use super::model::AiAnswer;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const MAX_SESSIONS: usize = 50;
static STORE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionRecord {
    pub id: String,
    pub saved_at_unix_ms: u64,
    pub question: Option<String>,
    pub answer: AiAnswer,
    #[serde(default)]
    pub feedback: Option<String>,
}

fn sessions_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位数据目录：{error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建数据目录：{error}"))?;
    Ok(directory.join("ai-sessions.json"))
}

fn read_sessions(app: &AppHandle) -> Vec<AiSessionRecord> {
    sessions_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Vec<AiSessionRecord>>(&bytes).ok())
        .unwrap_or_default()
}

fn write_sessions(app: &AppHandle, sessions: &[AiSessionRecord]) -> Result<(), String> {
    let path = sessions_path(app)?;
    let bytes = serde_json::to_vec_pretty(sessions)
        .map_err(|error| format!("无法序列化 AI 会话：{error}"))?;
    // 先写同目录临时文件再替换：崩溃/写满时不会留下半个 JSON，也不会让
    // read_sessions 把整份历史当成损坏数据丢掉。
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes).map_err(|error| format!("无法保存 AI 会话：{error}"))?;
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("无法保存 AI 会话：{error}"));
    }
    Ok(())
}

pub fn append(app: &AppHandle, question: Option<String>, answer: &AiAnswer) -> Result<(), String> {
    let _guard = match STORE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut sessions = read_sessions(app);
    let saved_at = super::now_unix_ms();
    sessions.push(AiSessionRecord {
        id: format!("{}-{saved_at}", answer.request_id),
        saved_at_unix_ms: saved_at,
        question,
        answer: answer.clone(),
        feedback: None,
    });
    if sessions.len() > MAX_SESSIONS {
        sessions.sort_by_key(|record| record.saved_at_unix_ms);
        let excess = sessions.len() - MAX_SESSIONS;
        sessions.drain(0..excess);
    }
    write_sessions(app, &sessions)
}

pub fn list(app: &AppHandle) -> Vec<AiSessionRecord> {
    let _guard = match STORE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut sessions = read_sessions(app);
    sessions.sort_by(|left, right| right.saved_at_unix_ms.cmp(&left.saved_at_unix_ms));
    sessions.truncate(MAX_SESSIONS);
    sessions
}

pub fn set_feedback(
    app: &AppHandle,
    request_id: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    let _guard = match STORE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut sessions = read_sessions(app);
    let mut changed = false;
    for record in sessions.iter_mut() {
        if record.answer.request_id == request_id {
            record.feedback = feedback.map(str::to_string);
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    write_sessions(app, &sessions)
}

pub fn delete(app: &AppHandle, session_id: &str) -> Result<(), String> {    let _guard = match STORE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut sessions = read_sessions(app);
    sessions.retain(|record| record.id != session_id);
    write_sessions(app, &sessions)
}

pub fn clear(app: &AppHandle) -> Result<(), String> {
    let _guard = match STORE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    match std::fs::remove_file(sessions_path(app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清除 AI 会话：{error}")),
    }
}
