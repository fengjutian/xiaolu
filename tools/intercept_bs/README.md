# intercept_bs · 安装拦截（只读检查首版）

首版只做近期安装包的签名与捆绑风险提示，不运行安装包、不联网查询信誉、不注入安装流程。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
intercept_bs.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读检查并返回表格 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 检查范围

- `%USERPROFILE%\Downloads` 与 `%USERPROFILE%\Desktop` 下最近 180 天的 `.exe` / `.msi`
- 每项读取：Authenticode 签名状态与签名者、公司/产品/版本信息、大小、修改时间

## 风险等级（离线启发式）

| 等级 | 条件 |
|---|---|
| 高 | 未签名、签名无效、哈希不匹配或不受信任 |
| 中 | 签名状态未知、签名有效但缺少签名者/公司信息 |
| 低 | 签名有效且有发布者信息（仍不能排除安装界面捆绑） |

## 边界

- 只读：不运行安装包、不修改文件、不联网查询云端信誉
- 不注入安装程序、不安装驱动、不拦截进程
- 捆绑项只能由用户在安装界面选择；提示建议“自定义安装”并拒绝附加组件

## 构建与测试

```powershell
cargo build --release --manifest-path tools/intercept_bs/Cargo.toml
cargo test --manifest-path tools/intercept_bs/Cargo.toml
```

测试覆盖未签名、签名无效、有效签名含/缺公司信息与表格行构建。
