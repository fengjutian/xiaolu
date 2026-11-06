# software_pm · 软件权限管理

查看软件启动行为与权限入口：启动项、自动服务、用户计划任务、应用通知开关、防火墙规则统计；并可启停**当前用户**启动项。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
software_pm.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 采集各类视图，并返回当前用户 Run/RunOnce 启动项与确认令牌 |
| `toggle_startup` | 是 | 校验令牌后停用或恢复一个当前用户启动项 |

`toggle_startup` 参数：

```json
{ "key": "Run", "name": "OneDrive", "enable": false }
```

## 启动项启停机制

- 只处理 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 与 `RunOnce`
- 停用：读取原命令 → 写入 `HKCU\Software\SystemLens\DisabledStartup`（值名 `Run::<名称>`）→ 删除 Run 值
- 启用：从备份读回命令 → 写回 Run → 删除备份值
- 删除失败时自动回滚刚写入的备份，避免“两处都存在/都不存在”的不一致
- 写入后重新读取注册表校验状态；不一致直接报错

## 安全机制

- 令牌绑定“当前用户启动项集合（键、名称、命令、启用状态）”；集合变化后旧令牌失效
- 参数只接受 `Run`/`RunOnce` 与合法值名；值名不能包含 `::`
- 不处理 HKLM/系统级启动项、服务、计划任务、通知或防火墙规则
- 操作写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`（只记录键、名称、启用与否，不记录命令）

## 回滚

- 停用后可在同一界面点击“启用（还原）”
- 若工具不可用，可手动把 `HKCU\Software\SystemLens\DisabledStartup` 中对应值写回 Run 键

## 数据来源

- 注册表 `HKCU\...\Run`、`RunOnce` 与 `Software\SystemLens\DisabledStartup`
- `Win32_StartupCommand`、`Win32_Service`、`Get-ScheduledTask`、通知设置、`Get-NetFirewallRule`
  （以上为只读视图）

## 构建与测试

```powershell
cargo build --release --manifest-path tools/software_pm/Cargo.toml
cargo test --manifest-path tools/software_pm/Cargo.toml
```

## 限制

- 仅当前用户 Run/RunOnce；系统级启动项保持只读。
- 防火墙仅统计已启用规则数量，不列出单条明细。
- 表格按类别截断：启动项 40、自动服务 60、计划任务 40、通知 20。
- 部分 WMI/防火墙查询在受限权限或精简系统上可能失败，失败原因写入“说明”区。
