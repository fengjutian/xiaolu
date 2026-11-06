use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::c_void;
use std::path::PathBuf;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};

const ACTION: &str = "apply";
const ACTION_SETTINGS: &str = "apply_settings";
const ACCESS_SCHEME: u32 = 16;

#[derive(Clone, Copy)]
struct PowerSetting {
    id: &'static str,
    label: &'static str,
    subgroup: &'static str,
    setting: &'static str,
    unit: &'static str,
    min: u32,
    max: u32,
    /// 注册表/API 原始值到展示单位的换算倍数（分钟 → 秒为 60）。
    scale: u32,
    note: &'static str,
}

const SETTINGS: [PowerSetting; 5] = [
    PowerSetting {
        id: "processor-min",
        label: "处理器最小状态",
        subgroup: "54533251-82be-4824-96c1-47b60b740d00",
        setting: "893dee8e-2bef-41e0-89c6-b55d0929964c",
        unit: "%",
        min: 0,
        max: 100,
        scale: 1,
        note: "降低可省电，过低可能影响响应速度。",
    },
    PowerSetting {
        id: "processor-max",
        label: "处理器最大状态",
        subgroup: "54533251-82be-4824-96c1-47b60b740d00",
        setting: "bc5038f7-23e0-4960-96da-33abaf5935ec",
        unit: "%",
        min: 0,
        max: 100,
        scale: 1,
        note: "限制上限可降温，但会损失性能。",
    },
    PowerSetting {
        id: "display-off",
        label: "关闭显示器",
        subgroup: "7516b95f-f776-4464-8c53-06167f40cc99",
        setting: "3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e",
        unit: "分钟",
        min: 0,
        max: 300,
        scale: 60,
        note: "0 表示永不关闭；Windows 最长支持 5 小时。",
    },
    PowerSetting {
        id: "sleep",
        label: "睡眠",
        subgroup: "238c9fa8-0aad-41ed-83f4-97be242c8f20",
        setting: "29f6c1db-86da-48c5-9fdb-f2b67b1f44da",
        unit: "分钟",
        min: 0,
        max: 1440,
        scale: 60,
        note: "0 表示永不睡眠。",
    },
    PowerSetting {
        id: "disk-off",
        label: "关闭硬盘",
        subgroup: "0012ee47-9041-4b5d-9b77-535fba8b1442",
        setting: "6738e2c4-e8a5-4a42-b16a-e040e769756e",
        unit: "分钟",
        min: 0,
        max: 1440,
        scale: 60,
        note: "SSD 通常无需调整。",
    },
];

fn setting_by_id(id: &str) -> Option<&'static PowerSetting> {
    SETTINGS.iter().find(|setting| setting.id == id)
}

fn setting_guids(setting: &PowerSetting) -> Option<(Guid, Guid)> {
    Some((Guid::parse(setting.subgroup)?, Guid::parse(setting.setting)?))
}

fn validate_value(setting: &PowerSetting, value: u64) -> Result<u32, ToolError> {
    let value = u32::try_from(value).map_err(|_| {
        ToolError::new(
            "INVALID_INPUT",
            format!("{}的取值超出范围。", setting.label),
        )
    })?;
    if value < setting.min || value > setting.max {
        return Err(ToolError::new(
            "INVALID_INPUT",
            format!(
                "{}必须在 {}–{} {} 之间。",
                setting.label, setting.min, setting.max, setting.unit
            ),
        ));
    }
    Ok(value)
}

fn raw_value(setting: &PowerSetting, display: u32) -> u32 {
    display.saturating_mul(setting.scale)
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

impl Guid {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        Some(Guid {
            data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            data2: u16::from_le_bytes([bytes[4], bytes[5]]),
            data3: u16::from_le_bytes([bytes[6], bytes[7]]),
            data4: [
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15],
            ],
        })
    }

    fn to_text(&self) -> String {
        format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7]
        )
    }

    fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_matches(|character| character == '{' || character == '}');
        let parts = text.split('-').collect::<Vec<_>>();
        if parts.len() != 5
            || parts[0].len() != 8
            || parts[1].len() != 4
            || parts[2].len() != 4
            || parts[3].len() != 4
            || parts[4].len() != 12
        {
            return None;
        }
        let data1 = u32::from_str_radix(parts[0], 16).ok()?;
        let data2 = u16::from_str_radix(parts[1], 16).ok()?;
        let data3 = u16::from_str_radix(parts[2], 16).ok()?;
        let mut data4 = [0u8; 8];
        data4[0] = u8::from_str_radix(&parts[3][0..2], 16).ok()?;
        data4[1] = u8::from_str_radix(&parts[3][2..4], 16).ok()?;
        for index in 0..6 {
            data4[2 + index] =
                u8::from_str_radix(&parts[4][index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(Guid {
            data1,
            data2,
            data3,
            data4,
        })
    }
}

#[repr(C)]
#[derive(Default)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[link(name = "powrprof")]
extern "system" {
    fn PowerGetActiveScheme(root: *mut c_void, guid: *mut *mut Guid) -> u32;
    fn PowerSetActiveScheme(root: *mut c_void, guid: *const Guid) -> u32;
    fn PowerEnumerate(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        access: u32,
        index: u32,
        buffer: *mut u8,
        size: *mut u32,
    ) -> u32;
    fn PowerReadFriendlyName(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        setting: *const Guid,
        buffer: *mut u8,
        size: *mut u32,
    ) -> u32;
    fn PowerReadACValueIndex(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        setting: *const Guid,
        value: *mut u32,
    ) -> u32;
    fn PowerReadDCValueIndex(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        setting: *const Guid,
        value: *mut u32,
    ) -> u32;
    fn PowerWriteACValueIndex(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        setting: *const Guid,
        value: u32,
    ) -> u32;
    fn PowerWriteDCValueIndex(
        root: *mut c_void,
        scheme: *const Guid,
        subgroup: *const Guid,
        setting: *const Guid,
        value: u32,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(pointer: *mut c_void) -> *mut c_void;
    fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
}

fn active_plan() -> Option<Guid> {
    let mut pointer: *mut Guid = std::ptr::null_mut();
    // SAFETY: 传入空根键由 API 使用当前用户；成功后需要 LocalFree 释放返回的 GUID。
    let rc = unsafe { PowerGetActiveScheme(std::ptr::null_mut(), &mut pointer) };
    if rc != 0 || pointer.is_null() {
        return None;
    }
    let guid = unsafe { *pointer };
    unsafe {
        LocalFree(pointer as *mut c_void);
    }
    Some(guid)
}

fn friendly_name(guid: &Guid) -> Option<String> {
    let mut buffer = [0u8; 2048];
    let mut size = buffer.len() as u32;
    // SAFETY: buffer 与 size 成对传入，size 为字节容量。
    let rc = unsafe {
        PowerReadFriendlyName(
            std::ptr::null_mut(),
            guid,
            std::ptr::null(),
            std::ptr::null(),
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if rc != 0 || size == 0 {
        return None;
    }
    let length = (size as usize).min(buffer.len());
    let mut units = Vec::new();
    for chunk in buffer[..length].chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let text = String::from_utf16(&units).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn enumerate_plans() -> Vec<(Guid, String)> {
    let mut plans = Vec::new();
    for index in 0..64u32 {
        let mut buffer = [0u8; 16];
        let mut size = buffer.len() as u32;
        // SAFETY: 缓冲区固定为 GUID 大小，索引越界会返回非 0。
        let rc = unsafe {
            PowerEnumerate(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                ACCESS_SCHEME,
                index,
                buffer.as_mut_ptr(),
                &mut size,
            )
        };
        if rc != 0 {
            break;
        }
        if let Some(guid) = Guid::from_bytes(&buffer) {
            let name = friendly_name(&guid).unwrap_or_else(|| guid.to_text());
            plans.push((guid, name));
        }
    }
    plans
}

fn power_source() -> &'static str {
    let mut status = SystemPowerStatus::default();
    // SAFETY: 结构体布局与 SYSTEM_POWER_STATUS 一致。
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return "未知";
    }
    match status.ac_line_status {
        1 => "交流电",
        0 => "电池",
        _ => "未知",
    }
}

fn read_setting(plan: &Guid, setting: &PowerSetting, ac: bool) -> Option<u32> {
    let (subgroup, setting_guid) = setting_guids(setting)?;
    let mut value: u32 = 0;
    // SAFETY: GUID 均来自固定常量，value 为可写输出参数。
    let rc = unsafe {
        if ac {
            PowerReadACValueIndex(
                std::ptr::null_mut(),
                plan,
                &subgroup,
                &setting_guid,
                &mut value,
            )
        } else {
            PowerReadDCValueIndex(
                std::ptr::null_mut(),
                plan,
                &subgroup,
                &setting_guid,
                &mut value,
            )
        }
    };
    (rc == 0).then_some(value)
}

fn write_setting(plan: &Guid, setting: &PowerSetting, ac: bool, raw: u32) -> u32 {
    let Some((subgroup, setting_guid)) = setting_guids(setting) else {
        return 87; // ERROR_INVALID_PARAMETER
    };
    // SAFETY: GUID 均来自固定常量，raw 已按范围校验。
    unsafe {
        if ac {
            PowerWriteACValueIndex(
                std::ptr::null_mut(),
                plan,
                &subgroup,
                &setting_guid,
                raw,
            )
        } else {
            PowerWriteDCValueIndex(
                std::ptr::null_mut(),
                plan,
                &subgroup,
                &setting_guid,
                raw,
            )
        }
    }
}

fn display_value(setting: &PowerSetting, raw: Option<u32>) -> Option<u32> {
    raw.map(|value| value / setting.scale)
}

fn format_value(setting: &PowerSetting, value: Option<u32>) -> String {
    match value {
        Some(value) => format!("{value} {}", setting.unit),
        None => "未知".into(),
    }
}

/// 读取当前计划的全部设置项，供预览和“应用后”结果复用。
fn collect_settings(plan: &Guid) -> Vec<Value> {
    SETTINGS
        .iter()
        .map(|setting| {
            json!({
                "id": setting.id,
                "label": setting.label,
                "unit": setting.unit,
                "min": setting.min,
                "max": setting.max,
                "ac": display_value(setting, read_setting(plan, setting, true)),
                "dc": display_value(setting, read_setting(plan, setting, false)),
                "note": setting.note,
            })
        })
        .collect()
}

fn snapshot_dir() -> PathBuf {
    tool_protocol::state_dir().join("pc_optimizer")
}

/// 写入本次修改的原值、回读值与结果，供事后核对。
fn write_snapshot(payload: &Value) -> Result<PathBuf, String> {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建快照目录：{error}"))?;
    let path = dir.join(format!(
        "run-{}-{}.json",
        tool_protocol::now_ms(),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("无法序列化快照：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("无法写入快照：{error}"))?;
    Ok(path)
}

/// 单项结果：只有写入成功且回读一致才算“已应用”。
fn item_status(
    rc_ac: u32,
    rc_dc: u32,
    after_ac: Option<u32>,
    after_dc: Option<u32>,
    target_ac: u32,
    target_dc: u32,
) -> &'static str {
    let ac_ok = rc_ac == 0 && after_ac == Some(target_ac);
    let dc_ok = rc_dc == 0 && after_dc == Some(target_dc);
    match (ac_ok, dc_ok) {
        (true, true) => "已应用",
        (true, false) | (false, true) => "部分应用",
        (false, false) => "失败",
    }
}

fn item_reason(
    setting: &PowerSetting,
    rc_ac: u32,
    rc_dc: u32,
    after_ac: Option<u32>,
    after_dc: Option<u32>,
    target_ac: u32,
    target_dc: u32,
) -> String {
    let mut parts = Vec::new();
    if rc_ac != 0 {
        parts.push(format!("交流写入错误码 {rc_ac}"));
    } else if after_ac != Some(target_ac) {
        parts.push(format!(
            "交流回读 {}，目标 {}",
            format_value(setting, display_value(setting, after_ac)),
            format_value(setting, display_value(setting, Some(target_ac)))
        ));
    }
    if rc_dc != 0 {
        parts.push(format!("电池写入错误码 {rc_dc}"));
    } else if after_dc != Some(target_dc) {
        parts.push(format!(
            "电池回读 {}，目标 {}",
            format_value(setting, display_value(setting, after_dc)),
            format_value(setting, display_value(setting, Some(target_dc)))
        ));
    }
    parts.join("；")
}

fn token_digest(plans: &[(Guid, String)], active: Option<Guid>) -> String {
    let mut parts = plans
        .iter()
        .map(|(guid, _)| guid.to_text())
        .collect::<Vec<_>>();
    parts.push(format!(
        "active:{}",
        active
            .map(|guid| guid.to_text())
            .unwrap_or_else(|| "none".into())
    ));
    for setting in SETTINGS.iter() {
        parts.push(format!("setting:{}", setting.id));
    }
    let refs = parts.iter().map(String::as_str).collect::<Vec<_>>();
    digest_of(&refs)
}

fn preview() -> Result<Value, ToolError> {
    let plans = enumerate_plans();
    if plans.is_empty() {
        return Err(ToolError::new(
            "INTERNAL",
            "无法枚举电源计划：PowerEnumerate 没有返回任何方案。",
        ));
    }
    let active = active_plan();
    let active_name = active
        .and_then(|guid| {
            plans
                .iter()
                .find(|(candidate, _)| *candidate == guid)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| "未知".into());
    let token = tool_protocol::issue_token(ACTION, &token_digest(&plans, active))?;

    let rows = plans
        .iter()
        .map(|(guid, name)| {
            let current = active == Some(*guid);
            json!({
                "name": name,
                "status": if current { "当前使用" } else { "可切换" },
                "guid": guid.to_text(),
            })
        })
        .collect::<Vec<_>>();

    let settings_rows = active.map(collect_settings).unwrap_or_default();

    let mut data = table(
        json!([
            item("当前计划", active_name),
            item("计划数量", plans.len().to_string()),
            item("供电方式", power_source()),
        ]),
        json!([
            column("name", "电源计划"),
            column("status", "状态"),
            column("guid", "GUID"),
        ]),
        Value::Array(rows),
        json!([
            "只读预览：可切换电源计划，也可修改当前计划的处理器/显示/睡眠/硬盘设置。",
            "所有修改会立即生效，原值写入本地操作日志；可重新输入原值恢复。",
            "计划列表或活动计划变化后旧确认令牌失效，需要重新预览。",
            "电源设置写入在受限账户或沙箱环境可能返回权限错误。",
        ]),
    );
    if !settings_rows.is_empty() {
        if let Value::Object(map) = &mut data {
            map.insert("powerSettings".into(), Value::Array(settings_rows));
        }
    }
    attach_token(&mut data, token);
    Ok(data)
}

fn apply(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "切换电源计划需要确认令牌，请先预览并选择计划。",
        )
    })?;
    let requested = request
        .parameters
        .get("planGuid")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 planGuid。"))?;
    let target = Guid::parse(requested)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "planGuid 不是合法的计划标识。"))?;

    let plans = enumerate_plans();
    if plans.is_empty() {
        return Err(ToolError::new(
            "INTERNAL",
            "无法枚举电源计划，已取消切换。",
        ));
    }
    tool_protocol::verify_token(token, ACTION, &token_digest(&plans, active_plan()))?;

    let target_name = plans
        .iter()
        .find(|(guid, _)| *guid == target)
        .map(|(_, name)| name.clone())
        .ok_or_else(|| {
            ToolError::new(
                "INVALID_INPUT",
                "目标电源计划不在当前列表中，可能已被移除。",
            )
        })?;
    let previous = active_plan();
    let previous_name = previous
        .and_then(|guid| {
            plans
                .iter()
                .find(|(candidate, _)| *candidate == guid)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| "未知".into());

    // SAFETY: target 来自已枚举且校验过的计划 GUID。
    let rc = unsafe { PowerSetActiveScheme(std::ptr::null_mut(), &target) };
    if rc == 5 {
        return Err(ToolError::new(
            "PERMISSION_DENIED",
            "当前进程没有写入电源策略的权限（受限账户或沙箱环境）。请在普通桌面会话中重试；仍失败时以管理员身份运行。",
        ));
    }
    if rc != 0 {
        return Err(ToolError::new(
            "INTERNAL",
            format!("PowerSetActiveScheme 失败（错误码 {rc}）。"),
        ));
    }
    let current = active_plan();
    if current != Some(target) {
        return Err(ToolError::new(
            "INTERNAL",
            "切换后读取到的活动计划与目标不一致。",
        ));
    }

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "pc_optimizer",
        "action": ACTION,
        "fromName": previous_name,
        "fromGuid": previous.map(|guid| guid.to_text()),
        "toName": target_name,
        "toGuid": target.to_text(),
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));

    Ok(table(
        json!([
            item("已切换到", target_name.clone()),
            item("原计划", previous_name),
        ]),
        json!([
            column("name", "电源计划"),
            column("status", "状态"),
            column("guid", "GUID"),
        ]),
        json!([{
            "name": target_name,
            "status": "当前使用",
            "guid": target.to_text(),
        }]),
        json!([
            "切换已生效；原计划已写入本地操作日志，可再次预览后切换回去。",
            "可在预览页继续修改当前计划的单项电源设置。",
        ]),
    ))
}

struct SettingChange {
    setting: &'static PowerSetting,
    ac: u32,
    dc: u32,
}

fn apply_settings(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "修改电源设置需要确认令牌，请先预览并修改数值。",
        )
    })?;
    let changes = request
        .parameters
        .get("changes")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 changes。"))?;
    if changes.is_empty() {
        return Err(ToolError::new(
            "INVALID_INPUT",
            "没有需要应用的电源设置。",
        ));
    }

    let plans = enumerate_plans();
    if plans.is_empty() {
        return Err(ToolError::new(
            "INTERNAL",
            "无法枚举电源计划，已取消修改。",
        ));
    }
    let active = active_plan()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法读取当前活动电源计划。"))?;
    tool_protocol::verify_token(token, ACTION, &token_digest(&plans, Some(active)))?;
    let plan_name = plans
        .iter()
        .find(|(guid, _)| *guid == active)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "当前计划".into());

    let mut parsed: Vec<SettingChange> = Vec::new();
    let mut seen = HashSet::new();
    for change in changes {
        let id = change
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", "电源设置项缺少 id。"))?;
        let setting = setting_by_id(id).ok_or_else(|| {
            ToolError::new("INVALID_INPUT", format!("不支持的电源设置：{id}"))
        })?;
        if !seen.insert(setting.id) {
            return Err(ToolError::new(
                "INVALID_INPUT",
                format!("{} 重复出现。", setting.label),
            ));
        }
        let ac = change.get("ac").and_then(Value::as_u64).ok_or_else(|| {
            ToolError::new(
                "INVALID_INPUT",
                format!("{}缺少交流电取值。", setting.label),
            )
        })?;
        let dc = change.get("dc").and_then(Value::as_u64).ok_or_else(|| {
            ToolError::new("INVALID_INPUT", format!("{}缺少电池取值。", setting.label))
        })?;
        parsed.push(SettingChange {
            setting,
            ac: validate_value(setting, ac)?,
            dc: validate_value(setting, dc)?,
        });
    }

    let originals = parsed
        .iter()
        .map(|change| {
            (
                change.setting,
                read_setting(&active, change.setting, true),
                read_setting(&active, change.setting, false),
            )
        })
        .collect::<Vec<_>>();

    for change in &parsed {
        for (ac, value) in [(true, change.ac), (false, change.dc)] {
            let rc = write_setting(
                &active,
                change.setting,
                ac,
                raw_value(change.setting, value),
            );
            if rc != 0 {
                let (code, message) = if rc == 5 {
                    (
                        "PERMISSION_DENIED",
                        "当前进程没有写入电源设置的权限（受限账户或沙箱环境）。请在普通桌面会话中重试；仍失败时以管理员身份运行。",
                    )
                } else {
                    (
                        "INTERNAL",
                        "写入电源设置失败，请检查设备是否支持该项目。",
                    )
                };
                return Err(ToolError::new(
                    code,
                    format!("{}：错误码 {rc}。", message),
                ));
            }
        }
    }

    // 写入索引后激活当前计划，让改动立即生效。
    // SAFETY: active 来自系统返回的活动计划 GUID。
    let rc = unsafe { PowerSetActiveScheme(std::ptr::null_mut(), &active) };
    if rc == 5 {
        return Err(ToolError::new(
            "PERMISSION_DENIED",
            "当前进程没有应用电源设置的权限，请以管理员身份重试。",
        ));
    }
    if rc != 0 {
        return Err(ToolError::new(
            "INTERNAL",
            format!("应用电源设置失败（错误码 {rc}）。"),
        ));
    }

    for (change, (_, _, _)) in parsed.iter().zip(originals.iter()) {
        let ac_after = read_setting(&active, change.setting, true);
        let dc_after = read_setting(&active, change.setting, false);
        if ac_after != Some(raw_value(change.setting, change.ac))
            || dc_after != Some(raw_value(change.setting, change.dc))
        {
            return Err(ToolError::new(
                "INTERNAL",
                format!("{}写入后回读不一致。", change.setting.label),
            ));
        }
    }

    let audit_changes = parsed
        .iter()
        .zip(originals.iter())
        .map(|(change, (_, ac_before, dc_before))| {
            json!({
                "id": change.setting.id,
                "beforeAc": display_value(change.setting, *ac_before),
                "beforeDc": display_value(change.setting, *dc_before),
                "afterAc": change.ac,
                "afterDc": change.dc,
            })
        })
        .collect::<Vec<_>>();
    let _ = tool_protocol::append_audit_log(json!({
        "tool": "pc_optimizer",
        "action": ACTION_SETTINGS,
        "planGuid": active.to_text(),
        "planName": plan_name,
        "changes": audit_changes,
        "result": "ok",
        "at": tool_protocol::now_ms(),
    }));

    let rows = parsed
        .iter()
        .zip(originals.iter())
        .map(|(change, (_, ac_before, dc_before))| {
            json!({
                "label": change.setting.label,
                "before": format!(
                    "{} / {}",
                    format_value(change.setting, display_value(change.setting, *ac_before)),
                    format_value(change.setting, display_value(change.setting, *dc_before))
                ),
                "after": format!(
                    "{} / {}",
                    format_value(change.setting, Some(change.ac)),
                    format_value(change.setting, Some(change.dc))
                ),
                "status": "已应用",
            })
        })
        .collect::<Vec<_>>();

    Ok(table(
        json!([
            item("应用项", parsed.len().to_string()),
            item("当前计划", plan_name.clone()),
        ]),
        json!([
            column("label", "设置项"),
            column("before", "原值（交流 / 电池）"),
            column("after", "新值（交流 / 电池）"),
            column("status", "结果"),
        ]),
        Value::Array(rows),
        json!([
            "电源设置已应用并激活当前计划。",
            "原值已写入本地操作日志；可重新输入原值恢复，或在“电源选项”中恢复默认。",
            "确认令牌一次性使用，继续调整需要重新预览。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "pc_optimizer",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION, ACTION_SETTINGS],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => apply(request),
            ACTION_SETTINGS => apply_settings(request),
            other => Err(ToolError::unsupported(other)),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_roundtrips() {
        let text = "381b4222-f694-41f0-9685-ff5bb260df2e";
        let guid = Guid::parse(text).expect("valid guid");
        assert_eq!(guid.to_text(), text);
        assert_eq!(
            Guid::parse("{381b4222-f694-41f0-9685-ff5bb260df2e}")
                .expect("braced guid")
                .to_text(),
            text
        );
    }

    #[test]
    fn guid_rejects_invalid_text() {
        assert!(Guid::parse("not-a-guid").is_none());
        assert!(Guid::parse("381b4222-f694-41f0-9685-ff5bb260df2").is_none());
        assert!(Guid::parse("381b4222-f694-41f0-9685-ff5bb260df2g").is_none());
    }

    #[test]
    fn token_digest_binds_active_plan_and_settings() {
        let a = (
            Guid::parse("381b4222-f694-41f0-9685-ff5bb260df2e").unwrap(),
            "平衡".to_string(),
        );
        let b = (
            Guid::parse("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c").unwrap(),
            "高性能".to_string(),
        );
        let active_a = Some(a.0);
        let active_b = Some(b.0);
        assert_eq!(
            token_digest(&[a.clone(), b.clone()], active_a),
            token_digest(&[b.clone(), a.clone()], active_a)
        );
        assert_ne!(
            token_digest(&[a.clone()], active_a),
            token_digest(&[a], active_b)
        );
    }

    #[test]
    fn validates_setting_ranges_and_units() {
        let processor = setting_by_id("processor-min").expect("processor setting");
        assert_eq!(validate_value(processor, 100).unwrap(), 100);
        assert!(validate_value(processor, 101).is_err());
        assert!(validate_value(processor, 0).is_ok());

        let sleep = setting_by_id("sleep").expect("sleep setting");
        assert_eq!(validate_value(sleep, 30).unwrap(), 30);
        assert!(validate_value(sleep, 1441).is_err());
        assert_eq!(raw_value(sleep, 30), 1800);
        assert_eq!(display_value(sleep, Some(1800)), Some(30));

        let display = setting_by_id("display-off").expect("display setting");
        assert_eq!(raw_value(display, 10), 600);
        assert_eq!(display_value(display, Some(600)), Some(10));
        assert!(validate_value(display, 300).is_ok());
        assert!(validate_value(display, 301).is_err());
    }

    #[test]
    fn unknown_setting_ids_are_rejected() {
        assert!(setting_by_id("processor-min").is_some());
        assert!(setting_by_id("nope").is_none());
    }
}
