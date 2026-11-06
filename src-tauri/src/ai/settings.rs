//! AI configuration and secret storage.
//!
//! Non-secret settings are stored as JSON in the app config directory. The API
//! key never goes into that file: on Windows it is encrypted with DPAPI under
//! the current user account and written to a separate credential file.

use super::model::AiSettings;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const KEY_FILE: &str = "ai-credentials.bin";
const KEY_MAGIC: &[u8; 4] = b"SLK1";
const KEY_STORAGE_LABEL: &str = "Windows DPAPI（当前用户）";

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位配置目录：{error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("无法创建配置目录：{error}"))?;
    Ok(directory)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("ai-settings.json"))
}

fn key_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join(KEY_FILE))
}

pub fn load_settings(app: &AppHandle) -> AiSettings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<AiSettings>(&bytes).ok())
        .map(|settings| settings.sanitize())
        .unwrap_or_default()
}

pub fn save_settings(app: &AppHandle, settings: &AiSettings) -> Result<AiSettings, String> {
    let sanitized = settings.sanitize();
    validate_base_url(&sanitized.base_url)?;
    let path = settings_path(app)?;
    let bytes = serde_json::to_vec_pretty(&sanitized)
        .map_err(|error| format!("无法序列化 AI 设置：{error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("无法保存 AI 设置：{error}"))?;
    Ok(sanitized)
}

pub fn validate_base_url(base_url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(base_url.trim())
        .map_err(|_| "模型服务地址不是有效的 URL。".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("模型服务地址不能包含用户名或密码。".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("模型服务地址不能包含查询参数或片段。".to_string());
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = parsed.host_str().unwrap_or_default();
            let host = host.trim_start_matches('[').trim_end_matches(']');
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .map(|address| address.is_loopback())
                    .unwrap_or(false);
            if loopback {
                Ok(())
            } else {
                Err("模型服务地址必须使用 HTTPS；仅本机回环地址允许 HTTP。".to_string())
            }
        }
        _ => Err("模型服务地址仅支持 HTTP 或 HTTPS。".to_string()),
    }
}

pub fn key_storage_label() -> &'static str {
    KEY_STORAGE_LABEL
}

pub fn has_api_key(app: &AppHandle) -> bool {
    key_path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| bytes.starts_with(KEY_MAGIC) && bytes.len() > KEY_MAGIC.len())
        .unwrap_or(false)
}

pub fn load_api_key(app: &AppHandle) -> Result<Option<String>, String> {
    let path = key_path(app)?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取 AI 凭据：{error}")),
    };
    if !bytes.starts_with(KEY_MAGIC) {
        return Err("AI 凭据文件格式无效，请重新保存 API 密钥。".to_string());
    }
    let plain = unprotect(&bytes[KEY_MAGIC.len()..])?;
    let key = String::from_utf8(plain)
        .map_err(|_| "AI 凭据内容不是有效的 UTF-8 文本。".to_string())?;
    if key.is_empty() {
        Ok(None)
    } else {
        Ok(Some(key))
    }
}

pub fn save_api_key(app: &AppHandle, api_key: &str) -> Result<(), String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API 密钥不能为空。".to_string());
    }
    if key.len() > 4096 {
        return Err("API 密钥长度异常。".to_string());
    }
    let protected = protect(key.as_bytes())?;
    let mut payload = Vec::with_capacity(KEY_MAGIC.len() + protected.len());
    payload.extend_from_slice(KEY_MAGIC);
    payload.extend_from_slice(&protected);
    let path = key_path(app)?;
    std::fs::write(path, payload).map_err(|error| format!("无法保存 AI 凭据：{error}"))
}

pub fn clear_api_key(app: &AppHandle) -> Result<(), String> {
    match std::fs::remove_file(key_path(app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法删除 AI 凭据：{error}")),
    }
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "Windows 凭据保护失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(output.pbData as *const u8, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "无法解密 AI 凭据（可能由其他 Windows 用户保存）：{}",
            std::io::Error::last_os_error()
        ));
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(output.pbData as *const u8, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("AI 凭据保护目前仅支持 Windows。".to_string())
}

#[cfg(not(windows))]
fn unprotect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("AI 凭据保护目前仅支持 Windows。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_loopback_http() {
        assert!(validate_base_url("https://api.deepseek.com/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url("http://localhost:8080").is_ok());
        assert!(validate_base_url("http://[::1]:8080/v1").is_ok());
        assert!(validate_base_url("http://example.com/v1").is_err());
    }

    #[test]
    fn rejects_loopback_lookalikes_and_credentials() {
        assert!(validate_base_url("http://127.0.0.1.evil.com/v1").is_err());
        assert!(validate_base_url("http://127.0.0.1@evil.com/v1").is_err());
        assert!(validate_base_url("https://user:pass@api.example.com/v1").is_err());
        assert!(validate_base_url("ftp://example.com/v1").is_err());
        assert!(validate_base_url("https://api.example.com/v1?token=leak").is_err());
    }

    #[test]
    fn sanitizes_settings_ranges() {
        let settings = AiSettings {
            enabled: true,
            provider: "custom".into(),
            base_url: " https://api.example.com/v1/ ".into(),
            model: "  ".into(),
            max_output_tokens: 999_999,
            timeout_seconds: 1,
            ..AiSettings::default()
        }
        .sanitize();
        assert_eq!(settings.provider, "openai-compatible");
        assert_eq!(settings.base_url, "https://api.example.com/v1");
        assert_eq!(settings.model, AiSettings::default().model);
        assert_eq!(settings.max_output_tokens, 4096);
        assert_eq!(settings.timeout_seconds, 10);

        let prices = AiSettings {
            input_price_per_million: -1.0,
            output_price_per_million: f64::NAN,
            ..AiSettings::default()
        }
        .sanitize();
        assert_eq!(prices.input_price_per_million, 0.0);
        assert_eq!(prices.output_price_per_million, 0.0);

        let full_endpoint = AiSettings {
            base_url: "https://api.example.com/v1/chat/completions/".into(),
            ..AiSettings::default()
        }
        .sanitize();
        assert_eq!(full_endpoint.base_url, "https://api.example.com/v1");
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip() {
        let secret = "sk-test-密钥-123";
        let protected = protect(secret.as_bytes()).expect("protect");
        assert_ne!(protected.as_slice(), secret.as_bytes());
        let plain = unprotect(&protected).expect("unprotect");
        assert_eq!(String::from_utf8(plain).unwrap(), secret);
    }
}
