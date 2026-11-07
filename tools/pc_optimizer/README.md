# pc_optimizer · 电脑性能优化

电源计划与逐项电源设置的写操作工具：只读列出计划与当前设置，用户确认后切换计划、修改处理器/显示/睡眠/硬盘设置，或一键应用整套预设。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
pc_optimizer.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 枚举电源计划、当前计划、供电方式、5 项设置与 3 套预设，并签发确认令牌 |
| `apply` | 是 | 校验令牌后切换当前使用的电源计划，写入操作日志 |
| `apply_settings` | 是 | 校验令牌后整批写入设置、逐项回读并重新激活计划；单项失败不影响其他项 |
| `apply_preset` | 是 | 校验令牌后按预设整批写入 5 项设置，一次授权、一次快照、可一键撤销 |

`apply` 参数：`{ "planGuid": "<计划 GUID>" }`。

`apply_settings` 参数：

```json
{
  "changes": [
    { "id": "processor-min", "ac": 10, "dc": 20 },
    { "id": "display-off", "ac": 10, "dc": 5 }
  ]
}
```

`apply_preset` 参数：`{ "preset": "saver" | "balanced" | "performance" }`。

## 一键预设

| ID | 名称 | 取向 |
|---|---|---|
| `saver` | 省电 | 处理器上限 60%、显示 5 分钟、睡眠 15 分钟、关盘 10 分钟 |
| `balanced` | 均衡 | 处理器可满载，显示 10 分钟、睡眠 30 分钟、关盘 20 分钟 |
| `performance` | 性能 | 处理器满载；交流电下不休眠、不关盘，电池侧仍保留睡眠与关盘时间 |

预设只改**当前计划**的这几项取值，不切换电源计划；取值表在工具内固定，界面从预览结果读取，不在前端硬编码。应用后结果页给出「撤销本次修改」。

## 可修改的电源设置

| ID | 设置项 | 范围 | 单位 |
|---|---|---|---|
| `processor-min` | 处理器最小状态 | 0–100 | % |
| `processor-max` | 处理器最大状态 | 0–100 | % |
| `display-off` | 关闭显示器 | 0–300 | 分钟 |
| `sleep` | 睡眠 | 0–1440 | 分钟 |
| `disk-off` | 关闭硬盘 | 0–1440 | 分钟 |

时间类设置以分钟输入，工具内部换算为秒；0 表示永不。

## 数据来源与安全

**读取**（普通权限即可）：

- `PowerEnumerate` / `PowerReadFriendlyName` / `PowerGetActiveScheme`
- `PowerReadACValueIndex` / `PowerReadDCValueIndex`
- `GetSystemPowerStatus`：交流电/电池状态

**写入**（必须提权）：

- 电源计划的取值与「当前使用的计划」都落在 `HKLM\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes`，普通权限写入只会拿到 `ERROR_ACCESS_DENIED`（实测错误码 5）。
- 因此切换计划与写入设置一律交给独立提权助手 `lens_helper.exe`（op：`power_plan_activate` / `power_settings_write`），宿主不直接提权；整批设置一次写入、一次激活，用户只会被问一次 UAC。
- 助手里的 `powercfg` 别名走固定白名单（`SUB_PROCESSOR`/`SUB_VIDEO`/`SUB_SLEEP`/`SUB_DISK` 等 9 个），不接受任意命令行参数；电源计划 GUID 必须符合标准格式。
- `powercfg` 的错误以 ASCII 错误码回传再翻译成中文原因，避免系统控制台编码导致乱码。

其他约束：

- 令牌绑定“计划列表 + 当前活动计划 + 设置项集合”；任一变化旧令牌失效
- 一次性令牌由宿主记录，5 分钟过期
- 逐项独立执行：单项失败只影响该项，不中断其他项
- 写入后逐项回读；与目标不一致标记“部分应用/失败”，并在“原因”列说明
- 范围非法、重复项、未知设置、未知预设直接拒绝

## 回滚

每次写入两类记录：

- 操作日志：`%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`（计数、计划、快照路径）
- 完整快照：`%LOCALAPPDATA%\SystemLens\pc_optimizer\run-<时间戳>-<进程号>.json`（每项原值、目标值、回读值、结果）

`apply_settings` 与 `apply_preset` 的结果附带新的一次性令牌与 `undoChanges`，界面可直接“撤销本次修改”；也可以重新输入快照中的原值，或在“电源选项”中恢复默认。计划切换则重新预览后切回原计划。

> 令牌的动作名必须签发与校验一致：曾经用 `apply_settings` 签发、用 `apply` 校验，导致“撤销本次修改”被判「确认令牌与当前动作不匹配」而永远失败。现在统一走 `TOKEN_ACTION` 常量，并有单测守护。

## 构建

```powershell
cargo build --release --manifest-path tools/pc_optimizer/Cargo.toml
```

## 测试

```powershell
cargo test --manifest-path tools/pc_optimizer/Cargo.toml
```

## 当前边界

- 不实现启动项、服务与注册表类优化。
- 写入需要一次 UAC 授权；用户取消授权时不写入任何内容，并返回 `ELEVATION_CANCELLED`。
- 预设只覆盖这 5 项设置，不含电源计划切换、处理器 boost、显卡或电源模式（Modern Standby）设置。
- 电源设置是否可见/可写取决于设备与厂商驱动；回读不一致会标记为“部分应用/失败”，不会假装成功。
