# sys_clean · Windows 系统瘦身

列出当前用户已安装的 Appx 应用包；白名单内的内置应用可移除（只影响当前用户，可从 Microsoft Store 重装）。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
sys_clean.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 列出应用包，标记白名单可移除项，返回确认令牌 |
| `remove_appx` | 是 | 移除一个白名单应用包（当前用户） |
| `export_list` | 否 | 导出白名单可移除清单到本地 JSON |

`remove_appx` 参数：

```json
{ "fullName": "Microsoft.XboxApp_48.89.18001.0_x64__8wekyb3d8bbwe" }
```

## 白名单

只允许移除以下应用族（精确匹配）：

- Xbox 相关：`Microsoft.XboxApp`、`Microsoft.XboxGamingOverlay`、`Microsoft.XboxGameOverlay`、`Microsoft.XboxSpeechToTextOverlay`
- 媒体/资讯：`Microsoft.ZuneMusic`、`Microsoft.ZuneVideo`、`Microsoft.BingWeather`、`Microsoft.BingNews`
- 其他：`Microsoft.WindowsMaps`、`Microsoft.MicrosoftSolitaireCollection`、`Microsoft.SkypeApp`、`Microsoft.GetHelp`、`Microsoft.Getstarted`、`Microsoft.MicrosoftOfficeHub`、`Microsoft.OneConnect`、`Microsoft.People`、`Microsoft.WindowsFeedbackHub`、`Microsoft.YourPhone`

不在列表中的应用一律只读。

## 安全机制

- 应用包全名只允许字母、数字、`.`、`_`、`-`，拒绝引号、空格与命令分隔符。
- 执行前重新扫描，确认包仍存在且属于白名单。
- 只执行 `Remove-AppxPackage -Package <fullName>`，不处理系统组件、驱动或预置包。
- 移除后重新查询确认应用包已消失。
- 操作写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`。

## 导出清单

`export_list` 把白名单可移除应用写成 JSON：

```text
%LOCALAPPDATA%\SystemLens\sys_clean\appx-removals-<时间戳>.json
```

只包含白名单应用，不含系统关键组件；可用于审计或重装后对照。

## 边界

- 只影响当前用户；系统预置（provisioned）包不会被移除。
- 移除不可逆到“原样”，需要从 Microsoft Store 重新安装。
- 不清理应用数据目录、不修改系统服务。
- 不提供“一键移除全部”，必须逐项确认。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/sys_clean/Cargo.toml
cargo test --manifest-path tools/sys_clean/Cargo.toml
```
