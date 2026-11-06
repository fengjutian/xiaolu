use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use lopdf::{Document, Object};
use tool_protocol::{attach_token, column, digest_of, human_bytes, item, table, Request, ToolError};

const ACTION: &str = "apply";
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

struct Plan {
    mode: String,
    inputs: Vec<PathBuf>,
    output: PathBuf,
    pages: String,
    angle: i64,
    overwrite: bool,
}

impl Plan {
    fn digest(&self) -> String {
        let mut parts = vec![
            self.mode.clone(),
            self.output.to_string_lossy().into_owned(),
            self.pages.clone(),
            self.angle.to_string(),
            if self.overwrite { "overwrite" } else { "safe" }.to_string(),
        ];
        for input in &self.inputs {
            parts.push(input.to_string_lossy().into_owned());
        }
        let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
        digest_of(&refs)
    }
}

fn plan_from(parameters: &Value) -> Result<Plan, ToolError> {
    let mode = string_field(parameters, "mode").trim().to_lowercase();
    if !["merge", "split", "rotate"].contains(&mode.as_str()) {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "mode 只能是 merge、split 或 rotate。",
        ));
    }
    let inputs = parameters
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
    let overwrite = bool_field(parameters, "overwrite");
    if output.exists() && !overwrite {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!(
                "输出文件已存在：{}；请更换路径或勾选覆盖。",
                output.display()
            ),
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
        overwrite,
    })
}

fn document_pages(input: &Path) -> Result<u32, String> {
    Ok(Document::load(input)
        .map_err(|error| format!("无法读取 {}：{error}", input.display()))?
        .get_pages()
        .len() as u32)
}

fn info(path: &Path) -> Result<Value, ToolError> {
    let document = Document::load(path)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取 PDF：{error}")))?;
    let pages = document.get_pages().len();
    let version = document.version.clone();
    let encrypted = document.is_encrypted();
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

fn preview(parameters: &Value) -> Result<Value, ToolError> {
    let plan = plan_from(parameters)?;
    let mut input_rows = Vec::new();
    let mut total_pages = 0u32;
    for input in &plan.inputs {
        let pages = document_pages(input).map_err(|message| ToolError::new("INTERNAL", message))?;
        total_pages = total_pages.saturating_add(pages);
        input_rows.push(json!({
            "name": input.to_string_lossy(),
            "pages": pages.to_string(),
        }));
    }

    if plan.mode == "split" || plan.mode == "rotate" {
        let first_pages = document_pages(&plan.inputs[0])
            .map_err(|message| ToolError::new("INTERNAL", message))?;
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
        ]),
        json!([
            column("name", "输入文件"),
            column("pages", "页数"),
        ]),
        Value::Array(input_rows),
        json!([
            "预览只读取 PDF 结构，不会写入文件。",
            "合并会保留各文件的页面对象；拆分只保留指定页码；旋转按给定角度累加。",
            "输出文件已存在默认拒绝，需要显式勾选覆盖。",
            "加密 PDF 是否需要密码取决于文件本身，读取失败会直接报错。",
        ]),
    );
    attach_token(&mut data, token);
    Ok(data)
}

fn save_document(document: &mut Document, plan: &Plan) -> Result<(), ToolError> {
    if plan.output.exists() && !plan.overwrite {
        return Err(ToolError::new("INVALID_INPUT", "输出文件已存在。"));
    }
    if let Some(parent) = plan.output.parent() {
        if !parent.is_dir() {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("输出目录不存在：{}", parent.display()),
            ));
        }
    }
    document
        .save(&plan.output)
        .map_err(|error| ToolError::new("INTERNAL", format!("保存 PDF 失败：{error}")))?;
    Ok(())
}

fn merge(plan: &Plan) -> Result<usize, ToolError> {
    let mut documents = plan
        .inputs
        .iter()
        .map(Document::load)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ToolError::new("INTERNAL", format!("读取 PDF 失败：{error}")))?;
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
    save_document(&mut base, plan)?;
    Ok(total)
}

fn split(plan: &Plan) -> Result<usize, ToolError> {
    if plan.inputs.len() != 1 {
        return Err(ToolError::new("INVALID_INPUT", "拆分只接受一个输入 PDF。"));
    }
    let mut document = Document::load(&plan.inputs[0])
        .map_err(|error| ToolError::new("INTERNAL", format!("读取 PDF 失败：{error}")))?;
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
    save_document(&mut document, plan)?;
    Ok(kept)
}

fn rotate(plan: &Plan) -> Result<usize, ToolError> {
    if plan.inputs.len() != 1 {
        return Err(ToolError::new("INVALID_INPUT", "旋转只接受一个输入 PDF。"));
    }
    if plan.angle % 90 != 0 {
        return Err(ToolError::new("INVALID_INPUT", "旋转角度必须是 90 的倍数。"));
    }
    let mut document = Document::load(&plan.inputs[0])
        .map_err(|error| ToolError::new("INTERNAL", format!("读取 PDF 失败：{error}")))?;
    let total = document.get_pages().len() as u32;
    let targets = if plan.pages.trim().is_empty() {
        (1..=total).collect::<Vec<_>>()
    } else {
        parse_pages(&plan.pages, total)?
    };
    let pages = document.get_pages();
    for page_number in &targets {
        let Some(page_id) = pages.get(page_number).copied() else {
            continue;
        };
        if let Ok(object) = document.get_object_mut(page_id) {
            if let Ok(dict) = object.as_dict_mut() {
                let current = dict
                    .get(b"Rotate")
                    .ok()
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(0);
                let next = (((current + plan.angle) % 360) + 360) % 360;
                dict.set("Rotate", next);
            }
        }
    }
    save_document(&mut document, plan)?;
    Ok(targets.len())
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

    let (action_label, affected) = match plan.mode.as_str() {
        "merge" => ("合并", merge(&plan)?),
        "split" => ("拆分", split(&plan)?),
        _ => ("旋转", rotate(&plan)?),
    };
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "master_pdf",
        "action": plan.mode,
        "output": plan.output.to_string_lossy(),
        "pages": affected,
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("结果", format!("{action_label}完成")),
            item("页数", affected.to_string()),
            item("输出", plan.output.to_string_lossy()),
        ]),
        json!([column("name", "项目"), column("value", "值")]),
        json!([
            {"name": "输出文件", "value": plan.output.to_string_lossy()},
            {"name": "页数", "value": affected.to_string()},
        ]),
        json!([
            "新文件已写入输出路径；输入文件保持不变。",
            "本次操作已写入本地工具操作日志。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "master_pdf",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION, "info"],
        |request| match request.action.as_str() {
            "preview" => preview(&request.parameters),
            ACTION => apply(request),
            "info" => {
                let path = absolute_path(string_field(&request.parameters, "path"))?;
                info(&path)
            }
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
        assert!(parse_pages("1-5", 3).is_err());
        assert!(parse_pages("abc", 3).is_err());
    }

    #[test]
    fn merges_split_and_rotates_documents() {
        let dir = test_dir("pdf");
        let a = dir.join("a.pdf");
        let b = dir.join("b.pdf");
        sample_pdf(&a, 2);
        sample_pdf(&b, 3);

        let merge_plan = Plan {
            mode: "merge".into(),
            inputs: vec![a.clone(), b.clone()],
            output: dir.join("merged.pdf"),
            pages: String::new(),
            angle: 0,
            overwrite: false,
        };
        assert_eq!(merge(&merge_plan).unwrap(), 5);
        assert_eq!(Document::load(dir.join("merged.pdf")).unwrap().get_pages().len(), 5);

        let split_plan = Plan {
            mode: "split".into(),
            inputs: vec![a.clone()],
            output: dir.join("split.pdf"),
            pages: "2".into(),
            angle: 0,
            overwrite: false,
        };
        assert_eq!(split(&split_plan).unwrap(), 1);

        let rotate_plan = Plan {
            mode: "rotate".into(),
            inputs: vec![a.clone()],
            output: dir.join("rotated.pdf"),
            pages: "1".into(),
            angle: 90,
            overwrite: false,
        };
        assert_eq!(rotate(&rotate_plan).unwrap(), 1);
        let rotated = Document::load(dir.join("rotated.pdf")).unwrap();
        let page_id = *rotated.get_pages().values().next().unwrap();
        let rotation = rotated
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Rotate")
            .and_then(Object::as_i64)
            .unwrap();
        assert_eq!(rotation, 90);

        let _ = fs::remove_dir_all(&dir);
    }
}
