use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tool_protocol::{column, item, table, human_bytes, ToolError};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn script() -> &'static str {
    r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
function DirStat($path){
  if(-not $path -or -not (Test-Path $path)){ return @{exists=$false;bytes=0;files=0} }
  $bytes=0L; $files=0
  try {
    Get-ChildItem -Path $path -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
      $files++; $bytes += $_.Length
      if($files -ge 20000){ return }
    }
  } catch {}
  return @{exists=$true;bytes=$bytes;files=$files}
}
$local=$env:LOCALAPPDATA; $app=$env:APPDATA
$edge=DirStat (Join-Path $local 'Microsoft\Edge\User Data\Default')
$chrome=DirStat (Join-Path $local 'Google\Chrome\User Data\Default')
$firefoxRoot=Join-Path $app 'Mozilla\Firefox\Profiles'
$firefox=@{exists=$false;bytes=0L;files=0}
if(Test-Path $firefoxRoot){
  $firefox=@{exists=$true;bytes=0L;files=0}
  Get-ChildItem $firefoxRoot -Directory -ErrorAction SilentlyContinue | ForEach-Object {
    $stat=DirStat $_.FullName
    $firefox.bytes += $stat.bytes; $firefox.files += $stat.files
  }
}
$recent=DirStat (Join-Path $app 'Microsoft\Windows\Recent')
$jumpRoot=Join-Path $app 'Microsoft\Windows\Recent\AutomaticDestinations'
$jump=DirStat $jumpRoot

$clipboardType='未知'; $clipboardLength=0
try {
  Add-Type -AssemblyName System.Windows.Forms
  if([System.Windows.Forms.Clipboard]::ContainsText()){
    $clipboardType='文本'
    $clipboardLength=([System.Windows.Forms.Clipboard]::GetText()).Length
  } elseif([System.Windows.Forms.Clipboard]::ContainsImage()){ $clipboardType='图片' }
  elseif([System.Windows.Forms.Clipboard]::ContainsFileDropList()){ $clipboardType='文件列表' }
  else { $clipboardType='空或不支持' }
} catch { $clipboardType='不可读取' }

$dns=0
try { $dns=@(Get-DnsClientCache -ErrorAction Stop).Count } catch {
  try { $dns=@(ipconfig /displaydns | Select-String -Pattern 'Record Name|记录名称' -ErrorAction SilentlyContinue).Count } catch {}
}

@{edge=$edge;chrome=$chrome;firefox=$firefox;recent=$recent;jump=$jump;clipboardType=$clipboardType;clipboardLength=$clipboardLength;dnsEntries=$dns} | ConvertTo-Json -Depth 4 -Compress
"#
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn exists(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn build_rows(payload: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, label) in [
        ("edge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
    ] {
        let group = payload.get(key).cloned().unwrap_or(Value::Null);
        let present = exists(&group, "exists");
        let bytes = number(&group, "bytes");
        let files = number(&group, "files");
        rows.push(json!({
            "category": "浏览器数据",
            "item": label,
            "value": if present { human_bytes(bytes) } else { "未安装/无数据".to_string() },
            "count": files,
            "note": if present { "配置文件占用（历史、缓存、Cookie 等）" } else { "未发现默认配置目录" },
        }));
    }

    let recent = payload.get("recent").cloned().unwrap_or(Value::Null);
    rows.push(json!({
        "category": "最近文件",
        "item": "Recent 目录",
        "value": if exists(&recent, "exists") { human_bytes(number(&recent, "bytes")) } else { "不存在".to_string() },
        "count": number(&recent, "files"),
        "note": "打开过的文档与快捷方式记录",
    }));

    let jump = payload.get("jump").cloned().unwrap_or(Value::Null);
    rows.push(json!({
        "category": "跳转列表",
        "item": "AutomaticDestinations",
        "value": if exists(&jump, "exists") { human_bytes(number(&jump, "bytes")) } else { "不存在".to_string() },
        "count": number(&jump, "files"),
        "note": "任务栏最近项目记录",
    }));

    rows.push(json!({
        "category": "剪贴板",
        "item": "当前内容类型",
        "value": text(payload, "clipboardType"),
        "count": number(payload, "clipboardLength"),
        "note": "只读取类型与文本长度，不读取、不记录具体内容",
    }));

    rows.push(json!({
        "category": "DNS 缓存",
        "item": "本地解析记录",
        "value": format!("{} 条", number(payload, "dnsEntries")),
        "count": number(payload, "dnsEntries"),
        "note": "最近访问的域名解析缓存",
    }));

    rows
}

fn scan() -> Result<Value, ToolError> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", script()])
        .output()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法读取痕迹数据：{error}")))?;
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
        .filter(|line| line.starts_with('{'))
        .last()
        .ok_or_else(|| ToolError::new("INTERNAL", "痕迹扫描没有返回 JSON"))?;
    let payload: Value = serde_json::from_str(json_line)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法解析痕迹数据：{error}")))?;
    let rows = build_rows(&payload);
    let browser_bytes = ["edge", "chrome", "firefox"]
        .iter()
        .map(|key| number(&payload.get(key).cloned().unwrap_or(Value::Null), "bytes"))
        .sum::<u64>();
    let recent_bytes = number(
        &payload.get("recent").cloned().unwrap_or(Value::Null),
        "bytes",
    );
    let jump_bytes = number(
        &payload.get("jump").cloned().unwrap_or(Value::Null),
        "bytes",
    );

    Ok(table(
        json!([
            item("检查类别", "5"),
            item("浏览器数据", human_bytes(browser_bytes)),
            item("最近文件", human_bytes(recent_bytes + jump_bytes)),
            item("DNS 缓存", number(&payload, "dnsEntries").to_string()),
        ]),
        json!([
            column("category", "类别"),
            column("item", "项目"),
            column("value", "占用/状态"),
            column("count", "数量"),
            column("note", "说明"),
        ]),
        Value::Array(rows),
        json!([
            "只读扫描：只统计痕迹文件与状态，不删除、不清空、不上传任何内容。",
            "剪贴板只读取类型与文本长度，不读取、不记录具体内容；文件列表/图片只报告类型。",
            "痕迹边界目前限定为浏览器配置、Recent、跳转列表、剪贴板与 DNS 缓存。",
            "清除痕迹属于不可回滚写操作，需后续单独设计确认与恢复方案，本工具暂不提供。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve(
        "privacy_incognito",
        env!("CARGO_PKG_VERSION"),
        &["preview"],
        |_| scan(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        json!({
            "edge": {"exists": true, "bytes": 2048, "files": 12},
            "chrome": {"exists": false, "bytes": 0, "files": 0},
            "firefox": {"exists": false, "bytes": 0, "files": 0},
            "recent": {"exists": true, "bytes": 512, "files": 3},
            "jump": {"exists": true, "bytes": 256, "files": 2},
            "clipboardType": "文本",
            "clipboardLength": 42,
            "dnsEntries": 17
        })
    }

    #[test]
    fn builds_all_categories() {
        let rows = build_rows(&fixture());
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().any(|row| row["category"] == "浏览器数据"
            && row["item"] == "Edge"
            && row["value"] == "2.0 KB"));
        assert!(rows.iter().any(|row| row["item"] == "Chrome"
            && row["value"] == "未安装/无数据"));
        assert!(rows.iter().any(|row| row["category"] == "剪贴板"
            && row["value"] == "文本"
            && row["count"] == 42));
        assert!(rows.iter().any(|row| row["category"] == "DNS 缓存"
            && row["value"] == "17 条"));
    }

    #[test]
    fn tolerates_missing_payload_fields() {
        let rows = build_rows(&json!({}));
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().any(|row| row["item"] == "Edge"
            && row["value"] == "未安装/无数据"));
        assert!(rows.iter().any(|row| row["category"] == "DNS 缓存"
            && row["value"] == "0 条"));
    }
}
