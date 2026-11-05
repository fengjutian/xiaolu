use serde_json::{json, Value};
use std::io::{BufRead, Write};

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

/// 标准工具进程入口：握手、读取请求、输出结果。
/// handler 接收 action，返回 `data`；错误统一转为 `error` 消息。
pub fn serve<F>(tool_id: &str, version: &str, actions: &[&str], handler: F)
where
    F: Fn(&str) -> Result<Value, ToolError>,
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
            .unwrap_or_default();
        if !actions.contains(&action) {
            write(
                &mut stdout,
                json!({
                    "type":"error","id":id,"code":"UNSUPPORTED_ACTION",
                    "message": ToolError::unsupported(action).message
                }),
            );
            continue;
        }
        match handler(action) {
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

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
