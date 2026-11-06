# privacy_protection · 隐私保护

检查诊断数据级别、广告 ID、位置/相机/麦克风权限；其中 4 项当前用户设置可直接应用并撤销，诊断数据策略保持只读。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
privacy_protection.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 读取诊断数据策略与 4 项 HKCU 隐私设置，返回确认令牌 |
| `apply_settings` | 是 | 逐项写入 HKCU、立即回读，并返回撤销令牌 |

`apply_settings` 参数：

```json
{ "changes": [{ "id": "advertising", "value": "0" }] }
```

可应用项：

| ID | 设置 | 取值 |
|---|---|---|
| `advertising` | 广告 ID | `0` 关闭 / `1` 启用 |
| `location` | 位置权限 | `Allow` / `Deny` |
| `camera` | 相机权限 | `Allow` / `Deny` |
| `microphone` | 麦克风权限 | `Allow` / `Deny` |

## 安全机制

- 只写 `HKCU` 的 4 项设置；**诊断数据级别位于 HKLM 策略，不修改**。
- 每项独立写入并回读；单项失败不影响其他项。
- 结果附带新的一次性令牌与 `privacyUndo`，可一键撤销。
- 快照：`%LOCALAPPDATA%\SystemLens\privacy_protection\apply-<时间戳>-<进程号>.json`，含原值、新值与回读结果。
- 权限值大小写敏感（`Allow` / `Deny`），非法值直接拒绝。

## 边界

- 不修改组策略、不修改 HKLM、不调用系统设置界面。
- 位置/相机/麦克风是“当前用户全局开关”；应用级授权仍需在系统设置中逐项调整。
- `Allow` 只代表系统允许应用请求，不代表权限已被使用。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/privacy_protection/Cargo.toml
cargo test --manifest-path tools/privacy_protection/Cargo.toml
```
