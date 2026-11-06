# master_pdf · PDF 大师

独立 PDF 工具：查看信息、合并、按页拆分与旋转。所有写操作输出新文件，不修改输入。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
master_pdf.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `info` | 否 | 读取页数、PDF 版本、大小、加密状态与标题 |
| `preview` | 否 | 校验合并/拆分/旋转计划并签发确认令牌 |
| `apply` | 是 | 校验令牌后写出新 PDF |

### 参数

合并：

```json
{
  "mode": "merge",
  "inputs": ["C:\\Users\\me\\a.pdf", "C:\\Users\\me\\b.pdf"],
  "output": "C:\\Users\\me\\merged.pdf",
  "overwrite": false
}
```

拆分（保留第 1–3、5 页）：

```json
{
  "mode": "split",
  "inputs": ["C:\\Users\\me\\a.pdf"],
  "output": "C:\\Users\\me\\part.pdf",
  "pages": "1-3,5",
  "overwrite": false
}
```

旋转（第 1、2 页顺时针 90°）：

```json
{
  "mode": "rotate",
  "inputs": ["C:\\Users\\me\\a.pdf"],
  "output": "C:\\Users\\me\\rotated.pdf",
  "pages": "1,2",
  "angle": 90,
  "overwrite": false
}
```

`info` 参数：`{ "path": "C:\\...\\a.pdf" }`。

## 安全限制

- 只接受绝对路径；拒绝 `..`、网络路径与磁盘根目录
- 禁止写入 Windows / Program Files / ProgramData 等系统目录
- 输出必须是 `.pdf`；已存在默认拒绝，需显式 `overwrite: true`
- 页码越界、角度非 90 倍数、输入非 PDF 直接拒绝
- 加密 PDF 读取失败时明确报错，不尝试破解

## 回滚

- 只写新文件，输入保持不变；默认拒绝覆盖
- 不提供撤销；审计日志记录模式、输出路径与页数

## 构建与测试

```powershell
cargo build --release --manifest-path tools/master_pdf/Cargo.toml
cargo test --manifest-path tools/master_pdf/Cargo.toml
```

测试使用 lopdf 生成样例 PDF，验证合并页数、拆分保留页码与旋转属性。

## 限制

- 不处理表单、书签、注释的跨文档合并细节（页面内容与资源保留）
- 不提供 OCR、压缩、加密或签名
- 超大 PDF 受内存限制
