use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchEntry {
    pub name: String,
    pub source: String,
    pub target: String,
    pub app_id: String,
    pub target_path: String,
    pub arguments: String,
}

#[tauri::command]
pub async fn list_start_menu_apps() -> Result<Vec<LaunchEntry>, String> {
    tauri::async_runtime::spawn_blocking(collect)
        .await
        .map_err(|error| format!("快捷启动列表任务失败：{error}"))?
}

fn collect() -> Result<Vec<LaunchEntry>, String> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| format!("无法读取开始菜单：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{') || line.starts_with('['))
        .last()
        .ok_or_else(|| "开始菜单查询没有返回 JSON".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(json_line).map_err(|error| format!("无法解析开始菜单：{error}"))?;
    let array = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    let mut entries = array
        .into_iter()
        .filter_map(|item| serde_json::from_value::<LaunchEntry>(item).ok())
        .filter(|entry| !entry.name.trim().is_empty())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries.truncate(MAX_ENTRIES);
    Ok(entries)
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$items=[System.Collections.Generic.List[object]]::new()
$roots=@("$env:ProgramData\Microsoft\Windows\Start Menu\Programs","$env:APPDATA\Microsoft\Windows\Start Menu\Programs")
$shell=New-Object -ComObject WScript.Shell
foreach($root in $roots){
  if(Test-Path $root){
    Get-ChildItem -Path $root -Recurse -Filter *.lnk -ErrorAction SilentlyContinue | ForEach-Object {
      try {
        $shortcut=$shell.CreateShortcut($_.FullName)
        if($shortcut.TargetPath){
          $items.Add(@{name=$_.BaseName;source='开始菜单';target=$_.FullName;appId='';targetPath=[string]$shortcut.TargetPath;arguments=[string]$shortcut.Arguments})
        }
      } catch {}
    }
  }
}
try {
  Get-StartApps -ErrorAction Stop | ForEach-Object {
    $items.Add(@{name=[string]$_.Name;source='应用';target='';appId=[string]$_.AppID;targetPath='';arguments=''})
  }
} catch {}
@($items) | ConvertTo-Json -Depth 3 -Compress
"#
}

fn start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(program_data) = std::env::var_os("ProgramData") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    roots
}

/// 快捷方式必须位于开始菜单目录内，且为 `.lnk`。
pub fn validate_shortcut(path: &str, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err("快捷方式必须是绝对路径".into());
    }
    if path.extension().and_then(|value| value.to_str()).map(|value| value.to_lowercase()) != Some("lnk".into()) {
        return Err("只允许启动 .lnk 快捷方式".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("无法解析快捷方式：{error}"))?;
    let allowed = roots.iter().any(|root| {
        root.canonicalize()
            .map(|canonical_root| canonical.starts_with(canonical_root))
            .unwrap_or(false)
    });
    if !allowed {
        return Err("快捷方式不在开始菜单目录内".into());
    }
    Ok(canonical)
}

pub fn validate_app_id(app_id: &str) -> Result<String, String> {
    let app_id = app_id.trim();
    if app_id.is_empty() || app_id.len() > 300 {
        return Err("应用标识无效".into());
    }
    if !app_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "._!-".contains(character))
    {
        return Err("应用标识包含不允许的字符".into());
    }
    Ok(app_id.to_string())
}

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[cfg(windows)]
fn open_shortcut(path: &Path) -> Result<(), String> {
    let operation: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: 字符串以 NUL 结尾，参数均为只读指针。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        return Err(format!("无法启动快捷方式（ShellExecute 返回 {result}）"));
    }
    Ok(())
}

#[cfg(windows)]
fn app_ids() -> Result<Vec<String>, String> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ForEach-Object { $_.AppID } | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| format!("无法读取应用列表：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .ok_or_else(|| "应用列表为空".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|error| format!("无法解析应用列表：{error}"))?;
    Ok(match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::String(item) => vec![item],
        _ => Vec::new(),
    })
}

#[tauri::command]
pub async fn launch_start_menu_entry(
    kind: String,
    target: String,
    app_id: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || launch(&kind, &target, &app_id))
        .await
        .map_err(|error| format!("启动快捷方式任务失败：{error}"))?
}

fn launch(kind: &str, target: &str, app_id: &str) -> Result<String, String> {
    match kind {
        "shortcut" => {
            let path = validate_shortcut(target, &start_menu_roots())?;
            open_shortcut(&path)?;
            Ok(format!("已启动：{}", path.display()))
        }
        "app" => {
            let app_id = validate_app_id(app_id)?;
            let known = app_ids()?;
            if !known.iter().any(|known_id| known_id == &app_id) {
                return Err("应用标识不在当前开始菜单应用列表中".into());
            }
            Command::new("explorer.exe")
                .arg(format!("shell:AppsFolder\\{app_id}"))
                .spawn()
                .map_err(|error| format!("无法启动应用：{error}"))?;
            Ok(format!("已启动应用：{app_id}"))
        }
        _ => Err("不支持的启动类型".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_shortcut_and_outside_paths() {
        let roots = vec![PathBuf::from("C:\\Users\\me\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs")];
        assert!(validate_shortcut("relative.lnk", &roots).is_err());
        assert!(validate_shortcut("C:\\Windows\\notepad.exe", &roots).is_err());
        assert!(validate_shortcut("C:\\Windows\\System32\\calc.lnk", &roots).is_err());
    }

    #[test]
    fn validates_app_id_characters() {
        assert!(validate_app_id("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App").is_ok());
        assert!(validate_app_id("").is_err());
        assert!(validate_app_id("bad id with spaces").is_err());
        assert!(validate_app_id("bad\\id").is_err());
        assert!(validate_app_id("bad/id").is_err());
    }

    #[test]
    #[ignore = "依赖真实开始菜单"]
    fn lists_real_start_menu() {
        let entries = collect().expect("collect start menu");
        assert!(!entries.is_empty(), "expected at least one start menu entry");
        assert!(entries.iter().any(|entry| !entry.name.trim().is_empty()));
    }
}
