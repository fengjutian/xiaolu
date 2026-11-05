# 小鲁 · System Lens

一个使用 Tauri 2、React 和 Rust 构建的本地优先 Windows 系统信息与健康检查应用。

## 当前功能

- 系统与 CPU 概览
- 实时 CPU、内存指标（每 5 秒刷新）
- 磁盘和分区空间
- 资源占用最高的进程
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
