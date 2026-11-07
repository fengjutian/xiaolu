# lens_helper · 提权助手

独立提权进程：宿主自己不提权，需要写系统级设置时由工具把请求交给这个助手，助手以管理员身份执行后把结果写回请求文件。

## 运行方式

只接受一种调用方式：请求文件路径。

```text
lens_helper.exe --request "<请求文件路径>"
```

请求文件必须满足：

- 位于 `%LOCALAPPDATA%\SystemLens\elevated\` 下，文件名必须是 `req-<32 位十六进制 nonce>.json`；
- `resultPath` 必须位于同一目录；
- `version` 必须是 `1`。

不满足时助手直接退出（退出码 2），不执行任何动作。

## 支持的操作

| op | 用途 | 调用方 |
|---|---|---|
| `system_startup_set` | 启停 HKLM Run/RunOnce 启动项 | software_pm |
| `service_set_start` | 修改服务启动类型 | software_pm |
| `task_set_enabled` | 启停计划任务 | software_pm |
| `pm_restore` | 批量恢复服务与计划任务 | software_pm |
| `sensor_read` | 读取受限传感器（风扇/温度/SMART） | ai_cooling |
| `defrag_volume` | 调用系统碎片整理 / TRIM | defrag |
| `hosts_apply` / `hosts_restore` | 写入/移除 Hosts 管理段 | adblock_master |
| `crash_dump_set` | 切换崩溃转储模式 | bsod_repair |
| `winopt_set` | 修改单项 HKLM 系统设置 | winopt_assistant |
| `winopt_set_batch` | 批量修改 HKLM 系统设置（整批一次提权，逐项隔离失败） | winopt_assistant |
| `expand_volume` | NTFS 卷向右扩展 | c_expand |
| `restore_point_create` | 创建系统还原点 | sys_clean |
| `power_limit_set` | 限制处理器最大状态 | ai_cooling |
| `power_plan_activate` | 切换当前使用的电源计划 | pc_optimizer |
| `power_settings_write` | 批量写入电源计划的 AC/DC 取值 | pc_optimizer |

## 安全约束

- 操作名走固定白名单，未知 op 直接拒绝。
- 每个 op 的入参单独校验：服务名、计划任务路径、注册表键名、Hosts 域名、卷名、电源计划 GUID、`powercfg` 别名等，校验不通过不执行。
- `powercfg` 的别名只有 9 个固定值（`SUB_PROCESSOR`/`SUB_VIDEO`/`SUB_SLEEP`/`SUB_DISK`/`PROCTHROTTLEMIN`/`PROCTHROTTLEMAX`/`VIDEOIDLE`/`STANDBYIDLE`/`DISKIDLE`），不会把外部字符串拼进命令行。
- 需要执行外部命令时，统一回传 ASCII 错误码 `RC=<code>` 再翻译成中文原因，避免系统控制台编码导致乱码。
- 拉起助手的 PowerShell 窗口使用 `CREATE_NO_WINDOW`，不闪黑框。

## 结果文件

助手把结果写成一行 JSON：

```json
{ "ok": true, "nonce": "...", "op": "...", "message": "...", "data": { }, "at": 0 }
```

失败时 `ok` 为 `false`，`error` 是原因文本。
