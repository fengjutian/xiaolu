# pc_optimizer · 电脑性能优化

电源计划部分的首个写操作工具：只读列出所有电源计划与当前活动计划，用户选择并确认后切换到目标计划。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
pc_optimizer.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 枚举电源计划、标记当前计划、显示供电方式，并签发确认令牌 |
| `apply` | 是 | 校验令牌后调用 `PowerSetActiveScheme` 切换计划，并写入操作日志 |

`apply` 需要参数 `{ "planGuid": "<计划 GUID>" }`，令牌由宿主在预览后回传。

## 数据来源与安全

- `PowerEnumerate` / `PowerReadFriendlyName` / `PowerGetActiveScheme` / `PowerSetActiveScheme`（powrprof）
- `GetSystemPowerStatus`：交流电/电池状态
- 不使用 `powercfg` 子进程，不接受任意命令行参数
- 令牌绑定预览时的计划列表摘要；列表变化后旧令牌失效
- 一次性令牌由宿主记录，5 分钟过期

## 回滚

每次 `apply` 都会把原计划与目标计划的名称、GUID、时间写入：

```text
%LOCALAPPDATA%\SystemLens\tool-actions.jsonl
```

回滚方式：重新预览后把原计划切回来即可。

## 构建

```powershell
cargo build --release --manifest-path tools/pc_optimizer/Cargo.toml
```

## 测试

```powershell
cargo test --manifest-path tools/pc_optimizer/Cargo.toml
```

## 当前边界

- 只实现电源计划切换；启动项、服务与注册表类优化仍未实现。
- 不修改计划内的单项设置（处理器最小状态、硬盘休眠等）。
- 不静默提权；普通用户权限即可切换活动计划。
