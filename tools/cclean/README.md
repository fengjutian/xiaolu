# cclean · C 盘瘦身

只读预览工具：统计 C 盘常见可清理位置、回收站、更新缓存与系统托管文件。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
cclean.exe --stdio
```

宿主通过 stdin/stdout 行分隔 JSON 通信，协议见 `docs/TOOL_PACKAGE_SPEC.md`。

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读扫描并返回占用，不删除任何文件 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 构建

```powershell
cargo build --release --manifest-path tools/cclean/Cargo.toml
```

产物：`tools/cclean/target/release/cclean.exe`，发布时与 `tool.json` 一起放入 `tools-dist/cclean/`。

## 测试

```powershell
cargo test --manifest-path tools/cclean/Cargo.toml
```

## 限制

- 单目录最多 15 万文件，超出标记 `truncated`。
- 普通权限下部分系统目录只能统计到部分内容。
- 不做删除；删除能力必须走 `preview → confirmationToken → apply`。
