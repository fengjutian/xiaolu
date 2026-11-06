use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const ACTION: &str = "apply";
const MAX_FILES: usize = 20_000;
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// 拒绝写入这些系统目录，避免误把压缩包解到系统位置。
const BLOCKED_ROOTS: [&str; 4] = [
    "c:\\windows",
    "c:\\program files",
    "c:\\program files (x86)",
    "c:\\programdata",
];

fn absolute_path(raw: &str) -> Result<PathBuf, ToolError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "路径不能为空。"));
    }
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(ToolError::new("INVALID_INPUT", "只支持绝对路径。"));
    }
    if text.starts_with("\\\\") {
        return Err(ToolError::new("INVALID_INPUT", "暂不支持网络路径。"));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ToolError::new("INVALID_INPUT", "路径不能包含上级引用。"));
    }
    Ok(path)
}

fn ensure_not_system(path: &Path) -> Result<(), ToolError> {
    let lower = path.to_string_lossy().to_lowercase().replace('/', "\\");
    for root in BLOCKED_ROOTS {
        if lower == root || lower.starts_with(&format!("{root}\\")) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("不允许写入系统目录：{}", path.display()),
            ));
        }
    }
    if path.parent().is_none() {
        return Err(ToolError::new("INVALID_INPUT", "不能使用磁盘根目录。"));
    }
    Ok(())
}

fn string_field<'a>(parameters: &'a Value, key: &str) -> &'a str {
    parameters
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn bool_field(parameters: &Value, key: &str) -> bool {
    parameters
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

struct Plan {
    mode: String,
    sources: Vec<PathBuf>,
    archive: PathBuf,
    destination: PathBuf,
    overwrite: bool,
}

impl Plan {
    fn digest(&self) -> String {
        let mut parts = vec![self.mode.clone(), self.archive.to_string_lossy().into_owned()];
        parts.push(self.destination.to_string_lossy().into_owned());
        parts.push(if self.overwrite { "overwrite" } else { "safe" }.to_string());
        for source in &self.sources {
            parts.push(source.to_string_lossy().into_owned());
        }
        let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
        digest_of(&refs)
    }
}

fn plan_from(parameters: &Value) -> Result<Plan, ToolError> {
    let mode = string_field(parameters, "mode").trim().to_lowercase();
    if mode != "compress" && mode != "extract" {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "mode 只能是 compress 或 extract。",
        ));
    }
    let archive = absolute_path(string_field(parameters, "archive"))?;
    if mode == "compress" {
        if archive.extension().and_then(|value| value.to_str()).map(|value| value.to_lowercase()) != Some("zip".into()) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                "压缩包目标必须是 .zip 结尾。",
            ));
        }
        ensure_not_system(&archive)?;
        if archive.exists() && !bool_field(parameters, "overwrite") {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("目标已存在：{}；请更换路径或勾选覆盖。", archive.display()),
            ));
        }
        let sources = parameters
            .get("sources")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(absolute_path)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        if sources.is_empty() {
            return Err(ToolError::new("INVALID_INPUT", "请至少提供一个源路径。"));
        }
        for source in &sources {
            if !source.exists() {
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    format!("源路径不存在：{}", source.display()),
                ));
            }
        }
        Ok(Plan {
            mode,
            sources,
            archive,
            destination: PathBuf::new(),
            overwrite: bool_field(parameters, "overwrite"),
        })
    } else {
        if !archive.is_file() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("压缩包不存在：{}", archive.display()),
            ));
        }
        let destination = absolute_path(string_field(parameters, "destination"))?;
        ensure_not_system(&destination)?;
        if destination.exists() && !destination.is_dir() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                "解压目标已存在且不是目录。",
            ));
        }
        Ok(Plan {
            mode,
            sources: Vec::new(),
            archive,
            destination,
            overwrite: bool_field(parameters, "overwrite"),
        })
    }
}

fn scan_sources(sources: &[PathBuf]) -> Result<(usize, u64), ToolError> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    for source in sources {
        if source.is_file() {
            files += 1;
            bytes = bytes.saturating_add(fs::metadata(source).map(|m| m.len()).unwrap_or(0));
            continue;
        }
        for entry in WalkDir::new(source).follow_links(false).min_depth(1) {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            if entry.path_is_symlink() || !entry.file_type().is_file() {
                continue;
            }
            files += 1;
            if files > MAX_FILES {
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    format!("文件数超过上限（{}）。", MAX_FILES),
                ));
            }
            if let Ok(metadata) = entry.metadata() {
                bytes = bytes.saturating_add(metadata.len());
                if bytes > MAX_TOTAL_BYTES {
                    return Err(ToolError::new(
                        "INVALID_INPUT",
                        format!("总大小超过上限（{}）。", human_bytes(MAX_TOTAL_BYTES)),
                    ));
                }
            }
        }
    }
    Ok((files, bytes))
}

fn preview(parameters: &Value) -> Result<Value, ToolError> {
    let plan = plan_from(parameters)?;
    let token = tool_protocol::issue_token(ACTION, &plan.digest())?;

    if plan.mode == "compress" {
        let (files, bytes) = scan_sources(&plan.sources)?;
        let mut data = table(
            json!([
                item("模式", "压缩"),
                item("源数量", plan.sources.len().to_string()),
                item("文件数", files.to_string()),
                item("原始大小", human_bytes(bytes)),
                item("目标", plan.archive.to_string_lossy()),
            ]),
            json!([
                column("name", "源路径"),
                column("kind", "类型"),
            ]),
            Value::Array(
                plan.sources
                    .iter()
                    .map(|source| {
                        json!({
                            "name": source.to_string_lossy(),
                            "kind": if source.is_dir() { "目录" } else { "文件" },
                        })
                    })
                    .collect(),
            ),
            json!([
                "预览只统计文件数量与大小，不会创建压缩包。",
                "符号链接不会跟随；文件数或总大小超限时拒绝执行。",
                "目标文件已存在默认拒绝，需要显式勾选覆盖（会先删除旧文件）。",
            ]),
        );
        attach_token(&mut data, token);
        return Ok(data);
    }

    let file = File::open(&plan.archive)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开压缩包：{error}")))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| ToolError::new("INTERNAL", format!("压缩包无效：{error}")))?;
    let mut rows = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len().min(200) {
        if let Ok(entry) = archive.by_index(index) {
            total = total.saturating_add(entry.size());
            rows.push(json!({
                "name": entry.name(),
                "size": human_bytes(entry.size()),
                "compressed": human_bytes(entry.compressed_size()),
            }));
        }
    }
    let mut data = table(
        json!([
            item("模式", "解压"),
            item("条目数", archive.len().to_string()),
            item("解压后大小", human_bytes(total)),
            item("目标", plan.destination.to_string_lossy()),
        ]),
        json!([
            column("name", "条目"),
            column("size", "原始大小"),
            column("compressed", "压缩后"),
        ]),
        Value::Array(rows),
        json!([
            "预览只读取压缩包目录，不会写入文件。",
            "解压时会校验每个条目的路径，拒绝跳出目标目录（Zip-Slip）。",
            "默认跳过已存在的文件；不跟随符号链接，并有单条与总量上限。",
        ]),
    );
    attach_token(&mut data, token);
    Ok(data)
}

fn compress(plan: &Plan) -> Result<Value, ToolError> {
    if plan.archive.exists() {
        if !plan.overwrite {
            return Err(ToolError::new("INVALID_INPUT", "目标文件已存在。"));
        }
        fs::remove_file(&plan.archive)
            .map_err(|error| ToolError::new("INTERNAL", format!("无法覆盖目标文件：{error}")))?;
    }
    if let Some(parent) = plan.archive.parent() {
        if !parent.is_dir() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("目标目录不存在：{}", parent.display()),
            ));
        }
    }
    let file = File::create(&plan.archive)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建压缩包：{error}")))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut files = 0usize;
    let mut bytes = 0u64;

    for source in &plan.sources {
        let base = source.parent().unwrap_or(source.as_path());
        let entries: Vec<PathBuf> = if source.is_file() {
            vec![source.clone()]
        } else {
            WalkDir::new(source)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| !entry.path_is_symlink() && entry.file_type().is_file())
                .map(|entry| entry.path().to_path_buf())
                .collect()
        };
        for path in entries {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let name = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let metadata = match fs::metadata(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            files += 1;
            if files > MAX_FILES || bytes.saturating_add(metadata.len()) > MAX_TOTAL_BYTES {
                let _ = fs::remove_file(&plan.archive);
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    "压缩过程中超过文件数或大小上限，已删除未完成的压缩包。",
                ));
            }
            bytes = bytes.saturating_add(metadata.len());
            writer
                .start_file(name, options)
                .map_err(|error| ToolError::new("INTERNAL", format!("无法写入条目：{error}")))?;
            let mut input = File::open(&path)
                .map_err(|error| ToolError::new("INTERNAL", format!("无法读取：{error}")))?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = input
                    .read(&mut buffer)
                    .map_err(|error| ToolError::new("INTERNAL", format!("读取失败：{error}")))?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| ToolError::new("INTERNAL", format!("写入失败：{error}")))?;
            }
        }
    }
    writer
        .finish()
        .map_err(|error| ToolError::new("INTERNAL", format!("完成压缩包失败：{error}")))?;

    let archive_size = fs::metadata(&plan.archive).map(|m| m.len()).unwrap_or(0);
    Ok(table(
        json!([
            item("结果", "压缩完成"),
            item("文件数", files.to_string()),
            item("原始大小", human_bytes(bytes)),
            item("压缩包大小", human_bytes(archive_size)),
            item("目标", plan.archive.to_string_lossy()),
        ]),
        json!([column("name", "项目"), column("value", "值")]),
        json!([
            {"name": "目标文件", "value": plan.archive.to_string_lossy()},
            {"name": "文件数", "value": files.to_string()},
        ]),
        json!([
            "压缩包已写入目标位置；原文件保持不变。",
            "本次操作已写入本地工具操作日志。",
        ]),
    ))
}

fn extract(plan: &Plan) -> Result<Value, ToolError> {
    fs::create_dir_all(&plan.destination)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建解压目录：{error}")))?;
    let canonical_destination = plan
        .destination
        .canonicalize()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析解压目录：{error}")))?;

    let file = File::open(&plan.archive)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开压缩包：{error}")))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| ToolError::new("INTERNAL", format!("压缩包无效：{error}")))?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut skipped = 0usize;
    let mut unsafe_entries = 0usize;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ToolError::new("INTERNAL", format!("读取条目失败：{error}")))?;
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            unsafe_entries += 1;
            continue;
        };
        if entry.size() > MAX_ENTRY_BYTES {
            skipped += 1;
            continue;
        }
        bytes = bytes.saturating_add(entry.size());
        if bytes > MAX_TOTAL_BYTES || files >= MAX_FILES {
            return Err(ToolError::new(
                "INVALID_INPUT",
                "解压超过文件数或大小上限，已停止。",
            ));
        }
        let target = canonical_destination.join(&relative);
        if !target.starts_with(&canonical_destination) {
            unsafe_entries += 1;
            continue;
        }
        if target.exists() && !plan.overwrite {
            skipped += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ToolError::new("INTERNAL", format!("无法创建目录：{error}")))?;
        }
        let mut output = File::create(&target)
            .map_err(|error| ToolError::new("INTERNAL", format!("无法写入文件：{error}")))?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = entry
                .read(&mut buffer)
                .map_err(|error| ToolError::new("INTERNAL", format!("读取条目失败：{error}")))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| ToolError::new("INTERNAL", format!("写入文件失败：{error}")))?;
        }
        files += 1;
    }

    Ok(table(
        json!([
            item("结果", "解压完成"),
            item("文件数", files.to_string()),
            item("解压大小", human_bytes(bytes)),
            item("跳过（已存在/超限）", skipped.to_string()),
            item("不安全条目", unsafe_entries.to_string()),
            item("目标", plan.destination.to_string_lossy()),
        ]),
        json!([column("name", "项目"), column("value", "值")]),
        json!([
            {"name": "目标目录", "value": plan.destination.to_string_lossy()},
            {"name": "写入文件", "value": files.to_string()},
        ]),
        json!([
            "已拒绝跳出目标目录的条目（Zip-Slip）并跳过超过单条大小上限的文件。",
            "默认不覆盖已存在文件；需要覆盖时请重新预览并勾选覆盖。",
            "本次操作已写入本地工具操作日志。",
        ]),
    ))
}

fn list(parameters: &Value) -> Result<Value, ToolError> {
    let archive_path = absolute_path(string_field(parameters, "archive"))?;
    if !archive_path.is_file() {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!("压缩包不存在：{}", archive_path.display()),
        ));
    }
    let file = File::open(&archive_path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法打开压缩包：{error}")))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| ToolError::new("INTERNAL", format!("压缩包无效：{error}")))?;
    let mut rows = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len().min(500) {
        if let Ok(entry) = archive.by_index(index) {
            total = total.saturating_add(entry.size());
            rows.push(json!({
                "name": entry.name(),
                "size": human_bytes(entry.size()),
                "compressed": human_bytes(entry.compressed_size()),
            }));
        }
    }
    Ok(table(
        json!([
            item("压缩包", archive_path.to_string_lossy()),
            item("条目数", archive.len().to_string()),
            item("解压后大小", human_bytes(total)),
        ]),
        json!([
            column("name", "条目"),
            column("size", "原始大小"),
            column("compressed", "压缩后"),
        ]),
        Value::Array(rows),
        json!(["列表为只读操作，不会写入任何文件；最多展示 500 条。"]),
    ))
}

fn apply(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "压缩/解压需要确认令牌，请先预览并确认。",
        )
    })?;
    let plan = plan_from(&request.parameters)?;
    tool_protocol::verify_token(token, ACTION, &plan.digest())?;

    let result = if plan.mode == "compress" {
        compress(&plan)?
    } else {
        extract(&plan)?
    };
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "omni_zip",
        "action": plan.mode,
        "archive": plan.archive.to_string_lossy(),
        "destination": plan.destination.to_string_lossy(),
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));
    Ok(result)
}

fn main() {
    tool_protocol::serve_with_request(
        "omni_zip",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION, "list"],
        |request| match request.action.as_str() {
            "preview" => preview(&request.parameters),
            ACTION => apply(request),
            "list" => list(&request.parameters),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_and_parent_paths() {
        assert!(absolute_path("relative\\file.zip").is_err());
        assert!(absolute_path("C:\\temp\\..\\evil.zip").is_err());
        assert!(absolute_path("\\\\server\\share\\a.zip").is_err());
        assert!(absolute_path("C:\\temp\\ok.zip").is_ok());
    }

    #[test]
    fn blocks_system_directories() {
        assert!(ensure_not_system(Path::new("C:\\Windows\\Temp\\a.zip")).is_err());
        assert!(ensure_not_system(Path::new("C:\\Program Files\\a.zip")).is_err());
        assert!(ensure_not_system(Path::new("C:\\Users\\me\\a.zip")).is_ok());
    }

    #[test]
    fn plan_digest_binds_mode_and_paths() {
        let base = Plan {
            mode: "compress".into(),
            sources: vec![PathBuf::from("C:\\Users\\me\\a.txt")],
            archive: PathBuf::from("C:\\Users\\me\\out.zip"),
            destination: PathBuf::new(),
            overwrite: false,
        };
        let mut other = Plan {
            mode: "compress".into(),
            sources: vec![PathBuf::from("C:\\Users\\me\\b.txt")],
            archive: PathBuf::from("C:\\Users\\me\\out.zip"),
            destination: PathBuf::new(),
            overwrite: false,
        };
        assert_eq!(base.digest(), base.digest());
        assert_ne!(base.digest(), other.digest());
        other.overwrite = true;
        assert_ne!(base.digest(), other.digest());
    }

    #[test]
    fn plan_requires_zip_extension() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp");
        std::fs::create_dir_all(&base).expect("create test dir");
        let source = base.join("omni-zip-test-source.txt");
        std::fs::write(&source, b"x").expect("write source");
        let archive = base.join("omni-zip-test-out.zip");
        let _ = std::fs::remove_file(&archive);

        let params = json!({
            "mode": "compress",
            "sources": [source.to_string_lossy()],
            "archive": "C:\\Users\\me\\out.rar",
            "overwrite": false
        });
        assert!(plan_from(&params).is_err());
        let ok = json!({
            "mode": "compress",
            "sources": [source.to_string_lossy()],
            "archive": archive.to_string_lossy(),
            "overwrite": false
        });
        assert!(plan_from(&ok).is_ok());

        let _ = std::fs::remove_file(&source);
    }

    #[test]
    fn compresses_and_extracts_roundtrip() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(format!("roundtrip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("sub")).expect("create dir");
        std::fs::write(base.join("a.txt"), b"alpha").expect("write a");
        std::fs::write(base.join("sub").join("b.txt"), b"beta").expect("write b");

        let archive = base.join("out.zip");
        let compress_plan = Plan {
            mode: "compress".into(),
            sources: vec![base.join("a.txt"), base.join("sub")],
            archive: archive.clone(),
            destination: PathBuf::new(),
            overwrite: false,
        };
        compress(&compress_plan).expect("compress");
        assert!(archive.is_file());

        let extract_dir = base.join("extract");
        let extract_plan = Plan {
            mode: "extract".into(),
            sources: Vec::new(),
            archive: archive.clone(),
            destination: extract_dir.clone(),
            overwrite: false,
        };
        extract(&extract_plan).expect("extract");
        assert_eq!(std::fs::read(extract_dir.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(
            std::fs::read(extract_dir.join("sub").join("b.txt")).unwrap(),
            b"beta"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
