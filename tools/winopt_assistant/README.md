# winopt_assistant · Windows 优化助手

系统设置体检与用户级调整：读取常见 Windows 设置项，标注正常 / 可选 / 建议调整；其中 5 项 HKCU 设置可直接应用并还原，其余设置保持只读。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
winopt_assistant.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 读取注册表与服务状态，生成建议表并返回 5 项可应用设置与确认令牌 |
| `apply_settings` | 是 | 逐项写入 HKCU 设置、立即回读，并返回撤销令牌 |

`apply_settings` 参数：

```json
{ "changes": [{ "id": "transparency", "value": 0 }] }
```

可应用项：`visualFx`、`transparency`、`windowAnimate`、`storageSense`、`gameMode`。其余项（休眠、快速启动、诊断数据、系统还原、长路径、Windows 搜索）需要管理员权限，保持只读。

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

- 只写 `HKCU` 的 5 项设置；HKLM 项只读，不请求提权。
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
