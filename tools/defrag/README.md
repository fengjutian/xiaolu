# defrag · 磁盘碎片整理

查看固定卷、物理磁盘介质与 TRIM 状态；可经管理员授权执行 HDD 优化或 SSD TRIM。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
defrag.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 枚举固定卷、介质与 TRIM 状态，签发确认令牌 |
| `optimize_disk` | 是 | UAC 启动 `lens_helper.exe`，调用系统 `defrag.exe` 执行分析/优化/TRIM |

`optimize_disk` 参数：

```json
{ "drive": "C", "mode": "analyze" }
```

`mode` 取值：

| 值 | 含义 | 限制 |
|---|---|---|
| `analyze` | `defrag C: /A` | 任意固定卷 |
| `optimize` | `defrag C: /O` | 仅 HDD |
| `trim` | `defrag C: /L` | 仅 SSD |

## 安全机制

- 令牌绑定卷列表与介质类型；卷/介质变化后旧令牌失效。
- 执行由独立 `lens_helper.exe` 完成，逐次 UAC；取消 UAC 不执行。
- 盘符只接受单个字母，模式只接受 `analyze/optimize/trim`。
- SSD 禁止传统碎片整理，只允许分析或 TRIM。
- 命令由系统 `defrag.exe` 执行，参数以数组传入，不经过 cmd/shell。
- 执行记录写入 `%LOCALAPPDATA%\SystemLens\tool-actions.jsonl`。

## 边界

- 不显示碎片率明细（`defrag /A` 的输出受系统语言影响）。
- 执行耗时从数分钟到数小时不等，工具会等待命令退出。
- 不处理动态磁盘、存储空间或非固定卷。
- 不创建恢复点；整理是原地操作，不会删除用户文件。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/defrag/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/defrag/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `defrag.exe` 放在同一目录。
