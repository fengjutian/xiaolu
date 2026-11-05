# 传感器与温度子系统设计（sensor-helper）

- 文档状态：设计稿，待评审
- 版本：0.1
- 日期：2026-11-05
- 适用基线：小鲁 System Lens 0.12.0
- 关联文档：[完整功能规划](FULL_FEATURE_ROADMAP.md)、[项目需求](../PROJECT_REQUIREMENTS.md)

## 1. 结论摘要

1. **分三层采集，不做“一刀切”**：T0 只用 Windows 文档化接口（普通权限、无驱动）；T1 在 T0 基础上做按次管理员读取（受限 SMART/NVMe）；T2 才是可选传感器 helper + 内核驱动，**默认关闭、单独评审、不随安装包静默部署**。
2. **CPU/主板温度在 T0/T1 下大概率不可用**，这不是实现缺陷，而是 Windows 公开接口的覆盖边界。产品必须显示“不可用 + 原因 + 可选升级入口”，不能推断或伪造。
3. **T2 建议延后到独立决策**：LibreHardwareMonitorLib 为 MPL-2.0，可商用、可闭源集成，但其底层访问依赖内核驱动组件（当前第三方声明中出现 PawnIO.Modules，LGPL-2.1）；同时 WinRing0 一类驱动已被 Microsoft Defender 标记为易受攻击驱动。该决策涉及许可、签名、AV 误报、回滚四项评审，不能混在“补温度”里顺手做。
4. **本阶段真正该先落地的是产品层**：统一数据模型、SQLite 历史、曲线、阈值告警、托盘摘要。这些不依赖新驱动，能把现有 T0 数据的价值立刻释放出来。
5. 建议排期：S1（T0 收敛 + 数据模型）→ S2（历史/告警/托盘）→ S3（T1 按次提权）→ S4（T2 评审门禁）。

## 2. 现状与差距

### 2.1 现有实现

| 位置 | 现状 |
| --- | --- |
| [sensors.rs](../src-tauri/src/sensors.rs) | `collect()` 通过一次 PowerShell 调用返回 `gpu` / `thermal` / `storage` / `smart` 四段 JSON |
| GPU | 仅 `nvidia-smi.exe`（温度、风扇、功耗、核心频率），无该命令则整段为空 |
| 温度 | `MSAcpi_ThermalZoneTemperature`，多数整机/笔记本返回空或不可靠 |
| 存储健康 | `Get-PhysicalDisk` + `Get-StorageReliabilityCounter` |
| ATA SMART | `MSStorageDriver_FailurePredictStatus/Data`，需管理员且 NVMe/RAID/USB 常不暴露 |
| 前端 | [App.tsx](../src/App.tsx) 每 10 秒刷新，只看当前值，不落库、不告警 |
| 托盘 | [lib.rs](../src-tauri/src/lib.rs) 仅“显示主窗口/退出” |

### 2.2 差距清单

- 无独立 helper，无进程隔离；PowerShell 冷启动 1–3 秒，无法支撑 2 秒级采样。
- 无 AMD/Intel GPU 路径；无主板温度、无电压、无 CPU 频率实时值（仅 GPU 频率）。
- 无历史、无最大/最小值、无阈值、无通知、无托盘摘要。
- 数据模型缺“来源版本、采集时间、可用性、错误码”，不可用原因只有一句汇总文案。
- 无 SQLite，无 24 小时保留策略，无休眠/唤醒、热插拔处理。
- 无测试样本库（固定 JSON），传感器解析无法回归。

## 3. 目标与非目标

### 目标

- 用**一套数据模型**同时承载 T0/T1/T2 的数据，前端不感知来源差异。
- 每个值都带：来源、采集时间、单位、可用性、错误码。不可用必须可解释。
- 采样与 UI 解耦：后端缓存最新快照，前端按需拉取；历史由后端落库。
- 温度告警可配置、可去抖、休眠恢复不误报。
- 供应链可审计：版本、哈希、许可、来源全部可查。

### 非目标（本设计不承诺）

- 与鲁大师温度/分数一一对应。
- 默认安装内核驱动、默认常驻管理员权限。
- 风扇调速、电压调节、超频等写操作。
- 在无数据时推断温度或“估算健康度”。

## 4. 分层采集架构

```text
T0 普通用户（默认，随应用启动）
  ├─ PDH / GetSystemTimes         CPU 使用率、频率
  ├─ GlobalMemoryStatusEx         内存
  ├─ GetDiskFreeSpaceEx           卷空间
  ├─ Get-PhysicalDisk + ReliabilityCounter   磁盘健康摘要
  ├─ MSAcpi_ThermalZoneTemperature           ACPI 热区（常空）
  ├─ nvidia-smi（存在才用）        NVIDIA GPU 温度/风扇/功耗/频率
  └─ 电池 / 网络 / 进程（已有）

T1 按次提权（用户点“读取受限信息”才弹 UAC，退出即失效）
  ├─ IOCTL_STORAGE_QUERY_PROPERTY  NVMe Health Log（协议特定属性）
  ├─ SMART_RCV_DRIVE_DATA          部分 ATA SMART 页
  ├─ MSStorageDriver_*             需要管理员才可见的 SMART 页
  └─ 受限的 Storage Reliability    部分厂商标识字段

T2 可选 helper（默认关闭，独立评审后提供）
  ├─ sensor-helper.exe（.NET 8，自包含）
  ├─ LibreHardwareMonitorLib      CPU/主板/GPU/风扇/电压/功耗
  └─ 底层驱动组件（PawnIO 或等价方案）——必须用户显式同意才可安装
```

### 4.1 覆盖矩阵（预期，需实测校准）

| 数据项 | T0 | T1 | T2（LHM） |
| --- | --- | --- | --- |
| CPU 使用率/频率 | ✅ PDH | — | ✅ |
| CPU 封装温度（Intel/AMD） | ❌ 基本不可用 | ⚠️ 极少设备 | ✅ |
| 主板温度 | ❌ | ❌ | ✅（依主板 SuperIO） |
| NVIDIA GPU 温度/风扇/功耗/频率 | ✅ nvidia-smi | — | ✅ |
| AMD GPU 温度/风扇/功耗 | ❌ | ❌ | ✅ |
| Intel 核显温度 | ❌ | ❌ | ⚠️ 依平台 |
| 磁盘温度/寿命/通电时间 | ✅ 多数 NVMe/SATA | ✅ 补全 | ✅ |
| ATA SMART 原始属性 | ⚠️ 需管理员，覆盖低 | ✅ | ✅ |
| NVMe Health Log | ⚠️ 摘要 | ✅ | ✅ |
| 风扇转速（机箱/CPU） | ❌ | ❌ | ✅（依 SuperIO） |
| 电压 | ❌ | ❌ | ✅（依 SuperIO） |
| 电池健康 | ✅ 已有 | — | — |

> 表中 ✅/⚠️/❌ 是设计假设，S1 结束前必须在至少 4 台真实机器上校准并回填本表。

### 4.2 关键约束

- T0 **不得**为了拿数据而启动 PowerShell（冷启动成本 + 攻击面 + EDR 噪音）。T0 全部改为 Rust 直接调用 Win32/PDH/CIM（CIM 走 `IWbemLocator` 或保留一次批量 WMI 查询作为过渡）。
- T0 的任何失败都不能让整个快照失败：单项失败 → 该值 `availability=unavailable` + `errorCode`。
- T1 提权必须是**独立进程、单次任务、可超时**，不能把主进程提权。
- T2 的驱动组件不得静默安装、不得自动更新、不得随应用启动而自动加载。

## 5. 统一数据模型

### 5.1 值对象

```rust
pub struct SensorValue {
    pub id: String,              // 稳定 ID：{vendor}-{hardware}-{kind}，跨重启不变
    pub hardware_id: String,     // 物理设备稳定标识（非序列号，可哈希）
    pub hardware_name: String,   // 展示名
    pub category: SensorCategory,// temperature | fan | voltage | power | clock | load | other
    pub name: String,
    pub value: Option<f64>,      // 不可用时为 None，禁止填 0 冒充
    pub unit: String,
    pub source: SensorSource,    // 见 5.2
    pub captured_at_unix_ms: u64,
    pub availability: Availability, // ok | unavailable | stale | denied | unsupported
    pub error_code: Option<String>, // 稳定错误码，见 5.3
    pub quality: Quality,        // measured | derived | estimated（本期只允许前两者）
}
```

兼容策略：现有 `SensorReading` 增加**可选**字段（`hardwareId`、`capturedAtUnixMs`、`availability`、`errorCode`、`quality`）。`value` 序列化为 `null`（不是 0），前端类型需同步放宽为 `number | null`；过渡期旧组件用 `availability` 决定是否渲染数值，未提供新字段时按 `ok` 处理。

### 5.2 来源枚举

`acpi` / `nvidia-smi` / `windows-storage` / `wmi-smart` / `nvme-health-log` / `lhm` / `vendor-sdk`。
前端必须原样展示来源，不允许只显示“系统”。

### 5.3 错误码（稳定、可文档化）

| 码 | 含义 | 用户文案要点 |
| --- | --- | --- |
| `SENSOR_UNAVAILABLE` | 接口存在但无数据 | 该硬件未通过此接口公开数据 |
| `SENSOR_DENIED` | 需要管理员 | 可点击“以管理员读取” |
| `SENSOR_UNSUPPORTED` | 平台/型号不支持 | 说明具体原因 |
| `SENSOR_HELPER_MISSING` | 未安装 helper | 提供可选安装入口 |
| `SENSOR_HELPER_CRASHED` | helper 异常退出 | 展示退出码，提示重试/上报 |
| `SENSOR_DRIVER_BLOCKED` | 驱动被系统/AV 拦截 | 不得引导用户关闭安全软件 |
| `SENSOR_TIMEOUT` | 采集超时 | 可重试 |

### 5.4 快照与历史

```rust
pub struct SensorSnapshot {
    pub schema_version: u32,      // 从 1 起
    pub captured_at_unix_ms: u64,
    pub values: Vec<SensorValue>,
    pub storage: Vec<StorageHealth>,   // 保留现有结构
    pub smart_devices: Vec<SmartDevice>,
    pub limitations: Vec<Limitation>,  // {code, message, remediation}
    pub sources: Vec<SourceStatus>,    // {source, ok, lastError, lastSuccessAt}
}
```

## 6. helper 进程设计

### 6.1 形态与打包

- 独立可执行文件 `sensor-helper.exe`，.NET 8 **自包含**（不依赖用户预装 .NET；代价约 70–110 MB）。
- 不用 trimming/AOT：LHM 依赖 WMI 与反射，裁剪风险大于收益；体积问题用安装包压缩与按需下载缓解。
- 通过 Tauri `externalBin` 作为 sidecar 放置，路径固定为应用资源目录，**不接受用户指定路径**。
- 发布时对 helper 与全部随包 DLL 做 Authenticode 签名，并记录 SHA-256 清单。

### 6.2 生命周期

```text
应用启动
  └─ T2 已启用? ──否──> 不启动 helper
        │是
        ├─ 校验 helper 哈希/签名 → 失败则禁用并记录错误
        ├─ 启动子进程（Job Object 绑定，禁继承句柄）
        ├─ 握手：hello/protocol 版本/能力集
        ├─ 周期采样（默认 2000ms，前台 1000ms，后台 5000ms）
        ├─ 主进程崩溃 → Job Object 连带结束 helper
        └─ 应用退出 → shutdown → 等待 2s → TerminateJobObject
```

- 单实例：命名互斥体，避免多开抢驱动句柄。
- 看门狗：连续 3 次采样超时（>5s）→ 重启 helper；5 分钟内 3 次重启 → 熔断，降级为 T0 并提示。
- 休眠/唤醒：监听 `WM_POWERBROADCAST`，唤醒后丢弃旧样本并重新握手。

### 6.3 传输

**stdio 行分隔 JSON**（每行一个对象，UTF-8，单行 ≤ 256 KiB）。

- 理由：无监听端口、无命名管道 ACL、无网络暴露面；父子进程天然绑定生命周期。
- 不做请求队列：helper 只接受 3 条命令 `hello` / `sample` / `shutdown`，拒绝任何参数化命令。
- 主进程读取必须带超时与行长度上限，防止 helper 被替换后无限输出。

### 6.4 IPC 协议 v1

握手（helper → 主进程，启动后立即发送）：

```json
{"type":"hello","protocol":1,"helperVersion":"1.0.0","lib":"LibreHardwareMonitorLib 0.9.x","capabilities":["cpu","gpu","motherboard","storage","fan","voltage","power","clock"],"driver":{"name":"PawnIO","version":"x.y.z","state":"ready|blocked|absent"}}
```

采样请求/响应：

```json
{"type":"sample","requestId":42,"intervalMs":2000}
{"type":"snapshot","requestId":42,"capturedAtUnixMs":1762345678901,"values":[{"id":"cpu.package.temp","category":"temperature","name":"CPU 封装","value":61.5,"unit":"°C","source":"lhm","quality":"measured"}],"errors":[{"source":"gpu.amd","code":"SENSOR_UNAVAILABLE","message":"..."}]}
```

规则：

- `protocol` 不匹配 → 主进程拒绝使用并提示升级（不允许“尽力解析”）。
- 未知字段忽略，未知 `type` 返回 `{"type":"error","code":"BAD_REQUEST"}`。
- 单条消息超限、非法 JSON、非 UTF-8 → helper 终止并记录。
- 所有时间戳为 Unix 毫秒，数值为有限浮点（`NaN/Inf` 拒绝）。

### 6.5 崩溃隔离

- helper 崩溃只影响 `sources[lhm]`，T0 数据继续可用。
- 崩溃转储：默认不收集；用户显式同意后只收集退出码、最后一条消息、版本号，不含硬件标识。

## 7. 权限模型

| 层 | 触发 | 令牌 | 持续 | 失败表现 |
| --- | --- | --- | --- | --- |
| T0 | 应用启动 | 普通用户 | 常驻 | 值不可用 + 原因 |
| T1 | 用户点击“读取受限信息” | 提权 helper（`runas`） | 单次任务，结束即退出 | `SENSOR_DENIED`，可重试 |
| T2 | 用户在设置中显式开启 | 普通用户 + 已安装驱动 | 常驻（可随时关） | `SENSOR_DRIVER_BLOCKED` / `MISSING` |

规则：

1. 主进程永不提权；任何提权都是独立进程、固定参数、单次任务。
2. UAC 文案必须说明“读取磁盘健康/SMART，不修改系统”，并给出取消路径。
3. T2 开启流程必须列出：将安装的驱动名称、发布者、版本、来源链接、卸载方式、已知风险（AV 误报/系统拦截）。
4. 不提供“始终以管理员运行”的快捷方式；不写计划任务、不注册服务。
5. 所有提权操作写审计日志：时间、操作 ID、目标设备（哈希）、结果、退出码。日志本地保存 30 天，可一键清空。

## 8. 许可与供应链审查

### 8.1 LibreHardwareMonitorLib

- 许可：**MPL-2.0**（文件级 copyleft）。可闭源集成，但：
  - 分发时必须附带 MPL-2.0 文本与版权声明；
  - 对 MPL 覆盖文件的修改必须以 MPL-2.0 公开；
  - 不得使用其名称/图标暗示官方背书。
- 依赖：需在锁定版本上生成完整依赖清单（NuGet 传递依赖），逐项核对许可。
- 版本锁定：`Directory.Packages.props` 固定版本 + `packages.lock.json`；升级必须走 PR 并重跑测试矩阵。

### 8.2 底层驱动组件（关键风险）

- LHM 的 CPU/主板传感器依赖 Ring0 访问。当前上游第三方声明中出现 **PawnIO.Modules（LGPL-2.1）**，说明底层已从 WinRing0 系方案迁移；具体随包形式（是否捆绑驱动安装器）必须按锁定版本核实，不能凭印象。
- **WinRing0 已被 Microsoft Defender 归类为易受攻击驱动**（[Microsoft 支持页](https://support.microsoft.com/zh-cn/windows/security/threat-malware-protection/microsoft-defender-antivirus-alert-vulnerabledriver-winnt-winring0)），大量监控工具因此被拦截。任何依赖此类驱动的方案都必须按“可能被 AV/系统阻止”设计，而不是假设可用。
- 若采用 LGPL-2.1 驱动组件：
  - 动态链接 + 提供替换方式，满足 LGPL 要求；
  - 提供对应源码获取途径；
  - 驱动签名、WHQL/attestation、分发合规单独评审。
- **红线**：不绕过驱动签名、不引导用户关闭 Defender/内存完整性、不静默安装驱动。

### 8.3 供应链要求

- 所有第三方二进制记录：名称、版本、来源 URL、SHA-256、许可、用途，落盘到 `docs/third-party/`。
- 构建可复现：固定 SDK 版本、锁定 NuGet、记录 MSBuild 参数。
- 发布前扫描：`dotnet list package --vulnerable`、二进制签名校验、依赖许可清单核对。

## 9. 数据与隐私

- 传感器数据**不出本机**；无遥测、无云端对比。
- 历史只存数值、单位、来源、时间与稳定硬件 ID（哈希后），不存序列号、MAC、路径。
- SQLite 保留策略：原始样本 24 小时（可配 1–7 天），小时聚合 30 天；超限自动清理。
- 导出报告默认脱敏；温度历史默认不写入报告，需用户勾选。
- 数据库位于 `%LOCALAPPDATA%\<app>\sensors.db`，普通用户可读写，不使用全局共享位置。

### 9.1 SQLite 草案

```sql
CREATE TABLE sensor_samples (
  captured_at_ms INTEGER NOT NULL,
  sensor_id     TEXT    NOT NULL,
  value         REAL,
  unit          TEXT    NOT NULL,
  source        TEXT    NOT NULL,
  availability  TEXT    NOT NULL,
  PRIMARY KEY (captured_at_ms, sensor_id)
) WITHOUT ROWID;

CREATE TABLE sensor_hourly (
  hour_ms    INTEGER NOT NULL,
  sensor_id  TEXT    NOT NULL,
  min_value  REAL, max_value REAL, avg_value REAL,
  samples    INTEGER NOT NULL,
  PRIMARY KEY (hour_ms, sensor_id)
) WITHOUT ROWID;

CREATE TABLE alert_state (
  sensor_id   TEXT PRIMARY KEY,
  state       TEXT NOT NULL,        -- normal | warning | critical
  changed_at  INTEGER NOT NULL,
  last_notified_at INTEGER
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

写入策略：批量事务（每 5 秒或 100 行）、`PRAGMA journal_mode=WAL`、`synchronous=NORMAL`；启动时做一次 `integrity_check` 失败则重建并保留旧库备份。

## 10. 告警与通知

- 阈值：每类传感器可配（默认 CPU 85/95°C、GPU 80/90°C、磁盘 55/65°C），支持全局静音与免打扰时段。
- 去抖：连续 3 个样本越限才触发；回到阈值以下 10°C 才恢复（滞回）。
- 通知：Windows Toast（AppUserModelID 已注册）；同一传感器 10 分钟内最多 1 条。
- 休眠恢复：丢弃跨越休眠区间的样本，不补告警；启动后 60 秒内不告警（避免冷启动误报）。
- 托盘摘要：悬停显示 CPU/GPU/最高温度；点击打开主窗口；可在设置中关闭。

## 11. 测试矩阵

### 11.1 功能与边界

- 每类传感器的 `ok/unavailable/denied/unsupported/stale` 五态渲染。
- 无 NVIDIA、无电池、无 SMART、无温度的设备上不崩、不空白。
- helper 缺失/哈希不符/版本不符/中途崩溃/输出超长/输出非 JSON。
- 提权被拒绝、提权超时、提权后设备消失。
- 休眠/唤醒、设备热插拔、驱动被 AV 拦截。

### 11.2 硬件矩阵

| 维度 | 覆盖 |
| --- | --- |
| CPU | Intel 12/13/14 代、AMD Ryzen 5000/7000 |
| GPU | NVIDIA RTX、AMD RX、Intel 核显 |
| 存储 | SATA HDD、SATA SSD、NVMe、USB 存储、RAID/虚拟盘 |
| 形态 | 台式机、笔记本、虚拟机、Windows To Go |
| 系统 | Windows 10 22H2、Windows 11 当前支持版本（含开启内存完整性） |

### 11.3 交叉验证

- 温度与 HWiNFO64 / 厂商工具对比（仅内部验证，不随包分发），典型偏差目标 ≤ 3°C；超出必须记录原因，不做平均掩盖。
- SMART 与 `smartctl` 输出对比固定样本。
- 磁盘健康与 Windows“优化驱动器/存储设置”对比。

### 11.4 稳定性与性能

- 连续 8 小时采集：内存增长 < 20 MB，句柄数不持续增长。
- helper 常驻 CPU < 0.5%（空闲）、内存 < 120 MB。
- 采样 2 秒时，主进程 UI 帧率无明显下降（采样不在 UI 线程）。

### 11.5 自动化

- Rust：数据模型校验、错误码映射、SQLite 迁移/保留策略。
- helper：固定 JSON 样本回放（不依赖真实硬件）、协议模糊测试。
- 前端：空数据、错误、加载、告警去抖的组件测试（当前项目尚无前端测试框架，需先引入 Vitest + Testing Library）。

## 12. 分阶段交付

### S1：T0 收敛与数据模型（约 1 周，无新依赖）

- `sensors.rs` 拆为 `sensor/mod.rs`、`sensor/t0_*.rs`；去掉 T0 的 PowerShell 依赖。
- 引入 `SensorValue`/`Snapshot` 新模型，旧字段保留兼容。
- 校准覆盖矩阵，回填第 4 节表格。
- 验收：4 台机器上无 PowerShell 也能出 T0 数据；不可用项全部带错误码。

### S2：历史、曲线、告警、托盘（约 1–2 周）

- 引入 SQLite（`rusqlite`，bundled），迁移脚本与保留策略。
- 前端：温度历史曲线、最大/最小值、阈值设置页、告警横幅。
- 托盘：摘要与快捷入口。
- 验收：24 小时数据可回溯；休眠恢复不误报；8 小时稳定性通过。

### S3：T1 按次提权（约 1 周）

- 独立提权 helper（固定参数），读取 NVMe Health Log 与受限 SMART。
- 审计日志与失败降级。
- 验收：拒绝 UAC 后功能可用性不下降；提权进程退出后无残留。

### S4：T2 评审门禁（先评审，后实现）

评审必须回答：

1. 是否接受随包分发内核驱动组件？若否，T2 终止，温度止步于 T1。
2. 若接受：驱动来源、签名方式、许可义务、卸载/回滚方案、AV 误报处置。
3. 若接受：默认关闭的产品文案与首次开启流程。

只有评审通过才进入实现；实现预估 2–3 周（含签名、许可材料、矩阵测试）。

## 13. 决策记录（待确认）

| ID | 决策 | 建议 | 备选 |
| --- | --- | --- | --- |
| D1 | 是否引入内核驱动 | 本阶段不引入，T2 单独立项 | 直接集成 LHM |
| D2 | helper 通信 | stdio 行 JSON | 命名管道 / 本地 HTTP |
| D3 | helper 运行时 | .NET 8 自包含 | 框架依赖 + 引导安装 |
| D4 | 是否复用 LHM | 是（MPL-2.0 可闭源集成），但仅限 T2 | 自研 Ring0（不建议） |
| D5 | 采样频率 | 前台 1s / 默认 2s / 后台 5s | 固定 2s |
| D6 | 历史保留 | 原始 24h + 小时聚合 30 天 | 全部 7 天 |

## 14. 风险登记

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 驱动被 Defender/内存完整性拦截 | T2 大面积不可用 | 按不可用设计；不引导关闭安全功能；提供 T0 降级 |
| MPL/LGPL 合规遗漏 | 法务与再分发风险 | 锁定版本 + 许可清单 + 源码提供 + 发布前评审 |
| helper 体积过大 | 安装包膨胀 | 按需下载 + 签名校验；或框架依赖方案（需评估） |
| PowerShell 依赖被 EDR 拦截 | T0 现有功能失效 | S1 移除 T0 的 PowerShell 路径 |
| 传感器读数偏差 | 用户误判 | 交叉验证 + 显示来源 + 不生成“健康百分比” |
| 主板 SuperIO 兼容性长尾 | 覆盖率不及预期 | 明确支持列表；不支持时展示原因，不承诺全覆盖 |

## 15. 附录

### 15.1 现有命令与类型映射

| 现有 | 变更 |
| --- | --- |
| `get_sensor_snapshot` | 返回结构升级（`values` + `limitations` 对象化），保留旧字段一个版本 |
| `SensorSnapshot.readings` | 逐步由 `values` 取代，前端双读过渡 |
| `SensorReading` | 增加可选 `hardwareId/availability/errorCode/quality` |

### 15.2 参考链接

- LibreHardwareMonitor（MPL-2.0）：<https://github.com/LibreHardwareMonitor/LibreHardwareMonitor>
- LibreHardwareMonitorLib NuGet：<https://www.nuget.org/packages/LibreHardwareMonitorLib/>
- Microsoft Defender 对 WinRing0 的说明：<https://support.microsoft.com/zh-cn/windows/security/threat-malware-protection/microsoft-defender-antivirus-alert-vulnerabledriver-winnt-winring0>
- Windows 存储可靠性计数器：<https://learn.microsoft.com/windows/win32/api/winioctl/ns-winioctl-storage_reliability_counter>
- NVMe Health Log（IOCTL_STORAGE_QUERY_PROPERTY）：<https://learn.microsoft.com/windows/win32/api/winioctl/ni-winioctl-ioctl_storage_query_property>
- PDH 性能计数器：<https://learn.microsoft.com/windows/win32/perfctrs/using-the-pdh-functions-to-consume-counter-data>

### 15.3 待核实清单

- [ ] 锁定 LHM 版本的实际依赖树与随包驱动组件（PawnIO 的引入方式、是否需单独安装）。
- [ ] 该版本在开启“内存完整性”的 Windows 11 上的实际可用性。
- [ ] AMD ADLX / Intel IGCL 的许可是否允许随包分发（若要走厂商 SDK 路线）。
- [ ] .NET 8 自包含 helper 在 Windows 10 22H2 的最小依赖与体积实测。
- [ ] Toast 通知在未安装开始菜单快捷方式时的 AppUserModelID 注册方式。
