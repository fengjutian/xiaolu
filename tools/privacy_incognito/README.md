# privacy_incognito · 无痕模式（只读痕迹扫描）

首版只读扫描本地痕迹，明确边界、不删除任何数据。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
privacy_incognito.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读统计痕迹并返回表格 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 痕迹边界

| 类别 | 数据 |
|---|---|
| 浏览器数据 | Edge / Chrome / Firefox 默认配置目录占用与文件数 |
| 最近文件 | `%APPDATA%\Microsoft\Windows\Recent` |
| 跳转列表 | `Recent\AutomaticDestinations` |
| 剪贴板 | 当前内容类型与文本长度（**不读取内容**） |
| DNS 缓存 | `Get-DnsClientCache` 条数（回退 ipconfig 统计） |

## 隐私与安全

- 只读：不删除、不清空、不上传任何内容
- 剪贴板只报告类型与长度；图片/文件列表只报告类型
- 不读取浏览器历史条目、URL、Cookie 内容，仅统计占用
- 清除痕迹属于不可回滚写操作，本版本不提供；后续需单独设计确认与恢复方案

## 构建与测试

```powershell
cargo build --release --manifest-path tools/privacy_incognito/Cargo.toml
cargo test --manifest-path tools/privacy_incognito/Cargo.toml
```

## 限制

- 仅覆盖默认浏览器配置目录；多用户/自定义配置目录未覆盖
- 单目录最多统计 2 万个文件
- 部分目录在受限权限下可能读取不完整
