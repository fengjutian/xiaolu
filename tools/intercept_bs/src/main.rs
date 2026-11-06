use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, human_bytes, item, table, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_INSTALLERS: usize = 20;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerItem {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub modified: String,
    pub signature_status: String,
    pub signer: String,
    pub company: String,
    pub product: String,
    pub version: String,
}

/// 根据签名与发布者信息给出离线启发式风险等级；不进行联网信誉查询。
pub fn risk_of(item: &InstallerItem) -> (&'static str, &'static str) {
    let status = item.signature_status.to_lowercase();
    let signer = item.signer.trim();
    if status.contains("notsigned") || status.is_empty() {
        return ("高", "未签名：来源无法验证，安装时务必选择自定义安装");
    }
    if status.contains("hashmismatch") || status.contains("nottrusted") || status.contains("unknown") {
        return ("高", "签名无效或不受信任：建议重新从官网下载");
    }
    if !status.contains("valid") {
        return ("中", "签名状态未知：请核对发布者");
    }
    if signer.is_empty() {
        return ("中", "签名有效但未读到签名者：请核对官网");
    }
    if item.company.trim().is_empty() && item.product.trim().is_empty() {
        return ("中", "签名有效但缺少公司/产品信息：仍可能在安装界面捆绑附加项");
    }
    ("低", "签名有效；捆绑项仍取决于安装界面，请选择自定义安装并拒绝附加组件")
}

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$roots=@("$env:USERPROFILE\Downloads","$env:USERPROFILE\Desktop")
$files=@()
foreach($root in $roots){
  foreach($ext in @('*.exe','*.msi')){
    if(Test-Path $root){ $files += @(Get-ChildItem -Path $root -File -Filter $ext -ErrorAction SilentlyContinue) }
  }
}
$cutoff=(Get-Date).AddDays(-180)
$selected=@($files | Where-Object { $_.LastWriteTime -gt $cutoff } | Sort-Object LastWriteTime -Descending | Select-Object -First 20)
$items=[System.Collections.Generic.List[object]]::new()
foreach($file in $selected){
  try {
    $sig=Get-AuthenticodeSignature -FilePath $file.FullName -ErrorAction SilentlyContinue
    $vi=$file.VersionInfo
    $items.Add(@{
      name=$file.Name; path=$file.FullName; bytes=[uint64]$file.Length;
      modified=$file.LastWriteTime.ToString('yyyy-MM-dd HH:mm');
      signatureStatus=$(if($sig){[string]$sig.Status}else{'Unknown'});
      signer=$(if($sig -and $sig.SignerCertificate){[string]$sig.SignerCertificate.Subject}else{''});
      company=[string]$vi.CompanyName; product=[string]$vi.ProductName; version=[string]$vi.FileVersion
    })
  } catch {}
}
@($items) | ConvertTo-Json -Depth 3 -Compress
"#
}

fn collect() -> Result<Vec<InstallerItem>, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取安装包：{error}")))?;
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
        .filter(|line| line.starts_with('{') || line.starts_with('['))
        .last()
        .ok_or_else(|| ToolError::new("INTERNAL", "安装包查询没有返回 JSON"))?;
    let value: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析安装包：{error}")))?;
    let array = match value {
        Value::Array(items) => items,
        other => vec![other],
    };
    let mut items = array
        .into_iter()
        .filter_map(|item| serde_json::from_value::<InstallerItem>(item).ok())
        .filter(|item| !item.name.trim().is_empty())
        .collect::<Vec<_>>();
    items.truncate(MAX_INSTALLERS);
    Ok(items)
}

fn build_rows(items: &[InstallerItem]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            let (level, hint) = risk_of(item);
            let publisher = if item.signer.is_empty() {
                if item.company.is_empty() {
                    "无签名者/无公司".to_string()
                } else {
                    item.company.clone()
                }
            } else {
                item.signer.clone()
            };
            let company = if item.product.is_empty() {
                item.company.clone()
            } else {
                format!("{} · {}", item.company, item.product)
            };
            json!({
                "file": item.name,
                "version": if item.version.is_empty() { "未知".to_string() } else { item.version.clone() },
                "publisher": publisher,
                "company": company,
                "size": human_bytes(item.bytes),
                "modified": item.modified,
                "risk": level,
                "hint": hint,
                "path": item.path,
            })
        })
        .collect()
}

fn scan() -> Result<Value, ToolError> {
    let items = collect()?;
    let rows = build_rows(&items);
    let unsigned = items
        .iter()
        .filter(|item| risk_of(item).0 == "高")
        .count();
    let medium = items
        .iter()
        .filter(|item| risk_of(item).0 == "中")
        .count();
    let low = items
        .iter()
        .filter(|item| risk_of(item).0 == "低")
        .count();

    Ok(table(
        json!([
            item("近期安装包", items.len().to_string()),
            item("高风险", unsigned.to_string()),
            item("需核对", medium.to_string()),
            item("签名有效", low.to_string()),
        ]),
        json!([
            column("file", "安装包"),
            column("version", "版本"),
            column("publisher", "签名者/发布者"),
            column("size", "大小"),
            column("modified", "修改时间"),
            column("risk", "风险"),
            column("hint", "提示"),
        ]),
        Value::Array(rows),
        json!([
            "只读启发式检查：读取下载/桌面最近 180 天的 exe/msi，比较 Authenticode 签名与版本信息。",
            "不运行安装包、不联网查询云端信誉、不修改文件；签名有效也不能排除安装界面的捆绑项。",
            "安装时请选择“自定义安装”，逐项取消附加软件、工具栏与修改主页等选项。",
            "风险等级仅为离线提示：未签名/签名无效为高，缺少公司信息或签名状态未知为中。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "intercept_bs",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(status: &str, signer: &str, company: &str, product: &str) -> InstallerItem {
        InstallerItem {
            name: "setup.exe".into(),
            path: "C:\\Users\\me\\Downloads\\setup.exe".into(),
            bytes: 1024,
            modified: "2026-01-01 10:00".into(),
            signature_status: status.into(),
            signer: signer.into(),
            company: company.into(),
            product: product.into(),
            version: "1.0.0".into(),
        }
    }

    #[test]
    fn unsigned_is_high_risk() {
        let (level, _) = risk_of(&item("NotSigned", "", "", ""));
        assert_eq!(level, "高");
    }

    #[test]
    fn invalid_signature_is_high_risk() {
        let (level, _) = risk_of(&item("HashMismatch", "CN=Bad", "", ""));
        assert_eq!(level, "高");
    }

    #[test]
    fn valid_signature_with_company_is_low() {
        let (level, hint) = risk_of(&item("Valid", "CN=Example Corp", "Example Corp", "Setup"));
        assert_eq!(level, "低");
        assert!(hint.contains("自定义安装"));
    }

    #[test]
    fn valid_signature_without_company_is_medium() {
        let (level, _) = risk_of(&item("Valid", "CN=Example", "", ""));
        assert_eq!(level, "中");
    }

    #[test]
    fn builds_rows_with_risk_columns() {
        let items = vec![
            item("NotSigned", "", "", ""),
            item("Valid", "CN=Example Corp", "Example Corp", "Installer"),
        ];
        let rows = build_rows(&items);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["risk"], "高");
        assert_eq!(rows[1]["risk"], "低");
        assert!(rows[1]["publisher"].as_str().unwrap().contains("Example Corp"));
    }
}
