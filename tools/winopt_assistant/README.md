# winopt_assistant · Windows 优化助手

只读的系统设置体检：读取常见 Windows 设置项，标注正常 / 可选 / 建议调整，并给出**手动调整**建议。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
winopt_assistant.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读读取注册表与服务状态并生成建议表 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

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

- 只读：不修改任何注册表、服务或系统设置。
- 不提供“一键优化”；所有调整需用户在“设置”中手动完成。
- 读取失败显示“未知”，不推断、不虚构默认值。
- 不创建或删除还原点、不安装组件、不下载内容。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/winopt_assistant/Cargo.toml
cargo test --manifest-path tools/winopt_assistant/Cargo.toml
```
