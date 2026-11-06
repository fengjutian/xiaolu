# pc_optimizer · 电脑性能优化

电源计划与逐项电源设置的写操作工具：只读列出计划与当前设置，用户确认后切换计划或修改处理器/显示/睡眠/硬盘设置。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
pc_optimizer.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 枚举电源计划、当前计划、供电方式与 5 项设置，并签发确认令牌 |
| `apply` | 是 | 校验令牌后调用 `PowerSetActiveScheme` 切换计划，写入操作日志 |
| `apply_settings` | 是 | 校验令牌后写入逐项设置并激活当前计划，记录原值 |

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

## 可修改的电源设置

| ID | 设置项 | 范围 | 单位 |
|---|---|---|---|
| `processor-min` | 处理器最小状态 | 0–100 | % |
| `processor-max` | 处理器最大状态 | 0–100 | % |
| `display-off` | 关闭显示器 | 0–120 | 分钟 |
| `sleep` | 睡眠 | 0–1440 | 分钟 |
| `disk-off` | 关闭硬盘 | 0–1440 | 分钟 |

时间类设置以分钟输入，工具内部换算为秒；0 表示永不。

## 数据来源与安全

- `PowerEnumerate` / `PowerReadFriendlyName` / `PowerGetActiveScheme` / `PowerSetActiveScheme`
- `PowerReadACValueIndex` / `PowerReadDCValueIndex` / `PowerWriteACValueIndex` / `PowerWriteDCValueIndex`
- `GetSystemPowerStatus`：交流电/电池状态
- 不使用 `powercfg` 子进程，不接受任意命令行参数
- 令牌绑定“计划列表 + 当前活动计划 + 设置项集合”；任一变化旧令牌失效
- 一次性令牌由宿主记录，5 分钟过期
- 写入后逐项回读校验；范围非法、重复项、未知设置直接拒绝

## 回滚

每次 `apply` / `apply_settings` 都把原值与新值写入：

```text
%LOCALAPPDATA%\SystemLens\tool-actions.jsonl
```

- 计划切换：重新预览后切回原计划。
- 单项设置：重新输入日志中的原值应用，或在“电源选项”中恢复默认。

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
- 不静默提权；切换计划与写入设置通常需要普通用户权限，受限环境会返回 `PERMISSION_DENIED`。
- 电源设置是否可见/可写取决于设备与厂商驱动；回读不一致会报错而不是假装成功。
