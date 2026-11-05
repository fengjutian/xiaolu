# ludashi_minisetup.exe REA 分析与 Tauri 应用方案

> 文档状态：初始静态分析基线  
> 分析日期：2026-11-05  
> 目标文件：`ludashi_minisetup.exe`  
> 目标：基于可观察行为和独立需求，设计一款原创的 Windows 系统信息与健康检查应用；不复制原产品代码、商标、文案或视觉资源。

## 1. 结论摘要

当前样本是一个 **32 位 Windows 安装器/引导器**，不是足以代表鲁大师完整功能的主程序。REA 的静态证据确认它具备安装器常见的进程启动、文件操作、注册表、服务管理、网络下载、压缩包处理和签名检查能力，并包含大量压缩或高熵载荷。

因此，现阶段可以确定“安装器如何组织和启动安装”的部分边界，但不能据此声称已经恢复鲁大师的硬件检测、温度监控、性能测试、驱动管理或清理逻辑。Tauri 项目的第一版应采用独立产品定义；完整功能映射必须等待安装载荷提取或受控运行观察后再更新。

## 2. 分析范围与安全边界

本轮分析仅执行 REA 的只读、非运行式检查：

- 未启动 `ludashi_minisetup.exe`；
- 未安装鲁大师；
- 未连接其网络服务；
- 未提取或复用其 PNG、图标、文案、协议文本或其他私有资源；
- 未对系统注册表、服务或文件做安装器行为测试；
- 未进行 Ghidra、Hopper、IDA 等深度反编译，因为当前环境未配置这些原生分析引擎。

## 3. 样本身份

| 属性 | 观察结果 |
| --- | --- |
| 路径 | `D:\github\xiaolu\ludashi_minisetup.exe` |
| 大小 | 225,181,344 字节 |
| SHA-256 | `ee9c5cdbcc9451f09633f5a9e438a3114245d767604e6728e27beefa36aff875` |
| 格式 | PE32 |
| 架构 | x86 / i386 / 32 位 |
| 文件版本 | `6.1026.4845.1020` |
| 产品版本 | `6.1026.4845.1020` |
| 导出模块名 | `Install.exe` |

Windows 版本资源中的版本号来自系统只读元数据查询；其余核心身份由 REA 对样本字节直接观察得到。

## 4. REA 静态证据

### 4.1 Authenticode 证书表

REA 找到一个 PKCS#7 证书项，签名者证书主体为 `成都奇鲁科技有限公司`，证书有效期字段为 2024-05-21 至 2027-05-20，链中包含 DigiCert 代码签名 CA。

这只证明样本携带上述证书结构。当前检查 **没有验证** 签名、证书链、时间戳或 Authenticode 摘要是否有效，因此不能把它写成“签名已验证”或“文件可信”。

证据：`ev_f0e650a1406b8c00999c4a570f89c9973eed08b5930d52a545d21df35c94206b`

### 4.2 导入能力面

REA 完整读取了 20 个 DLL、486 个普通导入、0 个延迟导入。关键能力如下：

| 能力 | 代表性导入 | 可支持的有限结论 |
| --- | --- | --- |
| 进程管理 | `CreateProcessW`、`OpenProcess`、`TerminateProcess`、Toolhelp API | 安装器具备创建、枚举及操作进程的静态能力 |
| 跨进程内存 | `VirtualAllocEx`、`ReadProcessMemory`、`WriteProcessMemory` | 存在跨进程内存操作能力；静态导入不能证明实际调用路径或用途 |
| 文件与目录 | `CreateFileW`、`CopyFileW`、`MoveFileExW`、`DeleteFileW`、`CreateDirectoryW` | 具备安装文件落盘、移动和删除所需能力 |
| 注册表 | `RegOpenKeyExW`、`RegSetValueExW`、`RegDeleteKeyW` | 具备读取、写入和删除注册表项的能力 |
| Windows 服务 | `OpenSCManagerW`、`CreateServiceW`、`StartServiceW`、`DeleteService` | 具备创建、启动、修改和删除服务的能力 |
| 下载 | `URLDownloadToFileW`、`URLDownloadToCacheFileW`、`InternetGetConnectedState` | 具备联网检测及文件下载能力；没有观察实际 URL 或运行时请求 |
| 压缩/安装包 | `SetupIterateCabinetW`、Cabinet API、`SHFileOperationW` | 具备处理 Cabinet/安装文件的能力 |
| 签名检查 | `WinVerifyTrust`、`WTHelperProvDataFromStateData` | 具备调用 Windows 信任验证 API 的能力 |
| 加密 | CryptoAPI、`CryptGenRandom`、`CryptEncrypt`、`CryptDecrypt` | 具备基础加密和随机数能力；用途未知 |
| 图形界面 | USER32、GDI32、GDI+ | 安装器包含原生窗口、绘图与 PNG 加载能力 |

证据：`ev_2b8232c79469ac896c5b49c288ed82ad7d6a9c1b7ae236fab904bb169ee9713d`

### 4.3 资源结构

REA 完整读取了 65 个资源、146 个资源目录项。主要资源类型：

| 类型 | 数量 | 说明 |
| --- | ---: | --- |
| `PNG` | 39 | 安装器界面位图资源；不得直接复用 |
| 图标资源 `RT_3` | 9 | 图标图像 |
| `LAYER_ZIP` | 3 | 分层压缩载荷 |
| `DLL_7Z` | 1 | 内嵌 7-Zip 相关组件 |
| `DLL_NETBRIDGE` | 1 | 名称指向网络桥接组件；具体行为尚未反编译 |
| `EXE_SIGNEX` | 1 | 名称指向签名相关可执行载荷；具体行为未知 |
| `EDGE_INST` | 1 | 名称暗示 Edge 相关安装载荷；不能仅凭名称断言其行为 |
| `SETUPDATA` / `SETUPCONFIG` | 各 1 | 安装数据与配置资源 |
| `CHECK_SIGN` | 1 | 名称暗示签名检查资源或模块 |
| `LICENCE` | 1 | 许可文本资源；不复用其内容 |

较大的内嵌资源包括：`DLL_7Z` 655,133 字节，三个 `LAYER_ZIP` 分别为 302,358、578,825 和 533,008 字节，`EXE_SIGNEX` 323,001 字节，`EDGE_INST` 225,740 字节，`DLL_NETBRIDGE` 105,907 字节。

资源名称能帮助确定后续探针，但不能单独证明对应代码被执行。

证据：REA `inspect_pe_resources` 对同一 SHA-256 样本的完整观察。

### 4.4 导出、TLS 与加载保护

导出表只有两个导出：

- `_Start@12`，地址 `0x44e390`；
- `StartEast`，地址 `0x44e460`。

TLS 目录存在，但 REA 没有发现非空 TLS 回调。加载配置声明了安全 Cookie、SafeSEH 表（1,231 项）以及 `IMAGE_GUARD_CF_INSTRUMENTED`。CFG 位是链接器声明，当前检查没有验证实际指令或运行时强制执行情况。

证据：

- 导出：`ev_39ae6fd580423a72aab0efd8dc7576f41c47bb99f954b762da0b295442054025`
- TLS/加载配置：`ev_8d82a2ec3c2bef8b0bba24b036af550644378d2ace54dfdd85b810542e52756f`

### 4.5 压缩或加密载荷特征

REA 测得全文件熵为 `7.9994` bits/byte。PE 前部若干代码块熵约为 5.4–6.7，而从约 1.7 MiB 后大量区块接近 8.0。这与“大型压缩或加密载荷附着在安装器中”的解释一致，但熵本身不能区分压缩、加密或随机数据。

REA 的通用制品图只识别到一个 PE 根节点，并明确限制为“只覆盖根文件字节，未检查子容器条目”。因此当前还没有完整的安装载荷清单。

证据：

- 熵：`ev_6e30e9dd9e353702b9124e06338741d3beae83039cebec48d7c9ad03c08aa471`
- 制品图：`ev_cab3c3e96cfc2cacd2e4bc09e2259877f43cd02e53cf68d6c78888fe28868f5e`
- 根清单：`ev_21089a8492e1365e0cd54458b79df5b296e957f31595a6ec8da11f0b8c952db3`

## 5. 已确认、推断与未知项

### 已确认

- 样本是 32 位 PE 安装器，带有成都奇鲁科技有限公司的代码签名证书结构；
- 样本静态导入覆盖安装、下载、服务、注册表、进程和 UI 能力；
- 样本包含多个压缩/辅助模块资源以及大量 PNG；
- 文件主体的大部分区域为高熵数据；
- 当前分析未运行目标文件。

### 合理推断

- 安装器可能先校验环境或载荷，再解压本地组件或从网络下载补充组件；
- 安装器可能管理 Windows 服务和注册表项；
- 大型高熵区域很可能保存压缩安装载荷。

这些都是基于静态能力和资源命名的推断，不能当作已观察的运行行为。

### 仍未知

- 安装后主程序、驱动和服务的完整文件清单及哈希；
- 实际下载地址、请求参数、更新协议与遥测行为；
- 硬件检测、温度监控、性能测试和清理功能的真实算法；
- 服务、驱动、计划任务及开机启动项的具体名称和生命周期；
- 安装器运行时到底调用哪些导入；
- 内嵌载荷的容器格式、解压密码或完整目录结构；
- UI 页面、用户流程和错误处理的运行时表现。

## 6. 独立 Tauri 产品定义

### 6.1 产品定位

项目暂定名为 **System Lens**。它是一款本地优先、透明可审计的 Windows 系统信息和健康检查工具，不使用鲁大师名称、标识、文案或素材，也不尝试兼容其私有协议。

### 6.2 第一阶段 MVP

第一阶段只实现可以通过 Windows 公共接口独立开发并验证的功能：

1. 系统概览：Windows 版本、设备名称、启动时间、CPU、内存、磁盘和 GPU 基本信息；
2. 实时监控：CPU、内存、磁盘占用及基础进程列表；
3. 存储健康：磁盘容量、文件系统、可用空间和 SMART 可用性说明；
4. 启动项查看：只读列出常见注册表启动项和启动目录；
5. 诊断报告：导出不含敏感标识的 JSON/Markdown 报告；
6. 权限透明：默认普通用户权限，需要管理员权限的操作必须单独解释并确认。

第一阶段不实现驱动安装、自动修复、注册表清理、进程注入、静默下载或后台常驻服务。

### 6.3 后续候选功能

以下功能必须经过进一步证据和风险评估后再决定：

- 温度与风扇传感器读取；
- 可复现的 CPU、内存、磁盘和 GPU 基准测试；
- 驱动状态查看；
- 大文件与磁盘空间分析；
- 系统服务和启动项的受控管理。

## 7. Tauri 技术架构

建议采用 Tauri 2 + Rust + React + TypeScript。前端负责展示和交互，所有系统读取都通过窄化的 Tauri command 调用 Rust 服务。

```text
React UI
  ├─ Dashboard
  ├─ Hardware
  ├─ Performance
  ├─ Storage
  ├─ Startup
  └─ Reports
        │ typed commands/events
Tauri IPC boundary
        │
Rust application services
  ├─ system_info
  ├─ metrics_sampler
  ├─ storage_probe
  ├─ process_reader
  ├─ startup_reader
  └─ report_exporter
        │
Windows adapters
  ├─ Win32 / Registry
  ├─ WMI or CIM where justified
  ├─ Performance counters
  └─ storage/device APIs
```

### 7.1 建议目录

```text
src/
  app/
  pages/
  components/
  features/
  lib/tauri.ts
  types/
src-tauri/
  src/
    commands/
    domain/
    services/
    platform/windows/
    report/
    error.rs
    lib.rs
  capabilities/
tests/
docs/
```

### 7.2 IPC 数据契约

Rust command 不返回松散 JSON；前后端共享稳定 DTO：

- `SystemSummary`
- `CpuInfo` / `MemoryInfo` / `GpuInfo`
- `MetricSample`
- `DiskInfo`
- `ProcessSummary`
- `StartupEntry`
- `DiagnosticReport`
- `AppError { code, message, remediation }`

实时指标使用有界采样频率和事件推送；窗口不可见时降低采样频率。任何可能改变系统状态的 command 与只读 command 分离，并默认不实现。

## 8. 安全、隐私与合规要求

- 默认离线运行，不上传硬件标识、进程列表或报告；
- 报告导出前对用户名、主机名、序列号、MAC、路径等字段做可见的脱敏选择；
- Tauri capability 采用最小权限，不开放任意 shell、任意文件系统或任意 URL；
- 不执行下载后程序，不实现跨进程内存写入；
- 不复制样本中的证书、PNG、图标、许可文本或二进制模块；
- 每个系统修改功能都要有变更预览、显式确认、结果回读和可恢复方案；
- 基准测试必须显示时长、负载、温度风险和取消入口。

## 9. 实施阶段与验收条件

### 阶段 A：证据补全

- 在隔离环境中提取安装载荷，记录每个文件的路径、大小、哈希和签名；
- 对主程序、服务和驱动分别建立 REA Evidence；
- 如需运行观察，使用快照虚拟机，记录文件、注册表、服务、进程和网络差异；
- 把每个产品功能映射到静态或运行时证据，未知项保持未知。

验收：形成可追溯的“功能—证据—独立实现决策”矩阵。

### 阶段 B：Tauri 工程骨架

- 初始化 Tauri 2、React、TypeScript 和 Rust 工程；
- 建立 lint、format、unit test 和 Windows CI；
- 完成 IPC 错误模型、capability 白名单和模拟数据层。

验收：开发构建、生产构建和测试在干净环境通过。

### 阶段 C：只读 MVP

- 实现系统概览、实时指标、存储、进程、启动项和报告导出；
- 为每个 Windows adapter 提供单元测试或可重复集成测试；
- UI 显示数据来源、更新时间、权限限制和不可用原因。

验收：普通用户权限下可稳定运行，无后台服务、无静默联网、无系统修改。

### 阶段 D：对照验证

- 仅比较公开可观察的功能结果和用户流程；
- 对数值差异定义容差、采样窗口和测试设备；
- 不以像素级复制或私有资源复用作为验收标准。

验收：核心场景有可重复测试记录，差异和未知项均有说明。

## 10. 下一步分析计划

1. 重启 Codex，使已注册的 REA MCP 工具进入活动会话；
2. 调用 `binary_session` 确认当前可用工具和原生 provider；
3. 优先识别样本是否包含可提取容器或 overlay，并以新目录保存派生样本；
4. 对派生 PE 分别执行签名、导入、资源、导出和托管/原生分类；
5. 在用户明确授权后，才在隔离虚拟机中执行安装器并记录运行时行为；
6. 更新本文档中的未知项和 MVP 决策，然后开始初始化 Tauri 工程。

## 11. 当前决策

当前可以进入 Tauri 工程骨架设计，但不应宣称已经“复刻鲁大师”。在获取主程序和运行时证据之前，开发目标限定为原创、只读、本地优先的 Windows 系统信息与健康检查 MVP。

## 12. 安装目录追加分析

> 追加分析目标：`D:\github\xiaolu\鲁大师\ludashi`  
> 分析方式：目录只读盘点，以及 REA 文件级 PE 导入表和证书表检查；未启动目录中的任何程序。

### 12.1 安装规模

安装目录共发现 810 个文件，总计 692,461,222 字节，其中包括：

| 类型 | 数量 |
| --- | ---: |
| EXE | 105 |
| DLL | 160 |
| SYS 内核驱动 | 10 |
| JSON | 26 |
| INI | 6 |

目录划分表明产品并非单一可执行文件，而是由主界面、硬件检测、驱动管理、性能测试、温度监控、清理、软件管理、更新、托盘、服务和多个独立工具组成的模块化 Windows 应用。

REA 的目录检查另外观察了 898 个目录条目、探测了 265 个文件并读取 86,420 字节的选定头部范围，覆盖状态为 `complete-within-scan`。目录快照摘要 SHA-256 为 `275bf8b434215d33a90ae0fde8f6ec5382abbea150815cf83af3c5667566fb56`。这个摘要只承诺路径、元数据和被读取的范围，不代表 692 MB 全部文件内容的完整哈希。

目录检查没有把该应用识别为 Electron、Unity 等特定运行时，也没有产生自动组件候选；这与目录中大量原生 PE32/PE32+ 文件的观察一致，但不能证明所有模块都是原生代码。完整 Evidence 保存在 `analysis/ludashi-desktop-evidence.json`。

REA Evidence：`ev_548982bfb4d8ed6f56c688403a267e2ebb2c9a50adbe4f836ab2d31f1824724d`

### 12.2 功能模块地图

以下映射以安装目录实体为依据。文件名和目录名可以证明模块存在，但仅有名称时不能证明其完整运行行为。

| 功能域 | 关键实体 | 证据强度 |
| --- | --- | --- |
| 主界面与托盘 | `ComputerZTray.exe`、`ComputerZTinyTray.exe`、`ComputerZ_CN.exe` | 文件存在；主托盘的导入和签名已由 REA 检查 |
| 硬件检测 | `ComputerZ_HardwareDll.dll`、`Hardware/`、`ComputerZ7*.dll`、`ComputerZ12*.dll` | 文件存在；核心硬件 DLL 的系统 API 能力已由 REA 确认 |
| 硬件监控 | `ComputerMonZ.dll`、`ComputerZMonHelper.exe`、`TemperatureHistory.dll` | 文件存在；温度历史 DLL 的导入和签名已由 REA 检查 |
| 后台服务 | `ComputerZService.exe`、`ComputerzService_x64.exe` | 文件存在；32 位服务程序的导入和签名已由 REA 检查 |
| 内核驱动 | `ComputerZ*.sys`、`HardwareProtect*.sys`、`LudashiProtect*.sys`、`ProtectFilter*.sys` | 文件存在；本轮未深度分析驱动行为 |
| 性能测试 | `nBench/BenchmarkLauncher*.exe`、`BenchmarkSuite.dll`、`DiskBenchmark.dll`、`AIBenchmark.dll`、GPU 测试组件 | 文件存在；启动器的导入和签名已由 REA 检查 |
| 驱动管理 | `DrvMgr/`、`SuperApp/driver_manager/` | 文件存在；具体扫描、下载和安装流程未知 |
| 垃圾清理 | `sweeper/CleanEngine.dll`、`SuperApp/sys_clean/`、`SuperApp/cclean/`、`SuperApp/clean_master/` | 文件存在；核心清理 DLL 的删除能力已由 REA 确认 |
| 软件管理 | `softmgr/`、`SuperApp/software_pm/` | 文件存在；运行流程未知 |
| 更新 | `LiveUpdate360.exe`、`ModuleUpdate.exe`、`update.exe`、`independent_update.exe` | 文件存在；未观察实际更新地址或协议 |
| Web UI 基础 | `Utils/cef69/libcef.dll`、`CefView.exe`、`WebView.dll` | 文件存在；说明至少部分界面可能使用 CEF，但未观察运行时页面 |
| 可选增强工具 | `SuperApp/` 下的系统加速、隐私清理、广告拦截、DLL 修复等 | 文件存在；不自动纳入独立产品 MVP |

### 12.3 REA 核心组件证据

#### 主托盘 `ComputerZTray.exe`

- SHA-256：`50abbe17be37f06dcafb9f3375d21d79b6ab5816da04557bb5dd1c65e518131a`
- PE32 / x86；
- 导入包含 USER32、GDI/GDI+、WINHTTP、WININET、IP Helper、CryptoAPI 和 WinTrust；
- 携带成都奇鲁科技有限公司的证书结构。

这支持“主托盘负责原生 UI，并具有网络和系统交互能力”的结论，但没有证明它在本次安装中实际发起过网络请求。

REA Evidence：

- 导入：`ev_4ccaaf4951c79a809ded675bf35656c93dd95d5e718bf4489d7d7f437260f6c1`
- 证书：`ev_42d8ef9f869f81cddad4f36a305044649f87662bcda7d78a8c92aea4b996a731`

#### 硬件模块 `ComputerZ_HardwareDll.dll`

- SHA-256：`393220e5e58cd501b176526ff91924c44d9bc270671b9b04923d54d48c1e24b7`
- PE32 / x86；460 个导入；
- SetupAPI 导入覆盖设备枚举、设备接口、设备实例 ID、设备注册表属性和驱动信息列表；
- PDH 导入覆盖性能计数器查询、采集和格式化；
- 系统导入覆盖逻辑处理器、内存状态、磁盘空间、驱动器类型、进程信息和网络适配器；
- `DeviceIoControl` 与服务控制 API 表明它具备与设备/驱动及服务交互的静态能力；
- 携带成都奇鲁科技有限公司的证书结构。

这为硬件清单和性能监控提供了强静态证据。传感器协议、具体驱动 IOCTL 和硬件识别算法仍需深度反编译或运行观察。

REA Evidence：

- 导入：`ev_9338c8b578938ba443d9582268291a87313b1180c17981c236b9a51cde2c602c`
- 证书：`ev_3e2ae887edfd628e1c5401f6f639f57d94dd97344e48487668b1bbf73d615dfc`

#### 硬件服务 `ComputerZService.exe`

- SHA-256：`1119c8e86f3def1a2600f412e2629316280edd83e4dd3fc99a615174a8fb356c`
- PE32 / x86；205 个导入；
- 携带成都奇鲁科技有限公司的证书结构。

静态证据确认独立服务可执行文件存在，但当前导入表不足以确定其 IPC 协议、启动类型、权限边界和具体职责。

REA Evidence：

- 导入：`ev_f747de7dc35d0ec781d53682b5ef6d70d200a0e0ee5edac4a86b6efeaae405a1`
- 证书：`ev_64763757974730743e5fc705dbd18debdefc30f62e7f64c6dc1615253093482c`

#### 性能测试启动器 `nBench/BenchmarkLauncher.exe`

- SHA-256：`ace7a45fbd345fcc85da3d407edf799bfab936b8d30e40ac45ff954347c20f1a`
- PE32 / x86；238 个导入；
- 导入覆盖进程枚举、进程优先级、性能计数器、文件映射、注册表和签名验证；
- 携带成都奇鲁科技有限公司的证书结构。

这证明独立 benchmark 启动与调度组件存在，但测试项目、计分公式、负载模型和结果归一化仍未知。

REA Evidence：

- 导入：`ev_d10dd1c1e5dd3c19bbb14b20662f57185cfc9d364bed24fec364deda18f7ab1e`
- 证书：`ev_4df4c9d5b4b3ad7916e804e7f4634f91efa023a2b6a75f4371d8430f7ed07657`

#### 清理引擎 `sweeper/CleanEngine.dll`

- SHA-256：`99b524a4d4a2dc327790898328afea491d966a60b6c1204c31863971a876d529`
- PE32 / x86；330 个导入；
- 明确导入文件删除、文件移动、注册表键值删除、进程枚举、设备 I/O、Shell 文件操作和 `SHEmptyRecycleBinW`；
- 携带成都奇鲁科技有限公司的证书结构。

这为“文件/注册表/回收站清理能力”提供了强静态证据，但没有给出实际规则库、匹配条件、排除列表或误删保护策略。

REA Evidence：

- 导入：`ev_d45d612a9208e4a454ea01a346841acbabfb31ce7c2aab786f5380909443dfaa`
- 证书：`ev_e7efaf437dd2d279642ce5a7e6f1bdd1243f0a5a4caf02e5f7e570e26aee7ed2`

#### 温度历史 `TemperatureHistory.dll`

- SHA-256：`822790709281ae8461fe8235d40e623ed5c2a71ab80fbc915b20fded0ef06efa`
- PE32 / x86；193 个导入；
- 导入覆盖文件映射、注册表、计时、磁盘空间和本地数据文件操作；
- 携带 Chengdu Qilu Technology Co. Ltd. 的证书结构。

这支持“温度历史独立持久化模块存在”的结论；温度采集来源、采样周期和数据格式仍未知。

REA Evidence：

- 导入：`ev_4894664726308b9b82db59bb5bce0780d45d73599f7acd9974c1a9c872a01408`
- 证书：`ev_241fb1d3e2abbb43a96651b6bf1485edf1caafb155d8532af8b65d6e755af3ea`

### 12.4 对 Tauri 方案的修订

安装后证据使下列能力可以从“候选”提升为独立产品的明确功能域，但实现仍须采用公开接口和原创代码：

1. **硬件清单**：CPU、内存、主板、显卡、显示器、磁盘、网络适配器和系统设备；
2. **实时监控**：CPU、内存、磁盘、网络和进程指标；
3. **温度历史**：在公开传感器接口可用时记录温度序列，清楚标注不可用来源；
4. **性能测试**：拆分 CPU、内存、磁盘和 GPU 测试，公开负载、时长和计分公式；
5. **磁盘空间分析**：只读扫描与可视化；
6. **清理预览**：第一阶段只生成候选项，不执行删除；后续删除必须逐项确认并支持回收站/恢复策略；
7. **驱动状态**：只读列出设备及当前驱动，不在 MVP 中提供自动下载和安装。

不纳入 MVP：后台常驻服务、内核驱动、浏览器防护、广告拦截、静默更新、进程优化、注册表自动清理和任何跨进程写内存行为。它们增加权限、安全、稳定性和合规风险，也不是实现透明系统信息工具的必要条件。

### 12.5 下一轮证据任务

1. 分析 10 个 `.sys` 驱动的签名、导入和设备接口候选；
2. 解析 `ComputerZService.exe` 与 `ipc/ipcservice.dll` 的 IPC 入口；
3. 检查 `nBench` 各测试 DLL 的导出和依赖，建立测试项目清单；
4. 检查温度历史的数据文件位置与格式，但不运行或修改原程序；
5. 分析主题 XML/UI 文件只用于页面结构和流程研究，不复用图片、文案和布局素材；
6. 在深度反编译 provider 可用后，仅对上述高价值组件做函数级调用链分析。
