use std::{path::PathBuf, process::Command};

#[tauri::command]
pub fn open_path_in_explorer(path: String) -> Result<(), String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err("只能打开绝对路径".to_string());
    }

    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("无法访问 {}：{error}", path.display()))?;

    let mut command = Command::new("explorer.exe");
    if metadata.is_file() {
        command.arg("/select,").arg(&path);
    } else {
        command.arg(&path);
    }

    command
        .spawn()
        .map_err(|error| format!("无法打开资源管理器：{error}"))?;
    Ok(())
}
