//! Redaction applied to the payload before it leaves the machine.
//!
//! The fact builder already avoids sensitive values, but redaction runs again
//! on the serialized payload as a defence-in-depth measure. It removes
//! sensitive keys recursively and masks Windows user-profile paths.

use super::model::RedactionSummary;
use serde_json::Value;

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = normalize_key(key);
    matches!(
        normalized.as_str(),
        "hostname"
            | "computername"
            | "username"
            | "user"
            | "serial"
            | "serialnumber"
            | "servicetag"
            | "macaddress"
            | "mac"
            | "ipaddress"
            | "ip"
            | "localpath"
            | "fullpath"
            | "filepath"
            | "folderpath"
            | "processcommandline"
            | "commandline"
            | "machineid"
            | "hardwareid"
            | "deviceid"
            | "sid"
            | "apikey"
            | "authorization"
            | "password"
            | "credential"
            | "credentials"
            | "token"
    ) || normalized.ends_with("fullpath")
        || normalized.ends_with("localpath")
}

fn has_drive_path(input: &str) -> bool {
    let characters: Vec<char> = input.chars().collect();
    for index in 0..characters.len().saturating_sub(2) {
        if !characters[index].is_ascii_alphabetic() {
            continue;
        }
        if characters[index + 1] != ':' {
            continue;
        }
        if characters[index + 2] != '\\' && characters[index + 2] != '/' {
            continue;
        }
        let previous_is_word = index > 0
            && (characters[index - 1].is_alphanumeric() || characters[index - 1] == '_');
        if !previous_is_word {
            return true;
        }
    }
    false
}

fn scrub_string(input: &str) -> Option<String> {
    let lowered = input.to_ascii_lowercase();
    let looks_like_profile_path = lowered.contains(":\\users\\")
        || lowered.contains(":\\documents and settings\\")
        || lowered.contains(":/users/");
    let looks_like_unc_path = input.trim_start().starts_with("\\\\");
    if looks_like_profile_path || looks_like_unc_path || has_drive_path(input) {
        return Some("[本地路径已脱敏]".to_string());
    }
    None
}

fn redact_value(value: &mut Value, removed: &mut usize, rules: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let mut keys_to_remove = Vec::new();
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    keys_to_remove.push(key.clone());
                    continue;
                }
                redact_value(child, removed, rules);
            }
            for key in keys_to_remove {
                map.remove(&key);
                *removed += 1;
                let rule = format!("移除字段：{}", normalize_key(&key));
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                redact_value(item, removed, rules);
            }
        }
        Value::String(text) => {
            if let Some(masked) = scrub_string(text) {
                *text = masked;
                *removed += 1;
                let rule = "遮罩 Windows 用户路径".to_string();
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
        }
        _ => {}
    }
}

/// Returns the redacted payload and a summary that can be shown to the user.
pub fn redact_payload(payload: &Value) -> (Value, RedactionSummary) {
    let mut redacted = payload.clone();
    let mut removed = 0;
    let mut rules = Vec::new();
    redact_value(&mut redacted, &mut removed, &mut rules);
    if rules.is_empty() {
        rules.push("按字段策略检查，未发现需要移除的敏感字段".to_string());
    }
    (
        redacted,
        RedactionSummary {
            removed_fields: removed,
            applied_rules: rules,
            excluded_facts: 0,
        },
    )
}

/// Drops facts that are marked sensitive before the payload is built.
pub fn filter_facts_for_model<'a>(
    facts: impl Iterator<Item = &'a super::model::DiagnosticFact>,
) -> (Vec<super::model::DiagnosticFact>, usize) {
    let mut kept = Vec::new();
    let mut excluded = 0;
    for fact in facts {
        if fact.sensitive {
            excluded += 1;
            continue;
        }
        kept.push(fact.clone());
    }
    (kept, excluded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_sensitive_keys_recursively() {
        let payload = json!({
            "snapshotId": "snap-1",
            "hostName": "DESKTOP-ABC",
            "nested": {
                "serialNumber": "SN123",
                "items": [{ "commandLine": "secret.exe /token x" }, { "safe": "ok" }]
            },
            "count": 3
        });
        let (redacted, summary) = redact_payload(&payload);
        assert!(redacted.get("hostName").is_none());
        assert!(redacted["nested"].get("serialNumber").is_none());
        assert!(redacted["nested"]["items"][0].get("commandLine").is_none());
        assert_eq!(redacted["nested"]["items"][1]["safe"], "ok");
        assert_eq!(redacted["count"], 3);
        assert_eq!(summary.removed_fields, 3);
    }

    #[test]
    fn masks_user_profile_paths() {
        let payload = json!({ "note": "C:\\Users\\alice\\Downloads\\dump.dmp" });
        let (redacted, summary) = redact_payload(&payload);
        assert_eq!(redacted["note"], "[本地路径已脱敏]");
        assert_eq!(summary.removed_fields, 1);
    }

    #[test]
    fn masks_arbitrary_drive_paths_but_not_urls() {
        let payload = json!({
            "note": "读取 D:\\github\\xiaolu\\src-tauri 失败",
            "url": "https://api.deepseek.com/v1",
            "drive": "C:",
        });
        let (redacted, _) = redact_payload(&payload);
        assert_eq!(redacted["note"], "[本地路径已脱敏]");
        assert_eq!(redacted["url"], "https://api.deepseek.com/v1");
        assert_eq!(redacted["drive"], "C:");
    }

    #[test]
    fn keeps_non_sensitive_tokens_like_keys() {
        let payload = json!({ "maxOutputTokens": 1200, "totalTokens": 42 });
        let (redacted, summary) = redact_payload(&payload);
        assert_eq!(redacted["maxOutputTokens"], 1200);
        assert_eq!(redacted["totalTokens"], 42);
        assert_eq!(summary.removed_fields, 0);
    }
}
