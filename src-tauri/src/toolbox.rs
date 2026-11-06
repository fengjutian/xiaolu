use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolActionDescriptor {
    pub id: String,
    pub label: String,
    pub write: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub risk: String,
    pub requires_admin: bool,
    pub status: String,
    pub note: String,
    pub ui_kind: String,
    pub actions: Vec<ToolActionDescriptor>,
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
        ui_kind: "none".into(),
        actions: Vec::new(),
    }
}

/// 29 项工具的静态注册表。实际可运行的工具由 `toolbox_host` 从 tools/ 目录发现，
/// 并覆盖同 id 的静态条目；这里保留完整清单用于展示“规划中”的工具。
pub fn tools() -> Vec<ToolDescriptor> {
    vec![
        tool("sys_clean", "Windows 系统瘦身", "清理", "移除系统自带的无用组件与预装应用（规划）", "write", true, "planned", "需要逐项确认与回滚设计"),
        tool("cclean", "C 盘瘦身", "清理", "只读统计 C 盘可清理位置与系统托管文件", "read_only", false, "planned", "工具包未安装"),
        tool("clean_master", "清理大师", "清理", "扫描并勾选固定清理位置，文件移入回收站可还原", "write", false, "planned", "工具包未安装"),
        tool("privacy_clean", "隐私清理", "隐私", "浏览记录、Cookie 与临时痕迹清理（规划）", "write", false, "planned", "涉及浏览器数据，需逐项确认"),
        tool("privacy_incognito", "无痕模式", "隐私", "只读扫描浏览器、最近文件、跳转列表、剪贴板与 DNS 痕迹", "read_only", false, "planned", "工具包未安装"),
        tool("privacy_protection", "隐私保护", "隐私", "权限与遥测开关检查（规划）", "read_only", false, "planned", "先做只读检查"),
        tool("defrag", "磁盘碎片整理", "优化", "HDD 碎片整理与 SSD TRIM（规划）", "system", true, "planned", "调用系统 API，需确认磁盘类型"),
        tool("c_expand", "C 盘扩容", "系统", "调整分区容量（规划）", "system", true, "planned", "高风险，需恢复点"),
        tool("dll_repair", "DLL 修复", "修复", "检查运行库与系统 DLL 缺失（规划）", "write", true, "planned", "不下载来源不明 DLL"),
        tool("bsod_repair", "蓝屏修复", "修复", "分析转储并给出修复建议（规划）", "write", true, "planned", "先做只读分析"),
        tool("driver_manager", "驱动管理", "驱动", "驱动检测、更新预检与安装（规划）", "system", true, "partial", "已具备检测与只读预检"),
        tool("gpu_opt", "显卡性能优化", "优化", "显卡电源与游戏配置建议（规划）", "system", true, "planned", "仅建议，不自动改"),
        tool("ai_cooling", "AI 智能降温", "优化", "温度趋势与降频建议（规划）", "read_only", false, "planned", "依赖传感器覆盖"),
        tool("pc_optimizer", "电脑性能优化", "优化", "预览并调整电源计划与逐项电源设置", "system", false, "planned", "工具包未安装"),
        tool("proc_opt", "电脑智能防卡顿", "优化", "高占用进程识别与建议（规划）", "read_only", false, "planned", "不结束进程"),
        tool("memopt_pro", "内存优化 Pro", "优化", "只读诊断内存占用、提交量与进程排行", "read_only", false, "planned", "工具包未安装"),
        tool("sys_boost", "系统优化大师", "优化", "只读汇总启动项、服务、计划任务、休眠与临时文件候选", "read_only", false, "planned", "工具包未安装"),
        tool("winopt_assistant", "Windows 优化助手", "优化", "只读检查视觉特效、存储感知、休眠与诊断数据等设置", "read_only", false, "planned", "工具包未安装"),
        tool("DuplicateFileClean", "重复文件清理", "清理", "重复文件检测与清理（规划）", "read_only", false, "partial", "已具备只读检测"),
        tool("eye_care", "护眼模式", "工具箱", "色温滤镜、定时休息提醒与时间段设置（应用内页面）", "read_only", false, "available", "应用内页面 · 非独立工具进程"),
        tool("multi_wechat", "微信多开", "工具箱", "多实例启动（规划）", "write", false, "planned", "需评估稳定性"),
        tool("omni_zip", "Windows 压缩工具", "工具箱", "压缩、解压与查看 ZIP，含路径与 Zip-Slip 防护", "write", false, "planned", "工具包未安装"),
        tool("start_menu_pro", "开始菜单 Pro", "工具箱", "应用内快捷启动器：搜索并启动开始菜单应用", "read_only", false, "available", "应用内页面 · 非独立工具进程"),
        tool("desktop_quick", "桌面助手", "工具箱", "应用内查看并打开桌面项目，含常用文件夹入口", "read_only", false, "available", "应用内页面 · 非独立工具进程"),
        tool("master_pdf", "PDF 大师", "工具箱", "查看信息、合并、拆分与旋转 PDF", "write", false, "planned", "工具包未安装"),
        tool("lockscreen", "锁屏", "工具箱", "预览后确认锁定当前 Windows 会话", "write", false, "planned", "工具包未安装"),
        tool("adblock_master", "广告拦截", "工具箱", "只读审计 Hosts、DNS、浏览器扩展与防火墙状态", "read_only", false, "planned", "工具包未安装"),
        tool("intercept_bs", "捆绑软件拦截", "工具箱", "安装包捆绑行为提示（规划）", "system", true, "planned", "需安装监控"),
        tool("software_pm", "软件权限管理", "工具箱", "启动项、自动服务、计划任务与通知视图；可启停当前用户启动项", "write", false, "planned", "工具包未安装"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_29_unique_tools() {
        let tools = tools();
        assert_eq!(tools.len(), 29);
        let mut ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 29);
        assert!(tools.iter().all(|tool| matches!(tool.status.as_str(), "available" | "partial" | "planned")));
    }
}
