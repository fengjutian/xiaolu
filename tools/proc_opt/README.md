# proc_opt · 电脑智能防卡顿

列出内存/CPU 占用较高的进程，支持逐项调整优先级或结束进程；系统关键进程与本工具自身受保护。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
proc_opt.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 列出高占用进程，为每个进程签发独立的结束/优先级令牌 |
| `set_priority` | 是 | 调整一个进程的优先级：空闲/低于正常/正常/高于正常/高 |
| `end_process` | 是 | 结束一个进程（不可撤销） |

参数：

```json
{ "pid": 1234, "name": "app.exe", "startTime": 456789, "priority": "below_normal" }
```

`end_process` 不需要 `priority`。

## 安全机制

- **逐进程令牌**：每个进程单独签发 `endToken` / `priorityToken`，令牌绑定 `PID + 名称 + 启动时间`，PID 被复用后旧令牌失效。
- **系统关键进程保护**：PID ≤ 4、本工具自身、`System`/`Registry`/`smss/csrss/wininit/winlogon/services/lsass/svchost` 禁止操作；结束前再调用 `IsProcessCritical` 复核。
- **回读校验**：优先级写入后回读确认；结束进程后探测进程是否仍存在。
- **审计**：结束/优先级变更写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`。
- **权限错误**：拒绝访问时返回明确错误，不静默失败。

## 优先级说明

| 值 | 含义 |
|---|---|
| `idle` | 空闲 |
| `below_normal` | 低于正常 |
| `normal` | 正常 |
| `above_normal` | 高于正常 |
| `high` | 高 |

不提供 `realtime`，避免抢占系统调度。

## 边界

- 优先级只影响本次运行，进程重启后恢复默认。
- 结束进程不会保存数据，可能丢失未保存内容。
- 不注入进程、不修改其他进程内存、不结束系统关键进程。
- 高权限进程（如以管理员运行的程序）在普通权限下会返回拒绝访问。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/proc_opt/Cargo.toml
cargo test --manifest-path tools/proc_opt/Cargo.toml
```
