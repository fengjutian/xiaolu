//! 清理大师：`clean_core` 清理引擎的表格展示层。
//!
//! 白名单、扫描、占用检测、回收站删除与清单写入全部收敛在 `tools/cleaner`，
//! 与 `cclean` 共用同一份实现 —— 两个工具对同一位置给出的占用/文件数必然一致。

use tool_protocol::ToolError;

const TOOL_ID: &str = "clean_master";
const ACTION: &str = "apply";

fn main() {
    tool_protocol::serve_with_request(
        TOOL_ID,
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION],
        |request| match request.action.as_str() {
            "preview" => clean_core::preview(TOOL_ID, ACTION),
            ACTION => clean_core::apply(TOOL_ID, ACTION, request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::path::Path;

    fn manifest() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tool.json");
        let text = fs::read_to_string(path).expect("read tool.json");
        serde_json::from_str(&text).expect("parse tool.json")
    }

    #[test]
    fn manifest_matches_tool_identity() {
        let manifest = manifest();
        assert_eq!(manifest["id"].as_str(), Some(TOOL_ID));
        assert_eq!(
            manifest["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn manifest_declares_served_actions() {
        let manifest = manifest();
        let actions = manifest["actions"].as_array().expect("actions");
        let preview = actions
            .iter()
            .find(|action| action["id"] == "preview")
            .expect("preview action");
        assert_eq!(preview["write"], false);
        let apply = actions
            .iter()
            .find(|action| action["id"] == ACTION)
            .expect("apply action");
        assert_eq!(apply["write"], true);
        assert_eq!(apply["requiresConfirmation"], true);
        assert_eq!(manifest["risk"], "write");
    }

    #[test]
    fn manifest_declares_rollback_to_recycle_bin() {
        let manifest = manifest();
        assert_eq!(manifest["rollback"], "recycle_bin");
    }
}
