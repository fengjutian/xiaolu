use serde_json::{json, Value};
use std::path::PathBuf;
use tool_protocol::{column, human_bytes, item, table};

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

fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn scan() -> Value {
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

    table(
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
            "修复/安装运行库需后续版本单独授权并提供来源校验。",
        ]),
    )
}

fn main() {
    tool_protocol::serve(
        "dll_repair",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| Ok(scan()),
    );
}
