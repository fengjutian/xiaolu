# bsod_repair · 蓝屏修复

查看最近转储文件与蓝屏/异常关机事件，并可经管理员授权切换崩溃转储模式。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
bsod_repair.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 列出 Minidump 与 30 天内蓝屏/异常关机事件，返回当前转储模式与令牌 |
| `set_dump_mode` | 是 | UAC 启动 helper，修改 `CrashControl\CrashDumpEnabled` |

`set_dump_mode` 参数：

```json
{ "mode": "small" }
```

| 模式 | 值 | 说明 |
|---|---|---|
| `none` | 0 | 不转储 |
| `complete` | 1 | 完整内存转储 |
| `kernel` | 2 | 内核内存转储 |
| `small` | 3 | 小内存转储（Minidump） |
| `automatic` | 7 | 自动内存转储（系统决定） |

## 安全机制

- 只修改 `HKLM\SYSTEM\CurrentControlSet\Control\CrashControl\CrashDumpEnabled` 一个值。
- 写入前读取原值，写入后回读确认。
- 由独立 `lens_helper.exe` 执行，逐次 UAC；取消 UAC 不修改。
- 修改记录写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`。

## 边界

- 只切换转储模式，不分析转储内容、不自动修复蓝屏原因。
- 不修改页面文件大小、不修改 `DumpFile`/`MinidumpDir` 路径。
- 完整内存转储需要足够的系统盘空间与页面文件配置。
- 模式变更通常立即生效，已存在的转储文件不会被清理。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/bsod_repair/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/bsod_repair/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `bsod_repair.exe` 放在同一目录。
