use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// 每个显示器的原始 Gamma Ramp（256 × RGB 的 u16）。
static ORIGINAL_RAMPS: Mutex<Vec<(String, Vec<u16>)>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EyeCareSettings {
    pub enabled: bool,
    /// 色温强度 0–100；0 表示不偏色。
    pub warmth: u32,
    pub break_interval_minutes: u32,
    pub break_duration_seconds: u32,
    pub schedule_enabled: bool,
    pub schedule_start_hour: u32,
    pub schedule_end_hour: u32,
}

impl Default for EyeCareSettings {
    fn default() -> Self {
        EyeCareSettings {
            enabled: false,
            warmth: 40,
            break_interval_minutes: 45,
            break_duration_seconds: 20,
            schedule_enabled: false,
            schedule_start_hour: 20,
            schedule_end_hour: 8,
        }
    }
}

impl EyeCareSettings {
    pub fn sanitize(&self) -> Self {
        EyeCareSettings {
            enabled: self.enabled,
            warmth: self.warmth.min(100),
            break_interval_minutes: self.break_interval_minutes.clamp(5, 180),
            break_duration_seconds: self.break_duration_seconds.clamp(5, 300),
            schedule_enabled: self.schedule_enabled,
            schedule_start_hour: self.schedule_start_hour.min(23),
            schedule_end_hour: self.schedule_end_hour.min(23),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EyeCareStatus {
    pub applied: bool,
    pub displays: Vec<String>,
    pub failed: Vec<String>,
    pub message: String,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DisplayDeviceW {
    cb: u32,
    device_name: [u16; 32],
    device_string: [u16; 128],
    state_flags: u32,
    device_id: [u16; 128],
    device_key: [u16; 128],
}

impl Default for DisplayDeviceW {
    fn default() -> Self {
        DisplayDeviceW {
            cb: std::mem::size_of::<DisplayDeviceW>() as u32,
            device_name: [0; 32],
            device_string: [0; 128],
            state_flags: 0,
            device_id: [0; 128],
            device_key: [0; 128],
        }
    }
}

#[link(name = "user32")]
extern "system" {
    fn EnumDisplayDevicesW(
        device: *const u16,
        index: u32,
        display: *mut DisplayDeviceW,
        flags: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateDCW(
        driver: *const u16,
        device: *const u16,
        port: *const u16,
        init: *const c_void,
    ) -> *mut c_void;
    fn DeleteDC(dc: *mut c_void) -> i32;
    fn GetDeviceGammaRamp(dc: *mut c_void, ramp: *mut c_void) -> i32;
    fn SetDeviceGammaRamp(dc: *mut c_void, ramp: *const c_void) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn display_names() -> Vec<String> {
    let mut names = Vec::new();
    for index in 0..16u32 {
        let mut device = DisplayDeviceW::default();
        // SAFETY: 结构体布局与 DISPLAY_DEVICEW 一致，cb 已填。
        let ok = unsafe {
            EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0)
        };
        if ok == 0 {
            break;
        }
        let end = device
            .device_name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(device.device_name.len());
        let name = String::from_utf16_lossy(&device.device_name[..end]);
        if !name.trim().is_empty() {
            names.push(name);
        }
    }
    names
}

fn read_ramp(device: &str) -> Option<Vec<u16>> {
    let driver = wide("DISPLAY");
    let device_wide = wide(device);
    // SAFETY: 设备名以 NUL 结尾；DC 使用后立即释放。
    let dc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            device_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if dc.is_null() {
        return None;
    }
    let mut ramp = vec![0u16; 768];
    // SAFETY: ramp 为 3×256 个 u16 的可写缓冲区。
    let ok = unsafe { GetDeviceGammaRamp(dc, ramp.as_mut_ptr() as *mut c_void) };
    unsafe {
        DeleteDC(dc);
    }
    (ok != 0).then_some(ramp)
}

fn write_ramp(device: &str, ramp: &[u16]) -> bool {
    let driver = wide("DISPLAY");
    let device_wide = wide(device);
    // SAFETY: 设备名以 NUL 结尾；DC 使用后立即释放。
    let dc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            device_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if dc.is_null() {
        return false;
    }
    // SAFETY: ramp 至少包含 768 个 u16。
    let ok = unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr() as *const c_void) };
    unsafe {
        DeleteDC(dc);
    }
    ok != 0
}

/// 暖色调整：降低蓝/绿通道，红色保持。
pub fn warm_ramp(original: &[u16], warmth: u32) -> Vec<u16> {
    let mut ramp = original.to_vec();
    if ramp.len() < 768 {
        return ramp;
    }
    let weight = (warmth.min(100) as f64) / 100.0;
    let green_scale = 1.0 - weight * 0.12;
    let blue_scale = 1.0 - weight * 0.38;
    for index in 0..256 {
        let green = f64::from(ramp[256 + index]) * green_scale;
        let blue = f64::from(ramp[512 + index]) * blue_scale;
        ramp[256 + index] = green.round().clamp(0.0, 65535.0) as u16;
        ramp[512 + index] = blue.round().clamp(0.0, 65535.0) as u16;
    }
    ramp
}

fn apply(enabled: bool, warmth: u32) -> EyeCareStatus {
    let mut originals = match ORIGINAL_RAMPS.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return EyeCareStatus {
                applied: false,
                displays: Vec::new(),
                failed: Vec::new(),
                message: "护眼滤镜状态不可用，请重启应用。".into(),
            }
        }
    };

    if !enabled {
        let mut failed = Vec::new();
        for (device, ramp) in originals.iter() {
            if !write_ramp(device, ramp) {
                failed.push(device.clone());
            }
        }
        let displays = originals.iter().map(|(device, _)| device.clone()).collect::<Vec<_>>();
        originals.clear();
        drop(originals);
        return EyeCareStatus {
            applied: false,
            displays,
            failed: failed.clone(),
            message: if failed.is_empty() {
                "已恢复原始色温。".into()
            } else {
                format!("部分显示器恢复失败：{}", failed.join("、"))
            },
        };
    }

    if originals.is_empty() {
        for device in display_names() {
            if let Some(ramp) = read_ramp(&device) {
                originals.push((device, ramp));
            }
        }
    }
    if originals.is_empty() {
        return EyeCareStatus {
            applied: false,
            displays: Vec::new(),
            failed: Vec::new(),
            message: "没有可用的显示器或驱动程序不允许调整色温。".into(),
        };
    }

    let mut displays = Vec::new();
    let mut failed = Vec::new();
    for (device, original) in originals.iter() {
        let ramp = warm_ramp(original, warmth);
        if write_ramp(device, &ramp) {
            displays.push(device.clone());
        } else {
            failed.push(device.clone());
        }
    }
    drop(originals);

    let applied = !displays.is_empty();
    EyeCareStatus {
        applied,
        message: if !applied {
            "色温调整失败：驱动可能不支持 Gamma Ramp，或当前处于 HDR/独占全屏模式。".into()
        } else if failed.is_empty() {
            format!("已应用 {warmth}% 暖色滤镜。")
        } else {
            format!("已应用，{} 个显示器失败。", failed.len())
        },
        displays,
        failed,
    }
}

pub fn restore_filter() {
    let _ = apply(false, 0);
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位配置目录：{error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建配置目录：{error}"))?;
    Ok(directory.join("eye-care.json"))
}

pub fn load_settings(app: &AppHandle) -> EyeCareSettings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<EyeCareSettings>(&bytes).ok())
        .map(|settings| settings.sanitize())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &EyeCareSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("无法序列化护眼设置：{error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("无法保存护眼设置：{error}"))
}

/// 应用启动时恢复上次启用状态；失败不影响主界面。
pub fn restore_on_startup(app: &AppHandle) {
    let settings = load_settings(app);
    if settings.enabled {
        let _ = apply(true, settings.warmth);
    }
}

#[tauri::command]
pub async fn get_eye_care_settings(app: AppHandle) -> Result<EyeCareSettings, String> {
    Ok(load_settings(&app))
}

#[tauri::command]
pub async fn save_eye_care_settings(
    app: AppHandle,
    settings: EyeCareSettings,
) -> Result<EyeCareSettings, String> {
    let sanitized = settings.sanitize();
    save_settings(&app, &sanitized)?;
    Ok(sanitized)
}

#[tauri::command]
pub async fn set_eye_care_filter(enabled: bool, warmth: u32) -> Result<EyeCareStatus, String> {
    tauri::async_runtime::spawn_blocking(move || apply(enabled, warmth.min(100)))
        .await
        .map_err(|error| format!("护眼滤镜任务失败：{error}"))
}

/// 应用退出或切换回默认色温时调用。
pub fn shutdown() {
    restore_filter();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Vec<u16> {
        let mut ramp = vec![0u16; 768];
        for index in 0..256 {
            let value = (index as u32 * 257) as u16;
            ramp[index] = value;
            ramp[256 + index] = value;
            ramp[512 + index] = value;
        }
        ramp
    }

    #[test]
    fn zero_warmth_keeps_original_ramp() {
        let original = baseline();
        assert_eq!(warm_ramp(&original, 0), original);
    }

    #[test]
    fn warmth_reduces_green_and_blue_but_not_red() {
        let original = baseline();
        let warm = warm_ramp(&original, 100);
        assert_eq!(warm[..256], original[..256]);
        assert!(warm[456] < original[456]);
        assert!(warm[712] < original[712]);
        assert!(warm[712] < warm[456], "blue should drop more than green");
    }

    #[test]
    fn warmth_is_clamped_to_valid_range() {
        let original = baseline();
        assert_eq!(warm_ramp(&original, 999), warm_ramp(&original, 100));
    }

    #[test]
    fn settings_are_sanitized() {
        let settings = EyeCareSettings {
            enabled: true,
            warmth: 500,
            break_interval_minutes: 0,
            break_duration_seconds: 1,
            schedule_enabled: true,
            schedule_start_hour: 99,
            schedule_end_hour: 42,
        }
        .sanitize();
        assert_eq!(settings.warmth, 100);
        assert_eq!(settings.break_interval_minutes, 5);
        assert_eq!(settings.break_duration_seconds, 5);
        assert_eq!(settings.schedule_start_hour, 23);
        assert_eq!(settings.schedule_end_hour, 23);
    }
}
