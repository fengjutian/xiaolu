# adblock_master · 广告拦截（只读审计首版）

首版只做 Hosts / DNS / 浏览器扩展 / 防火墙状态审计，不安装过滤驱动、不注入浏览器。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
adblock_master.exe --stdio
```

## 支持的动作

| 动作 | 说明 |
|---|---|
| `preview` | 只读审计并返回表格 |
| `apply` | 未实现，返回 `UNSUPPORTED_ACTION` |

## 检查内容

| 项目 | 数据来源 |
|---|---|
| Hosts 文件 | `%WINDIR%\System32\drivers\etc\hosts`（阻止/本机/重定向/重复/无效分类） |
| DNS 服务器 | `Get-DnsClientServerAddress`（IPv4，去重） |
| 浏览器扩展 | Edge / Chrome 扩展目录数量、Firefox 配置目录数量 |
| Windows 防火墙 | `Get-NetFirewallProfile` 启用配置文件数量 |

## 边界

- 只读：不修改 hosts、不新增防火墙/WFP 规则、不安装内核过滤驱动
- 不注入浏览器、不读取浏览历史或扩展内容
- 修改 hosts 前请备份；本工具不提供自动写入 hosts

## 构建与测试

```powershell
cargo build --release --manifest-path tools/adblock_master/Cargo.toml
cargo test --manifest-path tools/adblock_master/Cargo.toml
```

测试覆盖 hosts 分类解析、重复域名与空文件。
