use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::path::PathBuf;
use tool_protocol::{column, digest_of, human_bytes, item, table, Request, ToolError};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_BOOST: &str = "boost";
const ACTION_UNDO: &str = "undo_boost";
const VISUAL_FX: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects";
const WINDOW_METRICS: &str = "Control Panel\\Desktop\\WindowMetrics";
const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function RegCount($paths,$names){
  $count=0
  foreach($path in $paths){
    foreach($name in $names){
      try { $count += @((Get-Item -Path $path -ErrorAction Stop).GetValueNames() | Where-Object { $name -eq '*' -or $_ -like $name }).Count } catch {}
    }
  }
  return $count
}
$runPaths=@('HKCU:\Software\Microsoft\Windows\CurrentVersion\Run','HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce','HKLM:\Software\Microsoft\Windows\CurrentVersion\Run','HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce')
$startupCount=0
foreach($path in $runPaths){
  try { $startupCount += @((Get-Item -Path $path -ErrorAction Stop).GetValueNames()).Count } catch {}
}
$autoServices=0
try { $autoServices=@(Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.StartMode -eq 'Auto' }).Count } catch {}
$tasks=0
try { $tasks=@(Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' }).Count } catch {}

function FileSize($path){ if(Test-Path $path){ try { return (Get-Item $path -Force -ErrorAction Stop).Length } catch { return 0 } } return 0 }
$hiber=FileSize (Join-Path $env:SystemDrive 'hiberfil.sys')
$pagefile=FileSize (Join-Path $env:SystemDrive 'pagefile.sys')
$hibernateEnabled=0
try { $hibernateEnabled=[int](Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' -Name 'HibernateEnabled' -ErrorAction Stop).HibernateEnabled } catch {}

$tempBytes=0L; $tempFiles=0
try {
  $files=@(Get-ChildItem -Path $env:TEMP -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 20000)
  $tempFiles=$files.Count
  if($tempFiles -gt 0){ $tempBytes=[long](($files | Measure-Object -Property Length -Sum).Sum) }
} catch {}

@{startupCount=$startupCount;autoServices=$autoServices;tasks=$tasks;hiberBytes=$hiber;hiberEnabled=$hibernateEnabled;pagefileBytes=$pagefile;tempBytes=$tempBytes;tempFiles=$tempFiles} | ConvertTo-Json -Compress
"#
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn build_rows(payload: &Value) -> Vec<Value> {
    let mut rows = Vec::new();

    rows.push(json!({
        "item": "启动项",
        "current": format!("{} 项", number(payload, "startupCount")),
        "suggestion": "非必要项可用“软件权限管理”停用当前用户启动项",
        "impact": "中",
        "risk": "需确认",
        "note": "统计 HKCU/HKLM 的 Run 与 RunOnce",
    }));

    rows.push(json!({
        "item": "自动服务",
        "current": format!("{} 个", number(payload, "autoServices")),
        "suggestion": "检查第三方自动服务是否必须随系统启动",
        "impact": "中",
        "risk": "需管理员",
        "note": "StartMode = Auto 的服务总数",
    }));

    rows.push(json!({
        "item": "用户计划任务",
        "current": format!("{} 个", number(payload, "tasks")),
        "suggestion": "停用不再需要的更新/同步任务",
        "impact": "低",
        "risk": "需确认",
        "note": "排除 Microsoft 系统任务与已禁用任务",
    }));

    let hiber_bytes = number(payload, "hiberBytes");
    rows.push(json!({
        "item": "休眠文件",
        "current": if hiber_bytes > 0 { human_bytes(hiber_bytes) } else { "未启用".to_string() },
        "suggestion": if hiber_bytes > 0 {
            "不使用休眠/快速启动时，可关闭休眠释放 hiberfil.sys 空间"
        } else {
            "已关闭或无休眠文件"
        },
        "impact": if hiber_bytes > 0 { "中" } else { "无" },
        "risk": "需管理员",
        "note": "关闭休眠会同时禁用快速启动",
    }));

    let pagefile_bytes = number(payload, "pagefileBytes");
    rows.push(json!({
        "item": "页面文件",
        "current": if pagefile_bytes > 0 { human_bytes(pagefile_bytes) } else { "未检测到".to_string() },
        "suggestion": "建议保持系统托管；不要为释放空间直接关闭",
        "impact": "低",
        "risk": "高风险",
        "note": "禁用页面文件可能导致崩溃或应用异常",
    }));

    rows.push(json!({
        "item": "用户临时文件",
        "current": human_bytes(number(payload, "tempBytes")),
        "suggestion": "可用“清理大师”扫描后移入回收站",
        "impact": "低",
        "risk": "需确认",
        "note": format!("{} 个文件（单目录最多统计 2 万）", number(payload, "tempFiles")),
    }));

    rows
}

fn scan() -> Result<Value, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取优化候选：{error}")))?;
    if !output.status.success() {
        return Err(ToolError::new(
            "INTERNAL",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .last()
        .ok_or_else(|| ToolError::new("INTERNAL", "优化候选扫描没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析优化候选：{error}")))?;
    let rows = build_rows(&payload);
    let attention = rows
        .iter()
        .filter(|row| row.get("impact").and_then(Value::as_str) != Some("无"))
        .count();

    let mut data = table(
        json!([
            item("候选类别", rows.len().to_string()),
            item("可关注项", attention.to_string()),
            item("启动项", number(&payload, "startupCount").to_string()),
            item("自动服务", number(&payload, "autoServices").to_string()),
            item("临时文件", human_bytes(number(&payload, "tempBytes"))),
        ]),
        json!([
            column("item", "候选"),
            column("current", "当前状态"),
            column("suggestion", "建议"),
            column("impact", "影响"),
            column("risk", "风险"),
            column("note", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "分析部分只读；“一键加速”只改视觉效果/动画/透明与电源计划，并先存快照。",
            "不修改启动项、服务、计划任务、休眠或页面文件。",
            "页面文件与休眠设置风险较高，不建议为“跑分”或“释放空间”直接关闭。",
        ]),
    );
    attach_boost_tokens(&mut data)?;
    Ok(data)
}

/// 为预览结果附加「加速」与「撤销」两枚确认令牌。
///
/// 两个动作必须各自签发：确认令牌把动作名签进 HMAC 载荷（`a` 字段），
/// 撤销动作以 `undo_boost` 校验。若两处复用同一枚加速令牌，
/// 撤销会被判为「确认令牌与当前动作不匹配」，撤销按钮永远失败。
fn attach_boost_tokens(data: &mut Value) -> Result<(), ToolError> {
    let digest = digest_of(&["sys_boost"]);
    let boost = tool_protocol::issue_token(ACTION_BOOST, &digest)?;
    let undo = tool_protocol::issue_token(ACTION_UNDO, &digest)?;
    if let Value::Object(map) = data {
        map.insert("boostToken".into(), Value::String(boost));
        map.insert("undoToken".into(), Value::String(undo));
    }
    Ok(())
}

fn read_dword(path: &str, name: &str) -> Option<u32> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

fn read_string(path: &str, name: &str) -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

fn write_dword(path: &str, name: &str, value: u32) -> Result<(), ToolError> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开注册表 {path}：{error}")))?;
    key.set_value(name, &value)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入 {name}：{error}")))
}

fn write_string(path: &str, name: &str, value: &str) -> Result<(), ToolError> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开注册表 {path}：{error}")))?;
    key.set_value(name, &value.to_string())
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入 {name}：{error}")))
}

fn delete_value(path: &str, name: &str) {
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_WRITE) {
        let _ = key.delete_value(name);
    }
}

fn active_scheme() -> Option<String> {
    let output = Command::new("powercfg.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/getactivescheme"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('{')?;
    let end = text[start..].find('}')? + start;
    Some(text[start..=end].to_string())
}

fn set_scheme(guid: &str) -> Result<(), ToolError> {
    let status = Command::new("powercfg.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/setactive", guid])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动 powercfg：{error}")))?;
    if !status.success() {
        return Err(ToolError::new("INTERNAL", "切换电源计划失败。"));
    }
    Ok(())
}

fn boost_dir() -> PathBuf {
    tool_protocol::state_dir().join("sys_boost")
}

fn boost_snapshot() -> Result<(Value, PathBuf), ToolError> {
    let visual = read_dword(VISUAL_FX, "VisualFXSetting");
    let animate = read_string(WINDOW_METRICS, "MinAnimate");
    let transparency = read_dword(PERSONALIZE, "EnableTransparency");
    let scheme = active_scheme();
    let body = json!({
        "visualFx": visual,
        "minAnimate": animate,
        "transparency": transparency,
        "scheme": scheme,
    });
    let dir = boost_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建快照目录：{error}")))?;
    let path = dir.join(format!("boost-{}.json", tool_protocol::now_ms()));
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&body)
            .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化快照：{error}")))?,
    )
    .map_err(|error| ToolError::new("INTERNAL", format!("无法写入快照：{error}")))?;
    Ok((body, path))
}

fn boost(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new("CONFIRMATION_REQUIRED", "加速需要确认令牌，请先预览。")
    })?;
    tool_protocol::verify_token(token, ACTION_BOOST, &digest_of(&["sys_boost"]))?;
    let (before, path) = boost_snapshot()?;

    write_dword(VISUAL_FX, "VisualFXSetting", 2)?;
    write_string(WINDOW_METRICS, "MinAnimate", "0")?;
    write_dword(PERSONALIZE, "EnableTransparency", 0)?;
    set_scheme("SCHEME_MIN")?;

    if read_dword(VISUAL_FX, "VisualFXSetting") != Some(2) {
        return Err(ToolError::new("INTERNAL", "写入视觉效果后回读不一致。"));
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "sys_boost",
        "action": ACTION_BOOST,
        "snapshot": path.to_string_lossy(),
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([
            item("视觉效果", "性能优先"),
            item("窗口动画", "关闭"),
            item("透明效果", "关闭"),
            item("电源计划", "高性能"),
            item("快照", path.to_string_lossy()),
        ]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{
            "item": "加速项",
            "value": "视觉性能优先 + 关闭动画/透明 + 高性能电源计划",
        }]),
        json!([
            "原值已写入快照，可点“撤销加速”恢复。",
            format!("原始电源计划：{}", before.get("scheme").and_then(Value::as_str).unwrap_or("未知")),
            "不修改服务、启动项或驱动；这些请使用软件权限管理/驱动管理。",
        ]),
    ))
}

fn undo_boost(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new("CONFIRMATION_REQUIRED", "撤销加速需要确认令牌，请先预览。")
    })?;
    tool_protocol::verify_token(token, ACTION_UNDO, &digest_of(&["sys_boost"]))?;
    let dir = boost_dir();
    // 快照目录可能压根不存在（用户还没加速过就点了撤销），这属于“还没有快照”，
    // 不是内部错误：交给下面的空列表分支给出可读提示。
    let mut files = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("boost-") && name.ends_with(".json"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(ToolError::new(
                "INTERNAL",
                format!("无法读取快照目录：{error}"),
            ))
        }
    };
    files.sort();
    let path = files
        .last()
        .ok_or_else(|| ToolError::new("CONFLICT", "还没有保存过加速快照。"))?
        .clone();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取快照：{error}")))?;
    let body: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析快照：{error}")))?;

    match body.get("visualFx").and_then(Value::as_u64) {
        Some(value) => write_dword(VISUAL_FX, "VisualFXSetting", value as u32)?,
        None => delete_value(VISUAL_FX, "VisualFXSetting"),
    }
    match body.get("minAnimate").and_then(Value::as_str) {
        Some(value) => write_string(WINDOW_METRICS, "MinAnimate", value)?,
        None => delete_value(WINDOW_METRICS, "MinAnimate"),
    }
    match body.get("transparency").and_then(Value::as_u64) {
        Some(value) => write_dword(PERSONALIZE, "EnableTransparency", value as u32)?,
        None => delete_value(PERSONALIZE, "EnableTransparency"),
    }
    if let Some(scheme) = body.get("scheme").and_then(Value::as_str) {
        set_scheme(scheme)?;
    }
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "sys_boost",
        "action": ACTION_UNDO,
        "snapshot": path.to_string_lossy(),
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([item("结果", "已恢复加速前快照")]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{"item": "快照", "value": path.to_string_lossy()}]),
        json!([
            "视觉/动画/透明与电源计划已按快照恢复。",
            "若某项原本不存在，则已删除该项。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "sys_boost",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_BOOST, ACTION_UNDO],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_BOOST => boost(request),
            ACTION_UNDO => undo_boost(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        json!({
            "startupCount": 12,
            "autoServices": 80,
            "tasks": 7,
            "hiberBytes": 6_442_450_944u64,
            "hiberEnabled": 1,
            "pagefileBytes": 2_147_483_648u64,
            "tempBytes": 1_073_741_824u64,
            "tempFiles": 1234
        })
    }

    #[test]
    fn builds_six_candidates_with_risk_labels() {
        let rows = build_rows(&fixture());
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().any(|row| row["item"] == "休眠文件"
            && row["risk"] == "需管理员"
            && row["current"] == "6.0 GB"));
        assert!(rows.iter().any(|row| row["item"] == "页面文件"
            && row["risk"] == "高风险"));
        assert!(rows.iter().any(|row| row["item"] == "用户临时文件"
            && row["current"] == "1.0 GB"));
    }

    #[test]
    fn missing_values_do_not_fabricate() {
        let rows = build_rows(&json!({}));
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().any(|row| row["item"] == "休眠文件"
            && row["current"] == "未启用"));
        assert!(rows.iter().any(|row| row["item"] == "启动项"
            && row["current"] == "0 项"));
    }

    const TOKEN_SECRET_ENV: &str = "SYSTEM_LENS_TOKEN_SECRET";
    const STATE_DIR_ENV: &str = "SYSTEM_LENS_STATE_DIR";
    const TEST_SECRET: &str = "00112233445566778899aabbccddeeff";

    /// 依赖进程环境变量的测试必须串行：环境是进程全局状态，
    /// 并行执行时一个测试 remove_var 会让另一个测试随机失败。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn boost_and_undo_tokens_are_issued_separately() {
        let _guard = env_guard();
        std::env::set_var(TOKEN_SECRET_ENV, TEST_SECRET);
        let mut data = json!({});
        attach_boost_tokens(&mut data).expect("签发令牌");

        let boost = data
            .get("boostToken")
            .and_then(Value::as_str)
            .expect("boostToken");
        let undo = data
            .get("undoToken")
            .and_then(Value::as_str)
            .expect("undoToken");
        assert_ne!(boost, undo, "加速与撤销必须使用不同的令牌");

        let digest = digest_of(&["sys_boost"]);
        assert!(tool_protocol::verify_token(boost, ACTION_BOOST, &digest).is_ok());
        assert!(tool_protocol::verify_token(undo, ACTION_UNDO, &digest).is_ok());
        // 交叉使用必须失败：旧实现把同一枚加速令牌塞给两个动作，
        // 撤销动作以 undo_boost 校验加速令牌时会被判为动作不匹配。
        assert!(tool_protocol::verify_token(boost, ACTION_UNDO, &digest).is_err());
        assert!(tool_protocol::verify_token(undo, ACTION_BOOST, &digest).is_err());

        std::env::remove_var(TOKEN_SECRET_ENV);
    }

    #[test]
    fn undo_without_snapshot_reports_conflict_not_internal() {
        let _guard = env_guard();
        std::env::set_var(TOKEN_SECRET_ENV, TEST_SECRET);
        // 指向一个不存在的目录：用户还没加速过就点了撤销，属于“还没有快照”，
        // 应当给出可读提示，而不是“无法读取快照目录”这种内部错误。
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-scratch")
            .join(format!("sys-boost-undo-{}", std::process::id()));
        std::env::set_var(STATE_DIR_ENV, &scratch);

        let token = tool_protocol::issue_token(ACTION_UNDO, &digest_of(&["sys_boost"]))
            .expect("签发撤销令牌");
        let request = Request {
            action: ACTION_UNDO.to_string(),
            parameters: json!({}),
            confirmation: Some(token),
        };
        let error = undo_boost(&request).expect_err("没有快照时应当失败");
        assert_eq!(error.code, "CONFLICT");
        assert!(
            error.message.contains("还没有保存过加速快照"),
            "提示文案应当可读，实际：{}",
            error.message
        );

        std::env::remove_var(STATE_DIR_ENV);
        std::env::remove_var(TOKEN_SECRET_ENV);
    }

    #[test]
    fn manifest_declares_every_served_action() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tool.json");
        let text = std::fs::read_to_string(&path).expect("读取 tool.json");
        let manifest: Value = serde_json::from_str(&text).expect("解析 tool.json");

        let declared = manifest
            .get("actions")
            .and_then(Value::as_array)
            .expect("tool.json 缺少 actions")
            .iter()
            .map(|action| {
                (
                    action.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                    action.get("write").and_then(Value::as_bool).unwrap_or(false),
                )
            })
            .collect::<Vec<_>>();

        // 宿主只在清单声明过的动作上放行；漏声明会让界面按钮直接报“不支持动作”。
        for action in ["preview", ACTION_BOOST, ACTION_UNDO] {
            assert!(
                declared.iter().any(|(id, _)| id == action),
                "tool.json 未声明动作 {action}，宿主会拒绝执行该动作"
            );
        }
        for (id, write) in &declared {
            if id == ACTION_BOOST || id == ACTION_UNDO {
                assert!(*write, "动作 {id} 必须声明为写操作");
            }
        }
        assert_ne!(
            manifest.get("risk").and_then(Value::as_str),
            Some("read_only"),
            "带写动作的工具 risk 不能是 read_only"
        );
    }
}
