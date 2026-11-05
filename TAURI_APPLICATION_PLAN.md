# Tauri 系统健康应用方案

> 文档状态：独立产品设计基线  
> 更新日期：2026-11-05

## 1. 产品定位

本项目是一款本地优先、透明可审计的 Windows 系统信息与健康检查工具。所有功能基于公开系统接口和自主实现，不包含第三方产品名称、文件信息、逆向记录、私有协议或视觉资产。

## 2. MVP 功能

### 系统概览

- Windows 版本、设备名称和系统启动时间；
- CPU、内存、主板、GPU、显示器、磁盘和网络适配器摘要；
- 数据来源、采集时间和权限状态。

### 实时指标

- CPU 总体及逻辑处理器利用率；
- 内存已用量、可用量和提交量；
- 磁盘读写吞吐及空间使用；
- 网络收发速率；
- 基础进程列表及资源占用。

### 存储与启动项

- 磁盘、分区、文件系统和可用空间；
- 大目录与大文件只读分析；
- 常见注册表位置及启动目录中的启动项；
- 第一阶段不提供删除或禁用操作。

### 诊断报告

- 导出 JSON 和 Markdown；
- 导出前预览字段；
- 可选择隐藏用户名、主机名、序列号、MAC 地址和本地路径。

## 3. 后续功能

- 公开传感器接口可用时记录温度历史；
- CPU、内存、磁盘和 GPU 的可复现性能测试；
- 只读设备和驱动状态；
- 清理候选预览，但默认不执行删除。

## 4. 技术架构

```text
React UI
  ├─ Dashboard
  ├─ Hardware
  ├─ Performance
  ├─ Storage
  ├─ Processes
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
  ├─ Win32
  ├─ Registry read APIs
  ├─ Performance counters
  ├─ SetupAPI
  └─ WMI/CIM where justified
```

前端只负责展示和交互。系统访问由 Rust 服务封装，并通过最小化、强类型的 Tauri command 暴露。

## 5. 建议目录

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

## 6. IPC 数据契约

- `SystemSummary`
- `CpuInfo`
- `MemoryInfo`
- `GpuInfo`
- `MetricSample`
- `DiskInfo`
- `ProcessSummary`
- `StartupEntry`
- `DiagnosticReport`
- `AppError { code, message, remediation }`

命令不返回无约束 JSON。实时指标采用有界采样频率；窗口不可见时降低采样频率。

## 7. 安全与隐私

- 默认离线运行；
- 不上传硬件标识、进程列表或报告；
- Tauri capability 采用最小权限；
- 不开放任意 shell、任意文件系统或任意 URL；
- 第一阶段不安装驱动或服务；
- 不执行下载后的程序；
- 不实现跨进程写内存；
- 所有错误提供稳定代码、用户可读说明和处理建议。

## 8. 实施阶段

### 阶段 A：工程骨架

- 初始化 Tauri 2、React、TypeScript 和 Rust；
- 建立格式化、静态检查、单元测试和 Windows CI；
- 完成 IPC 错误模型、capability 白名单和模拟数据层。

验收：开发构建、生产构建和测试在干净环境通过。

### 阶段 B：只读 MVP

- 实现系统概览、实时指标、存储、进程、启动项和报告导出；
- 为 Windows adapter 提供单元测试或可重复集成测试；
- UI 显示数据来源、更新时间和不可用原因。

验收：普通用户权限下稳定运行，无后台服务、无静默联网、无系统修改。

### 阶段 C：性能与传感器

- 定义可复现的性能测试协议；
- 接入公开且许可兼容的传感器来源；
- 增加温度历史和结果对比。

验收：测试负载、计分公式、误差范围和取消行为均有文档。

## 9. 法律与内容边界

- 使用独立产品名称、图标、视觉系统和文案；
- 不复制第三方产品的界面、素材、代码、数据库或私有配置；
- 不保留第三方样本路径、哈希、证书主体或模块清单；
- 功能设计以通用用户需求和公开操作系统能力为依据；
- 引入第三方依赖前检查许可证并记录用途。
