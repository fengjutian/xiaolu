# driver_manager · 驱动管理

检查驱动异常与 Windows Update 驱动候选；可打开 Windows 官方驱动界面（设备管理器、Windows 更新、可选更新）手动完成安装或回滚。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
driver_manager.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 读取 PnP 驱动、异常设备与 Windows Update 驱动候选 |
| `open_official` | 否 | 打开系统自带驱动界面 |

`open_official` 参数：

```json
{ "id": "device_manager" }
```

可接受 ID：`device_manager`、`windows_update`、`optional_updates`。

## 检查内容

| 项目 | 来源 |
|---|---|
| 驱动清单 | `Win32_PnPSignedDriver` 的设备名、版本、签名状态 |
| 异常设备 | 未签名或 `ConfigManagerErrorCode != 0`（最多 30 条） |
| 更新候选 | `Microsoft.Update.Session` 查询未安装的驱动更新 |
| 版本比较 | 候选版本与已装驱动版本逐段比较 |

常见 PnP 错误码会转成中文说明，例如 28「未安装驱动」、10「无法启动」。

## 安全机制

- 只读检测：不下载驱动、不调用安装 API、不替换驱动文件。
- `open_official` 只接受三个固定 ID，命令与参数均为固定值，不接受任意路径或 URL。
- 安装/回滚由 Windows 设备管理器或 Windows 更新完成，使用系统自带的签名校验与回滚机制。
- 打开动作写入本地操作日志。

## 边界

- 不实现第三方驱动站下载、不静默安装、不做驱动包解压。
- Windows Update 常不返回签名标记；本工具不把“未知”当作未签名。
- 不检查驱动日期、不比较厂商官网版本；候选是否适配由 Windows Update 判定。
- 驱动回滚需在设备管理器“属性 → 驱动程序 → 回退驱动程序”中操作。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/driver_manager/Cargo.toml
cargo test --manifest-path tools/driver_manager/Cargo.toml
```
