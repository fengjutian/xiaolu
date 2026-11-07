# cclean · C 盘瘦身

C 盘视图的清理工具：与「清理大师」共用 `tools/cleaner`（package `clean-core`）里的清理引擎 —— 唯一白名单、唯一扫描器、唯一回收站删除实现。因此两个工具对同一位置给出的占用/文件数**必然一致**（由构造保证，而不是靠两边对齐）。

与「清理大师」的差异只在展示层：

- cclean 额外给出 C 盘容量（总量/可用）、分类占用（可安全清理 / 需确认 / 系统托管）；
- cclean 额外列出系统托管的大文件（`pagefile.sys`、`hiberfil.sys`、`swapfile.sys`），只统计不清理。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
cclean.exe --stdio
```

宿主通过 stdin/stdout 行分隔 JSON 通信，协议见 `docs/TOOL_PACKAGE_SPEC.md`。本工具已改用标准协议入口（`tool_protocol::serve_with_request`），不再手写握手与错误帧。

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 扫描固定位置 + C 盘容量 + 系统托管文件，返回占用、文件数与确认令牌 |
| `apply` | 是 | 校验令牌后把所选位置的文件移入回收站，逐文件记录结果并写入本地清单 |

`apply` 参数：`{ "candidateIds": ["user-temp", "crash-dumps"] }`。

## 清理位置（与清理大师同一份白名单）

| ID | 位置 | 管理员 | 可清理 |
|---|---|---|---|
| `user-temp` | `%TEMP%` | 否 | 是 |
| `windows-temp` | `%WINDIR%\Temp` | 是 | 是 |
| `thumbnail-cache` | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*`、`iconcache_*` | 否 | 是 |
| `crash-dumps` | `%LOCALAPPDATA%\CrashDumps` | 否 | 是 |
| `wer-user` | `%LOCALAPPDATA%\Microsoft\Windows\WER` | 否 | 是 |
| `wer-machine` | `%ProgramData%\Microsoft\Windows\WER` | 是 | 是 |
| `update-download` | `%WINDIR%\SoftwareDistribution\Download` | 是 | 是 |
| `delivery-optimization` | `%ProgramData%\...\DeliveryOptimization\Cache` | 是 | 是 |
| `edge-cache` | `%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\Cache` | 否 | 是 |
| `inet-cache` | `%LOCALAPPDATA%\Microsoft\Windows\INetCache` | 否 | 否（旧版网络缓存，清空会让旧程序重新登录） |
| `recycle-bin` | `%SystemDrive%\$Recycle.Bin` | 是 | 否（内容已是删除过的文件，需在回收站手动清空） |

工具不接受用户传入任意路径；参数只接受上表中的 ID。标为「否」的位置只统计，`apply` 会直接拒绝。

## 安全机制

与清理大师完全同源（都实现在 `tools/cleaner`）：

- **回收站回滚**：`SHFileOperationW` + `FOF_ALLOWUNDO`，可还原到原位置；
- **占用检测**：预览每位置抽样最多 200 个文件，执行时逐个检测占用/只读并跳过；
- **重解析点防护**：目录联接/符号链接不跟随，避免逃出白名单目录；
- **失败隔离**：每 256 个文件一批移入回收站，整批失败时逐文件重试；
- **限额**：单位置最多统计 2.5 万个文件，单次最多移入回收站 1 万个；
- **确认令牌**：绑定工具 id + 位置集合，一次性、5 分钟过期；
- **审计日志与清单**：写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl` 与 `%LOCALAPPDATA%\SystemLens\cleanup\cclean-<时间戳>-<进程号>.json`；
- **清理前后对比**：清理结果带 `diff` 字段（按位置的清理前后占用/文件数与释放量），与清理大师的对比结果完全一致。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/cclean/Cargo.toml
cargo test --manifest-path tools/cclean/Cargo.toml
```

仓库统一用共享构建目录，产物在 `.cargo-target/tools/release/cclean.exe`；`npm run toolbox:build` 会把它连同 `tool.json` 一起放进 `tools-dist/cclean/`。
