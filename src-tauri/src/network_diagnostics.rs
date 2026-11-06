use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpConnection {
    process_name: String,
    pid: u32,
    local_address: String,
    local_port: u16,
    remote_address: String,
    remote_port: u16,
    state: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsAdapter {
    alias: String,
    servers: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnosticReport {
    captured_at_unix_ms: u128,
    default_gateways: Vec<String>,
    dns: Vec<DnsAdapter>,
    proxy_enabled: bool,
    proxy_server: String,
    auto_config_url: String,
    hosts_custom_entries: usize,
    connections: Vec<TcpConnection>,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellReport {
    default_gateways: Vec<String>,
    dns: Vec<DnsAdapter>,
    proxy_enabled: bool,
    proxy_server: String,
    auto_config_url: String,
    hosts_custom_entries: usize,
    connections: Vec<TcpConnection>,
}

#[cfg(windows)]
pub(crate) fn collect() -> Result<NetworkDiagnosticReport, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$gateways=@(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue|Sort-Object RouteMetric|ForEach-Object{if($_.NextHop -and $_.NextHop -ne '0.0.0.0'){[string]$_.NextHop}})
$dns=@(Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue|Where-Object{$_.ServerAddresses.Count -gt 0}|ForEach-Object{@{alias=[string]$_.InterfaceAlias;servers=@($_.ServerAddresses|ForEach-Object{[string]$_})}})
$internet=Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
$hostsPath=Join-Path $env:SystemRoot 'System32\drivers\etc\hosts';$hostsCount=0;if(Test-Path $hostsPath){$hostsCount=@(Get-Content $hostsPath -ErrorAction SilentlyContinue|Where-Object{$_.Trim() -and -not $_.Trim().StartsWith('#')}).Count}
$processNames=@{};Get-Process -ErrorAction SilentlyContinue|ForEach-Object{$processNames[[uint32]$_.Id]=[string]$_.ProcessName}
$connections=@(Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue|Sort-Object OwningProcess,RemoteAddress,RemotePort|Select-Object -First 200|ForEach-Object{$pidValue=[uint32]$_.OwningProcess;@{processName=if($processNames.ContainsKey($pidValue)){$processNames[$pidValue]}else{'未知进程'};pid=$pidValue;localAddress=[string]$_.LocalAddress;localPort=[uint16]$_.LocalPort;remoteAddress=[string]$_.RemoteAddress;remotePort=[uint16]$_.RemotePort;state=[string]$_.State}})
@{defaultGateways=@($gateways|Select-Object -Unique);dns=@($dns);proxyEnabled=([bool]$internet.ProxyEnable);proxyServer=if($internet.ProxyServer){[string]$internet.ProxyServer}else{''};autoConfigUrl=if($internet.AutoConfigURL){[string]$internet.AutoConfigURL}else{''};hostsCustomEntries=[int]$hostsCount;connections=@($connections)}|ConvertTo-Json -Depth 5 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动网络诊断：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let report: PowerShellReport = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析网络诊断：{error}"))?;
    Ok(NetworkDiagnosticReport { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), default_gateways: report.default_gateways, dns: report.dns, proxy_enabled: report.proxy_enabled, proxy_server: report.proxy_server, auto_config_url: report.auto_config_url, hosts_custom_entries: report.hosts_custom_entries, connections: report.connections, source: "Windows NetTCPIP / DNS Client / Internet Settings".into() })
}

#[cfg(not(windows))]
pub(crate) fn collect() -> Result<NetworkDiagnosticReport, String> {
    Ok(NetworkDiagnosticReport { captured_at_unix_ms: 0, default_gateways: vec![], dns: vec![], proxy_enabled: false, proxy_server: String::new(), auto_config_url: String::new(), hosts_custom_entries: 0, connections: vec![], source: "unsupported".into() })
}

#[tauri::command]
pub async fn get_network_diagnostics() -> Result<NetworkDiagnosticReport, String> {
    tauri::async_runtime::spawn_blocking(collect).await.map_err(|error| format!("网络诊断任务失败：{error}"))?
}
