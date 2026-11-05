# 工具包规范：一工具一文件夹一程序

- 文档状态：规范稿，待评审
- 版本：0.1
- 日期：2026-11-05
- 适用：小鲁 System Lens 工具箱（31 项 SuperApp 工具）
- 关联：[功能需求 v2](FEATURE_REQUIREMENTS_V2.md)

## 1. 目标

把工具箱从“主应用里的一个大页面”改为**可独立开发、独立构建、独立发布的工具包**：

1. **一个工具一个文件夹**：源码、清单、资源、测试、构建产物都在同一目录。
2. **一个工具一个程序**：每个工具编译为独立可执行文件，由工具箱宿主按需启动，不共享主进程地址空间。
3. **默认只读**：没有声明写能力的工具只能预览；写操作必须逐次确认。
4. **可禁用/可移除**：删除一个工具目录不影响主应用与其他工具。
5. **可审计**：清单、版本、哈希、权限、执行日志全部可查。

## 2. 目录结构

### 2.1 仓库内（源码）

```text
tools/
  _template/                 # 新建工具时复制的模板
    tool.json
    README.md
    src/
      main.rs                # 独立二进制入口
    tests/
      fixtures/
  cclean/                    # 示例：C 盘瘦身
    tool.json
    README.md
    src/
      main.rs
      preview.rs
    tests/
      fixtures/
      preview_test.rs
  sys_clean/
  privacy_clean/
  ...
```

- 每个工具是 **独立 Cargo crate**（`[[bin]]` 名称 = 工具 id），加入根 workspace。
- 前端页面不放工具目录内；宿主通过清单决定渲染哪种通用 UI（见 5.3）。
- 资源（图标、规则、白名单）放工具目录，构建时随二进制复制。

### 2.2 安装后（运行目录）

```text
%LOCALAPPDATA%/<app>/tools/
  cclean/
    tool.json               # 安装时校验并记录哈希
    cclean.exe
    assets/
  sys_clean/
    ...
```

- 宿主只从该目录发现工具，**不接受用户指定任意路径**。
- 工具目录名必须等于 `tool.json.id`，否则拒绝加载。

## 3. 工具清单 `tool.json`

```json
{
  "schemaVersion": 1,
  "id": "cclean",
  "name": "C 盘瘦身",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "只读统计 C 盘可清理位置",
  "category": "cleanup",
  "risk": "read_only",
  "entry": "cclean.exe",
  "arguments": ["--stdio"],
  "permissions": {
    "readPaths": ["%TEMP%", "%WINDIR%\\Temp", "%SystemDrive%\\$Recycle.Bin"],
    "writePaths": [],
    "registry": [],
    "network": false,
    "elevation": "none"
  },
  "actions": [
    { "id": "preview", "label": "预览", "write": false },
    { "id": "apply", "label": "清理", "write": true, "requiresConfirmation": true }
  ],
  "limits": { "timeoutMs": 120000, "maxOutputBytes": 4194304 },
  "ui": { "kind": "table", "columns": ["label", "bytes", "category"] },
  "rollback": "none",
  "hash": "sha256:<构建时写入>",
  "signature": null
}
```

| 字段 | 说明 |
|---|---|
| `schemaVersion` | 清单结构版本，宿主按此拒绝未知结构 |
| `apiVersion` | 工具与宿主的 IPC 协议版本；不兼容则禁用并提示升级 |
| `risk` | `read_only` / `write` / `system`，决定 UI 警示与确认强度 |
| `permissions` | 能力声明，宿主据此展示权限并要求用户确认；越界访问视为违规 |
| `actions` | 允许的动作；`write=true` 必须带确认令牌 |
| `limits` | 超时与输出上限，超限宿主终止进程 |
| `ui.kind` | 通用渲染类型：`table` / `cards` / `text` / `custom`（custom 需宿主内置页面） |
| `rollback` | `none` / `recycle_bin` / `restore_point` / `custom` |
| `hash` / `signature` | 发布时写入；宿主加载前校验 |

## 4. 进程模型

```text
工具箱宿主（Tauri 主进程）
  ├─ 读取 tools/*/tool.json
  ├─ 校验 schema/apiVersion/hash/signature
  ├─ 用户点击工具 → 启动 <tool>.exe --stdio
  ├─ stdin/stdout 行分隔 JSON
  ├─ Job Object 绑定：宿主退出/超时 → 连带结束
  └─ 只解析工具输出中的结构化结果，不执行任意命令
```

规则：

1. 工具进程**只能**由宿主启动，命令行参数固定，不接受用户自定义参数。
2. 工具不得启动子进程（除系统 API 必需项，需在清单声明并在评审中说明）。
3. 工具默认无网络；需要网络的工具必须单独评审并在清单标 `network: true`。
4. 工具崩溃/超时只影响当前页面，宿主降级显示错误码与日志路径。
5. 工具输出超过 `maxOutputBytes` 立即终止并报错。
6. 提权工具由宿主通过 `runas` 启动独立进程，**一次一个动作**，结束即退出。

## 5. IPC 协议 v1

### 5.1 握手

工具启动后先发送：

```json
{"type":"hello","protocol":1,"toolId":"cclean","version":"1.0.0","actions":["preview","apply"],"elevated":false}
```

### 5.2 请求/响应

宿主：

```json
{"type":"request","id":1,"action":"preview","parameters":{},"confirmation":null}
```

工具：

```json
{"type":"progress","id":1,"percent":40,"message":"扫描用户临时文件"}
{"type":"result","id":1,"ok":true,"data":{"items":[],"totals":{}},"warnings":[]}
{"type":"error","id":1,"code":"PERMISSION_DENIED","message":"需要管理员权限","remediation":"以管理员身份重试"}
```

### 5.3 写操作确认

- 宿主先调用 `preview`，得到 `confirmationToken`（工具生成，含动作、对象摘要、过期时间）。
- 用户确认后，宿主把 token 原样回传 `apply`；工具校验 token 与本次对象一致才执行。
- token 一次性、默认 5 分钟过期；对象变化则失效。

### 5.4 错误码

`PERMISSION_DENIED` / `PATH_OUT_OF_SCOPE` / `UNSUPPORTED_ACTION` / `TIMEOUT` / `CANCELLED` / `INTERNAL` / `INVALID_INPUT`。

## 6. 权限与安全

1. **路径白名单**：工具只能访问清单 `readPaths`/`writePaths` 声明的路径；越界返回 `PATH_OUT_OF_SCOPE`。
2. **最小权限**：默认普通用户；`elevation: "per_action"` 的工具每次提权都弹 UAC，宿主不常驻管理员。
3. **写操作**：必须 `preview → confirmationToken → apply`，且 `risk != read_only`。
4. **回滚**：`rollback != none` 的工具必须在 `apply` 前记录回滚信息（回收站、恢复点或自定义日志）。
5. **供应链**：发布时写 `hash`；可选 Authenticode 签名。哈希不符、签名无效 → 宿主拒绝加载并提示。
6. **禁止项**：内核驱动、DLL 注入、跨进程写内存、静默下载执行、绕过签名。
7. **日志**：每次动作记录 id、工具、动作、参数摘要、结果、退出码、时间；不记录文件内容。

## 7. 构建与发布

### 7.1 工作区

```toml
# Cargo.toml（根）
[workspace]
members = ["src-tauri", "tools/cclean", "tools/sys_clean", "tools/privacy_clean"]
```

- 每个工具 crate 独立 `cargo test`、独立 `cargo build -p <tool>`。
- 共享逻辑放 `tools/shared/`（协议、路径校验、日志），但**不共享可变状态**。

### 7.2 打包

```text
dist-tools/
  cclean/
    tool.json
    cclean.exe
    assets/
```

- 构建脚本生成清单（写入版本、哈希、编译时间）。
- Tauri 打包时把 `dist-tools/*` 复制到应用资源的 `tools/`。
- 主应用不内置工具源码逻辑；工具箱只做发现、校验、启动、渲染。

### 7.3 版本

- `apiVersion` 与宿主协议匹配才加载；工具版本独立升级。
- 卸载/禁用：删除目录或在设置中禁用；宿主启动时重新发现。

## 8. 测试与验收

每个工具必须提供：

1. **单元测试**：核心解析/筛选/汇总逻辑，使用固定 fixtures，不依赖真实机器。
2. **协议测试**：hello/request/result/error 的 JSON 兼容；超时与取消。
3. **只读验证**：`preview` 不产生任何写操作（可用文件系统快照或访问日志验证）。
4. **路径越界测试**：`..`、绝对路径、符号链接逃逸均返回 `PATH_OUT_OF_SCOPE`。
5. **失败注入**：权限拒绝、目录不存在、输出超限、进程被杀。

宿主侧验收：

- 删除某工具目录后应用正常启动，工具箱不显示该工具。
- 清单无效/哈希不符 → 工具隐藏并记录原因。
- 工具崩溃不影响其他工具与主界面。
- 一个工具的 UI 渲染错误不影响其他工具。

## 9. 迁移计划

1. **P0 框架**：宿主发现 + 清单校验 + stdio 协议 + 通用表格渲染。
2. **P0 第一个工具**：把现有“C 盘瘦身”逻辑迁到 `tools/cclean`，主应用删除对应硬编码。
3. **P1 只读三件套**：`sys_clean`（组件/预装应用只读列表）、`privacy_clean`（候选统计）、`dll_repair`（运行库缺失检查）。
4. **P1 写操作**：`defrag`（系统 API）、`clean_master`（回收站/缓存），必须先完成确认令牌与回滚。
5. **P2**：其余工具按评审逐项接入；高风险工具（C 盘扩容、驱动安装、DLL 下载）单独评审。

## 10. 待确认

1. 工具用 Rust 独立二进制，还是允许 .NET/其他运行时（影响体积与签名）？
2. 工具 UI 是清单驱动的通用渲染，还是每个工具带自己的 HTML/前端资源？
3. 工具是否需要独立安装包/可选下载，还是随主应用分发？
4. 清单签名使用 Authenticode，还是仅 SHA-256 + 发布清单？
5. 共享 `tools/shared` 与“一程序一目录”是否冲突（共享代码编译进每个二进制）？
