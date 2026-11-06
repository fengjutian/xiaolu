# memopt_pro · 内存优化 Pro

只读内存诊断工具：读取物理内存、提交量、系统缓存、内核池与进程内存排行，给出观察建议。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
memopt_pro.exe --stdio
```

宿主通过 stdin/stdout 行分隔 JSON 通信，协议见 `docs/TOOL_PACKAGE_SPEC.md`。

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读采集并返回内存摘要与进程排行 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 数据来源

- `GlobalMemoryStatusEx`（kernel32）：物理内存、内存负载、提交上限
- `GetPerformanceInfo`（psapi）：提交量、系统缓存、内核分页/非分页池、进程/线程/句柄数
- `sysinfo`：进程工作集与提交内存排行

## 构建

```powershell
cargo build --release --manifest-path tools/memopt_pro/Cargo.toml
```

## 测试

```powershell
cargo test --manifest-path tools/memopt_pro/Cargo.toml
```

## 限制

- 只读，不强制清空工作集、不结束进程、不修改页面文件设置。
- `GetPerformanceInfo` 不可用时，仍显示物理内存与进程排行，并标注缺失原因。
- Windows“可用内存”包含待机缓存，本工具不把它等同于可立即释放的物理内存。
