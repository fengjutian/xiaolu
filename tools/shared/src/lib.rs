use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 确认令牌有效期：5 分钟。
pub const TOKEN_TTL_MS: u64 = 5 * 60 * 1000;
const TOKEN_SECRET_ENV: &str = "SYSTEM_LENS_TOKEN_SECRET";
const STATE_DIR_ENV: &str = "SYSTEM_LENS_STATE_DIR";

#[derive(Debug)]
pub struct ToolError {
    pub code: String,
    pub message: String,
}

impl ToolError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        ToolError {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unsupported(action: &str) -> Self {
        ToolError::new(
            "UNSUPPORTED_ACTION",
            format!("当前工具不支持动作 {action}，只提供只读预览。"),
        )
    }
}

fn write(out: &mut impl Write, message: Value) {
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

/// 宿主发来的请求。
pub struct Request {
    pub action: String,
    pub parameters: Value,
    pub confirmation: Option<String>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn secret_from_env() -> Result<Vec<u8>, ToolError> {
    let value = std::env::var(TOKEN_SECRET_ENV).map_err(|_| {
        ToolError::new(
            "CONFIGURATION",
            "缺少确认令牌密钥：工具必须由 System Lens 宿主启动。",
        )
    })?;
    decode_hex(value.trim())
        .map_err(|_| ToolError::new("CONFIGURATION", "确认令牌密钥格式无效。"))
}

fn decode_hex(text: &str) -> Result<Vec<u8>, ()> {
    if text.len() % 2 != 0 {
        return Err(());
    }
    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(text.len() / 2);
    for chunk in bytes.chunks(2) {
        let high = (chunk[0] as char).to_digit(16).ok_or(())?;
        let low = (chunk[1] as char).to_digit(16).ok_or(())?;
        result.push((high * 16 + low) as u8);
    }
    Ok(result)
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn hmac_sha256(secret: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC 接受任意长度密钥");
    mac.update(payload);
    mac.finalize().into_bytes().to_vec()
}

fn invalid_token(message: &str) -> ToolError {
    ToolError::new("INVALID_TOKEN", message)
}

/// 生成一次性确认令牌，签名绑定动作与对象摘要。
pub fn issue_token_with_secret(
    secret: &[u8],
    action: &str,
    digest: &str,
    ttl_ms: u64,
) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = now_ms();
    let nonce = format!(
        "{:x}-{:x}",
        now,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let payload = json!({
        "v": 1,
        "a": action,
        "d": digest,
        "e": now + ttl_ms,
        "n": nonce,
    })
    .to_string();
    let signature = hmac_sha256(secret, payload.as_bytes());
    format!("{}.{}", encode_hex(payload.as_bytes()), encode_hex(&signature))
}

/// 校验确认令牌：签名、动作、对象摘要与有效期。
pub fn verify_token_with_secret(
    secret: &[u8],
    token: &str,
    action: &str,
    digest: &str,
    now: u64,
) -> Result<(), ToolError> {
    let (payload_hex, signature_hex) = token
        .split_once('.')
        .ok_or_else(|| invalid_token("确认令牌格式无效。"))?;
    let payload = decode_hex(payload_hex).map_err(|_| invalid_token("确认令牌格式无效。"))?;
    let signature = decode_hex(signature_hex).map_err(|_| invalid_token("确认令牌格式无效。"))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| invalid_token("签名密钥无效。"))?;
    mac.update(&payload);
    mac.verify_slice(&signature)
        .map_err(|_| invalid_token("确认令牌签名无效。"))?;
    let claims: Value = serde_json::from_slice(&payload)
        .map_err(|_| invalid_token("确认令牌内容无效。"))?;
    if claims.get("a").and_then(Value::as_str) != Some(action) {
        return Err(invalid_token("确认令牌与当前动作不匹配。"));
    }
    if claims.get("d").and_then(Value::as_str) != Some(digest) {
        return Err(invalid_token("确认令牌与当前对象不匹配，对象可能已变化。"));
    }
    let expires_at = claims.get("e").and_then(Value::as_u64).unwrap_or(0);
    if expires_at < now {
        return Err(ToolError::new("TOKEN_EXPIRED", "确认令牌已过期，请重新预览。"));
    }
    Ok(())
}

/// 使用宿主下发的密钥生成令牌。
pub fn issue_token(action: &str, digest: &str) -> Result<String, ToolError> {
    let secret = secret_from_env()?;
    Ok(issue_token_with_secret(&secret, action, digest, TOKEN_TTL_MS))
}

/// 使用宿主下发的密钥校验令牌。
pub fn verify_token(token: &str, action: &str, digest: &str) -> Result<(), ToolError> {
    let secret = secret_from_env()?;
    verify_token_with_secret(&secret, token, action, digest, now_ms())
}

/// 把确认令牌附加到结果对象，供宿主在用户确认后回传。
pub fn attach_token(data: &mut Value, token: String) {
    if let Value::Object(map) = data {
        map.insert("confirmationToken".into(), Value::String(token));
        map.insert(
            "confirmationTokenExpiresAt".into(),
            json!(now_ms() + TOKEN_TTL_MS),
        );
    }
}

/// 对象列表摘要：用于把令牌绑定到预览时看到的那一组对象。
pub fn digest_of(parts: &[&str]) -> String {
    let mut sorted = parts.to_vec();
    sorted.sort_unstable();
    let mut hasher = Sha256::new();
    for part in sorted {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    encode_hex(&hasher.finalize())
}

pub fn state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(STATE_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(base).join("SystemLens");
    }
    std::env::temp_dir().join("SystemLens")
}

/// 追加一行本地操作审计日志，不记录文件内容。
pub fn append_audit_log(entry: Value) -> Result<(), ToolError> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建日志目录：{error}")))?;
    let path = dir.join("tool-actions.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入操作日志：{error}")))?;
    writeln!(file, "{entry}")
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入操作日志：{error}")))
}

/// 标准工具进程入口：握手、读取请求、输出结果。
/// handler 接收 action，返回 `data`；错误统一转为 `error` 消息。
pub fn serve<F>(tool_id: &str, version: &str, actions: &[&str], handler: F)
where
    F: Fn(&str) -> Result<Value, ToolError>,
{
    serve_with_request(tool_id, version, actions, |request| {
        handler(&request.action)
    });
}

/// 与 [`serve`] 相同，但把完整的请求（含参数与确认令牌）交给 handler。
pub fn serve_with_request<F>(tool_id: &str, version: &str, actions: &[&str], handler: F)
where
    F: Fn(&Request) -> Result<Value, ToolError>,
{
    if !std::env::args().any(|argument| argument == "--stdio") {
        eprintln!("{tool_id}: 仅支持由 System Lens 以 --stdio 启动");
        std::process::exit(2);
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    write(
        &mut stdout,
        json!({
            "type": "hello",
            "protocol": 1,
            "toolId": tool_id,
            "version": version,
            "actions": actions,
            "elevated": false
        }),
    );

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                write(
                    &mut stdout,
                    json!({"type":"error","id":0,"code":"INVALID_INPUT","message":"请求不是合法 JSON"}),
                );
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(json!(0));
        let action = request
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !actions.contains(&action.as_str()) {
            write(
                &mut stdout,
                json!({
                    "type":"error","id":id,"code":"UNSUPPORTED_ACTION",
                    "message": ToolError::unsupported(&action).message
                }),
            );
            continue;
        }
        let parsed = Request {
            action,
            parameters: request
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({})),
            confirmation: request
                .get("confirmation")
                .and_then(Value::as_str)
                .filter(|token| !token.trim().is_empty())
                .map(str::to_string),
        };
        match handler(&parsed) {
            Ok(data) => write(
                &mut stdout,
                json!({"type":"result","id":id,"ok":true,"data":data,"warnings":[]}),
            ),
            Err(error) => write(
                &mut stdout,
                json!({
                    "type":"error","id":id,"code":error.code,
                    "message":error.message
                }),
            ),
        }
    }
}

/// 通用表格结果：summary + columns + rows + notes。
pub fn table(summary: Value, columns: Value, rows: Value, notes: Value) -> Value {
    json!({
        "summary": summary,
        "columns": columns,
        "rows": rows,
        "notes": notes
    })
}

pub fn item(label: &str, value: impl Into<String>) -> Value {
    json!({"label": label, "value": value.into()})
}

pub fn column(key: &str, label: &str) -> Value {
    json!({"key": key, "label": label})
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-key";

    #[test]
    fn token_roundtrip_binds_action_and_digest() {
        let token = issue_token_with_secret(SECRET, "apply", "digest-a", TOKEN_TTL_MS);
        assert!(verify_token_with_secret(SECRET, &token, "apply", "digest-a", now_ms()).is_ok());
        assert_eq!(
            verify_token_with_secret(SECRET, &token, "lock", "digest-a", now_ms())
                .unwrap_err()
                .code,
            "INVALID_TOKEN"
        );
        assert_eq!(
            verify_token_with_secret(SECRET, &token, "apply", "digest-b", now_ms())
                .unwrap_err()
                .code,
            "INVALID_TOKEN"
        );
    }

    #[test]
    fn token_rejects_tampering_and_expiry() {
        let token = issue_token_with_secret(SECRET, "apply", "digest-a", 1_000);
        let mut parts = token.split('.');
        let payload = parts.next().expect("payload");
        let signature = parts.next().expect("signature");
        let tampered = format!("{payload}00.{signature}");
        assert_eq!(
            verify_token_with_secret(SECRET, &tampered, "apply", "digest-a", now_ms())
                .unwrap_err()
                .code,
            "INVALID_TOKEN"
        );
        assert_eq!(
            verify_token_with_secret(SECRET, &token, "apply", "digest-a", u64::MAX)
                .unwrap_err()
                .code,
            "TOKEN_EXPIRED"
        );
        assert!(verify_token_with_secret(SECRET, "not-a-token", "apply", "x", now_ms()).is_err());
    }

    #[test]
    fn digest_is_order_independent() {
        assert_eq!(digest_of(&["b", "a"]), digest_of(&["a", "b"]));
        assert_ne!(digest_of(&["a", "b"]), digest_of(&["a", "c"]));
    }

    #[test]
    fn audit_log_appends_json_line() {
        // 暂存目录放在测试二进制旁边，而不是系统临时目录：
        // 单元测试不应依赖 %TEMP% 可写（受限沙箱/CI 会直接拒绝），否则整套测试会假红。
        // 用 current_exe 定位也让它与 target 目录布局无关（构建产物可能放在共享目录里）。
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent().map(|parent| {
                    parent.join(format!("system-lens-audit-{}", std::process::id()))
                })
            })
            .expect("测试二进制路径");
        std::env::set_var(STATE_DIR_ENV, &dir);
        append_audit_log(json!({"tool": "test", "action": "run"})).expect("write audit log");
        let text = std::fs::read_to_string(dir.join("tool-actions.jsonl")).expect("read log");
        std::env::remove_var(STATE_DIR_ENV);
        std::fs::remove_dir_all(&dir).ok();
        assert!(text.contains("\"tool\":\"test\""));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
