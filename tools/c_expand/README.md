# c_expand · C 盘瘦身与扩容

分析卷容量、右侧相邻未分配空间与扩容可行性；可经管理员授权把 NTFS 卷向右扩展到相邻未分配空间。不做缩小、移动、新建或删除分区。

> **高风险**：分区操作可能导致数据丢失或系统无法启动。执行前必须备份重要数据、确认电源稳定并保留恢复介质。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
c_expand.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 分析卷、右侧未分配空间与可行性，签发确认令牌 |
| `expand_volume` | 是 | UAC 启动 helper，把 NTFS 卷向右扩展到相邻未分配空间 |

`expand_volume` 参数：

```json
{ "drive": "C", "targetBytes": 549755813888 }
```

## 判定规则

| 状态 | 条件 |
|---|---|
| 不可直接扩展 | 右侧没有相邻未分配空间 |
| 空间过小 | 右侧未分配不足 1 GB |
| 需先转换 GPT | MBR 且可用+未分配超过 2 TB |
| 可评估扩展 | 右侧存在 ≥1 GB 未分配空间 |

## 执行约束

- 只支持 **NTFS** 卷。
- 只允许 **向右扩展** 到同一磁盘紧邻的未分配空间；目标必须大于当前容量且不超过“当前容量 + 右侧未分配”。
- 助手执行前重新校验卷、分区、相邻空间与目标；BitLocker 保护开启时拒绝执行。
- 只调用 `Resize-Partition`；不移动数据、不跨盘、不缩小、不格式化、不新建/删除分区。
- 执行后重新读取卷容量，确认达到目标。
- 记录到 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`。

## 边界

- 不处理动态磁盘、存储空间、RAID 或跨盘扩展；这些需要第三方工具。
- 失败可能造成“部分完成”，必须到磁盘管理中确认状态；必要时运行 `chkdsk`。
- 系统/引导卷扩容风险最高；工具只提示，不代替备份与恢复介质。
- 不下载分区软件、不修改引导配置。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/c_expand/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/c_expand/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `c_expand.exe` 放在同一目录。
