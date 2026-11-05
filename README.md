# 小鲁 · System Lens

一个使用 Tauri 2、React 和 Rust 构建的本地优先 Windows 系统信息与健康检查应用。

完整功能路线图：[docs/FULL_FEATURE_ROADMAP.md](docs/FULL_FEATURE_ROADMAP.md)

## 当前功能

- 系统与 CPU 概览
- CPU、内存、磁盘和网络实时曲线（每 2 秒刷新）
- 实时上传/下载速度及累计流量
- 主板、BIOS、显卡、内存、硬盘和网卡详情
- 磁盘和分区空间、实时读写速度
- 资源占用最高的 30 个进程
- Windows 当前用户启动项启停管理（系统级启动项只读）
- 系统托盘常驻、关闭窗口后后台运行
- NVIDIA GPU 温度、风扇、功耗与频率（硬件支持时）
- Windows ACPI 热区与物理磁盘健康/温度
- Windows 设备与驱动清单、签名状态、缺失驱动和 PnP 异常检测
- ATA SMART 原始属性、当前值、最差值、原始计数和预测故障状态
- CPU 单核/多核、内存带宽和磁盘顺序读写性能测试
- 本地跑分历史和版本化综合评分
- WebGL2 离屏 GPU 着色器跑分和 GPU 像素吞吐
- Windows Update 官方驱动候选只读扫描
- 电池健康、待重启状态与最近 24 小时关键系统事件
- DNS、网关、代理、Hosts 统计和活动 TCP 连接诊断
- 自动脱敏的 JSON 诊断报告
- 浏览器演示模式与 Tauri 真实数据模式

## 开发

```powershell
npm install
npm run tauri:dev
```

仅构建前端：

```powershell
npm run build
```

检查 Rust：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

本项目不复用鲁大师的代码、商标或视觉资源，也不安装驱动或后台服务。

## 界面技术

- Mantine：桌面端组件、主题与布局
- Recharts：实时性能曲线
- Tabler Icons：一致的矢量图标

## 传感器限制

0.3.0 使用 Windows 标准接口和已安装的 NVIDIA 厂商工具，不额外安装内核驱动。部分机器不会通过这些接口公开 CPU 温度，界面会明确显示不可用；后续版本计划提供可选、独立且经过审查的传感器 helper。
