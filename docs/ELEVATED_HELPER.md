# 提权助手（lens_helper）设计

版本：0.1（2026-11-06）  
关联：[产品对标与验收标准](PRODUCT_PARITY_STANDARD.md)、[软件权限管理工具](../tools/software_pm/README.md)

## 1. 目标与边界

主程序与工具箱默认以普通用户运行。只有用户显式点击“系统级启停”时，才通过 UAC 启动一个独立、短生命周期的提权助手 `lens_helper.exe`。

助手不是通用提权执行器：

- 只接受白名单操作：`system_startup_set`（系统级启动项）、`service_set_start`（服务启动类型）、`task_set_enabled`（计划任务启停）与 `sensor_read`（受限传感器，只读）；
- 不执行任意命令；只调用固定系统工具：`sensor_read` 执行固定 PowerShell 只读查询，`task_set_enabled` 执行 `schtasks.exe` 固定参数；
- 只在 `HKLM` 的 `Run` / `RunOnce` 与专用停用备份之间移动注册表值，或修改服务 `Start` / 任务启用状态；
- 每次请求都需要用户逐次授权 UAC，不做常驻、不做服务。

## 2. 调用链

```text
software_pm.exe（普通权限）
  ├─ 读取并校验 HKLM 启动项
  ├─ 在 %LOCALAPPDATA%\SystemLens\elevated\ 写入 req-<nonce>.json
  ├─ Start-Process -Verb RunAs lens_helper.exe --request <req>
  │     └─ 用户确认 UAC
  ├─ 等待助手退出
  ├─ 读取 res-<nonce>.json（校验 nonce）
  └─ 重新读取 HKLM 启动项，确认状态
```

用户取消 UAC 时，`software_pm.exe` 返回 `ELEVATION_CANCELLED`，不修改任何内容。

## 3. 请求与结果

请求（`req-<nonce>.json`）：

```json
{
  "version": 1,
  "nonce": "32 位十六进制",
  "op": "system_startup_set",
  "key": "Run",
  "name": "ExampleAgent",
  "enable": false,
  "resultPath": "C:\\Users\\<user>\\AppData\\Local\\SystemLens\\elevated\\res-<nonce>.json"
}
```

结果（`res-<nonce>.json`）：

```json
{
  "ok": true,
  "nonce": "32 位十六进制",
  "op": "system_startup_set",
  "message": "已停用系统启动项",
  "at": 1793959502071
}
```

失败时 `ok=false` 并带 `error`。

服务启动类型请求额外字段：

```json
{
  "op": "service_set_start",
  "service": "VendorAgent",
  "startType": 4,
  "expectCurrent": 2
}
```

`startType` 只接受 2（自动）、3（手动）、4（禁用）；`expectCurrent` 必须与当前 `Start` 值一致，否则拒绝写入。

传感器读取请求没有业务参数；结果放在 `data`：

```json
{ "op": "sensor_read" }
```

`data` 包含 `zones` / `fans` / `probes` / `disks` / `smart` 五段，只读，不写任何键值。

计划任务请求：

```json
{ "op": "task_set_enabled", "taskPath": "\\Vendor\\Update", "enable": false }
```

`taskPath` 必须是以 `\` 开头的绝对路径，拒绝空段、`..`、引号与控制字符；执行 `schtasks.exe /Change /TN <path> /ENABLE|/DISABLE`，参数以数组传入，不经过 cmd/shell。

## 4. 校验与拒绝条件

助手在写入结果前执行以下校验；任一失败即退出，不触碰注册表：

- `--request` 必须指向 `%LOCALAPPDATA%\SystemLens\elevated\req-<nonce>.json`；
- `resultPath` 必须指向同目录 `res-<nonce>.json`，文件名必须与 nonce 一致；
- 目录使用规范化路径比较，拒绝目录跳转与符号链接逃逸；
- `version` 必须为 1；
- `op` 必须在白名单内；
- `key` 只能是 `Run` / `RunOnce`；
- `name` 非空、≤ 256 字符、不含 `::`、不含控制字符；
- `enable` 必须是布尔值。

助手不读取命令行里的业务参数；所有业务字段只来自受校验的请求文件。

## 5. 操作语义

- 停用：读取 HKLM Run 值 → 写入 `HKLM\SOFTWARE\SystemLens\DisabledStartup`（值名 `Run::<名称>`）→ 删除 Run 值。
- 启用：从备份读回命令 → 写回 Run → 删除备份值。
- 删除失败时回滚刚写入的备份，避免“两处都存在/都不存在”。
- 写入后立即回读，校验 Run 与备份的状态与目标一致。
- 命令内容只作为字符串搬运，不解析、不执行。

## 6. 威胁模型

| 风险 | 处理 |
|---|---|
| 任意进程伪造请求 | 每次仍需用户 UAC 确认；助手只做白名单操作，不执行命令 |
| 路径逃逸写文件 | 请求/结果路径必须位于固定目录且文件名匹配 nonce |
| 参数注入命令 | 不拼接命令行执行业务逻辑；注册表值只做字符串读写 |
| 重放旧请求 | 一次性 nonce + 请求/结果文件由调用方删除；操作是幂等的状态设置 |
| 助手被替换 | 发布时应签名并校验哈希；当前版本尚未实现签名校验（见待办） |
| 用户取消 UAC | 返回明确错误，不写入 |

## 7. 已知限制与待办

1. 目前已支持系统级启动项与服务启动类型；计划任务需要新增 `task_set_enabled` 操作并单独评审。
2. 助手尚未签名；发布前必须签名，并在 `software_pm` 侧校验哈希与版本。
3. 请求目录的 ACL 依赖用户配置目录默认权限；后续可增加仅当前用户可写的显式 ACL 校验。
4. 没有回滚快照文件；系统级启动项的原命令保存在 HKLM 备份键中，可人工恢复。
5. UAC 启动依赖 PowerShell `Start-Process -Verb RunAs`；后续可改为 `ShellExecuteExW`，减少一层依赖。

## 8. 验证

- 单元测试：nonce 格式、名称/键白名单、结果路径必须位于 elevated 目录。
- 非提权实测：助手返回 `ok=false` 与“拒绝访问”，不修改任何键。
- 提权实测（需人工）：停用一个系统级启动项 → 回读确认 → 再从界面启用还原。
