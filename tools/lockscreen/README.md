# lockscreen · 锁屏

首个带确认令牌的写操作工具：预览当前会话信息，确认后调用 `LockWorkStation` 锁定 Windows。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
lockscreen.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 显示当前用户、计算机名，并签发 5 分钟有效的一次性确认令牌 |
| `lock` | 是 | 校验确认令牌后调用 `LockWorkStation`，并写入本地操作日志 |

## 确认令牌

- 由预览生成，HMAC-SHA256 签名绑定动作（`lock`）与对象摘要（`workstation-lock`）。
- 密钥由宿主在启动工具进程时通过环境变量下发，前端无法伪造。
- 一次性：宿主记录已使用的令牌，重复提交会被拒绝。
- 5 分钟过期；过期或对象变化需重新预览。

## 构建

```powershell
cargo build --release --manifest-path tools/lockscreen/Cargo.toml
```

## 测试

```powershell
cargo test --manifest-path tools/lockscreen/Cargo.toml
```

## 边界

- 只调用系统 `LockWorkStation`；不实现自定义锁屏窗口、不注入 Explorer、不替换登录界面。
- 不修改任何文件、注册表或系统设置。
- 操作日志：`%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`（JSON Lines，不记录用户内容）。

## 定时锁定（界面）

锁屏页支持 1–720 分钟倒计时；倒计时结束前会重新预览取得新令牌再锁定。点“取消”或关闭页面不会锁定。倒计时只是前端计时，不写入系统计划任务。
