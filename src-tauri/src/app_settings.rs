use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub snapshot_interval_seconds: u32,
    pub metrics_retention_days: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            snapshot_interval_seconds: 2,
            metrics_retention_days: 7,
        }
    }
}

impl AppSettings {
    pub fn sanitize(&self) -> AppSettings {
        AppSettings {
            snapshot_interval_seconds: self.snapshot_interval_seconds.clamp(1, 30),
            metrics_retention_days: self.metrics_retention_days.clamp(1, 30),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub data_dir: String,
    pub config_dir: String,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位配置目录：{error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建配置目录：{error}"))?;
    Ok(directory.join("app-settings.json"))
}

pub fn load(app: &AppHandle) -> AppSettings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<AppSettings>(&bytes).ok())
        .map(|settings| settings.sanitize())
        .unwrap_or_default()
}

fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("无法序列化设置：{error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("无法保存设置：{error}"))
}

#[tauri::command]
pub async fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(load(&app))
}

#[tauri::command]
pub async fn save_app_settings(
    app: AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let sanitized = settings.sanitize();
    save(&app, &sanitized)?;
    Ok(sanitized)
}

#[tauri::command]
pub async fn get_app_paths(app: AppHandle) -> Result<AppPaths, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位数据目录：{error}"))?;
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位配置目录：{error}"))?;
    let _ = std::fs::create_dir_all(&data_dir);
    let _ = std::fs::create_dir_all(&config_dir);
    Ok(AppPaths {
        data_dir: data_dir.to_string_lossy().into_owned(),
        config_dir: config_dir.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_out_of_range_settings() {
        let settings = AppSettings {
            snapshot_interval_seconds: 0,
            metrics_retention_days: 99,
        }
        .sanitize();
        assert_eq!(settings.snapshot_interval_seconds, 1);
        assert_eq!(settings.metrics_retention_days, 30);

        let settings = AppSettings {
            snapshot_interval_seconds: 120,
            metrics_retention_days: 0,
        }
        .sanitize();
        assert_eq!(settings.snapshot_interval_seconds, 30);
        assert_eq!(settings.metrics_retention_days, 1);
    }
}
