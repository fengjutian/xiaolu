# adblock_master · 广告拦截

审计 Hosts、DNS、浏览器扩展与防火墙状态；可经管理员授权写入或移除 SystemLens 管理的 Hosts 规则段。不安装 WFP/内核过滤驱动。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
adblock_master.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 审计 Hosts/DNS/扩展/防火墙，返回确认令牌与当前管理段条数 |
| `apply_hosts` | 是 | UAC 启动 helper，写入 `0.0.0.0 <域名>` 到 SystemLens 管理段 |
| `restore_hosts` | 是 | UAC 启动 helper，移除 SystemLens 管理段 |

`apply_hosts` 参数：

```json
{ "domains": ["ads.example.com", "tracker.example.com"] }
```

## 内置规则

工具箱内置 32 个常见广告/追踪域名，可在界面点“载入内置规则”填入文本框，再逐条确认写入。内置列表只是起点，不保证覆盖所有广告；写入前仍会做域名校验。

## Hosts 管理段

写入内容位于固定标记之间：

```text
# BEGIN SystemLens AdBlock
0.0.0.0 ads.example.com
# END SystemLens AdBlock
```

- 只替换标记之间的内容，其他 hosts 行原样保留；
- 首次写入前备份到 `%LOCALAPPDATA%\SystemLens\adblock\hosts.backup`；
- 写入后尝试 `ipconfig /flushdns`；
- `restore_hosts` 只移除管理段，不恢复整份备份。

## 安全机制

- 域名逐条校验：长度、标签、字符集，拒绝空格、`..`、首尾连字符；
- 单次最多 500 个域名，自动去重并转小写；
- 令牌绑定 Hosts 写操作；取消 UAC 不修改文件；
- 助手只写固定路径 `%WINDIR%\System32\drivers\etc\hosts`，只操作标记段。

## 边界

- 不做内核/WFP 过滤、不注入浏览器、不修改 DNS 服务器；
- 不内置广告域名列表，规则由用户提供；
- Hosts 拦截对使用 DoH/代理的浏览器可能无效；
- 不处理 IPv6 专用规则（统一写 `0.0.0.0`）。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/adblock_master/Cargo.toml
cargo build --release --manifest-path tools/lens_helper/Cargo.toml
cargo test --manifest-path tools/adblock_master/Cargo.toml
```

打包时把 `lens_helper.exe` 与 `adblock_master.exe` 放在同一目录。
