use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};

const ACTION: &str = "apply";
const ACTION_SETTINGS: &str = "apply_settings";
const ACTION_PRESET: &str = "apply_preset";
/// 预览签发的令牌授权本次预览之后的全部写操作（切换计划、改单项、应用预设）。
/// 签发与校验必须用同一个动作名：两者不一致时「撤销本次修改」会被判动作不匹配。
const TOKEN_ACTION: &str = ACTION;
const ACCESS_SCHEME: u32 = 16;
/// 隐藏提权启动用的 PowerShell 窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy)]
struct PowerSetting {
    id: &'static str,
    label: &'static str,
    subgroup: &'static str,
    setting: &'static str,
    /// 提权助手调用 powercfg 用的别名，与上面的 GUID 一一对应。
    alias_subgroup: &'static str,
    alias_setting: &'static str,
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
        alias_subgroup: "SUB_PROCESSOR",
        alias_setting: "PROCTHROTTLEMIN",
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
        alias_subgroup: "SUB_PROCESSOR",
        alias_setting: "PROCTHROTTLEMAX",
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
        alias_subgroup: "SUB_VIDEO",
        alias_setting: "VIDEOIDLE",
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
        alias_subgroup: "SUB_SLEEP",
        alias_setting: "STANDBYIDLE",
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
        alias_subgroup: "SUB_DISK",
        alias_setting: "DISKIDLE",
        unit: "分钟",
        min: 0,
        max: 1440,
        scale: 60,
        note: "SSD 通常无需调整。",
    },
];

/// 一键预设：一组固定的电源设置目标值（交流 / 电池）。
struct Preset {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    items: &'static [(&'static str, u32, u32)],
}

const PRESETS: [Preset; 3] = [
    Preset {
        id: "saver",
        label: "省电",
        note: "压低处理器上限、缩短显示与睡眠时间；适合离电运行与负载较轻的场景。",
        items: &[
            ("processor-min", 5, 5),
            ("processor-max", 60, 60),
            ("display-off", 5, 5),
            ("sleep", 15, 15),
            ("disk-off", 10, 10),
        ],
    },
    Preset {
        id: "balanced",
        label: "均衡",
        note: "Windows 默认取向：处理器可满载，关闭显示器与睡眠时间居中。",
        items: &[
            ("processor-min", 5, 5),
            ("processor-max", 100, 100),
            ("display-off", 10, 10),
            ("sleep", 30, 30),
            ("disk-off", 20, 20),
        ],
    },
    Preset {
        id: "performance",
        label: "性能",
        note: "处理器保持满载，交流电下不自动睡眠、不关闭硬盘；更耗电。",
        items: &[
            ("processor-min", 100, 100),
            ("processor-max", 100, 100),
            ("display-off", 20, 10),
            ("sleep", 0, 30),
            ("disk-off", 0, 20),
        ],
    },
];

fn preset_by_id(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.id == id)
}

/// 续签令牌：结果页的「撤销本次修改」直接用它。
///
/// 动作名必须与写操作校验时完全一致 —— 两者不一致时撤销会被判
/// 「确认令牌与当前动作不匹配」而永远失败。
fn issue_continue_token(digest: &str) -> Option<String> {
    tool_protocol::issue_token(TOKEN_ACTION, digest).ok()
}

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

// ---------------------------------------------------------------------------
// 提权写入。
//
// 电源计划的取值与「当前使用的计划」都落在 HKLM，普通权限写入只会拿到
// ERROR_ACCESS_DENIED（实测错误码 5），所以这里统一交给独立提权助手执行：
// 宿主不直接提权，用户只会被问一次 UAC。
// ---------------------------------------------------------------------------

fn elevated_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("SystemLens")
        .join("elevated")
}

fn helper_path() -> Result<PathBuf, ToolError> {
    if let Some(path) = std::env::var_os("SYSTEM_LENS_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法定位工具路径：{error}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法定位工具目录。"))?;
    let candidates = [
        dir.join("lens_helper.exe"),
        dir.join("..").join("lens_helper").join("lens_helper.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| ToolError::new("HELPER_MISSING", "缺少提权助手 lens_helper.exe。"))
}

fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// 请求提权助手执行一次操作，返回助手写回的结果 JSON。
fn run_elevated(mut payload: Value) -> Result<Value, ToolError> {
    let dir = elevated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法创建提权请求目录：{error}")))?;
    let nonce_source = format!("{}-{}", tool_protocol::now_ms(), std::process::id());
    let nonce_full = tool_protocol::digest_of(&[&nonce_source]);
    let nonce = &nonce_full[..32];
    let request_path = dir.join(format!("req-{nonce}.json"));
    let result_path = dir.join(format!("res-{nonce}.json"));
    if let Value::Object(map) = &mut payload {
        map.insert("version".into(), json!(1));
        map.insert("nonce".into(), json!(nonce));
        map.insert("resultPath".into(), json!(result_path));
    }
    let serialized = serde_json::to_string(&payload)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法序列化提权请求：{error}")))?;
    std::fs::write(&request_path, serialized)
        .map_err(|error| ToolError::new("INTERNAL", format!("无法写入提权请求：{error}")))?;
    let helper = helper_path()?;
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ $p = Start-Process -FilePath {} -ArgumentList ('--request',('\"' + {} + '\"')) -Verb RunAs -PassThru -Wait -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_quote(&helper.to_string_lossy()),
        ps_quote(&request_path.to_string_lossy())
    );
    let status = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|error| ToolError::new("INTERNAL", format!("无法启动提权助手：{error}")))?;
    let result_text = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&request_path);
    let _ = std::fs::remove_file(&result_path);
    if !status.success() {
        if status.code() == Some(1223) {
            return Err(ToolError::new(
                "ELEVATION_CANCELLED",
                "已取消管理员授权，未修改电源设置。",
            ));
        }
        return Err(ToolError::new(
            "ELEVATION_FAILED",
            format!("提权助手执行失败（退出码 {:?}）。", status.code()),
        ));
    }
    let text = result_text
        .ok_or_else(|| ToolError::new("HELPER_FAILED", "提权助手没有返回结果。"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| ToolError::new("HELPER_FAILED", format!("无法解析助手结果：{error}")))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("提权助手返回失败。");
        return Err(ToolError::new("HELPER_FAILED", message.to_string()));
    }
    Ok(value)
}

/// 一批电源设置的写入结果。
struct HelperWrite {
    /// 每项的错误信息（None 表示写入成功）。
    errors: Vec<Option<String>>,
    /// 重新激活计划时的错误。
    activation_error: Option<String>,
}

/// 经提权助手一次性写入整批电源设置，并重新激活当前计划。
fn write_targets(targets: &[SettingChange]) -> Result<HelperWrite, ToolError> {
    let items = targets
        .iter()
        .map(|change| {
            json!({
                "id": change.setting.id,
                "subgroup": change.setting.alias_subgroup,
                "setting": change.setting.alias_setting,
                "ac": raw_value(change.setting, change.ac),
                "dc": raw_value(change.setting, change.dc),
            })
        })
        .collect::<Vec<_>>();
    let outcome = run_elevated(json!({"op": "power_settings_write", "settings": items}))?;
    let data = outcome.get("data").cloned().unwrap_or(Value::Null);
    let results = data
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let errors = targets
        .iter()
        .map(|change| {
            results
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(change.setting.id))
                .and_then(|entry| {
                    if entry.get("ok").and_then(Value::as_bool) == Some(true) {
                        None
                    } else {
                        entry
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }
                })
                .or_else(|| Some("提权助手没有返回这一项的结果。".to_string()))
        })
        .collect::<Vec<_>>();
    let activation_error = data
        .get("activationError")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(HelperWrite {
        errors,
        activation_error,
    })
}

/// 经提权助手切换当前使用的电源计划。
fn activate_plan_via_helper(guid: &Guid) -> Result<(), ToolError> {
    run_elevated(json!({"op": "power_plan_activate", "guid": guid.to_text()}))?;
    Ok(())
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
    helper_error: Option<&str>,
) -> String {
    if let Some(error) = helper_error {
        return error.to_string();
    }
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
    let token = tool_protocol::issue_token(TOKEN_ACTION, &token_digest(&plans, active))?;

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

    let settings_rows = active.map(|plan| collect_settings(&plan)).unwrap_or_default();

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
    if let Value::Object(map) = &mut data {
        map.insert("powerPresets".into(), Value::Array(presets_json()));
    }
    attach_token(&mut data, token);
    Ok(data)
}

/// 预设清单，供界面直接渲染（界面不硬编码预设取值）。
fn presets_json() -> Vec<Value> {
    PRESETS
        .iter()
        .map(|preset| {
            let items = preset
                .items
                .iter()
                .filter_map(|(id, ac, dc)| {
                    let setting = setting_by_id(id)?;
                    Some(json!({
                        "id": setting.id,
                        "label": setting.label,
                        "unit": setting.unit,
                        "ac": ac,
                        "dc": dc,
                    }))
                })
                .collect::<Vec<_>>();
            json!({
                "id": preset.id,
                "label": preset.label,
                "note": preset.note,
                "items": items,
            })
        })
        .collect()
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
    tool_protocol::verify_token(token, TOKEN_ACTION, &token_digest(&plans, active_plan()))?;

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

    // 切换当前计划需要写 HKLM，普通权限会被拒绝，交给提权助手执行。
    activate_plan_via_helper(&target)?;
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

struct ItemResult {
    setting: &'static PowerSetting,
    before_ac: Option<u32>,
    before_dc: Option<u32>,
    target_ac: u32,
    target_dc: u32,
    rc_ac: u32,
    rc_dc: u32,
    after_ac: Option<u32>,
    after_dc: Option<u32>,
    status: &'static str,
    /// 提权助手返回的失败原因（成功时为 None）。
    helper_error: Option<String>,
}

/// 解析前端传来的逐项改动，并做范围校验。
fn parse_changes(changes: &[Value]) -> Result<Vec<SettingChange>, ToolError> {
    let mut parsed: Vec<SettingChange> = Vec::new();
    let mut seen = HashSet::new();
    for change in changes {
        let id = change
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", "电源设置项缺少 id。"))?;
        let setting = setting_by_id(id)
            .ok_or_else(|| ToolError::new("INVALID_INPUT", format!("不支持的电源设置：{id}")))?;
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
            ToolError::new(
                "INVALID_INPUT",
                format!("{}缺少电池取值。", setting.label),
            )
        })?;
        parsed.push(SettingChange {
            setting,
            ac: validate_value(setting, ac)?,
            dc: validate_value(setting, dc)?,
        });
    }
    Ok(parsed)
}

/// 把预设解析成逐项改动。预设是固定表，取值天然在范围内，这里仍走一次校验。
fn preset_changes(preset: &Preset) -> Result<Vec<SettingChange>, ToolError> {
    let mut parsed = Vec::with_capacity(preset.items.len());
    for (id, ac, dc) in preset.items {
        let setting = setting_by_id(id)
            .ok_or_else(|| ToolError::new("INTERNAL", format!("预设引用了未知设置：{id}")))?;
        parsed.push(SettingChange {
            setting,
            ac: validate_value(setting, *ac as u64)?,
            dc: validate_value(setting, *dc as u64)?,
        });
    }
    Ok(parsed)
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
    let parsed = parse_changes(changes)?;
    apply_changes(ACTION_SETTINGS, token, parsed)
}

/// 应用一套批量预设：一次提权写完整套设置，一次快照，可一键撤销。
fn apply_preset(request: &Request) -> Result<Value, ToolError> {
    let token = request.confirmation.as_deref().ok_or_else(|| {
        ToolError::new(
            "CONFIRMATION_REQUIRED",
            "应用预设需要确认令牌，请先预览并选择预设。",
        )
    })?;
    let preset_id = request
        .parameters
        .get("preset")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", "缺少参数 preset。"))?;
    let preset = preset_by_id(preset_id)
        .ok_or_else(|| ToolError::new("INVALID_INPUT", format!("不支持的预设：{preset_id}")))?;
    let parsed = preset_changes(preset)?;
    apply_changes(ACTION_PRESET, token, parsed)
}

/// 写电源设置的公共流程：校验令牌 → 记录原值 → 一次提权写完整批 → 回读复核。
fn apply_changes(
    action: &str,
    token: &str,
    parsed: Vec<SettingChange>,
) -> Result<Value, ToolError> {
    let plans = enumerate_plans();
    if plans.is_empty() {
        return Err(ToolError::new(
            "INTERNAL",
            "无法枚举电源计划，已取消修改。",
        ));
    }
    let active = active_plan()
        .ok_or_else(|| ToolError::new("INTERNAL", "无法读取当前活动电源计划。"))?;
    tool_protocol::verify_token(token, TOKEN_ACTION, &token_digest(&plans, Some(active)))?;
    let plan_name = plans
        .iter()
        .find(|(guid, _)| *guid == active)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "当前计划".into());

    let started_at = tool_protocol::now_ms();
    // 先记录原值，再一次性提权写入整批：用户只会被问一次 UAC。
    let before = parsed
        .iter()
        .map(|change| {
            (
                read_setting(&active, change.setting, true),
                read_setting(&active, change.setting, false),
            )
        })
        .collect::<Vec<_>>();
    let write = write_targets(&parsed)?;

    let mut results: Vec<ItemResult> = Vec::with_capacity(parsed.len());
    for (index, change) in parsed.iter().enumerate() {
        let target_ac = raw_value(change.setting, change.ac);
        let target_dc = raw_value(change.setting, change.dc);
        let after_ac = read_setting(&active, change.setting, true);
        let after_dc = read_setting(&active, change.setting, false);
        let helper_error = write.errors[index].clone();
        let (rc_ac, rc_dc) = if helper_error.is_some() { (1, 1) } else { (0, 0) };
        results.push(ItemResult {
            setting: change.setting,
            before_ac: before[index].0,
            before_dc: before[index].1,
            target_ac,
            target_dc,
            rc_ac,
            rc_dc,
            after_ac,
            after_dc,
            status: item_status(rc_ac, rc_dc, after_ac, after_dc, target_ac, target_dc),
            helper_error,
        });
    }

    // 提权助手在写入后重新激活当前计划；激活失败不回滚已写入项，交由用户重试。
    let any_written = results
        .iter()
        .any(|result| result.rc_ac == 0 || result.rc_dc == 0);
    let activation_error = if any_written {
        write.activation_error.clone()
    } else {
        None
    };

    let applied = results
        .iter()
        .filter(|result| result.status == "已应用")
        .count();
    let partial = results
        .iter()
        .filter(|result| result.status == "部分应用")
        .count();
    let failed = results
        .iter()
        .filter(|result| result.status == "失败")
        .count();

    let rows = results
        .iter()
        .map(|result| {
            let status = if activation_error.is_some() && result.status != "失败" {
                "已写入，激活失败"
            } else {
                result.status
            };
            json!({
                "label": result.setting.label,
                "before": format!(
                    "{} / {}",
                    format_value(result.setting, display_value(result.setting, result.before_ac)),
                    format_value(result.setting, display_value(result.setting, result.before_dc))
                ),
                "after": format!(
                    "{} / {}",
                    format_value(result.setting, display_value(result.setting, result.after_ac)),
                    format_value(result.setting, display_value(result.setting, result.after_dc))
                ),
                "status": status,
                "reason": item_reason(
                    result.setting,
                    result.rc_ac,
                    result.rc_dc,
                    result.after_ac,
                    result.after_dc,
                    result.target_ac,
                    result.target_dc,
                    result.helper_error.as_deref(),
                ),
            })
        })
        .collect::<Vec<_>>();

    let undo = results
        .iter()
        .filter_map(|result| {
            let before_ac = result.before_ac?;
            let before_dc = result.before_dc?;
            if before_ac == result.target_ac && before_dc == result.target_dc {
                return None;
            }
            Some(json!({
                "id": result.setting.id,
                "ac": display_value(result.setting, Some(before_ac)),
                "dc": display_value(result.setting, Some(before_dc)),
            }))
        })
        .collect::<Vec<_>>();

    let finished_at = tool_protocol::now_ms();
    let snapshot = json!({
        "tool": "pc_optimizer",
        "version": env!("CARGO_PKG_VERSION"),
        "action": action,
        "startedAt": started_at,
        "finishedAt": finished_at,
        "planGuid": active.to_text(),
        "planName": plan_name,
        "activationError": activation_error,
        "counts": {"applied": applied, "partial": partial, "failed": failed},
        "items": results
            .iter()
            .map(|result| json!({
                "id": result.setting.id,
                "label": result.setting.label,
                "beforeAc": result.before_ac,
                "beforeDc": result.before_dc,
                "targetAc": result.target_ac,
                "targetDc": result.target_dc,
                "afterAc": result.after_ac,
                "afterDc": result.after_dc,
                "rcAc": result.rc_ac,
                "rcDc": result.rc_dc,
                "status": result.status,
            }))
            .collect::<Vec<_>>(),
    });
    let snapshot_path = write_snapshot(&snapshot);

    let _ = tool_protocol::append_audit_log(json!({
        "tool": "pc_optimizer",
        "action": action,
        "planGuid": active.to_text(),
        "planName": plan_name,
        "applied": applied,
        "partial": partial,
        "failed": failed,
        "activationError": activation_error,
        "snapshot": snapshot_path.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
        "at": finished_at,
    }));

    let activation_label = if !any_written {
        "未执行".to_string()
    } else {
        activation_error
            .as_ref()
            .map(|error| format!("失败（{error}）"))
            .unwrap_or_else(|| "正常".into())
    };
    let activation_note = if !any_written {
        "所有项均未写入成功，未执行激活；请检查权限或设备是否支持这些设置。"
    } else if activation_error.is_some() {
        "写入已发生，但激活当前计划失败；重新预览后再次应用可重试激活。"
    } else {
        "当前计划已重新激活，改动立即生效。"
    };

    let mut data = table(
        json!([
            item("成功", applied.to_string()),
            item("部分应用", partial.to_string()),
            item("失败", failed.to_string()),
            item("当前计划", plan_name),
            item("激活", activation_label),
        ]),
        json!([
            column("label", "设置项"),
            column("before", "原值（交流 / 电池）"),
            column("after", "回读（交流 / 电池）"),
            column("status", "结果"),
            column("reason", "原因"),
        ]),
        Value::Array(rows),
        json!([
            "每项独立执行：单项失败不会回滚或中断其他项；回读与目标不一致会写入“原因”列。",
            "已生成新的一次性令牌，可直接点击“撤销本次修改”恢复原值。",
            activation_note,
            snapshot_path
                .as_ref()
                .map(|path| format!("本次原值与回读已写入快照：{}", path.display()))
                .unwrap_or_else(|error| format!("快照写入失败：{error}")),
        ]),
    );
    if let Value::Object(map) = &mut data {
        map.insert(
            "powerSettings".into(),
            Value::Array(collect_settings(&active)),
        );
        // 结果页要能继续用预设（再切一次），所以预设清单也随结果带回去。
        map.insert("powerPresets".into(), Value::Array(presets_json()));
        map.insert("undoChanges".into(), Value::Array(undo));
    }
    let plans_after = enumerate_plans();
    if let Some(current) = active_plan() {
        // 续签的令牌必须与校验时用同一个动作名，否则「撤销本次修改」会被判动作不匹配。
        if let Some(token) = issue_continue_token(&token_digest(&plans_after, Some(current))) {
            attach_token(&mut data, token);
        }
    }
    Ok(data)
}

fn main() {
    tool_protocol::serve_with_request(
        "pc_optimizer",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION, ACTION_SETTINGS, ACTION_PRESET],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => apply(request),
            ACTION_SETTINGS => apply_settings(request),
            ACTION_PRESET => apply_preset(request),
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

    #[test]
    fn item_status_requires_write_and_readback() {
        assert_eq!(
            item_status(0, 0, Some(100), Some(200), 100, 200),
            "已应用"
        );
        assert_eq!(
            item_status(0, 0, Some(99), Some(200), 100, 200),
            "部分应用"
        );
        assert_eq!(item_status(5, 5, None, None, 100, 200), "失败");
        assert_eq!(
            item_status(0, 5, Some(100), None, 100, 200),
            "部分应用"
        );
        assert_eq!(
            item_status(0, 0, Some(100), Some(199), 100, 200),
            "部分应用"
        );
    }

    #[test]
    fn item_reason_reports_per_channel_failures() {
        let setting = setting_by_id("processor-max").expect("processor setting");
        assert_eq!(
            item_reason(setting, 0, 0, Some(100), Some(100), 100, 100, None),
            ""
        );

        let reason = item_reason(setting, 5, 0, Some(100), Some(80), 100, 100, None);
        assert!(reason.contains("交流写入错误码 5"));
        assert!(reason.contains("电池回读"));
    }

    #[test]
    fn item_reason_prefers_the_helper_message() {
        let setting = setting_by_id("processor-max").expect("processor setting");
        let reason = item_reason(setting, 1, 1, None, None, 100, 100, Some("powercfg 失败"));
        assert_eq!(reason, "powercfg 失败");
    }

    #[test]
    fn presets_stay_inside_setting_ranges() {
        for preset in PRESETS.iter() {
            assert_eq!(
                preset.items.len(),
                SETTINGS.len(),
                "预设 {} 必须覆盖全部设置项",
                preset.id
            );
            for (id, ac, dc) in preset.items {
                let setting = setting_by_id(id).expect("known setting");
                assert!(
                    *ac >= setting.min && *ac <= setting.max,
                    "{} 的交流值越界",
                    preset.id
                );
                assert!(
                    *dc >= setting.min && *dc <= setting.max,
                    "{} 的电池值越界",
                    preset.id
                );
            }
            // 预设也要能通过与逐项修改同一套校验。
            let changes = preset_changes(preset).expect("preset changes");
            assert_eq!(changes.len(), SETTINGS.len());
        }
    }

    #[test]
    fn preset_ids_are_stable() {
        assert!(preset_by_id("saver").is_some());
        assert!(preset_by_id("balanced").is_some());
        assert!(preset_by_id("performance").is_some());
        assert!(preset_by_id("turbo").is_none());
    }

    #[test]
    fn continue_token_can_be_used_by_the_next_write() {
        // 「撤销本次修改」用的就是续签令牌：签发与校验的动作名必须一致，
        // 否则撤销会被判「确认令牌与当前动作不匹配」而永远失败。
        std::env::set_var(
            "SYSTEM_LENS_TOKEN_SECRET",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        );
        let token = issue_continue_token("digest-a").expect("continue token");
        assert!(
            tool_protocol::verify_token(&token, TOKEN_ACTION, "digest-a").is_ok(),
            "续签令牌必须能通过下一次写操作的校验"
        );
        assert!(
            tool_protocol::verify_token(&token, "apply_settings", "digest-a").is_err(),
            "续签令牌不应被当成别的动作"
        );
        std::env::remove_var("SYSTEM_LENS_TOKEN_SECRET");
    }
}
