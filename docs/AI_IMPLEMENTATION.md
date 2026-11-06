# AI 健康解读：实现说明与安全边界

- 状态：AI-0、AI-1 已实现；AI-2 只读工具网关与 AI-3 能力探测首版已实现
- 版本：0.55.0
- 关联文档：[AI 能力需求](AI_REQUIREMENTS.md)、[项目需求](../PROJECT_REQUIREMENTS.md)

## 1. 已实现范围

| 能力 | 实现 |
| --- | --- |
| 诊断事实模型 | `DiagnosticFact`：快照 ID、类别、严重度、值、来源、采集时间、置信度、可用性、证据引用、敏感标记 |
| 不可变快照 | 一次采集生成稳定 `snapshotId`；内存中最多保留 5 份，追问必须引用同一 ID |
| 本地规则引擎 | CPU/内存/磁盘/温度告警/SMART/驱动/崩溃/电池规则，结论自带事实引用 |
| 脱敏 | 事实构建阶段不采集敏感字段；发送载荷再做递归字段移除和路径遮罩 |
| Provider | 厂商无关 `AiProvider` trait；首版 OpenAI 兼容流式实现（DeepSeek/OpenAI/vLLM/Ollama 等） |
| 结构化输出 | 强制 JSON 协议；解析代码围栏；最多 3 个结论；无效证据引用直接丢弃 |
| 流式与取消 | SSE 增量事件、前端进度、`cancel_ai_request` 取消正在进行的请求 |
| 降级 | 模型不可用、超时、HTTP 错误或两次输出非法时返回本地规则诊断，不阻断原始页面 |
| 密钥保护 | Windows DPAPI（当前用户）加密到独立凭据文件；设置 JSON/日志/回答中不出现密钥 |
| 发送预览 | `preview_ai_payload` 返回脱敏后的完整诊断载荷、估算输入 token 和脱敏摘要 |
| 本地会话 | 回答自动保存到 app data 的 `ai-sessions.json`（最多 50 条）；支持逐条删除和全部清除，不保存原始事实和 API 密钥 |
| 会话绑定 | 历史追问按 `snapshotId` 过滤；快照变化后旧回答在界面标记为历史结果 |
| 只读检查编排（AI-2） | 模型可请求白名单只读检查；每轮最多 3 项、最多 2 轮、总上限 6 项；宿主校验参数、限制超时、顺序执行、脱敏并截断结果，再发起最终解读 |
| 多轮与原生工具调用 | 支持 OpenAI `tools` / `tool_calls` 流式协议，最多 2 轮、共 6 项只读检查；也可使用受控 `requestedChecks` JSON 计划协议（最多 2 轮） |
| 模型能力探测（AI-3） | 一次最小化探测：服务可达性、JSON 结构化输出、原生工具调用、延迟；不发送本机诊断数据 |
| AI 报告导出 | AI 页可导出独立 Markdown/JSON 报告；诊断报告页可勾选“AI 解读”，把本地 AI 会话并入 JSON/Markdown/HTML 报告 |
| 固定评测样本 | 8 个脱敏样本覆盖正常、过热、磁盘健康、内存压力、驱动异常、网络故障、数据缺失和信号冲突 |
| 工具审计 | 每次只读检查写入 `ai-tool-audit.jsonl`：请求 ID、工具 ID、耗时、成功状态、脱敏后的参数，不记录结果正文；页面可查看最近 30 条并清除 |

已支持的白名单工具：

| 工具 ID | 内容 | 隐私处理 |
| --- | --- | --- |
| `event_log.recent` | System/Application 最近 1–7 天 critical/error/warning 事件 | 只返回来源、事件 ID、级别和时间，不返回日志全文 |
| `drivers.problems` | 驱动总数、缺失/异常数量和最多 3 个问题设备名 | 不返回设备 ID |
| `network.summary` | 网关、DNS、代理、Hosts 和连接状态统计 | 不返回 IP 或远程地址 |
| `startup.summary` | 启动项名称、范围、启用状态 | 不返回注册表命令或路径 |
| `scheduled_tasks.summary` | 计划任务总数、状态和失败任务名称 | 不返回任务路径或参数 |
| `sensors.storage` | 温度、本地告警、SMART 健康、磨损/温度/错误计数 | 不返回序列号、设备实例名或原始 SMART 属性表 |
| `diagnostics.summary` | 待重启、崩溃/转储、服务异常、电池健康 | 不返回事件日志全文或转储内容 |

未实现（保留给后续里程碑）：模型能力探测、费用估算、本地模型专用适配器、AI 辅助写操作。**只读检查网关不包含任何 shell、脚本、路径、注册表或写操作入口。**

## 2. 数据流

```text
现有采集器（system / sensors / drivers / diagnostics / system_history）
        ↓
DiagnosticFact[]  ← 不含主机名、用户名、序列号、MAC/IP、命令行、完整路径
        ↓
不可变 DiagnosticSnapshot（snapshotId + 事实 + 本地规则结论 + 数据限制）
        ↓  用户点击“预览”或“解读”
build_payload → redact_payload → OpenAI 兼容 /chat/completions（流式）
        ↓  模型返回 findings 和可选的 requestedChecks
validate（证据必须存在于快照 facts）→ 白名单只读检查（每轮最多 3 个、最多 2 轮）
        ↓  工具结果脱敏/截断，再发给同一模型 → 最终 JSON
AiAnswer（含 checks、证据、限制、token 用量）
        ↓
界面：结论 / 严重度 / 置信度 / 证据 / 建议 / 限制 / 只读检查 / token 用量
```

失败路径：请求异常或两次校验失败 → `degraded: true` 的 `AiAnswer`，携带 `degradedReason` 与 `localFindings`。

## 3. 隐私默认值

默认不发送：

- 主机名、用户名、SID、机器 ID；
- 硬件序列号、设备 ID、MAC/IP；
- 完整本地路径、进程命令行；
- 事件日志全文、原始转储内容；
- API 密钥。

发送前额外执行的脱敏规则：

- 递归移除敏感键名（`hostName`、`userName`、`serial`、`macAddress`、`commandLine`、`apiKey` 等）；
- 遮罩 Windows 用户目录、任意盘符路径和 UNC 路径；
- 只发送事实摘要，不发送原始报告文件。

设置与凭据位置（Tauri 配置目录）：

- `ai-settings.json`：启用状态、Provider、HTTPS 地址、模型名、token 上限、超时；
- `ai-credentials.bin`：`SLK1` + DPAPI 密文；
- `ai-sessions.json`：最近 50 条 AI 回答与追问（应用数据目录，不含原始事实、载荷或密钥），可在页面逐条删除或全部清除；
- `ai-tool-audit.jsonl`：只读检查审计元数据（应用数据目录），不含结果正文。

### 本地规则优先

模型回答通过证据校验后，还会与本地规则结论比对：如果本地规则把某项判定为 warning/critical，而模型结论没有引用同一证据，回答的限制说明中会明确标注“请以本地规则为准”，避免模型把高风险事实解释为正常。

## 4. 配置方式

1. 打开“AI 解读 → 模型与隐私”；
2. 选择“OpenAI 兼容接口”，填写 HTTPS 服务地址（例如 `https://api.deepseek.com/v1`）、模型名和 API 密钥；
3. 保存后回到“AI 解读”，生成本机健康快照，先点“预览将发送的数据”检查载荷；
4. 点击“让 AI 解读本机健康”，可随时取消；之后可围绕同一快照追问；
5. 可选：在“模型与隐私”页点击“检测模型能力”。若原生工具调用可用，可开启“原生工具调用”；否则保持关闭，使用受控 `requestedChecks` 协议；
6. 在“AI 解读”页可将当前快照的 AI 结论导出为 Markdown/JSON 报告。

本机模型（如 Ollama）可使用 `http://127.0.0.1:11434/v1`；只有回环地址允许 HTTP，其他地址必须 HTTPS。服务需支持 `POST /chat/completions`、`stream: true` 和 JSON 输出。

## 5. 安全边界

- 模型输出是**不可信数据**：只作为 JSON 解析并校验，不能改变工具权限或系统指令；
- 提示词明确把事实数据视为数据，不允许执行命令、写注册表或生成下载地址；
- 只读检查只能使用固定工具 ID；参数按类型反序列化并白名单归一化，未知字段直接拒绝；
- 每个工具都有独立超时，每轮最多 3 项、最多 2 轮、总数不超过 6 项；不存在模型指定路径、命令行或脚本的入口；
- 工具结果在发送回模型前再次脱敏并限制大小；审计只记录元数据，不记录结果正文；
- 本地规则决定严重度，模型不得把 `critical`/`warning` 说成正常；
- 没有证据引用的结论不展示；没有把握时写入 `limitations` 并降低置信度；
- 请求只在用户显式启用并点击解析后发生，关闭 AI 后不产生任何模型网络请求。

写操作（清理、卸载、驱动安装、系统设置修改）仍完全在 AI 之外，继续走本地 UI 预览、二次确认、确认令牌、审计和回滚流程。

取消或超时会让 AI 请求立即返回；已经进入系统采集的同步只读任务可能在后台短暂完成，但其结果不会被使用、发送或写入会话。

## 6. 代码结构

| 文件 | 职责 |
| --- | --- |
| `src-tauri/src/ai/model.rs` | 事实、快照、回答、设置、流事件模型 |
| `src-tauri/src/ai/facts.rs` | 从现有采集器构建事实与快照 |
| `src-tauri/src/ai/rules.rs` | 本地规则引擎与降级结论 |
| `src-tauri/src/ai/redact.rs` | 载荷脱敏和敏感事实过滤 |
| `src-tauri/src/ai/prompt.rs` | 系统提示词、载荷与消息构建、token 估算 |
| `src-tauri/src/ai/provider.rs` | Provider trait、OpenAI 兼容流式客户端、取消句柄 |
| `src-tauri/src/ai/answer.rs` | JSON 解析、证据校验、字段归一化、检查请求归一化 |
| `src-tauri/src/ai/tools.rs` | 只读工具白名单、参数校验、超时/取消、结果脱敏与审计、原生工具 Schema |
| `src-tauri/src/ai/evaluation.rs` | 固定脱敏评测样本与证据/隐私一致性测试 |
| `src/aiReport.ts` | AI 解读 Markdown/JSON 报告导出 |
| `src-tauri/src/ai/settings.rs` | 设置与 DPAPI 凭据存储 |
| `src-tauri/src/ai/mod.rs` | 快照缓存、请求编排、Tauri 命令 |
| `src/AiPanel.tsx` / `src/aiApi.ts` | 页面、设置、预览、流式进度、追问与历史标记 |

## 7. 测试

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib ai::
npx tsc --noEmit
```

`ai::` 单元测试覆盖：脱敏字段与路径、URL 不误伤、设置范围与 HTTPS 校验、DPAPI 往返、SSE 解析、取消句柄、证据校验、结论数量上限、写操作预览强制确认、快照历史过滤、规则引擎可用性、只读工具白名单/参数拒绝/结果截断、检查请求数量上限。

固定评测样本位于 `src-tauri/src/ai/fixtures/`，覆盖正常、过热、磁盘健康、内存压力、驱动异常、网络故障、数据缺失和信号冲突；测试会校验本地规则、证据引用、载荷中无敏感字段、不可用事实不能作为证据，以及本地规则优先提示。

原生工具调用默认关闭，建议先在“模型与隐私”页执行“检测模型能力”。探测会发送两个最小请求（工具调用探测、JSON 输出探测），不包含本机快照或诊断数据。

模型服务真实调用未纳入自动化测试（需要外部凭据）；上线前应按 [AI 需求第 10 节](AI_REQUIREMENTS.md#10-可观测性与评测) 建立固定脱敏样本集并做失败降级验收。
