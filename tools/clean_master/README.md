# clean_master · 清理大师

首个带“回收站回滚”的清理工具：扫描固定白名单位置，用户勾选后把文件移入回收站，可从回收站还原。执行阶段会为每个文件留痕，并写入本地完整清单。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
clean_master.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 扫描 9 个固定位置，返回占用、文件数、占用抽样与确认令牌 |
| `apply` | 是 | 校验令牌后把所选位置的文件移入回收站，逐文件记录结果并写入本地清单 |

`apply` 参数：`{ "candidateIds": ["user-temp", "crash-dumps"] }`。

## 固定清理位置

| ID | 位置 | 管理员 |
|---|---|---|
| `user-temp` | `%TEMP%` | 否 |
| `windows-temp` | `%WINDIR%\Temp` | 是 |
| `thumbnail-cache` | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*`、`iconcache_*` | 否 |
| `crash-dumps` | `%LOCALAPPDATA%\CrashDumps` | 否 |
| `wer-user` | `%LOCALAPPDATA%\Microsoft\Windows\WER` | 否 |
| `wer-machine` | `%ProgramData%\Microsoft\Windows\WER` | 是 |
| `update-download` | `%WINDIR%\SoftwareDistribution\Download` | 是 |
| `delivery-optimization` | `%ProgramData%\...\DeliveryOptimization\Cache` | 是 |
| `edge-cache` | `%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\Cache` | 否 |

工具不接受用户传入任意路径；参数只接受上表中的 ID。

## 安全机制

- **回收站回滚**：`SHFileOperationW` + `FOF_ALLOWUNDO`，文件进入回收站，可还原到原位置。
- **占用检测**：预览对每个位置抽样最多 200 个文件；执行时逐个尝试写句柄，被占用或只读的文件跳过。
- **失败隔离**：每 256 个文件一批移入回收站；整批失败时自动逐文件重试，单个坏文件不会中断其他文件。
- **路径白名单**：位置来自环境变量，符号链接不跟随，只处理文件、不删除目录本身。
- **限额**：单位置最多统计 2.5 万个文件；单次最多移入回收站 1 万个文件。
- **确认令牌**：令牌绑定候选位置集合；位置集合变化后旧令牌失效；一次性、5 分钟过期。
- **审计日志**：`%LOCALAPPDATA%\SystemLens\tool-actions.jsonl` 记录释放字节、文件数、跳过数、失败数与清单路径。
- **完整清单**：每次执行写入 `%LOCALAPPDATA%\SystemLens\cleanup\clean-<时间戳>-<进程号>.json`，包含每个文件的原路径、大小、结果与失败原因。

## 执行结果与还原

`apply` 返回逐文件明细表（最多 500 行），列为：

| 列 | 说明 |
|---|---|
| 位置 | 候选位置名称（如“用户临时文件”） |
| 文件 | 文件完整路径 |
| 大小 | 文件字节数 |
| 结果 | 已移入回收站 / 跳过：占用/只读/无权限 / 跳过：已不存在 / 跳过：超出单次上限 / 失败 |
| 原因 | 跳过或失败的具体原因 |

汇总徽章包含释放空间、移入回收站文件数、跳过占用/只读、失败、超出上限，以及执行前后扫描的“复核减少”。完整明细以本地清单为准；还原时打开回收站，按原路径还原即可。

## 边界

- 不修改注册表、服务、计划任务或启动项。
- 不删除目录本身；不提供“粉碎/安全擦除”。
- 系统级位置在普通权限下会失败或被跳过，失败原因会写入结果。
- 正在运行的浏览器缓存通常被占用，建议关闭浏览器后清理。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/clean_master/Cargo.toml
cargo test --manifest-path tools/clean_master/Cargo.toml
```
