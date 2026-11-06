# ai_cooling · AI 智能降温

读取 ACPI 温度与 CPU 负载并给出降温建议；可选用管理员权限一次性读取受限传感器（ACPI 风扇/温度探针、磁盘 SMART），不修改任何设置。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
ai_cooling.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 普通权限读取 ACPI 热区、CPU 负载，生成建议 |
| `read_elevated` | 否 | UAC 启动 `lens_helper.exe`，一次性读取 ACPI 风扇/探针、磁盘温度与 SMART |

`read_elevated` 不需要参数，也不会修改任何设置。

## T1 受限读取

- 通过 `lens_helper.exe` 提权，请求/结果只经 `%LOCALAPPDATA%\SystemLens\elevated\` 下的 JSON 交换。
- 读取内容：
  - `MSAcpi_ThermalZoneTemperature` 热区；
  - `Win32_Fan` 风扇状态/目标转速；
  - `Win32_TemperatureProbe` 温度探针；
  - `Get-PhysicalDisk` + `Get-StorageReliabilityCounter` 磁盘温度/磨损/通电时间；
  - `MSStorageDriver_FailurePredictStatus` 故障预测。
- 用户取消 UAC 返回 `ELEVATION_CANCELLED`，不读取任何数据。
- 助手退出后权限即失效，不常驻、不写注册表。

## 覆盖边界

- ACPI 热区/风扇/探针是否可用取决于主板与固件；很多机器只暴露热区，甚至完全不暴露。
- **CPU 封装温度、主板温度、风扇转速的完整覆盖仍需要可选传感器驱动（T2）**，当前不安装内核驱动。
- SMART/磁盘温度在部分 NVMe、RAID、USB 设备上不可用。
- 不控制风扇、不修改电源计划、不调整电压。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/ai_cooling/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/ai_cooling/Cargo.toml
cargo test --manifest-path tools/lens_helper/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `ai_cooling.exe` 放在同一目录。

## 处理器上限（软件降温）

`set_power_limit` 经 UAC 调用 `powercfg`，把当前电源方案的处理器最大状态设为 60/80/100%（交流与电池），设回 100% 即恢复。它限制的是处理器功耗上限，**不是风扇转速控制**；部分固件可能忽略该设置。风扇/EC 控制需要厂商驱动，本工具不做。
