use serde_json::{json, Value};
use tool_protocol::{attach_token, column, item, table, Request, ToolError};

const ACTION: &str = "lock";
const DIGEST: &str = "workstation-lock";

#[link(name = "user32")]
extern "system" {
    fn LockWorkStation() -> i32;
}

fn preview() -> Result<Value, ToolError> {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "未知".into());
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "未知".into());
    let token = tool_protocol::issue_token(ACTION, DIGEST)?;

    let mut data = table(
        json!([
            item("当前用户", user),
            item("计算机", host),
            item("锁定方式", "Windows 会话锁定"),
        ]),
        json!([
            column("item", "项目"),
            column("value", "说明"),
        ]),
        json!([
            { "item": "执行后立即生效", "value": "屏幕锁定，需要重新登录解锁" },
            { "item": "系统修改", "value": "不结束进程、不修改文件或注册表" },
            { "item": "令牌有效期", "value": "5 分钟，只能使用一次" },
        ]),
        json!([
            "预览只读取当前用户与计算机名，不锁定屏幕。",
            "点击执行后，Windows 会立即锁定当前会话；这是不可撤销的界面动作。",
            "本工具不复制第三方自定义锁屏或注入行为，只调用系统 LockWorkStation。",
        ]),
    );
    attach_token(&mut data, token);
    Ok(data)
}

fn lock(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "锁屏需要确认令牌，请先预览并确认。",
        )
    })?;
    tool_protocol::verify_token(token, ACTION, DIGEST)?;

    // SAFETY: LockWorkStation 无参数，失败时返回 0。
    let ok = unsafe { LockWorkStation() };
    if ok == 0 {
        let code = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(0);
        return Err(ToolError::new(
            "INTERNAL",
            format!("LockWorkStation 调用失败（错误码 {code}）。"),
        ));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "lockscreen",
        "action": ACTION,
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([item("结果", "已请求锁定当前会话")]),
        json!([]),
        json!([]),
        json!([
            "锁定动作已提交给 Windows。",
            "解锁后可在“工具箱”重新预览；本次操作已写入本地操作日志。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "lockscreen",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => lock(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable() {
        assert_eq!(DIGEST, "workstation-lock");
        assert_eq!(ACTION, "lock");
    }
}
