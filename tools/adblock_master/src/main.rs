use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ACTION_HOSTS: &str = "hosts_write";
const HOSTS_BEGIN: &str = "# BEGIN SystemLens AdBlock";
const HOSTS_END: &str = "# END SystemLens AdBlock";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HostsStats {
    pub entries: usize,
    pub blocked: usize,
    pub local: usize,
    pub redirects: usize,
    pub duplicates: usize,
    pub invalid: usize,
}

/// 解析 hosts 文本：忽略注释与空行，按目标域名归类。
pub fn parse_hosts(text: &str) -> HostsStats {
    let mut stats = HostsStats::default();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(ip) = parts.next() else {
            continue;
        };
        let domains = parts.collect::<Vec<_>>();
        if domains.is_empty() {
            stats.invalid += 1;
            continue;
        }
        stats.entries += 1;
        let loopback = ip == "127.0.0.1" || ip == "0.0.0.0" || ip == "::1";
        let is_localhost = domains
            .iter()
            .all(|domain| domain.eq_ignore_ascii_case("localhost"));
        if loopback && !is_localhost {
            stats.blocked += 1;
        } else if loopback {
            stats.local += 1;
        } else {
            stats.redirects += 1;
        }
        for domain in domains {
            if !seen.insert(domain.to_lowercase()) {
                stats.duplicates += 1;
            }
        }
    }
    stats
}

fn hosts_path() -> PathBuf {
    let windows = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    windows.join("System32").join("drivers").join("etc").join("hosts")
}

fn read_hosts() -> (HostsStats, Option<String>) {
    match std::fs::read_to_string(hosts_path()) {
        Ok(text) => (parse_hosts(&text), None),
        Err(error) => (HostsStats::default(), Some(format!("无法读取 hosts：{error}"))),
    }
}

fn hosts_digest() -> String {
    digest_of(&["hosts"])
}

fn valid_domain(domain: &str) -> bool {
    let domain = domain.trim();
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

fn managed_block_count(text: &str) -> usize {
    let mut inside = false;
    let mut count = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == HOSTS_BEGIN {
            inside = true;
            continue;
        }
        if trimmed == HOSTS_END {
            inside = false;
            continue;
        }
        if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
            count += 1;
        }
    }
    count
}

fn elevated_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("SystemLens")
        .join("elevated")
}

fn helper_path() -> Result<PathBuf, ToolError> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法定位工具路径：{error}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    let candidates = [
        dir.join("lens_helper.exe"),
        dir.join("..").join("lens_helper").join("lens_helper.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            ToolError::new(
                "HELPER_MISSING",
                "缺少提权助手 lens_helper.exe，请重新打包或安装。",
            )
        })
}

fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn run_elevated(mut payload: Value) -> Result<Value, ToolError> {
    let dir = elevated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建提权请求目录：{error}")))?;
    let nonce_source = format!("{}-{}", tool_protocol::now_ms(), std::process::id());
    let nonce_full = tool_protocol::digest_of(&[&nonce_source]);
    let nonce = &nonce_full[..32];
    let request_path = dir.join(format!("req-{nonce}.json"));
    let result_path = dir.join(format!("res-{nonce}.json"));
    if let Value::Object(map) = &mut payload {
        map.insert("version".into(), json!(1));
        map.insert("nonce".into(), json!(nonce));
        map.insert("resultPath".into(), json!(result_path));
    }
    let serialized = serde_json::to_string(&payload)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化提权请求：{error}")))?;
    std::fs::write(&request_path, serialized)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入提权请求：{error}")))?;

    let helper = helper_path()?;
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath {} -ArgumentList ('--request',('\"' + {} + '\"')) -Verb RunAs -PassThru -Wait -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_quote(&helper.to_string_lossy()),
        ps_quote(&request_path.to_string_lossy())
    );
    let status = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;

    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);

    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new(
                "ELEVATION_CANCELLED",
                "已取消管理员授权，未修改 Hosts。",
            ));
        }
        return Err(ToolError::new(
            "ELEVATION_FAILED",
            format!("提权助手执行失败（退出码 {:?}）。", status.code()),
        ));
    }
    let text = result_text
        .ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$dns=@()
try { $dns=@(Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction Stop | ForEach-Object { $_.ServerAddresses } | Where-Object { $_ } | Sort-Object -Unique) } catch {}
function ExtCount($path){ if(Test-Path $path){ try { return @(Get-ChildItem $path -Directory -ErrorAction SilentlyContinue).Count } catch { return 0 } } return 0 }
$edge=ExtCount (Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data\Default\Extensions')
$chrome=ExtCount (Join-Path $env:LOCALAPPDATA 'Google\Chrome\User Data\Default\Extensions')
$firefox=ExtCount (Join-Path $env:APPDATA 'Mozilla\Firefox\Profiles')
$fwEnabled=0
try { $fwEnabled=@(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { $_.Enabled -eq 'True' }).Count } catch {}
@{dns=@($dns);edgeExtensions=$edge;chromeExtensions=$chrome;firefoxProfiles=$firefox;firewallProfiles=$fwEnabled} | ConvertTo-Json -Compress
"#
}

fn count(value: &Value, key: &str) -> usize {
    value.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn build_rows(payload: &Value, hosts: &HostsStats, hosts_error: &Option<String>) -> Vec<Value> {
    let mut rows = Vec::new();
    rows.push(json!({
        "item": "Hosts 文件",
        "current": if hosts.entries == 0 { "无自定义条目".to_string() } else { format!("{} 条映射", hosts.entries) },
        "detail": format!(
            "阻止 {} · 本机 {} · 重定向 {} · 重复 {} · 无效 {}",
            hosts.blocked, hosts.local, hosts.redirects, hosts.duplicates, hosts.invalid
        ),
        "suggestion": if hosts.blocked > 0 {
            "已有阻止映射；新增/删除 hosts 前请备份并确认来源"
        } else {
            "可手动添加 hosts 规则，或使用浏览器扩展；本工具不安装过滤驱动"
        },
    }));

    let dns = payload
        .get("dns")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "未读取".into());
    rows.push(json!({
        "item": "DNS 服务器",
        "current": dns,
        "detail": "来自 Get-DnsClientServerAddress",
        "suggestion": "广告拦截通常依赖过滤 DNS；更换前请确认服务商隐私政策与可用性",
    }));

    rows.push(json!({
        "item": "浏览器扩展",
        "current": format!(
            "Edge {} · Chrome {}",
            count(payload, "edgeExtensions"),
            count(payload, "chromeExtensions")
        ),
        "detail": format!("Firefox 配置目录 {} 个", count(payload, "firefoxProfiles")),
        "suggestion": "优先使用浏览器扩展做站点级过滤；本工具不注入浏览器",
    }));

    rows.push(json!({
        "item": "Windows 防火墙",
        "current": format!("{} 个配置文件启用", count(payload, "firewallProfiles")),
        "detail": "只读取启用状态",
        "suggestion": "本工具不添加防火墙/WFP 规则，也不安装内核过滤驱动",
    }));

    if let Some(error) = hosts_error {
        rows.push(json!({
            "item": "读取错误",
            "current": error,
            "detail": "hosts 解析失败",
            "suggestion": "以普通权限检查文件是否存在与可读",
        }));
    }
    rows
}

fn scan() -> Result<Value, ToolError> {
    let (hosts, hosts_error) = read_hosts();
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取网络配置：{error}")))?;
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
        .ok_or_else(|| ToolError::new("INTERNAL", "网络配置查询没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析网络配置：{error}")))?;
    let rows = build_rows(&payload, &hosts, &hosts_error);
    let dns_count = payload
        .get("dns")
        .and_then(Value::as_array)
        .map(|values| values.len())
        .unwrap_or(0);
    let extensions = count(&payload, "edgeExtensions") + count(&payload, "chromeExtensions");

    let hosts_text = std::fs::read_to_string(hosts_path()).ok();
    let managed = hosts_text
        .as_deref()
        .map(managed_block_count)
        .unwrap_or(0);
    let mut data = table(
        json!([
            item("Hosts 条目", hosts.entries.to_string()),
            item("阻止映射", hosts.blocked.to_string()),
            item("SystemLens 规则", managed.to_string()),
            item("DNS 服务器", dns_count.to_string()),
            item("浏览器扩展", extensions.to_string()),
        ]),
        json!([
            column("item", "检查项"),
            column("current", "当前状态"),
            column("detail", "明细"),
            column("suggestion", "说明/建议"),
        ]),
        Value::Array(rows),
        json!([
            "审计部分只读：不安装过滤驱动、不注入浏览器。",
            "可通过下方文本框写入/移除 SystemLens 管理的 Hosts 段；写入需要管理员授权。",
            "只维护 # BEGIN/# END 标记之间的内容，不改动其他 hosts 行；写入前自动备份。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("hostsManagedDomains".into(), json!(managed));
        map.insert("hostsPath".into(), json!(hosts_path().to_string_lossy()));
    }
    if let Ok(token) = tool_protocol::issue_token(ACTION_HOSTS, &hosts_digest()) {
        attach_token(&mut data, token);
    }
    Ok(data)
}

fn require_token(request: &Request) -> Result<&str, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "修改 Hosts 需要确认令牌，请先审计。",
        )
    })?;
    tool_protocol::verify_token(token, ACTION_HOSTS, &hosts_digest())?;
    Ok(token)
}

fn apply_hosts(request: &Request) -> Result<Value, ToolError> {
    require_token(request)?;
    let domains = request
        .parameters
        .get("domains")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 domains。"))?;
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for domain in domains {
        let domain = domain
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !valid_domain(&domain) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("域名格式无效：{domain}"),
            ));
        }
        if seen.insert(domain.clone()) {
            normalized.push(domain);
        }
    }
    if normalized.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "至少需要一个域名。"));
    }
    if normalized.len() > 500 {
        return Err(ToolError::new("INVALID_INPUT", "单次最多写入 500 个域名。"));
    }

    let result = run_elevated(json!({"op": "hosts_apply", "domains": normalized}))?;
    let count = result
        .get("data")
        .and_then(|data| data.get("domains"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "adblock_master",
        "action": "apply_hosts",
        "domains": count,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([
            item("写入域名", count.to_string()),
            item("Hosts", "已更新"),
        ]),
        json!([
            column("item", "项目"),
            column("value", "结果"),
        ]),
        json!([{
            "item": "SystemLens 管理段",
            "value": format!("已写入 {count} 条规则"),
        }]),
        json!([
            "只修改标记之间的内容，其他 hosts 行保持不变。",
            "写入前已备份到 %LOCALAPPDATA%\\SystemLens\\adblock\\hosts.backup。",
            "已尝试刷新 DNS 缓存。",
        ]),
    ))
}

fn restore_hosts(request: &Request) -> Result<Value, ToolError> {
    require_token(request)?;
    let result = run_elevated(json!({"op": "hosts_restore"}))?;
    let removed = result
        .get("data")
        .and_then(|data| data.get("removed"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "adblock_master",
        "action": "restore_hosts",
        "removed": removed,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([
            item("移除规则", removed.to_string()),
        ]),
        json!([
            column("item", "项目"),
            column("value", "结果"),
        ]),
        json!([{
            "item": "SystemLens 管理段",
            "value": if removed == 0 { "没有需要移除的规则".to_string() } else { format!("已移除 {removed} 条规则") },
        }]),
        json!([
            "只移除 SystemLens 标记之间的内容，其他 hosts 行保持不变。",
            "已尝试刷新 DNS 缓存。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "adblock_master",
        env!("CARGO_PKG_VERSION"),
        &["preview", "apply_hosts", "restore_hosts"],
        |request| match request.action.as_str() {
            "preview" => scan(),
            "apply_hosts" => apply_hosts(request),
            "restore_hosts" => restore_hosts(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_categories() {
        let text = r#"
# comment
127.0.0.1 localhost
127.0.0.1 ads.example.com
0.0.0.0 tracker.example.com other.example.com

192.168.1.10 nas.local
bad-line-without-domain
"#;
        let stats = parse_hosts(text);
        assert_eq!(stats.entries, 4);
        assert_eq!(stats.local, 1);
        assert_eq!(stats.blocked, 2);
        assert_eq!(stats.redirects, 1);
        assert_eq!(stats.invalid, 1);
    }

    #[test]
    fn counts_duplicate_domains() {
        let stats = parse_hosts("127.0.0.1 a.com\n0.0.0.0 a.com\n");
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.duplicates, 1);
    }

    #[test]
    fn empty_hosts_is_default() {
        assert_eq!(parse_hosts(""), HostsStats::default());
    }

    #[test]
    fn validates_domains() {
        assert!(valid_domain("ads.example.com"));
        assert!(valid_domain("example.com"));
        assert!(!valid_domain(""));
        assert!(!valid_domain("-bad.com"));
        assert!(!valid_domain("bad-.com"));
        assert!(!valid_domain("bad..com"));
        assert!(!valid_domain("bad domain.com"));
    }

    #[test]
    fn counts_only_managed_block() {
        let text = "127.0.0.1 localhost\r\n# BEGIN SystemLens AdBlock\r\n0.0.0.0 ads.com\r\n0.0.0.0 tracker.com\r\n# END SystemLens AdBlock\r\n1.1.1.1 foo.com\r\n";
        assert_eq!(managed_block_count(text), 2);
        assert_eq!(managed_block_count("127.0.0.1 localhost\r\n"), 0);
    }
}
