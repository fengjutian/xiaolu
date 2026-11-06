//! Preview-only repair planning (AI-4 first step).
//!
//! The model may suggest an existing controlled write tool, but this module
//! never executes it. It only builds a host-side preview: risk, rollback,
//! evidence and (for cleanup) read-only scanned targets. Execution still
//! requires the toolbox's own preview, secondary confirmation and token.

use super::model::{AiRepairPreview, AiRepairTarget, DiagnosticSnapshot};
use std::collections::HashSet;

pub struct RepairToolSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub risk: &'static str,
    pub reversible: bool,
    pub requires_admin: bool,
    pub summary: &'static str,
    pub rollback: &'static str,
}

pub const REPAIR_TOOLS: &[RepairToolSpec] = &[
    RepairToolSpec {
        id: "clean_master",
        label: "清理大师",
        risk: "medium",
        reversible: true,
        requires_admin: false,
        summary: "扫描固定白名单清理位置并生成逐文件清单；执行时文件移入回收站，可从回收站还原。",
        rollback: "执行后可从 Windows 回收站手动还原；本预览不移动或删除任何文件。",
    },
    RepairToolSpec {
        id: "pc_optimizer",
        label: "电脑性能优化",
        risk: "medium",
        reversible: true,
        requires_admin: false,
        summary: "预览电源计划与逐项电源设置的原值；执行时逐项写入并记录原值，可撤销本次修改。",
        rollback: "工具箱结果页提供一次性“撤销本次修改”令牌，原值写入本地操作日志。",
    },
    RepairToolSpec {
        id: "software_pm",
        label: "软件权限管理",
        risk: "medium",
        reversible: true,
        requires_admin: true,
        summary: "预览启动项、服务、计划任务与通知状态；当前用户启动项可逐项/批量启停。",
        rollback: "当前用户启动项支持恢复；系统级启动项和服务需 UAC + 独立 helper，并记录操作日志。",
    },
    RepairToolSpec {
        id: "winopt_assistant",
        label: "Windows 优化助手",
        risk: "low",
        reversible: true,
        requires_admin: true,
        summary: "预览视觉效果、透明效果、动画、存储感知、游戏模式等设置的应用前后状态。",
        rollback: "每项保存原值并支持单独还原；需要管理员的设置必须经 UAC helper 执行。",
    },
    RepairToolSpec {
        id: "driver_manager",
        label: "驱动管理",
        risk: "high",
        reversible: false,
        requires_admin: true,
        summary: "当前仅支持设备驱动检测与 Windows Update 官方候选预检；安装/回滚链路尚未实现。",
        rollback: "安装/回滚尚未实现，本预览只展示检测建议，不提供任何驱动安装动作。",
    },
];

pub fn find(tool_id: &str) -> Option<&'static RepairToolSpec> {
    REPAIR_TOOLS
        .iter()
        .find(|spec| spec.id.eq_ignore_ascii_case(tool_id))
}

pub fn catalog_prompt() -> String {
    REPAIR_TOOLS
        .iter()
        .map(|spec| format!("- {}（{}，风险 {}）：{}", spec.id, spec.label, spec.risk, spec.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

fn filter_evidence(snapshot: &DiagnosticSnapshot, evidence_refs: &[String]) -> Vec<String> {
    let available: HashSet<&str> = snapshot
        .facts
        .iter()
        .filter(|fact| fact.is_available() && !fact.sensitive)
        .map(|fact| fact.id.as_str())
        .collect();
    let mut refs = Vec::new();
    for reference in evidence_refs {
        if available.contains(reference.as_str()) && !refs.contains(reference) {
            refs.push(reference.clone());
        }
    }
    refs
}

/// Builds a preview only. There is deliberately no execute function in this
/// module and no confirmation-token issuance.
pub async fn preview(
    tool_id: &str,
    snapshot: &DiagnosticSnapshot,
    evidence_refs: &[String],
) -> Result<AiRepairPreview, String> {
    let spec = find(tool_id).ok_or_else(|| "该修复工具不在受控白名单中。".to_string())?;
    let evidence_refs = filter_evidence(snapshot, evidence_refs);
    if evidence_refs.is_empty() {
        return Err("修复预览至少需要一个当前快照中的可用证据引用。".to_string());
    }

    let mut notes = vec![
        "AI 只生成预览，不执行任何写操作。".to_string(),
        "实际执行必须在工具箱中重新预览，并经过二次确认和确认令牌。".to_string(),
    ];
    let mut targets = Vec::new();
    let mut summary = spec.summary.to_string();
    let mut status = "manual_only";

    if spec.id == "clean_master" {
        let scan = crate::cleanup_preview::scan_cleanup_candidates().await?;
        let mut candidate_count = 0usize;
        for candidate in scan.candidates.iter().filter(|candidate| candidate.exists && candidate.bytes > 0) {
            candidate_count += 1;
            if targets.len() < 10 {
                targets.push(AiRepairTarget {
                    label: candidate.label.clone(),
                    detail: candidate.id.clone(),
                    bytes: Some(candidate.bytes),
                    file_count: Some(candidate.file_count as u64),
                    requires_admin: candidate.requires_admin,
                });
            }
        }
        summary = format!(
            "固定白名单清理项：{} 项，约 {} 字节，{} 个文件。预览只读取元数据，未移动或删除任何文件。",
            candidate_count, scan.total_bytes, scan.total_files
        );
        status = "host_preview";
        if scan.candidates.iter().any(|candidate| candidate.requires_admin) {
            notes.push("部分目录需要管理员权限；AI 预览以当前权限为准。".to_string());
        }
    }

    if spec.requires_admin {
        notes.push("该工具的部分操作需要管理员权限，AI 不会自动提权。".to_string());
    }

    Ok(AiRepairPreview {
        tool_id: spec.id.to_string(),
        label: spec.label.to_string(),
        risk: spec.risk.to_string(),
        reversible: spec.reversible,
        requires_admin: spec.requires_admin,
        status: status.to_string(),
        summary,
        targets,
        rollback: spec.rollback.to_string(),
        notes,
        evidence_refs,
        generated_at_unix_ms: super::now_unix_ms(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_tools_are_unique_and_whitelisted() {
        let mut ids = HashSet::new();
        for spec in REPAIR_TOOLS {
            assert!(ids.insert(spec.id));
            assert!(spec.risk == "low" || spec.risk == "medium" || spec.risk == "high");
            assert!(!spec.rollback.is_empty());
        }
        assert!(find("clean_master").is_some());
        assert!(find("shell.exec").is_none());
    }

    #[test]
    fn catalog_lists_every_repair_tool() {
        let catalog = catalog_prompt();
        for spec in REPAIR_TOOLS {
            assert!(catalog.contains(spec.id));
        }
    }
}
