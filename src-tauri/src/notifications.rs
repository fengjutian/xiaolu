use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// 桌面通知设置：总开关、同一告警冷却时间与免打扰时段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub cooldown_minutes: u32,
    pub quiet_start_hour: u32,
    pub quiet_end_hour: u32,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        NotificationSettings {
            enabled: true,
            cooldown_minutes: 10,
            quiet_start_hour: 23,
            quiet_end_hour: 8,
        }
    }
}

impl NotificationSettings {
    pub fn sanitize(&self) -> NotificationSettings {
        NotificationSettings {
            enabled: self.enabled,
            cooldown_minutes: self.cooldown_minutes.clamp(1, 60),
            quiet_start_hour: self.quiet_start_hour.min(23),
            quiet_end_hour: self.quiet_end_hour.min(23),
        }
    }
}

/// 进程内冷却状态：同一告警 ID 在冷却时间内只弹一次。
#[derive(Default)]
pub struct NotificationRuntime {
    last_sent: Mutex<HashMap<String, u64>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationOutcome {
    pub status: String,
    pub message: String,
}

fn outcome(status: &str, message: impl Into<String>) -> NotificationOutcome {
    NotificationOutcome {
        status: status.into(),
        message: message.into(),
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 免打扰判断：`start == end` 表示不启用；跨零点时段（如 23–8）按环处理。
pub fn is_quiet_hour(hour: u32, start: u32, end: u32) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位配置目录：{error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建配置目录：{error}"))?;
    Ok(directory.join("notifications.json"))
}

fn load_settings(app: &AppHandle) -> NotificationSettings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<NotificationSettings>(&bytes).ok())
        .map(|settings| settings.sanitize())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &NotificationSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("无法序列化通知设置：{error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("无法保存通知设置：{error}"))
}

#[cfg(windows)]
fn local_hour() -> u32 {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTimeParts {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(time: *mut SystemTimeParts);
    }
    let mut time = SystemTimeParts::default();
    // SAFETY: 结构体布局与 Win32 SYSTEMTIME 一致。
    unsafe { GetLocalTime(&mut time) };
    u32::from(time.hour)
}

#[cfg(not(windows))]
fn local_hour() -> u32 {
    ((now_ms() / 3_600_000) % 24) as u32
}

fn show_toast(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| format!("系统通知发送失败：{error}"))
}

#[tauri::command]
pub async fn get_notification_settings(app: AppHandle) -> Result<NotificationSettings, String> {
    Ok(load_settings(&app))
}

#[tauri::command]
pub async fn save_notification_settings(
    app: AppHandle,
    settings: NotificationSettings,
) -> Result<NotificationSettings, String> {
    let sanitized = settings.sanitize();
    save_settings(&app, &sanitized)?;
    Ok(sanitized)
}

/// 传感器告警统一入口：关闭/免打扰/冷却时只记录状态，不弹窗。
#[tauri::command]
pub async fn notify_sensor_alert(
    app: AppHandle,
    runtime: State<'_, NotificationRuntime>,
    alert_id: String,
    level: String,
    title: String,
    message: String,
) -> Result<NotificationOutcome, String> {
    let settings = load_settings(&app);
    if !settings.enabled {
        return Ok(outcome("disabled", "桌面通知已关闭，仅保留应用内告警。"));
    }
    if is_quiet_hour(local_hour(), settings.quiet_start_hour, settings.quiet_end_hour) {
        return Ok(outcome(
            "quiet",
            format!(
                "免打扰时段（{}–{} 点）内只记录，不弹窗。",
                settings.quiet_start_hour, settings.quiet_end_hour
            ),
        ));
    }
    let now = now_ms();
    let cooldown_ms = u64::from(settings.cooldown_minutes) * 60_000;
    {
        let mut last_sent = runtime
            .last_sent
            .lock()
            .map_err(|_| "通知冷却状态不可用".to_string())?;
        if let Some(previous) = last_sent.get(&alert_id).copied() {
            if now.saturating_sub(previous) < cooldown_ms {
                return Ok(outcome("cooldown", "同一告警仍在冷却期内，未重复弹窗。"));
            }
        }
        last_sent.insert(alert_id.clone(), now);
        if last_sent.len() > 256 {
            last_sent.clear();
            last_sent.insert(alert_id.clone(), now);
        }
    }
    let title = if title.trim().is_empty() {
        "小鲁 · 温度告警".to_string()
    } else {
        title
    };
    let body = if message.trim().is_empty() {
        format!("传感器告警级别：{level}")
    } else {
        message
    };
    match show_toast(&app, &title, &body) {
        Ok(()) => Ok(outcome(
            "sent",
            "已发送 Windows 通知；未看到时请检查系统“通知和操作”设置。",
        )),
        Err(error) => {
            // 发送失败不应占用冷却时间，允许用户修复系统设置后重试。
            if let Ok(mut last_sent) = runtime.last_sent.lock() {
                if last_sent.get(&alert_id) == Some(&now) {
                    last_sent.remove(&alert_id);
                }
            }
            Ok(outcome("failed", error))
        }
    }
}

/// 设置页“测试通知”按钮：跳过冷却与免打扰，直接验证系统通知权限。
#[tauri::command]
pub async fn send_test_notification(app: AppHandle) -> Result<NotificationOutcome, String> {
    show_toast(
        &app,
        "小鲁 · 通知测试",
        "如果你看到这条消息，说明桌面通知已可用。",
    )
    .map(|()| outcome("sent", "测试通知已发送。"))
    .or_else(|error| Ok(outcome("failed", error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_settings_ranges() {
        let settings = NotificationSettings {
            enabled: true,
            cooldown_minutes: 0,
            quiet_start_hour: 30,
            quiet_end_hour: 99,
        }
        .sanitize();
        assert_eq!(settings.cooldown_minutes, 1);
        assert_eq!(settings.quiet_start_hour, 23);
        assert_eq!(settings.quiet_end_hour, 23);

        let settings = NotificationSettings {
            enabled: false,
            cooldown_minutes: 999,
            quiet_start_hour: 0,
            quiet_end_hour: 0,
        }
        .sanitize();
        assert_eq!(settings.cooldown_minutes, 60);
        assert!(!settings.enabled);
    }

    #[test]
    fn quiet_hour_handles_same_day_and_wrap_around() {
        // 23–8 跨零点。
        assert!(is_quiet_hour(23, 23, 8));
        assert!(is_quiet_hour(2, 23, 8));
        assert!(!is_quiet_hour(8, 23, 8));
        assert!(!is_quiet_hour(12, 23, 8));
        // 同日时段。
        assert!(is_quiet_hour(13, 12, 14));
        assert!(!is_quiet_hour(14, 12, 14));
        // 起止相同视为关闭免打扰。
        assert!(!is_quiet_hour(23, 0, 0));
    }
}
