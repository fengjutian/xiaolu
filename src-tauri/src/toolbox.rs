use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    id: String,
    name: String,
    category: String,
    description: String,
    risk: String,
    requires_admin: bool,
    status: String,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlimItem {
    id: String,
    label: String,
    path: String,
    bytes: u64,
    file_count: usize,
    category: String,
    requires_admin: bool,
    truncated: bool,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlimScan {
    captured_at_unix_ms: u64,
    drive_letter: String,
    drive_total_bytes: u64,
    drive_free_bytes: u64,
    items: Vec<SlimItem>,
    safe_bytes: u64,
    caution_bytes: u64,
    system_bytes: u64,
    file_count: usize,
    note: String,
}

fn tool(
    id: &str,
    name: &str,
    category: &str,
    description: &str,
    risk: &str,
    requires_admin: bool,
    status: &str,
    note: &str,
) -> ToolDescriptor {
    ToolDescriptor {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        description: description.into(),
        risk: risk.into(),
        requires_admin,
        status: status.into(),
        note: note.into(),
    }
}

/// 31 项 SuperApp 工具的注册表。只有 `available` 的项在工具箱里可执行。
pub fn tools() -> Vec<ToolDescriptor> {
    vec![
        tool("sys_clean", "Windows 系统瘦身", "清理", "移除系统自带的无用组件与预装应用（规划）", "write", true, "planned", "需要逐项确认与回滚设计"),
        tool("cclean", "C 盘瘦身", "清理", "只读统计 C 盘可清理位置与系统托管文件", "read_only", false, "available", "当前仅预览，不删除"),
        tool("clean_master", "清理大师", "清理", "聚合系统、应用与浏览器缓存清理（规划）", "write", true, "planned", "写操作需独立授权"),
        tool("privacy_clean", "隐私清理", "隐私", "浏览记录、Cookie 与临时痕迹清理（规划）", "write", false, "planned", "涉及浏览器数据，需逐项确认"),
        tool("privacy_incognito", "无痕模式", "隐私", "运行期间减少本地痕迹（规划）", "write", false, "planned", "需明确边界"),
        tool("privacy_protection", "隐私保护", "隐私", "权限与遥测开关检查（规划）", "read_only", false, "planned", "先做只读检查"),
        tool("defrag", "磁盘碎片整理", "优化", "HDD 碎片整理与 SSD TRIM（规划）", "system", true, "planned", "调用系统 API，需确认磁盘类型"),
        tool("c_expand", "C 盘扩容", "系统", "调整分区容量（规划）", "system", true, "planned", "高风险，需恢复点"),
        tool("dll_repair", "DLL 修复", "修复", "检查运行库与系统 DLL 缺失（规划）", "write", true, "planned", "不下载来源不明 DLL"),
        tool("bsod_repair", "蓝屏修复", "修复", "分析转储并给出修复建议（规划）", "write", true, "planned", "先做只读分析"),
        tool("driver_manager", "驱动管理", "驱动", "驱动检测、更新预检与安装（规划）", "system", true, "partial", "已具备检测与只读预检"),
        tool("gpu_opt", "显卡性能优化", "优化", "显卡电源与游戏配置建议（规划）", "system", true, "planned", "仅建议，不自动改"),
        tool("ai_cooling", "AI 智能降温", "优化", "温度趋势与降频建议（规划）", "read_only", false, "planned", "依赖传感器覆盖"),
        tool("pc_optimizer", "电脑性能优化", "优化", "启动项、服务与电源计划建议（规划）", "write", true, "planned", "只读建议优先"),
        tool("proc_opt", "电脑智能防卡顿", "优化", "高占用进程识别与建议（规划）", "read_only", false, "planned", "不结束进程"),
        tool("memopt_pro", "内存优化 Pro", "优化", "内存占用分析与释放建议（规划）", "write", true, "planned", "释放内存需谨慎"),
        tool("sys_boost", "系统优化大师", "优化", "一键优化候选（规划）", "write", true, "planned", "需回滚与确认"),
        tool("winopt_assistant", "Windows 优化助手", "优化", "系统设置建议（规划）", "write", true, "planned", "逐项确认"),
        tool("DuplicateFileClean", "重复文件清理", "清理", "重复文件检测与清理（规划）", "read_only", false, "partial", "已具备只读检测"),
        tool("eye_care", "护眼大师", "工具箱", "护眼色温与休息提醒（规划）", "read_only", false, "planned", "仅界面层"),
        tool("multi_wechat", "微信多开", "工具箱", "多实例启动（规划）", "write", false, "planned", "需评估稳定性"),
        tool("omni_zip", "Windows 压缩工具", "工具箱", "压缩/解压（规划）", "write", false, "planned", "第三方压缩库需许可审查"),
        tool("start_menu_pro", "开始菜单 Pro", "工具箱", "开始菜单增强（规划）", "system", false, "planned", "常驻组件，需评估"),
        tool("desktop_quick", "桌面侧边栏", "工具箱", "桌面文件整理与快捷访问（规划）", "write", false, "planned", "常驻组件，需评估"),
        tool("master_pdf", "PDF 大师", "工具箱", "PDF 合并/拆分（规划）", "write", false, "planned", "依赖 PDF 库"),
        tool("lockscreen", "锁屏", "工具箱", "快捷锁屏（规划）", "read_only", false, "planned", "调用系统锁屏"),
        tool("adblock_master", "广告拦截", "工具箱", "广告与弹窗拦截（规划）", "system", true, "planned", "需网络过滤组件"),
        tool("intercept_bs", "捆绑软件拦截", "工具箱", "安装包捆绑行为提示（规划）", "system", true, "planned", "需安装监控"),
        tool("push_accelerator", "推送加速", "工具箱", "推送通道优化（规划）", "system", true, "planned", "第三方组件，暂不实现"),
        tool("push_open_claw", "OpenClaw 推送组件", "工具箱", "第三方推送组件（规划）", "system", true, "planned", "仅登记，不集成"),
        tool("software_pm", "软件权限管理", "工具箱", "软件权限与启动行为查看（规划）", "read_only", false, "planned", "先做只读"),
    ]
}

fn category_for(id: &str) -> &'static str {
    match id {
        "user-temp" | "thumbnail-cache" | "edge-cache" | "inet-cache" | "crash-dumps"
        | "error-reporting" => "safe",
        "windows-temp" | "windows-update" | "recycle-bin" => "caution",
        _ => "caution",
    }
}

fn system_file(path: &str, label: &str, note: &str) -> Option<SlimItem> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(SlimItem {
        id: format!("system-{}", label),
        label: label.into(),
        path: path.into(),
        bytes: metadata.len(),
        file_count: 1,
        category: "system".into(),
        requires_admin: false,
        truncated: false,
        note: note.into(),
    })
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn scan_c_drive() -> SlimScan {
    let preview = crate::cleanup_preview::build_preview(None);
    let mut items: Vec<SlimItem> = preview
        .candidates
        .into_iter()
        .filter(|candidate| candidate.exists && candidate.bytes > 0)
        .map(|candidate| SlimItem {
            category: category_for(&candidate.id).into(),
            id: candidate.id,
            label: candidate.label,
            path: candidate.path,
            bytes: candidate.bytes,
            file_count: candidate.file_count,
            requires_admin: candidate.requires_admin,
            truncated: candidate.truncated,
            note: candidate.note,
        })
        .collect();

    items.extend(system_file(
        r"C:\pagefile.sys",
        "页面文件 pagefile.sys",
        "由系统管理；可在“高级系统设置”中调整大小或移动到其他盘。",
    ));
    items.extend(system_file(
        r"C:\hiberfil.sys",
        "休眠文件 hiberfil.sys",
        "由系统管理；可在电源设置中关闭休眠以释放空间。",
    ));
    items.extend(system_file(
        r"C:\swapfile.sys",
        "交换文件 swapfile.sys",
        "由系统管理，不建议直接删除。",
    ));

    items.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    let safe_bytes = items.iter().filter(|item| item.category == "safe").map(|item| item.bytes).sum();
    let caution_bytes = items.iter().filter(|item| item.category == "caution").map(|item| item.bytes).sum();
    let system_bytes = items.iter().filter(|item| item.category == "system").map(|item| item.bytes).sum();
    let file_count: usize = items.iter().map(|item| item.file_count).sum();

    let mut drive_total_bytes = 0u64;
    let mut drive_free_bytes = 0u64;
    if let Ok(disks) = sysinfo::Disks::new_with_refreshed_list().map_err(|error| error.to_string()) {
        if let Some(disk) = disks.iter().find(|disk| {
            disk.mount_point()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("c:")
        }) {
            drive_total_bytes = disk.total_space();
            drive_free_bytes = disk.available_space();
        }
    }

    SlimScan {
        captured_at_unix_ms: now_unix_ms(),
        drive_letter: "C:".into(),
        drive_total_bytes,
        drive_free_bytes,
        items,
        safe_bytes,
        caution_bytes,
        system_bytes,
        file_count,
        note: "只读预览：只统计占用，不删除任何文件；删除能力将在后续版本按项确认后提供。".into(),
    }
}

#[tauri::command]
pub async fn list_toolbox_tools() -> Result<Vec<ToolDescriptor>, String> {
    Ok(tools())
}

#[tauri::command]
pub async fn scan_c_drive_slim() -> Result<SlimScan, String> {
    tauri::async_runtime::spawn_blocking(scan_c_drive)
        .await
        .map_err(|error| format!("C 盘瘦身扫描任务失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_31_unique_tools() {
        let tools = tools();
        assert_eq!(tools.len(), 31);
        let mut ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 31);
        assert!(tools.iter().any(|tool| tool.id == "cclean" && tool.status == "available"));
        assert!(tools.iter().all(|tool| matches!(tool.status.as_str(), "available" | "partial" | "planned")));
    }

    #[test]
    fn classifies_cleanup_candidates() {
        assert_eq!(category_for("user-temp"), "safe");
        assert_eq!(category_for("windows-update"), "caution");
        assert_eq!(category_for("unknown-id"), "caution");
    }
}
