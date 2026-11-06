# sys_boost · 系统优化大师（候选汇总 + 一键加速）

只读汇总可优化候选，并提供可选的“一键加速”与快照撤销。加速只改视觉效果、窗口动画、透明效果与电源计划四类项，不触碰启动项、服务、计划任务、休眠与页面文件。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
sys_boost.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读扫描六类候选并返回建议，同时签发加速与撤销两枚确认令牌 |
| `boost` | 应用一键加速（需确认令牌） |
| `undo_boost` | 按最近快照撤销加速（需确认令牌） |

加速与撤销是两种不同动作，确认令牌把动作名签进 HMAC 载荷，**两枚令牌必须分别签发**，不可复用。

## 候选类别

| 候选 | 数据来源 | 风险 |
|---|---|---|
| 启动项 | HKCU/HKLM Run、RunOnce 值数量 | 需确认 |
| 自动服务 | `Win32_Service` StartMode=Auto 数量 | 需管理员 |
| 用户计划任务 | 非 Microsoft 且未禁用任务数量 | 需确认 |
| 休眠文件 | `hiberfil.sys` 大小与 HibernateEnabled | 需管理员 |
| 页面文件 | `pagefile.sys` 大小 | 高风险 |
| 用户临时文件 | `%TEMP%` 占用（最多 2 万文件） | 需确认 |

## 边界

- 扫描部分只读；加速部分只改视觉效果、窗口动画、透明效果与电源计划
- 不修改启动项、服务、计划任务、休眠或页面文件
- 不下载、不执行任何优化脚本；不做“跑分式”或“神秘一键优化”
- 页面文件与休眠标注高风险，不建议为释放空间直接关闭
- 启动项、服务等请使用 `software_pm`；其余设置修改请使用 `pc_optimizer`、`clean_master` 或系统设置逐项确认

## 构建与测试

```powershell
cargo build --release --manifest-path tools/sys_boost/Cargo.toml
cargo test --manifest-path tools/sys_boost/Cargo.toml
```

## 一键加速与撤销

`boost` 修改当前用户：

- `VisualFXSetting = 2`（视觉效果性能优先）
- `MinAnimate = "0"`（关闭窗口动画）
- `EnableTransparency = 0`（关闭透明）
- 电源计划切到“高性能”（`powercfg /setactive SCHEME_MIN`）

执行前把原值写入 `%LOCALAPPDATA%\SystemLens\sys_boost\boost-<时间戳>.json`；`undo_boost` 恢复最近快照，原本不存在的值会被删除。

不修改启动项、服务、计划任务、休眠或页面文件；这些仍由对应工具单独处理。
