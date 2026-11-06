use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, Request, ToolError};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DLLS: &[(&str, &str)] = &[
    ("vcruntime140.dll", "VC++ 2015-2022"),
    ("vcruntime140_1.dll", "VC++ 2015-2022"),
    ("msvcp140.dll", "VC++ 2015-2022"),
    ("msvcp140_1.dll", "VC++ 2015-2022"),
    ("msvcp140_2.dll", "VC++ 2015-2022"),
    ("concrt140.dll", "VC++ 并发运行库"),
    ("ucrtbase.dll", "Universal C Runtime"),
    ("api-ms-win-crt-runtime-l1-1-0.dll", "Universal C Runtime"),
    ("d3dcompiler_47.dll", "Direct3D 着色器编译"),
    ("xinput1_4.dll", "XInput 手柄"),
    ("mfplat.dll", "Media Foundation"),
];

const ACTION_OPEN: &str = "open_official";

fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn reg_string(path: &str, name: &str) -> Option<String> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

fn reg_dword(path: &str, name: &str) -> Option<u32> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

struct Runtime {
    id: &'static str,
    label: &'static str,
    installed: bool,
    version: String,
    url: &'static str,
    note: &'static str,
}

fn vcpp_runtime(id: &'static str, label: &'static str, arch: &str) -> Runtime {
    let path = format!("SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\{arch}");
    let installed = reg_dword(&path, "Installed").unwrap_or(0) == 1;
    let version = reg_string(&path, "Version").unwrap_or_else(|| "未安装".into());
    Runtime {
        id,
        label,
        installed,
        version,
        url: if arch == "x64" {
            "https://aka.ms/vs/17/release/vc_redist.x64.exe"
        } else {
            "https://aka.ms/vs/17/release/vc_redist.x86.exe"
        },
        note: "微软官方 VC++ 2015-2022 可再发行组件",
    }
}

fn runtimes() -> Vec<Runtime> {
    let dotnet_release = reg_dword(
        "SOFTWARE\\Microsoft\\NET Framework Setup\\NDP\\v4\\Full",
        "Release",
    );
    let dotnet_ok = dotnet_release.map(|release| release >= 528_040).unwrap_or(false);
    vec![
        vcpp_runtime("vcpp-x64", "VC++ 2015-2022 x64", "x64"),
        vcpp_runtime("vcpp-x86", "VC++ 2015-2022 x86", "x86"),
        Runtime {
            id: "dotnet48",
            label: ".NET Framework 4.8",
            installed: dotnet_ok,
            version: dotnet_release
                .map(|release| format!("Release {release}"))
                .unwrap_or_else(|| "未检测".into()),
            url: "https://go.microsoft.com/fwlink/?LinkId=2088631",
            note: "微软官方 .NET Framework 4.8 下载页",
        },
    ]
}

fn official_url(id: &str) -> Option<&'static str> {
    match id {
        "vcpp-x64" => Some("https://aka.ms/vs/17/release/vc_redist.x64.exe"),
        "vcpp-x86" => Some("https://aka.ms/vs/17/release/vc_redist.x86.exe"),
        "dotnet48" => Some("https://go.microsoft.com/fwlink/?LinkId=2088631"),
        _ => None,
    }
}

fn scan() -> Result<Value, ToolError> {
    let root = system_root();
    let architectures = [
        ("System32", root.join("System32")),
        ("SysWOW64", root.join("SysWOW64")),
    ];
    let mut rows = Vec::new();
    let mut missing = 0usize;
    let mut checked = 0usize;

    for (dll, component) in DLLS {
        for (arch, directory) in &architectures {
            let path = directory.join(dll);
            if *arch == "SysWOW64" && !directory.is_dir() {
                continue;
            }
            checked += 1;
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => {
                    rows.push(json!({
                        "dll": dll,
                        "component": component,
                        "arch": arch,
                        "status": "存在",
                        "size": human_bytes(metadata.len()),
                        "path": path.to_string_lossy(),
                    }));
                }
                _ => {
                    missing += 1;
                    rows.push(json!({
                        "dll": dll,
                        "component": component,
                        "arch": arch,
                        "status": "缺失",
                        "size": "-",
                        "path": path.to_string_lossy(),
                    }));
                }
            }
        }
    }

    let runtime_list = runtimes();
    let mut runtime_items = Vec::new();
    for runtime in &runtime_list {
        runtime_items.push(json!({
            "id": runtime.id,
            "label": runtime.label,
            "installed": runtime.installed,
            "version": runtime.version,
            "url": runtime.url,
            "note": runtime.note,
        }));
    }

    let mut data = table(
        json!([
            item("检查项", checked.to_string()),
            item("缺失项", missing.to_string()),
            item("系统目录", root.to_string_lossy()),
        ]),
        json!([
            column("dll", "DLL"),
            column("component", "运行库"),
            column("arch", "架构"),
            column("status", "状态"),
            column("size", "大小"),
            column("path", "路径"),
        ]),
        Value::Array(rows),
        json!([
            "只读检查：仅判断文件是否存在与大小，不下载、不替换任何 DLL。",
            "缺失不等于系统损坏，部分组件可能由应用自带或仅在需要时安装。",
            "可打开微软官方下载页手动安装运行库；本工具不自动下载或安装。",
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert("runtimeItems".into(), Value::Array(runtime_items));
    }
    Ok(data)
}

fn open_official(request: &Request) -> Result<Value, ToolError> {
    let id = request
        .parameters
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 id。"))?;
    let url = official_url(id)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "不支持运行库 ID。"))?;
    Command::new("rundll32.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开浏览器：{error}")))?;
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "dll_repair",
        "action": ACTION_OPEN,
        "id": id,
        "url": url,
        "at": tool_protocol::now_ms(),
    }));
    Ok(table(
        json!([item("已打开", id)]),
        json!([column("item", "项目"), column("value", "结果")]),
        json!([{"item": "官方下载页", "value": url}]),
        json!([
            "已在默认浏览器打开微软官方下载页。",
            "本工具不下载、不安装、不校验安装包；请从官方页面手动完成。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "dll_repair",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION_OPEN],
        |request| match request.action.as_str() {
            "preview" => scan(),
            ACTION_OPEN => open_official(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allowlisted_urls_are_opened() {
        assert!(official_url("vcpp-x64").is_some());
        assert!(official_url("vcpp-x86").is_some());
        assert!(official_url("dotnet48").is_some());
        assert!(official_url("https://evil.example").is_none());
        assert!(official_url("").is_none());
    }

    #[test]
    fn runtimes_cover_three_entries() {
        let list = runtimes();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|runtime| runtime.id == "vcpp-x64"));
        assert!(list.iter().any(|runtime| runtime.id == "dotnet48"));
    }
}
