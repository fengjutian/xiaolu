# sys_boost · 系统优化大师（只读候选汇总）

首版只读汇总可优化候选，不提供“一键优化”，不修改任何系统设置。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
sys_boost.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读扫描六类候选并返回建议 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

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

- 只读：不修改启动项、服务、计划任务、休眠或页面文件
- 不下载、不执行任何优化脚本；不做“跑分式”或“神秘一键优化”
- 页面文件与休眠标注高风险，不建议为释放空间直接关闭
- 实际修改请使用 `software_pm`、`pc_optimizer`、`clean_master` 或系统设置逐项确认

## 构建与测试

```powershell
cargo build --release --manifest-path tools/sys_boost/Cargo.toml
cargo test --manifest-path tools/sys_boost/Cargo.toml
```
