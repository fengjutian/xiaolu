# software_pm · 软件权限管理

只读查看软件相关的启动行为与权限入口：启动项、自动服务、用户计划任务、应用通知开关、防火墙规则统计。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
software_pm.exe --stdio
```

宿主通过 stdin/stdout 行分隔 JSON 通信，协议见 `docs/TOOL_PACKAGE_SPEC.md`。

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读采集并返回启动行为与权限入口摘要 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 数据来源

- `Win32_StartupCommand`（CIM）：注册表 Run 与启动文件夹登记的启动项
- `Win32_Service`：`StartMode = Auto` 的自动服务，标注系统组件/第三方软件
- `Get-ScheduledTask`：非 `\Microsoft\` 且未禁用的计划任务及触发条件
- 注册表 `...\Notifications\Settings`：应用级通知开关
- `Get-NetFirewallRule`：仅统计已启用规则数量，不修改规则

## 构建

```powershell
cargo build --release --manifest-path tools/software_pm/Cargo.toml
```

## 测试

```powershell
cargo test --manifest-path tools/software_pm/Cargo.toml
```

## 限制

- 只读，不启用/禁用启动项、服务、计划任务、通知或防火墙规则。
- 防火墙仅统计已启用规则总数与出入站、阻止数量，不列出单条规则明细。
- 表格按类别截断：启动项 40、自动服务 60、计划任务 40、通知 20，超出部分不展示。
- 部分查询在受限权限或精简系统上可能失败，失败原因会写入“说明”区。
