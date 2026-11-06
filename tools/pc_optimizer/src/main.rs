use serde_json::{json, Value};
use std::ffi::c_void;
use tool_protocol::{attach_token, column, digest_of, item, table, Request, ToolError};

const ACTION: &str = "apply";
const ACCESS_SCHEME: u32 = 16;

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

fn plan_digest(plans: &[(Guid, String)]) -> String {
    let texts = plans
        .iter()
        .map(|(guid, _)| guid.to_text())
        .collect::<Vec<_>>();
    let refs = texts.iter().map(String::as_str).collect::<Vec<_>>();
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
    let token = tool_protocol::issue_token(ACTION, &plan_digest(&plans))?;

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
            "只切换活动电源计划，不逐项修改处理器、磁盘或显示设置。",
            "选择可切换计划并确认后立即生效，原计划与目标计划会写入本地操作日志。",
            "计划列表变化后旧确认令牌失效，需要重新预览。",
        ]),
    );
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
    tool_protocol::verify_token(token, ACTION, &plan_digest(&plans))?;

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
            "本工具只切换计划，不修改计划内的单项电源设置。",
        ]),
    ))
}

fn main() {
    tool_protocol::serve_with_request(
        "pc_optimizer",
        env!("CARGO_PKG_VERSION"),
        &["preview", ACTION],
        |request| match request.action.as_str() {
            "preview" => preview(),
            ACTION => apply(request),
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
    fn plan_digest_is_stable_and_order_independent() {
        let a = (
            Guid::parse("381b4222-f694-41f0-9685-ff5bb260df2e").unwrap(),
            "平衡".to_string(),
        );
        let b = (
            Guid::parse("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c").unwrap(),
            "高性能".to_string(),
        );
        assert_eq!(plan_digest(&[a.clone(), b.clone()]), plan_digest(&[b, a]));
    }
}
