# DuplicateFileClean · 重复文件清理

扫描下载、文档、桌面三个目录中的重复文件，选择每组要保留的文件，把其余文件移入回收站。删除前用完整内容哈希复核，避免仅凭快速指纹误删。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
DuplicateFileClean.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 扫描 ≥ 1 MiB 的文件，按大小 + 首尾 64 KiB 指纹分组，返回确认令牌 |
| `clean_group` | 是 | 校验令牌与完整哈希后，把所选组中除保留项外的文件移入回收站 |

`clean_group` 参数：

```json
{ "groupId": "4749890858e59026", "keepPath": "C:\\Users\\me\\Downloads\\a.bin" }
```

## 安全机制

- **完整哈希复核**：扫描阶段只用首尾片段快速分组；执行时对保留文件与每个待删文件做完整 FNV-1a，哈希不一致直接跳过并说明原因。
- **服务端重扫**：`clean_group` 不信任前端路径，会重新扫描、按 `groupId` 找回组，并确认 `keepPath` 属于该组。
- **令牌绑定**：令牌绑定当前扫描到的组集合；文件变化后旧令牌失效。
- **回收站**：使用 `SHFileOperationW` + `FOF_ALLOWUNDO`，可在回收站按原路径还原。
- **逐文件结果**：每个文件返回“已移入回收站 / 跳过：内容不一致 / 失败”，单项失败不影响其他项。
- **范围**：只处理 `%USERPROFILE%\Downloads`、`Documents`、`Desktop`；不跟随符号链接；单文件至少 1 MiB。
- **清单**：`%LOCALAPPDATA%\SystemLens\duplicates\clean-<时间戳>-<进程号>.json`，含组信息与逐文件结果。

## 边界

- 不做“保留最新/最旧”自动策略，保留项由用户逐组选择。
- 不删除空目录，不修改注册表、服务或启动项。
- 扫描最多 2 万个文件、输出最多 50 组；超出会标记截断。
- 指纹分组是启发式，可能有误报；完整哈希复核会阻止误删，但会让执行阶段变慢。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/DuplicateFileClean/Cargo.toml
cargo test --manifest-path tools/DuplicateFileClean/Cargo.toml
```
