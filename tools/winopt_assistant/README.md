# winopt_assistant · Windows 优化助手

系统设置体检与调整：读取常见 Windows 设置项；5 项 HKCU 设置可直接应用并还原，6 项 HKLM 设置经 UAC 授权修改，可勾选后**整批只问一次 UAC**。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
winopt_assistant.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 读取注册表与服务状态，返回 5 项 HKCU（`winoptSettings`）、6 项 HKLM（`winoptAdminSettings`）与两套确认令牌 |
| `apply_settings` | 是 | 逐项写入 **HKCU** 设置；不接受管理员项 |
| `apply_admin_setting` | 是 | 单项写入 **HKLM** 设置（逐项一次 UAC） |
| `apply_admin_settings` | 是 | **整批**写入 HKLM 设置，整批只提权一次，逐项隔离失败 |

`apply_settings` 参数（HKCU）：

```json
{ "changes": [{ "id": "transparency", "value": 0 }] }
```

`apply_admin_settings` 参数（HKLM，整批）：

```json
{ "changes": [{ "id": "telemetry", "value": 0 }, { "id": "longPaths", "value": 1 }] }
```

可应用项：`visualFx`、`transparency`、`windowAnimate`、`storageSense`、`gameMode`（HKCU，直接写）；`telemetry`、`fastStartup`、`hibernate`、`systemRestore`、`longPaths`、`searchService`（HKLM，由 `lens_helper.exe` 的 `winopt_set` / `winopt_set_batch` 执行）。Windows 搜索服务可切自动/手动/禁用，**禁用会破坏开始菜单搜索**。

两套令牌互不通用：用户级动作认 `confirmationToken`，管理员动作认 `winoptAdminToken`，混用会被判「确认令牌与当前动作不匹配」。管理员项的取值必须落在工具自己的选项表内，任意值一律拒绝。

## 检查项

| 设置项 | 数据来源 |
|---|---|
| 视觉效果 | `HKCU\...\Explorer\VisualEffects\VisualFXSetting` |
| 透明效果 | `HKCU\...\Themes\Personalize\EnableTransparency` |
| 窗口动画 | `HKCU\Control Panel\Desktop\WindowMetrics\MinAnimate` |
| 存储感知 | `HKCU\...\StorageSense\Parameters\StoragePolicy\01` |
| 休眠 | `HKLM\SYSTEM\CurrentControlSet\Control\Power\HibernateEnabled` |
| 快速启动 | `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power\HiberbootEnabled` |
| 游戏模式 | `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` |
| 诊断数据 | `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection\AllowTelemetry` |
| 系统还原 | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore` |
| Windows 搜索 | `WSearch` 服务状态 |
| 长路径支持 | `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled` |

## 边界

- 只写 5 项 HKCU 与 6 项 HKLM 白名单设置，HKCU / HKLM 两条通道完全分开。
- HKLM 修改由独立 `lens_helper.exe` 执行，整批一次 UAC；取消 UAC 不写入。
- 每项独立写入并回读；单项失败不影响其他项。
- 不提供“全部一键优化”；用户需选择具体项。
- 读取失败显示“未知”，不推断、不虚构默认值。
- 不创建或删除还原点、不安装组件、不下载内容。
- 每次执行写入快照：`%LOCALAPPDATA%\SystemLens\winopt_assistant\apply-<时间戳>-<进程号>.json`，包含原值、新值与回读结果。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/winopt_assistant/Cargo.toml
cargo test --manifest-path tools/winopt_assistant/Cargo.toml
```
