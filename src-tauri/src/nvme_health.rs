use serde::Serialize;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 49;
const PROPERTY_STANDARD_QUERY: u32 = 0;
const PROTOCOL_TYPE_NVME: u32 = 3;
const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
const NVME_LOG_PAGE_HEALTH_INFO: u32 = 2;
const NVME_HEALTH_LOG_SIZE: usize = 512;
const MAX_DRIVES: u32 = 32;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NvmeHealth {
    device: String,
    critical_warning: u8,
    temperature_c: Option<f64>,
    available_spare_percent: u8,
    spare_threshold_percent: u8,
    percentage_used: u8,
    data_read_bytes: u64,
    data_written_bytes: u64,
    power_on_hours: u64,
    power_cycles: u64,
    unsafe_shutdowns: u64,
    media_errors: u64,
    error_log_entries: u64,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NvmeHealthReport {
    captured_at_unix_ms: u64,
    devices: Vec<NvmeHealth>,
    access_denied: bool,
    unsupported: bool,
    last_error: Option<String>,
    note: String,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn le_u128(bytes: &[u8]) -> u128 {
    let mut buffer = [0u8; 16];
    buffer.copy_from_slice(&bytes[..16]);
    u128::from_le_bytes(buffer)
}

fn units_to_bytes(units: u128) -> u64 {
    units
        .saturating_mul(1000 * 512)
        .min(u128::from(u64::MAX)) as u64
}

fn count_u64(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}

/// 解析 512 字节的 NVMe Health Information Log。
pub fn parse_health_log(device: &str, bytes: &[u8]) -> Option<NvmeHealth> {
    if bytes.len() < NVME_HEALTH_LOG_SIZE {
        return None;
    }
    let critical_warning = bytes[0];
    let kelvin = u16::from_le_bytes([bytes[1], bytes[2]]);
    let temperature_c = if kelvin > 0 {
        Some(f64::from(kelvin) - 273.15)
    } else {
        None
    };
    Some(NvmeHealth {
        device: device.into(),
        critical_warning,
        temperature_c,
        available_spare_percent: bytes[3],
        spare_threshold_percent: bytes[4],
        percentage_used: bytes[5],
        data_read_bytes: units_to_bytes(le_u128(&bytes[32..48])),
        data_written_bytes: units_to_bytes(le_u128(&bytes[48..64])),
        power_on_hours: count_u64(le_u128(&bytes[128..144])),
        power_cycles: count_u64(le_u128(&bytes[112..128])),
        unsafe_shutdowns: count_u64(le_u128(&bytes[144..160])),
        media_errors: count_u64(le_u128(&bytes[160..176])),
        error_log_entries: count_u64(le_u128(&bytes[176..192])),
        source: "NVMe Health Information Log (IOCTL_STORAGE_QUERY_PROPERTY)".into(),
    })
}

#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    pub const GENERIC_READ: u32 = 0x8000_0000;
    pub const FILE_SHARE_READ: u32 = 0x0000_0001;
    pub const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    pub const OPEN_EXISTING: u32 = 3;
    pub const INVALID_HANDLE_VALUE: isize = -1;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            creation: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
        pub fn DeviceIoControl(
            device: *mut c_void,
            code: u32,
            input: *mut c_void,
            input_size: u32,
            output: *mut c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
        pub fn GetLastError() -> u32;
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn query_drive(index: u32) -> Result<Option<NvmeHealth>, (bool, String)> {
    use ffi::*;
    let path = format!(r"\\.\PhysicalDrive{index}");
    let name = wide(&path);
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == INVALID_HANDLE_VALUE {
        let code = unsafe { GetLastError() };
        return match code {
            2 | 3 => Ok(None),
            5 => Err((true, "需要管理员权限才能读取 NVMe 健康日志".into())),
            32 => Err((false, "设备正在被其他程序独占".into())),
            _ => Err((false, format!("打开设备失败（Win32 {code}）"))),
        };
    }

    #[repr(C)]
    struct PropertyQuery {
        property_id: u32,
        query_type: u32,
        additional: [u8; 1],
    }
    #[repr(C)]
    struct ProtocolData {
        protocol_type: u32,
        data_type: u32,
        request_value: u32,
        request_sub_value: u32,
        data_offset: u32,
        data_length: u32,
        fixed_return: u32,
        request_sub_value2: u32,
        request_sub_value3: u32,
        request_sub_value4: u32,
    }

    let query = PropertyQuery {
        property_id: STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY,
        query_type: PROPERTY_STANDARD_QUERY,
        additional: [0],
    };
    let protocol = ProtocolData {
        protocol_type: PROTOCOL_TYPE_NVME,
        data_type: NVME_DATA_TYPE_LOG_PAGE,
        request_value: NVME_LOG_PAGE_HEALTH_INFO,
        request_sub_value: 0,
        data_offset: std::mem::size_of::<ProtocolData>() as u32,
        data_length: NVME_HEALTH_LOG_SIZE as u32,
        fixed_return: 0,
        request_sub_value2: 0,
        request_sub_value3: 0,
        request_sub_value4: 0,
    };

    let mut input = vec![0u8; std::mem::size_of::<PropertyQuery>() + std::mem::size_of::<ProtocolData>()];
    unsafe {
        std::ptr::copy_nonoverlapping(
            &query as *const PropertyQuery as *const u8,
            input.as_mut_ptr(),
            std::mem::size_of::<PropertyQuery>(),
        );
        std::ptr::copy_nonoverlapping(
            &protocol as *const ProtocolData as *const u8,
            input.as_mut_ptr().add(std::mem::size_of::<PropertyQuery>()),
            std::mem::size_of::<ProtocolData>(),
        );
    }

    let mut output = vec![0u8; 4096];
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            input.as_mut_ptr() as *mut std::ffi::c_void,
            input.len() as u32,
            output.as_mut_ptr() as *mut std::ffi::c_void,
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let code = unsafe { GetLastError() };
        unsafe { CloseHandle(handle) };
        return Err(match code {
            1 => (false, "NVMe 健康日志查询不受支持".into()),
            5 => (true, "需要管理员权限才能读取 NVMe 健康日志".into()),
            _ => (false, format!("NVMe 健康日志查询失败（Win32 {code}）")),
        });
    }
    unsafe { CloseHandle(handle) };

    if returned < 8 + std::mem::size_of::<ProtocolData>() as u32 {
        return Ok(None);
    }
    let version = u32::from_le_bytes([output[0], output[1], output[2], output[3]]);
    let _ = version;
    let descriptor_offset = 8usize;
    let protocol_type = u32::from_le_bytes([
        output[descriptor_offset],
        output[descriptor_offset + 1],
        output[descriptor_offset + 2],
        output[descriptor_offset + 3],
    ]);
    let data_type = u32::from_le_bytes([
        output[descriptor_offset + 4],
        output[descriptor_offset + 5],
        output[descriptor_offset + 6],
        output[descriptor_offset + 7],
    ]);
    if protocol_type != PROTOCOL_TYPE_NVME || data_type != NVME_DATA_TYPE_LOG_PAGE {
        return Ok(None);
    }
    let data_offset = u32::from_le_bytes([
        output[descriptor_offset + 16],
        output[descriptor_offset + 17],
        output[descriptor_offset + 18],
        output[descriptor_offset + 19],
    ]) as usize;
    let start = descriptor_offset + data_offset;
    let end = start + NVME_HEALTH_LOG_SIZE;
    if end > output.len() {
        return Ok(None);
    }
    Ok(parse_health_log(&path, &output[start..end]))
}

#[cfg(windows)]
pub fn query() -> NvmeHealthReport {
    let mut devices = Vec::new();
    let mut access_denied = false;
    let mut unsupported = false;
    let mut last_error = None;
    for index in 0..MAX_DRIVES {
        match query_drive(index) {
            Ok(Some(health)) => devices.push(health),
            Ok(None) => {}
            Err((denied, message)) => {
                access_denied |= denied;
                if message.contains("不支持") {
                    unsupported = true;
                }
                last_error = Some(message);
            }
        }
    }
    let note = if devices.is_empty() {
        if access_denied {
            "普通权限无法读取 NVMe 健康日志，请以管理员身份重新启动后重试。"
        } else if unsupported {
            "当前控制器或驱动不支持 NVMe 健康日志直读，已回退到可靠性计数器。"
        } else {
            "没有找到可直读健康日志的 NVMe 设备。"
        }
    } else {
        "直读 NVMe Health Information Log；字段来自设备固件。"
    };
    NvmeHealthReport {
        captured_at_unix_ms: now_unix_ms(),
        devices,
        access_denied,
        unsupported,
        last_error,
        note: note.into(),
    }
}

#[cfg(not(windows))]
pub fn query() -> NvmeHealthReport {
    NvmeHealthReport {
        captured_at_unix_ms: 0,
        devices: Vec::new(),
        access_denied: false,
        unsupported: true,
        last_error: None,
        note: "NVMe 直读仅支持 Windows。".into(),
    }
}

#[tauri::command]
pub async fn get_nvme_health() -> Result<NvmeHealthReport, String> {
    tauri::async_runtime::spawn_blocking(query)
        .await
        .map_err(|error| format!("NVMe 健康查询任务失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_health_log_fields() {
        let mut bytes = vec![0u8; NVME_HEALTH_LOG_SIZE];
        bytes[0] = 0x04;
        bytes[1..3].copy_from_slice(&320u16.to_le_bytes());
        bytes[3] = 100;
        bytes[4] = 10;
        bytes[5] = 7;
        bytes[32..48].copy_from_slice(&1000u128.to_le_bytes());
        bytes[48..64].copy_from_slice(&2000u128.to_le_bytes());
        bytes[112..128].copy_from_slice(&5u128.to_le_bytes());
        bytes[128..144].copy_from_slice(&1234u128.to_le_bytes());
        bytes[144..160].copy_from_slice(&2u128.to_le_bytes());
        bytes[160..176].copy_from_slice(&1u128.to_le_bytes());
        bytes[176..192].copy_from_slice(&3u128.to_le_bytes());

        let health = parse_health_log(r"\\.\PhysicalDrive0", &bytes).unwrap();
        assert_eq!(health.critical_warning, 0x04);
        assert_eq!(health.temperature_c.map(|value| value.round()), Some(47.0));
        assert_eq!(health.available_spare_percent, 100);
        assert_eq!(health.percentage_used, 7);
        assert_eq!(health.data_read_bytes, 1000 * 512_000);
        assert_eq!(health.data_written_bytes, 2000 * 512_000);
        assert_eq!(health.power_cycles, 5);
        assert_eq!(health.power_on_hours, 1234);
        assert_eq!(health.unsafe_shutdowns, 2);
        assert_eq!(health.media_errors, 1);
        assert_eq!(health.error_log_entries, 3);
    }

    #[test]
    fn rejects_short_buffers() {
        assert!(parse_health_log("x", &[0u8; 128]).is_none());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "需要在本机尝试直读 NVMe 健康日志"]
    fn live_query_nvme() {
        let report = query();
        println!(
            "devices={} access_denied={} unsupported={} last_error={:?} note={}",
            report.devices.len(),
            report.access_denied,
            report.unsupported,
            report.last_error,
            report.note
        );
        for device in &report.devices {
            println!(
                "{} temp={:?} used={}% spare={} media_errors={} hours={}",
                device.device,
                device.temperature_c,
                device.percentage_used,
                device.available_spare_percent,
                device.media_errors,
                device.power_on_hours
            );
        }
    }
}
