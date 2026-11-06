use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{column, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    Ok(table(
        json!([
            item("Hosts 条目", hosts.entries.to_string()),
            item("阻止映射", hosts.blocked.to_string()),
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
            "只读审计：只读取 hosts、DNS、浏览器扩展与防火墙状态，不修改任何规则。",
            "不安装 WFP/内核过滤驱动、不注入浏览器；广告过滤建议优先使用浏览器扩展或可信 DNS。",
            "修改 hosts 前请备份；本工具不提供自动写入 hosts 的功能。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "adblock_master",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
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
}
