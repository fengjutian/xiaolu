# omni_zip · Windows 压缩工具

独立 ZIP 工具：压缩、解压与查看列表。包含路径校验、Zip-Slip 防护、符号链接不跟随与覆盖保护。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
omni_zip.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 校验压缩/解压计划、统计文件数与大小，签发确认令牌 |
| `apply` | 是 | 校验令牌后执行压缩或解压 |
| `list` | 否 | 只读查看压缩包目录（最多 500 条） |

### 参数

压缩：

```json
{
  "mode": "compress",
  "sources": ["C:\\Users\\me\\Documents\\report.docx"],
  "archive": "C:\\Users\\me\\Desktop\\report.zip",
  "overwrite": false
}
```

解压：

```json
{
  "mode": "extract",
  "archive": "C:\\Users\\me\\Desktop\\report.zip",
  "destination": "C:\\Users\\me\\Desktop\\report",
  "overwrite": false
}
```

列表：`{ "archive": "C:\\...\\report.zip" }`。

## 安全限制

- 只接受绝对路径；拒绝 `..`、网络路径与磁盘根目录
- 禁止向 `C:\Windows`、`Program Files`、`ProgramData` 等系统目录写入
- 压缩目标必须是 `.zip`；已存在默认拒绝，需显式 `overwrite: true`
- 解压使用 `enclosed_name()` 做 Zip-Slip 校验，拒绝跳出目标目录的条目
- 不跟随符号链接；单条 ≤2 GiB、总量 ≤8 GiB、文件数 ≤2 万
- 默认跳过已存在文件；不提供删除/粉碎功能

## 回滚

- 压缩只新增目标 zip，不修改源文件
- 解压默认跳过已存在文件；覆盖需显式勾选
- 工具不提供撤销；审计日志记录目标、模式与结果

## 构建与测试

```powershell
cargo build --release --manifest-path tools/omni_zip/Cargo.toml
cargo test --manifest-path tools/omni_zip/Cargo.toml
```

测试包含真实压缩/解压往返（在 crate 的 `target/test-tmp` 中进行）。

## 限制

- 不支持分卷、加密、7z/RAR 或自解压
- 不支持网络路径与跨盘相对引用
- 大目录受文件数与总大小上限限制
