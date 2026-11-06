# software_pm · 软件权限管理

查看软件启动行为与权限入口：启动项、自动服务、用户计划任务、应用通知开关、防火墙规则统计；可启停**当前用户**启动项，也可在管理员授权下启停**系统级**启动项。

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
| `toggle_startups` | 是 | 批量启停，逐项执行、逐项回读，单项失败不中断其他项 |
| `restore_all` | 是 | 恢复所有已停用的当前用户启动项 |
| `toggle_system_startup` | 是 | 通过 UAC 启动 `lens_helper.exe` 启停一个系统级（HKLM）启动项 |
| `set_service_start` | 是 | 通过 UAC 修改第三方 Win32 服务的启动类型（自动/手动/禁用） |

`toggle_startup` 参数：

```json
{ "key": "Run", "name": "OneDrive", "enable": false }
```

`toggle_startups` 参数：

```json
{
  "changes": [
    { "key": "Run", "name": "OneDrive", "enable": false },
    { "key": "Run", "name": "Steam", "enable": true }
  ]
}
```

`restore_all` 不需要参数。`toggle_system_startup` 参数与 `toggle_startup` 相同。

## 系统级启动项（HKLM）

- 宿主工具本身不提权；点击“停用/启用”时才通过 `Start-Process -Verb RunAs` 弹出 UAC。
- 独立助手 `lens_helper.exe` 只接受 `%LOCALAPPDATA%\SystemLens\elevated\req-<nonce>.json`，
  把结果写入同目录 `res-<nonce>.json`；请求/结果文件名必须与 nonce 匹配，目录以外的路径直接拒绝。
- 助手只支持 `system_startup_set` 一种操作，只在
  `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run|RunOnce` 与
  `HKLM\SOFTWARE\SystemLens\DisabledStartup` 之间移动值，不执行任何命令。
- 用户取消 UAC 时返回 `ELEVATION_CANCELLED`，不修改任何内容。
- 助手退出后，工具会重新读取 HKLM 启动项确认状态，再做一次回读校验。

## 启动项启停机制

- 只处理 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 与 `RunOnce`
- 停用：读取原命令 → 写入 `HKCU\Software\SystemLens\DisabledStartup`（值名 `Run::<名称>`）→ 删除 Run 值
- 启用：从备份读回命令 → 写回 Run → 删除备份值
- 删除失败时自动回滚刚写入的备份，避免“两处都存在/都不存在”的不一致
- 每个启动项写入后直接回读注册表校验；不一致标记为“失败”，不影响其余项
- 每次执行生成快照：`%LOCALAPPDATA%\SystemLens\software_pm\startup-<时间戳>-<进程号>.json`

## 服务启动类型

- 只列出第三方 Win32 服务（`ImagePath` 不在 `\Windows\`），当前值来自
  `HKLM\SYSTEM\CurrentControlSet\Services\<名称>\Start`。
- 可选值：2 自动、3 手动、4 禁用；写入前要求 `expectCurrent` 与当前值一致，避免并发修改被覆盖。
- 修改由 `lens_helper.exe` 的 `service_set_start` 操作完成，逐次 UAC；只改 Start 值，不停止正在运行的服务，重启后完全生效。
- 写入后回读校验，不一致会返回错误，不会假装成功。

## 安全机制

- 令牌绑定“当前用户启动项集合（键、名称、命令、启用状态）”；集合变化后旧令牌失效
- 参数只接受 `Run`/`RunOnce` 与合法值名；值名不能包含 `::`
- 服务、计划任务、通知与防火墙规则保持只读；系统级启动项只能经独立 helper 处理
- 操作写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`（只记录键、名称、启用与否，不记录命令）

## 回滚

- 停用后可在同一界面点击“启用（还原）”，或点击“恢复全部停用项”一次还原
- 若工具不可用，可手动把 `HKCU\Software\SystemLens\DisabledStartup` 中对应值写回 Run 键
- 系统级停用项保存在 `HKLM\SOFTWARE\SystemLens\DisabledStartup`，由助手按原命令还原
- 执行结果写入操作日志与 `software_pm/startup-*.json` 快照，可核对原命令与结果

## 数据来源

- 注册表 `HKCU\...\Run`、`RunOnce` 与 `Software\SystemLens\DisabledStartup`
- `Win32_StartupCommand`、`Win32_Service`、`Get-ScheduledTask`、通知设置、`Get-NetFirewallRule`
  （以上为只读视图）

## 构建与测试

```powershell
cargo build --release --manifest-path tools/software_pm/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/software_pm/Cargo.toml
cargo test --manifest-path tools/lens_helper/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `software_pm.exe` 放在同一目录。

## 限制

- 服务与计划任务只读查看；服务启动类型可改，计划任务修改尚未实现。
- 系统级启动项与服务启动类型需要用户逐次授权 UAC。
- 防火墙仅统计已启用规则数量，不列出单条明细。
- 表格按类别截断：启动项 40、自动服务 60、计划任务 40、通知 20。
- 部分 WMI/防火墙查询在受限权限或精简系统上可能失败，失败原因写入“说明”区。
