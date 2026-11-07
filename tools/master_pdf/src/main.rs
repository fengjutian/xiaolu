use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, LoadOptions, Object, Permissions};
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};

const ACTION: &str = "apply";
const PAGES_ACTION: &str = "pages";
const INFO_ACTION: &str = "info";
/// 页面预览最多返回的行数，避免超大 PDF 把界面刷爆。
const MAX_PAGE_ROWS: usize = 500;
/// 输出冲突策略。
const CONFLICT_FAIL: &str = "fail";
const CONFLICT_OVERWRITE: &str = "overwrite";
const CONFLICT_RENAME: &str = "rename";
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

/// 解析 "1-3,5" 形式的页码；返回去重升序的 1-based 页码。
fn parse_pages(spec: &str, total: u32) -> Result<Vec<u32>, ToolError> {
    let mut pages = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            let start = start
                .trim()
                .parse::<u32>()
                .map_err(|_| ToolError::new("INVALID_INPUT", "页码范围无效。"))?;
            let end = end
                .trim()
                .parse::<u32>()
                .map_err(|_| ToolError::new("INVALID_INPUT", "页码范围无效。"))?;
            if start == 0 || end < start || end > total {
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    format!("页码超出范围（1–{total}）。"),
                ));
            }
            for page in start..=end {
                pages.push(page);
            }
        } else {
            let page = part
                .parse::<u32>()
                .map_err(|_| ToolError::new("INVALID_INPUT", "页码无效。"))?;
            if page == 0 || page > total {
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    format!("页码超出范围（1–{total}）。"),
                ));
            }
            pages.push(page);
        }
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

/// 权限项 → PDF 权限位。界面用短名字传，工具在这里展开。
fn parse_permissions(values: &[String]) -> Result<Permissions, ToolError> {
    let mut permissions = Permissions::empty();
    for value in values {
        match value.trim().to_lowercase().as_str() {
            "print" => {
                permissions |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY
            }
            "copy" => {
                permissions |= Permissions::COPYABLE | Permissions::COPYABLE_FOR_ACCESSIBILITY
            }
            "modify" => permissions |= Permissions::MODIFIABLE,
            "annotate" => permissions |= Permissions::ANNOTABLE,
            "fill" => permissions |= Permissions::FILLABLE,
            "assemble" => permissions |= Permissions::ASSEMBLABLE,
            other => {
                return Err(ToolError::new(
                    "INVALID_INPUT",
                    format!("未知的权限项：{other}。"),
                ))
            }
        }
    }
    Ok(permissions)
}

/// 权限位的可读描述，用于结果回执。
fn describe_permissions(values: &[String]) -> String {
    let labels = [
        ("print", "可打印"),
        ("copy", "可复制"),
        ("modify", "可修改"),
        ("annotate", "可批注"),
        ("fill", "可填写"),
        ("assemble", "可组织页面"),
    ];
    let mut parts = Vec::new();
    for (key, label) in labels {
        if values
            .iter()
            .any(|value| value.trim().eq_ignore_ascii_case(key))
        {
            parts.push(label);
        }
    }
    if parts.is_empty() {
        "仅允许阅读".to_string()
    } else {
        parts.join("、")
    }
}

struct Plan {
    mode: String,
    inputs: Vec<PathBuf>,
    output: PathBuf,
    pages: String,
    angle: i64,
    conflict: String,
    /// 输入 PDF 的打开密码（读取加密文件用）。
    password: String,
    /// 输出 PDF 的用户密码；非空则加密输出。
    out_password: String,
    /// 输出权限项（print/copy/modify/annotate/fill/assemble）。
    permissions: Vec<String>,
}

impl Plan {
    fn digest(&self) -> String {
        let mut parts = vec![
            self.mode.clone(),
            self.output.to_string_lossy().into_owned(),
            self.pages.clone(),
            self.angle.to_string(),
            format!("conflict:{}", self.conflict),
            // 只绑定「是否加密」，不把密码本身写进令牌载荷（载荷是可解的十六进制）。
            format!("encrypted:{}", !self.out_password.is_empty()),
        ];
        for permission in &self.permissions {
            parts.push(format!("perm:{permission}"));
        }
        for input in &self.inputs {
            parts.push(input.to_string_lossy().into_owned());
        }
        let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
        digest_of(&refs)
    }

    fn encrypted(&self) -> bool {
        !self.out_password.is_empty()
    }
}

/// 收集输入文件：既支持 inputs 数组，也支持单个 path。
fn collect_inputs(parameters: &Value) -> Result<Vec<PathBuf>, ToolError> {
    let mut inputs = parameters
        .get("inputs")
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
    if inputs.is_empty() {
        let single = string_field(parameters, "path").trim();
        if !single.is_empty() {
            inputs.push(absolute_path(single)?);
        }
    }
    if inputs.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "请至少提供一个输入 PDF。"));
    }
    for input in &inputs {
        if !input.is_file() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("输入文件不存在：{}", input.display()),
            ));
        }
        if input
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_lowercase())
            != Some("pdf".into())
        {
            return Err(ToolError::new("INVALID_INPUT", "输入必须是 PDF 文件。"));
        }
    }
    Ok(inputs)
}

fn plan_from(parameters: &Value) -> Result<Plan, ToolError> {
    let mode = string_field(parameters, "mode").trim().to_lowercase();
    if !["merge", "split", "rotate"].contains(&mode.as_str()) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "mode 只能是 merge、split 或 rotate。",
        ));
    }
    let inputs = collect_inputs(parameters)?;
    let output = absolute_path(string_field(parameters, "output"))?;
    if output
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_lowercase())
        != Some("pdf".into())
    {
        return Err(ToolError::new("INVALID_INPUT", "输出必须是 .pdf 文件。"));
    }
    ensure_not_system(&output)?;

    // 冲突策略：优先取 conflict；没有时兼容旧的 overwrite 布尔值。
    let mut conflict = string_field(parameters, "conflict").trim().to_lowercase();
    if conflict.is_empty() {
        conflict = if bool_field(parameters, "overwrite") {
            CONFLICT_OVERWRITE.to_string()
        } else {
            CONFLICT_FAIL.to_string()
        };
    }
    if ![CONFLICT_FAIL, CONFLICT_OVERWRITE, CONFLICT_RENAME].contains(&conflict.as_str()) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "conflict 只能是 fail、overwrite 或 rename。",
        ));
    }

    let permissions = parameters
        .get("permissions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // 提前校验权限项，别等到写文件时才发现写错了名字。
    parse_permissions(&permissions)?;

    let out_password = string_field(parameters, "outPassword").to_string();
    if !out_password.is_empty() && out_password.len() > 127 {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "输出密码过长（PDF 最多 127 字节）。",
        ));
    }

    Ok(Plan {
        mode,
        inputs,
        output,
        pages: string_field(parameters, "pages").trim().to_string(),
        angle: parameters
            .get("angle")
            .and_then(Value::as_i64)
            .unwrap_or(90),
        conflict,
        password: string_field(parameters, "password").to_string(),
        out_password,
        permissions,
    })
}

/// 打开 PDF；加密文件用密码解密，密码不对给出明确原因。
///
/// 必须在**加载期**就把密码交给 lopdf：加密文件的对象内容是密文，
/// 事后调 `decrypt` 已经太晚（对象解析阶段就丢了）。
fn open_document(path: &Path, password: &str) -> Result<Document, ToolError> {
    Document::load_with_options(path, LoadOptions::with_password(password)).map_err(|error| {
        if matches!(error, lopdf::Error::InvalidPassword) {
            if password.is_empty() {
                ToolError::new(
                    "PASSWORD_REQUIRED",
                    format!("{} 已加密，请输入打开密码。", path.display()),
                )
            } else {
                ToolError::new(
                    "INVALID_PASSWORD",
                    format!("{} 的密码不正确。", path.display()),
                )
            }
        } else {
            ToolError::new("INTERNAL", format!("无法读取 {}：{error}", path.display()))
        }
    })
}

/// 页面尺寸（毫米）与旋转角度。
fn page_geometry(document: &Document, page_id: lopdf::ObjectId) -> (f64, f64, i64) {
    let mut width = 0.0f64;
    let mut height = 0.0f64;
    let mut rotation = 0i64;
    if let Ok(page) = document.get_dictionary(page_id) {
        if let Ok(box_value) = page.get(b"MediaBox").and_then(Object::as_array) {
            let numbers = box_value
                .iter()
                .filter_map(|value| value.as_i64().ok())
                .map(|value| value as f64)
                .collect::<Vec<_>>();
            if numbers.len() >= 4 {
                width = (numbers[2] - numbers[0]).abs();
                height = (numbers[3] - numbers[1]).abs();
            }
        }
        rotation = page
            .get(b"Rotate")
            .ok()
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);
    }
    // 1 pt = 25.4 / 72 mm
    (width * 25.4 / 72.0, height * 25.4 / 72.0, rotation)
}

/// 抽取页面文本并压成一行摘要。
fn summarize_text(text: &str, limit: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "（无文本）".to_string();
    }
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let head = collapsed.chars().take(limit).collect::<String>();
    format!("{head}…")
}

/// 页面级预览：逐页给出文件、页码、尺寸、旋转与文本摘要。
fn page_preview(parameters: &Value) -> Result<Value, ToolError> {
    let inputs = collect_inputs(parameters)?;
    let password = string_field(parameters, "password").to_string();
    let mut rows = Vec::new();
    let mut total_pages = 0u32;
    let mut truncated = false;

    for input in &inputs {
        let document = open_document(input, &password)?;
        let name = input
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| input.to_string_lossy().into_owned());
        for (number, page_id) in document.get_pages() {
            if rows.len() >= MAX_PAGE_ROWS {
                truncated = true;
                break;
            }
            let (width, height, rotation) = page_geometry(&document, page_id);
            let text = document.extract_text(&[number]).unwrap_or_default();
            rows.push(json!({
                "file": name,
                "page": number.to_string(),
                "size": format!("{width:.0}×{height:.0} mm"),
                "rotate": format!("{rotation}°"),
                "text": summarize_text(&text, 120),
            }));
            total_pages = total_pages.saturating_add(1);
        }
        if truncated {
            break;
        }
    }

    let mut notes = vec![
        "页面预览为只读操作：只读取页面结构与文本，不写入任何文件。".to_string(),
        "尺寸按 MediaBox 换算为毫米；旋转为页面自身的 /Rotate 值。".to_string(),
    ];
    if truncated {
        notes.push(format!(
            "页面过多，只显示前 {MAX_PAGE_ROWS} 页（总页数 {total_pages}）。"
        ));
    }
    Ok(table(
        json!([
            item("文件数", inputs.len().to_string()),
            item("总页数", total_pages.to_string()),
        ]),
        json!([
            column("file", "文件"),
            column("page", "页码"),
            column("size", "尺寸"),
            column("rotate", "旋转"),
            column("text", "文本摘要"),
        ]),
        Value::Array(rows),
        json!(notes),
    ))
}

fn document_pages(input: &Path, password: &str) -> Result<u32, ToolError> {
    Ok(open_document(input, password)?.get_pages().len() as u32)
}

fn info(path: &Path, password: &str) -> Result<Value, ToolError> {
    let document = open_document(path, password)?;
    let pages = document.get_pages().len();
    let version = document.version.clone();
    let encrypted = document.was_encrypted();
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let title = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| object.as_reference().ok())
        .and_then(|id| document.get_object(id).ok())
        .and_then(|object| object.as_dict().ok())
        .and_then(|dict| dict.get(b"Title").ok())
        .and_then(|object| object.as_str().ok())
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();

    Ok(table(
        json!([
            item("文件", path.to_string_lossy()),
            item("页数", pages.to_string()),
            item("PDF 版本", version),
            item("文件大小", human_bytes(size)),
            item("加密", if encrypted { "是" } else { "否" }),
        ]),
        json!([column("name", "项目"), column("value", "值")]),
        json!([
            {"name": "标题", "value": if title.is_empty() { "未提供" } else { title.as_str() }},
            {"name": "页数", "value": pages.to_string()},
        ]),
        json!(["只读信息：不修改、不重写 PDF 文件。"]),
    ))
}

/// 找出第一个没被占用的自动改名路径：a.pdf → a (1).pdf → a (2).pdf。
fn next_free_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned());
    let parent = path.parent().unwrap_or(Path::new(""));
    for index in 1..1000 {
        let name = match &extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// 按冲突策略决定真正要写入的路径。
fn resolve_output(plan: &Plan) -> Result<PathBuf, ToolError> {
    if !plan.output.exists() {
        return Ok(plan.output.clone());
    }
    match plan.conflict.as_str() {
        CONFLICT_OVERWRITE => Ok(plan.output.clone()),
        CONFLICT_RENAME => Ok(next_free_path(&plan.output)),
        _ => {
            let suggestion = next_free_path(&plan.output);
            Err(ToolError::new(
                "INVALID_INPUT",
                format!(
                    "输出文件已存在：{}；请更换路径、选择「覆盖」，或选择「自动改名」（将写入 {}）。",
                    plan.output.display(),
                    suggestion.display()
                ),
            ))
        }
    }
}

/// 预览里说明输出当前的冲突状态，让用户在确认前就知道会写到哪。
fn describe_conflict(plan: &Plan) -> String {
    if !plan.output.exists() {
        return "输出路径可用，将直接写入。".to_string();
    }
    match plan.conflict.as_str() {
        CONFLICT_OVERWRITE => "输出文件已存在，将被覆盖。".to_string(),
        CONFLICT_RENAME => format!(
            "输出文件已存在，将自动改名为 {}。",
            next_free_path(&plan.output).display()
        ),
        _ => format!(
            "输出文件已存在，执行时会拒绝并提示改名建议（{}）。",
            next_free_path(&plan.output).display()
        ),
    }
}

fn preview(parameters: &Value) -> Result<Value, ToolError> {
    let plan = plan_from(parameters)?;
    let mut input_rows = Vec::new();
    let mut total_pages = 0u32;
    for input in &plan.inputs {
        let pages = document_pages(input, &plan.password)?;
        total_pages = total_pages.saturating_add(pages);
        input_rows.push(json!({
            "name": input.to_string_lossy(),
            "pages": pages.to_string(),
        }));
    }

    if plan.mode == "split" || plan.mode == "rotate" {
        let first_pages = document_pages(&plan.inputs[0], &plan.password)?;
        if plan.mode == "split" {
            let pages = parse_pages(&plan.pages, first_pages)?;
            if pages.is_empty() {
                return Err(ToolError::new("INVALID_INPUT", "拆分需要页码，例如 1-3,5。"));
            }
        } else if !plan.pages.trim().is_empty() {
            parse_pages(&plan.pages, first_pages)?;
        }
    }

    let token = tool_protocol::issue_token(ACTION, &plan.digest())?;
    let mut notes = vec![
        "预览只读取 PDF 结构，不会写入文件。".to_string(),
        "合并会保留各文件的页面对象；拆分只保留指定页码；旋转按给定角度累加。".to_string(),
        describe_conflict(&plan),
    ];
    if plan.encrypted() {
        notes.push(format!(
            "输出将用密码加密，权限：{}。",
            describe_permissions(&plan.permissions)
        ));
    }
    let mut data = table(
        json!([
            item("模式", match plan.mode.as_str() {
                "merge" => "合并",
                "split" => "拆分",
                _ => "旋转",
            }),
            item("输入文件", plan.inputs.len().to_string()),
            item("总页数", total_pages.to_string()),
            item("输出", plan.output.to_string_lossy()),
            item("输出加密", if plan.encrypted() { "是" } else { "否" }),
        ]),
        json!([
            column("name", "输入文件"),
            column("pages", "页数"),
        ]),
        Value::Array(input_rows),
        json!(notes),
    );
    attach_token(&mut data, token);
    Ok(data)
}

/// 生成 16 字节文件标识。PDF 加密要用 trailer 的 /ID 派生文件密钥，
/// 缺失时必须补一个（真实 PDF 一般都有，但程序生成的 PDF 常常没有）。
fn file_identifier() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    for offset in 0..2u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(std::process::id() as u64);
        hasher.write_u64(tool_protocol::now_ms().wrapping_add(offset));
        bytes.extend_from_slice(&hasher.finish().to_be_bytes());
    }
    bytes
}

/// 确保 trailer 里有 /ID；已有则保留（加密密钥就是用它算出来的）。
fn ensure_file_identifier(document: &mut Document) {
    if document.trailer.get(b"ID").is_ok() {
        return;
    }
    let identifier = file_identifier();
    document.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(identifier.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(identifier, lopdf::StringFormat::Hexadecimal),
        ]),
    );
}

/// 按权限设置加密文档（AES-128）。
fn encrypt_document(document: &mut Document, plan: &Plan) -> Result<(), ToolError> {
    ensure_file_identifier(document);
    let permissions = parse_permissions(&plan.permissions)?;
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: &plan.out_password,
        user_password: &plan.out_password,
        permissions,
    })
    .map_err(|error| ToolError::new("INTERNAL", format!("无法准备加密参数：{error}")))?;
    // AES-128 属于 PDF 1.6 引入的能力。
    document.version = "1.6".to_string();
    document
        .encrypt(&state)
        .map_err(|error| ToolError::new("INTERNAL", format!("加密输出失败：{error}")))?;
    Ok(())
}

/// 保存文档：必要时加密，再写到目标路径。
fn save_document(
    document: &mut Document,
    plan: &Plan,
    output: &Path,
) -> Result<(), ToolError> {
    if let Some(parent) = output.parent() {
        if !parent.is_dir() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("输出目录不存在：{}", parent.display()),
            ));
        }
    }
    if plan.encrypted() {
        encrypt_document(document, plan)?;
    }
    document
        .save(output)
        .map_err(|error| ToolError::new("INTERNAL", format!("保存 PDF 失败：{error}")))?;
    Ok(())
}

/// 写后回读校验：确认输出能打开、页数符合预期、旋转结果正确。
fn verify_output(
    output: &Path,
    plan: &Plan,
    expected_pages: usize,
    expected_rotations: &[(u32, i64)],
) -> Result<(), ToolError> {
    let document = open_document(output, &plan.out_password)?;
    let actual = document.get_pages().len();
    if actual != expected_pages {
        return Err(ToolError::new(
            "INTERNAL",
            format!("回读校验失败：输出 {actual} 页，预期 {expected_pages} 页。"),
        ));
    }
    let pages = document.get_pages();
    for (number, expected) in expected_rotations {
        let Some(page_id) = pages.get(number).copied() else {
            return Err(ToolError::new(
                "INTERNAL",
                format!("回读校验失败：输出缺少第 {number} 页。"),
            ));
        };
        let (_, _, rotation) = page_geometry(&document, page_id);
        if rotation != *expected {
            return Err(ToolError::new(
                "INTERNAL",
                format!("回读校验失败：第 {number} 页旋转为 {rotation}°，预期 {expected}°。"),
            ));
        }
    }
    Ok(())
}

fn merge(plan: &Plan, output: &Path) -> Result<usize, ToolError> {
    let mut documents = plan
        .inputs
        .iter()
        .map(|input| open_document(input, &plan.password))
        .collect::<Result<Vec<_>, _>>()?;
    let mut base = documents.remove(0);
    base.renumber_objects();
    let pages_id = base
        .catalog()
        .map_err(|error| ToolError::new("INTERNAL", format!("PDF 缺少目录：{error}")))?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| ToolError::new("INTERNAL", "PDF 页面树无效"))?;
    let mut next_id = base.max_id + 1;
    let mut total = base.get_pages().len();

    for mut document in documents {
        document.renumber_objects_with(next_id);
        next_id = document.max_id + 1;
        let pages = document.get_pages();
        for (_, page_id) in &pages {
            if let Ok(object) = document.get_object_mut(*page_id) {
                if let Ok(dict) = object.as_dict_mut() {
                    dict.set("Parent", pages_id);
                }
            }
        }
        {
            let pages_object = base
                .get_object_mut(pages_id)
                .map_err(|_| ToolError::new("INTERNAL", "页面树对象缺失"))?;
            let dict = pages_object
                .as_dict_mut()
                .map_err(|_| ToolError::new("INTERNAL", "页面树不是字典"))?;
            {
                let kids = dict
                    .get_mut(b"Kids")
                    .and_then(Object::as_array_mut)
                    .map_err(|_| ToolError::new("INTERNAL", "页面树缺少 Kids"))?;
                for (_, page_id) in &pages {
                    kids.push(Object::Reference(*page_id));
                }
            }
            let count = dict
                .get(b"Kids")
                .and_then(Object::as_array)
                .map(|kids| kids.len() as i64)
                .unwrap_or(0);
            dict.set("Count", count);
        }
        for (id, object) in document.objects {
            base.objects.insert(id, object);
        }
        base.max_id = base.max_id.max(next_id.saturating_sub(1));
        total += pages.len();
    }

    let _ = base.prune_objects();
    save_document(&mut base, plan, output)?;
    Ok(total)
}

fn split(plan: &Plan, output: &Path) -> Result<usize, ToolError> {
    if plan.inputs.len() != 1 {
        return Err(ToolError::new("INVALID_INPUT", "拆分只接受一个输入 PDF。"));
    }
    let mut document = open_document(&plan.inputs[0], &plan.password)?;
    let total = document.get_pages().len() as u32;
    let keep = parse_pages(&plan.pages, total)?;
    if keep.is_empty() {
        return Err(ToolError::new("INVALID_INPUT", "拆分需要页码，例如 1-3,5。"));
    }
    let remove = (1..=total)
        .filter(|page| !keep.contains(page))
        .collect::<Vec<_>>();
    document.delete_pages(&remove);
    document.prune_objects();
    document.renumber_objects();
    let kept = document.get_pages().len();
    save_document(&mut document, plan, output)?;
    Ok(kept)
}

/// 旋转页面；返回 (页数, [(页码, 新的旋转角度)])，后者用于写后回读校验。
fn rotate(plan: &Plan, output: &Path) -> Result<(usize, Vec<(u32, i64)>), ToolError> {
    if plan.inputs.len() != 1 {
        return Err(ToolError::new("INVALID_INPUT", "旋转只接受一个输入 PDF。"));
    }
    if plan.angle % 90 != 0 {
        return Err(ToolError::new("INVALID_INPUT", "旋转角度必须是 90 的倍数。"));
    }
    let mut document = open_document(&plan.inputs[0], &plan.password)?;
    let total = document.get_pages().len() as u32;
    let targets = if plan.pages.trim().is_empty() {
        (1..=total).collect::<Vec<_>>()
    } else {
        parse_pages(&plan.pages, total)?
    };
    let pages = document.get_pages();
    let mut expected = Vec::new();
    for page_number in &targets {
        let Some(page_id) = pages.get(page_number).copied() else {
            continue;
        };
        let mut next = 0i64;
        if let Ok(object) = document.get_object_mut(page_id) {
            if let Ok(dict) = object.as_dict_mut() {
                let current = dict
                    .get(b"Rotate")
                    .ok()
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(0);
                next = (((current + plan.angle) % 360) + 360) % 360;
                dict.set("Rotate", next);
            }
        }
        expected.push((*page_number, next));
    }
    save_document(&mut document, plan, output)?;
    Ok((targets.len(), expected))
}

fn apply(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "PDF 操作需要确认令牌，请先预览并确认。",
        )
    })?;
    let plan = plan_from(&request.parameters)?;
    tool_protocol::verify_token(token, ACTION, &plan.digest())?;

    let output = resolve_output(&plan)?;
    let (action_label, affected, rotations) = match plan.mode.as_str() {
        "merge" => ("合并", merge(&plan, &output)?, Vec::new()),
        "split" => ("拆分", split(&plan, &output)?, Vec::new()),
        _ => {
            let (count, rotations) = rotate(&plan, &output)?;
            ("旋转", count, rotations)
        }
    };
    verify_output(&output, &plan, affected, &rotations)?;

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "master_pdf",
        "action": plan.mode,
        "output": output.to_string_lossy(),
        "conflict": plan.conflict,
        "encrypted": plan.encrypted(),
        "pages": affected,
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("结果", format!("{action_label}完成")),
            item("页数", affected.to_string()),
            item("输出", output.to_string_lossy()),
            item("输出加密", if plan.encrypted() { "是" } else { "否" }),
            item("权限", if plan.encrypted() {
                describe_permissions(&plan.permissions)
            } else {
                "未加密，不受限".to_string()
            }),
            item("回读校验", format!("输出可正常打开，页数 {affected}")),
        ]),
        json!([column("name", "项目"), column("value", "值")]),
        json!([
            {"name": "输出文件", "value": output.to_string_lossy()},
            {"name": "页数", "value": affected.to_string()},
        ]),
        json!([
            "新文件已写入输出路径；输入文件保持不变。",
            "写后已重新打开输出校验页数与旋转结果。",
            "本次操作已写入本地工具操作日志。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "master_pdf",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION, INFO_ACTION, PAGES_ACTION],
        |request| match request.action.as_str() {
            "preview" => preview(&request.parameters),
            ACTION => apply(request),
            INFO_ACTION => {
                let path = absolute_path(string_field(&request.parameters, "path"))?;
                info(&path, string_field(&request.parameters, "password"))
            }
            PAGES_ACTION => page_preview(&request.parameters),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn sample_pdf(path: &Path, pages: u32) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..pages {
            let content_id = document.add_object(lopdf::Stream::new(
                lopdf::Dictionary::new(),
                Vec::new(),
            ));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => content_id,
                "Resources" => lopdf::Dictionary::new(),
            });
            kids.push(Object::Reference(page_id));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(kids),
                "Count" => i64::from(pages),
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).expect("save sample pdf");
    }

    fn test_dir(name: &str) -> PathBuf {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("create test dir");
        base
    }

    #[test]
    fn parses_page_specs_and_rejects_out_of_range() {
        assert_eq!(parse_pages("1-3,5", 10).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_pages("2", 2).unwrap(), vec![2]);
        assert!(parse_pages("0", 2).is_err());
        assert!(parse_pages("3-1", 5).is_err());
        assert!(parse_pages("9", 2).is_err());
    }

    #[test]
    fn parses_permission_items() {
        let permissions = parse_permissions(&["print".into(), "copy".into()]).unwrap();
        assert!(permissions.contains(Permissions::PRINTABLE));
        assert!(permissions.contains(Permissions::COPYABLE));
        assert!(!permissions.contains(Permissions::MODIFIABLE));
        assert!(parse_permissions(&["bogus".into()]).is_err());
    }

    #[test]
    fn summarizes_text_into_one_line() {
        assert_eq!(summarize_text("  hello \n world  ", 120), "hello world");
        assert_eq!(summarize_text("", 120), "（无文本）");
        assert_eq!(summarize_text("   ", 120), "（无文本）");
        let long = summarize_text(&"字".repeat(200), 10);
        assert_eq!(long.chars().count(), 11);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn conflict_policy_picks_expected_path() {
        let base = test_dir("conflict");
        let output = base.join("out.pdf");
        let mut plan = Plan {
            mode: "merge".into(),
            inputs: Vec::new(),
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: String::new(),
            permissions: Vec::new(),
        };

        // 不存在 → 直接写
        assert_eq!(resolve_output(&plan).unwrap(), output);

        // 已存在 + fail → 报错并给出建议名
        fs::write(&output, b"x").expect("write output");
        let error = resolve_output(&plan).unwrap_err();
        assert!(error.message.contains("自动改名"));
        assert!(error.message.contains("out (1).pdf"));

        // 已存在 + rename → 换成新名字
        plan.conflict = CONFLICT_RENAME.into();
        assert_eq!(resolve_output(&plan).unwrap(), base.join("out (1).pdf"));

        // 已存在 + overwrite → 仍写原路径
        plan.conflict = CONFLICT_OVERWRITE.into();
        assert_eq!(resolve_output(&plan).unwrap(), output);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn page_preview_lists_every_page() {
        let base = test_dir("pages");
        let source = base.join("sample.pdf");
        sample_pdf(&source, 3);
        let data = page_preview(&json!({
            "path": source.to_string_lossy(),
        }))
        .expect("page preview");
        let rows = data.get("rows").and_then(Value::as_array).expect("rows");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].get("page").and_then(Value::as_str), Some("1"));
        assert_eq!(
            rows[0].get("size").and_then(Value::as_str),
            Some("210×297 mm")
        );
        assert_eq!(rows[0].get("text").and_then(Value::as_str), Some("（无文本）"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn merge_writes_and_verifies_output() {
        let base = test_dir("merge");
        let first = base.join("a.pdf");
        let second = base.join("b.pdf");
        sample_pdf(&first, 2);
        sample_pdf(&second, 3);
        let output = base.join("merged.pdf");
        let plan = Plan {
            mode: "merge".into(),
            inputs: vec![first, second],
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: String::new(),
            permissions: Vec::new(),
        };
        let pages = merge(&plan, &output).expect("merge");
        assert_eq!(pages, 5);
        verify_output(&output, &plan, 5, &[]).expect("verify");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn rotate_result_is_verified_by_readback() {
        let base = test_dir("rotate");
        let source = base.join("a.pdf");
        sample_pdf(&source, 2);
        let output = base.join("rotated.pdf");
        let plan = Plan {
            mode: "rotate".into(),
            inputs: vec![source],
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: String::new(),
            permissions: Vec::new(),
        };
        let (count, rotations) = rotate(&plan, &output).expect("rotate");
        assert_eq!(count, 2);
        assert_eq!(rotations.len(), 2);
        assert_eq!(rotations[0].1, 90);
        verify_output(&output, &plan, 2, &rotations).expect("verify");
        // 写错角度时回读校验必须失败
        assert!(verify_output(&output, &plan, 2, &[(1, 180)]).is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn encrypted_output_needs_the_password() {
        let base = test_dir("encrypt");
        let source = base.join("a.pdf");
        sample_pdf(&source, 2);
        let output = base.join("secret.pdf");
        let plan = Plan {
            mode: "merge".into(),
            inputs: vec![source],
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: "s3cret".into(),
            permissions: vec!["print".into()],
        };
        merge(&plan, &output).expect("merge encrypted");
        assert!(output.is_file());

        // 没有密码读不出来
        let error = open_document(&output, "").unwrap_err();
        assert_eq!(error.code, "PASSWORD_REQUIRED");

        // 密码正确可以打开，且页数正确
        verify_output(&output, &plan, 2, &[]).expect("verify with password");

        // 输出确实带加密字典
        let raw = Document::load(&output).expect("load raw");
        assert!(raw.is_encrypted());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn encrypted_output_records_permissions() {
        let base = test_dir("perm");
        let source = base.join("a.pdf");
        sample_pdf(&source, 2);
        let output = base.join("secret.pdf");
        let plan = Plan {
            mode: "merge".into(),
            inputs: vec![source],
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: "s3cret".into(),
            permissions: vec!["print".into(), "copy".into()],
        };
        merge(&plan, &output).expect("merge encrypted");

        let raw = Document::load(&output).expect("load raw");
        assert!(raw.is_encrypted(), "输出必须带加密字典");
        let encrypt = raw.get_encrypted().expect("encrypt dict");
        let bits = encrypt.get(b"P").and_then(|value| value.as_i64()).unwrap_or(0);
        // PDF 权限位：bit3 打印、bit4 修改、bit5 复制。
        assert_eq!(bits & 4, 4, "应允许打印");
        assert_eq!(bits & 16, 16, "应允许复制");
        assert_eq!(bits & 8, 0, "不应允许修改");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn wrong_password_is_reported() {
        let base = test_dir("wrong-password");
        let source = base.join("a.pdf");
        sample_pdf(&source, 1);
        let output = base.join("secret.pdf");
        let mut plan = Plan {
            mode: "merge".into(),
            inputs: vec![source],
            output: output.clone(),
            pages: String::new(),
            angle: 90,
            conflict: CONFLICT_FAIL.into(),
            password: String::new(),
            out_password: "right".into(),
            permissions: Vec::new(),
        };
        merge(&plan, &output).expect("merge encrypted");
        let error = open_document(&output, "wrong").unwrap_err();
        assert_eq!(error.code, "INVALID_PASSWORD");
        plan.out_password = String::new();
        let _ = fs::remove_dir_all(&base);
    }
}
