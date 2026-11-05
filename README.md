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
