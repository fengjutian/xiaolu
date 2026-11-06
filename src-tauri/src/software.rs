use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftware {
    name: String,
    version: String,
    publisher: String,
    install_date: String,
    install_location: String,
    estimated_size_bytes: u64,
    scope: String,
    architecture: String,
    uninstall_available: bool,
    #[serde(default)]
    key: String,
    #[serde(default)]
    uninstall_string: String,
    #[serde(default)]
    quiet_uninstall_string: String,
    #[serde(default)]
    residue: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareInventory {
    captured_at_unix_ms: u128,
    total: usize,
    estimated_size_bytes: u64,
    residue_count: usize,
    items: Vec<InstalledSoftware>,
    source: String,
}

#[derive(Debug, Deserialize)]
struct SoftwarePayload {
    items: Vec<InstalledSoftware>,
}

/// 仅允许从三类卸载注册表位置启动卸载程序。
const ALLOWED_UNINSTALL_ROOTS: [&str; 3] = [
    "hklm:\\software\\microsoft\\windows\\currentversion\\uninstall",
    "hklm:\\software\\wow6432node\\microsoft\\windows\\currentversion\\uninstall",
    "hkcu:\\software\\microsoft\\windows\\currentversion\\uninstall",
];

/// 把卸载命令拆成程序与参数：支持带引号的完整路径和 MsiExec 形式。
pub fn split_command_line(command: &str) -> Result<(String, Vec<String>), String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("卸载命令为空".into());
    }
    if let Some(rest) = command.strip_prefix('"') {
        let Some(end) = rest.find('"') else {
            return Err("卸载命令引号不匹配".into());
        };
        let program = rest[..end].trim().to_string();
        if program.is_empty() {
            return Err("卸载命令缺少程序路径".into());
        }
        let args = split_args(rest[end + 1..].trim());
        Ok((program, args))
    } else {
        let mut parts = command.splitn(2, char::is_whitespace);
        let program = parts.next().unwrap_or("").trim().to_string();
        if program.is_empty() {
            return Err("卸载命令缺少程序路径".into());
        }
        let args = split_args(parts.next().unwrap_or("").trim());
        Ok((program, args))
    }
}

fn split_args(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split_whitespace().map(str::to_string).collect()
}

/// 校验注册表键标识；返回 (root, subkey)。
fn parse_uninstall_key(key: &str) -> Result<(String, String), String> {
    let (root, subkey) = key
        .split_once('|')
        .ok_or_else(|| "卸载项标识格式无效".to_string())?;
    let normalized = root.trim().to_lowercase();
    if !ALLOWED_UNINSTALL_ROOTS.contains(&normalized.as_str()) {
        return Err("不允许的卸载注册表位置".into());
    }
    if subkey.trim().is_empty() || subkey.contains('\\') || subkey.contains('/') {
        return Err("卸载项子键无效".into());
    }
    Ok((root.trim().to_string(), subkey.trim().to_string()))
}

fn is_residue(location: &str) -> bool {
    let location = location.trim().trim_matches('"').trim();
    if location.is_empty() {
        return false;
    }
    !Path::new(location).exists()
}

#[cfg(windows)]
fn collect() -> Result<SoftwareInventory, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$targets=@(
  @{path='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='所有用户';arch='64-bit'},
  @{path='HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='所有用户';arch='32-bit'},
  @{path='HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*';scope='当前用户';arch='当前用户'}
)
$seen=@{};$items=[System.Collections.Generic.List[object]]::new()
foreach($target in $targets){
  $root=$target.path.Substring(0,$target.path.Length-2)
  Get-ItemProperty $target.path -ErrorAction SilentlyContinue|ForEach-Object{
  if($_.DisplayName -and -not $_.SystemComponent -and -not $_.ParentKeyName){
    $key=([string]$_.DisplayName+'|'+[string]$_.DisplayVersion+'|'+$target.scope).ToLowerInvariant()
    if(-not $seen.ContainsKey($key)){$seen[$key]=$true;$items.Add(@{name=[string]$_.DisplayName;version=if($_.DisplayVersion){[string]$_.DisplayVersion}else{''};publisher=if($_.Publisher){[string]$_.Publisher}else{''};installDate=if($_.InstallDate){[string]$_.InstallDate}else{''};installLocation=if($_.InstallLocation){[string]$_.InstallLocation}else{''};estimatedSizeBytes=if($_.EstimatedSize){[uint64]$_.EstimatedSize*1024}else{0};scope=$target.scope;architecture=$target.arch;uninstallAvailable=([bool]($_.UninstallString -or $_.QuietUninstallString));key=($root+'|'+$_.PSChildName);uninstallString=if($_.UninstallString){[string]$_.UninstallString}else{''};quietUninstallString=if($_.QuietUninstallString){[string]$_.QuietUninstallString}else{''}})}
  }
}}
@{items=@($items|Sort-Object name)}|ConvertTo-Json -Depth 4 -Compress
"#;
    let output = Command::new("powershell.exe").creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]).output()
        .map_err(|error| format!("无法启动软件清单扫描：{error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let mut payload: SoftwarePayload = serde_json::from_slice(&output.stdout).map_err(|error| format!("无法解析软件清单：{error}"))?;
    for item in &mut payload.items {
        item.residue = is_residue(&item.install_location);
    }
    let total = payload.items.len();
    let estimated_size_bytes = payload.items.iter().map(|item| item.estimated_size_bytes).sum();
    let residue_count = payload.items.iter().filter(|item| item.residue).count();
    Ok(SoftwareInventory { captured_at_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), total, estimated_size_bytes, residue_count, items: payload.items, source: "Windows Uninstall Registry (HKLM/HKCU)".into() })
}

#[cfg(not(windows))]
fn collect() -> Result<SoftwareInventory, String> {
    Ok(SoftwareInventory {
        captured_at_unix_ms: 0,
        total: 0,
        estimated_size_bytes: 0,
        residue_count: 0,
        items: vec![],
        source: "unsupported".into(),
    })
}

#[tauri::command]
pub async fn get_software_inventory() -> Result<SoftwareInventory, String> {
    tauri::async_runtime::spawn_blocking(collect).await.map_err(|error| format!("软件清单任务失败：{error}"))?
}

#[cfg(windows)]
fn launch_uninstall(key: &str) -> Result<String, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let (root, subkey) = parse_uninstall_key(key)?;
    let (hive, relative_root) = if root.to_lowercase().starts_with("hklm:") {
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            root.to_lowercase().replacen("hklm:\\", "", 1),
        )
    } else {
        (
            RegKey::predef(HKEY_CURRENT_USER),
            root.to_lowercase().replacen("hkcu:\\", "", 1),
        )
    };
    let path = format!("{relative_root}\\{subkey}");
    let entry = hive
        .open_subkey_with_flags(&path, KEY_READ)
        .map_err(|error| format!("无法读取卸载项：{error}"))?;
    let uninstall: String = entry.get_value("UninstallString").unwrap_or_default();
    let quiet: String = entry.get_value("QuietUninstallString").unwrap_or_default();
    let command = if !uninstall.trim().is_empty() {
        uninstall
    } else if !quiet.trim().is_empty() {
        quiet
    } else {
        return Err("该系统项没有提供卸载命令".into());
    };

    let (program, args) = split_command_line(&command)?;
    let child = Command::new(&program)
        .args(&args)
        .spawn()
        .map_err(|error| format!("无法启动卸载程序 {program}：{error}"))?;
    let _ = child.id();
    Ok(format!("已启动卸载程序：{program}"))
}

#[cfg(not(windows))]
fn launch_uninstall(_key: &str) -> Result<String, String> {
    Err("当前平台不支持启动卸载程序".into())
}

#[tauri::command]
pub async fn launch_software_uninstall(key: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || launch_uninstall(&key))
        .await
        .map_err(|error| format!("启动卸载任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_and_unquoted_commands() {
        let (program, args) =
            split_command_line("\"C:\\Program Files\\App\\uninst.exe\" /S").unwrap();
        assert_eq!(program, "C:\\Program Files\\App\\uninst.exe");
        assert_eq!(args, vec!["/S"]);

        let (program, args) = split_command_line("MsiExec.exe /X{1234-ABCD}").unwrap();
        assert_eq!(program, "MsiExec.exe");
        assert_eq!(args, vec!["/X{1234-ABCD}"]);

        let (program, args) = split_command_line("\"C:\\App\\uninst.exe\"").unwrap();
        assert_eq!(program, "C:\\App\\uninst.exe");
        assert!(args.is_empty());
    }

    #[test]
    fn rejects_empty_or_broken_commands() {
        assert!(split_command_line("").is_err());
        assert!(split_command_line("   ").is_err());
        assert!(split_command_line("\"C:\\App\\uninst.exe").is_err());
    }

    #[test]
    fn validates_uninstall_registry_keys() {
        let ok = "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall|{GUID}";
        assert!(parse_uninstall_key(ok).is_ok());
        let ok_hkcu = "HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall|App";
        assert!(parse_uninstall_key(ok_hkcu).is_ok());
        assert!(parse_uninstall_key("HKLM:\\SOFTWARE\\Run|Bad").is_err());
        assert!(parse_uninstall_key("C:\\Temp|Bad").is_err());
        assert!(parse_uninstall_key("HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall|a\\b").is_err());
        assert!(parse_uninstall_key("no-separator").is_err());
    }

    #[test]
    fn detects_missing_install_location() {
        assert!(!is_residue(""));
        assert!(is_residue("Z:\\definitely-not-existing-system-lens"));
    }
}
