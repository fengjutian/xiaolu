# 小鲁 · 系统透镜

小鲁是一款面向 Windows 11 的本地系统信息、硬件健康与问题诊断工具。项目使用 Tauri 2、React 和 Rust 构建，优先在本机读取、分析并保存数据，不要求注册账户，也不安装后台服务。

当前版本：`0.57.0`

## 能做什么

### 系统与硬件

- 实时查看 CPU、内存、磁盘和网络状态
- 查看主板、BIOS、显卡、内存、硬盘、网卡及显示设备详情
- 读取硬盘 SMART、温度、电池健康、驱动签名与设备异常
- 查看高占用进程、启动项、计划任务、服务和已安装软件

### 性能与诊断

- CPU、内存、磁盘与 WebGL2 GPU 性能测试
- 建立本机历史基线并比较性能变化
- 分析蓝屏转储、崩溃事件、网络连接和驱动问题
- 提供磁盘空间 Treemap、大文件、重复文件与目录分析
- 导出经过脱敏的 JSON、Markdown 和自包含 HTML 报告

### 工具箱

应用包含 25 个独立工具进程，覆盖清理、系统优化、压缩、PDF、软件管理、隐私检查等场景。

- 只读工具只扫描和展示，不修改系统
- 写操作必须经过预览、二次确认和一次性确认令牌
- 需要管理员权限的操作由独立提权助手按次执行
- 清理操作优先移入回收站，优化操作保留回滚快照
- 每次执行都会生成本地操作记录

### AI 解读

AI 功能默认关闭，关闭后不影响检测、诊断、跑分和报告功能。启用后，模型只接收经过筛选和脱敏的诊断快照。

- 支持 OpenAI 兼容接口和流式取消
- API 密钥使用 Windows DPAPI 加密保存
- 发送前可以预览完整载荷
- AI 结论必须引用真实存在的诊断证据
- 模型不可用时自动回退到本地规则诊断
- AI 只能请求白名单内的只读检查，不能执行命令或写操作

详细设计见 [AI 实现说明](docs/AI_IMPLEMENTATION.md)。

## 技术栈

| 层级 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 前端 | React 19、TypeScript、Vite |
| UI 与图表 | Mantine、Recharts、Tabler Icons |
| 本地能力 | Rust、SQLite、Windows API |
| 产品官网 | Astro、React |

## 环境要求

桌面端目前仅面向 Windows。开始前请安装：

- Node.js 与 npm
- Rust stable 工具链
- Visual Studio Build Tools，包含“使用 C++ 的桌面开发”
- Windows 10/11 SDK
- Microsoft Edge WebView2 Runtime

## 本地开发

```powershell
cd D:\github\xiaolu
npm install
npm run tauri:dev
```

仅启动浏览器前端：

```powershell
npm run dev
```

## 构建与发布

### 完整桌面安装包

```powershell
npm run release
```

该命令会依次完成：

1. 编译 25 个工具及提权助手
2. 生成并校验 `tools-dist/`
3. 构建 React 前端
4. 编译 Rust/Tauri 桌面程序
5. 生成 MSI 和 NSIS 安装程序

安装包位于：

```text
target/release/bundle/msi/
target/release/bundle/nsis/
```

当前 MSI 使用简体中文，NSIS 安装程序支持简体中文和英文选择。

### 仅构建桌面端

如果 `tools-dist/` 已经是最新且校验完整，可以执行：

```powershell
npm run tauri:build
```

以下情况会中止打包：

- 工具或入口程序缺失
- `tool.json` 与源码不一致
- 工具源码修改后没有重新构建
- 需要提权的工具缺少 `lens_helper.exe`

### 工具箱命令

```powershell
npm run toolbox:build   # 编译工具并生成 tools-dist
npm run toolbox:verify  # 校验工具、清单和源码指纹
npm run toolbox:test    # 运行全部工具 crate 测试
```

### 常规检查

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## 产品官网

官网位于 `website/`，与桌面应用独立构建：

```powershell
cd website
npm install
npm run dev
```

生成静态站点：

```powershell
npm run build
```

产物位于 `website/dist/`。官网支持深色与浅色主题，并会记住用户选择。

## 项目结构

```text
xiaolu/
├─ src/                 React 桌面界面
├─ src-tauri/           Tauri 配置与 Rust 宿主
├─ tools/               独立工具、共享协议和提权助手
├─ tools-dist/          工具箱构建产物，不提交到仓库
├─ website/             Astro 产品官网
├─ docs/                需求、设计与实现文档
├─ public/              桌面前端静态资源
└─ package.json         开发和发布脚本
```

## 安全与隐私原则

- 默认本地运行，不要求登录
- 不静默上传系统数据
- 不安装内核驱动或后台服务
- 无法读取的指标明确显示不可用，不使用估算值冒充
- 写操作必须可预览、可审计，并尽可能可回滚
- 报告默认隐藏主机名、用户名、序列号、MAC 和本地路径等敏感信息

## 进一步阅读

- [完整功能路线图](docs/FULL_FEATURE_ROADMAP.md)
- [产品对标与验收标准](docs/PRODUCT_PARITY_STANDARD.md)
- [工具包规范](docs/TOOL_PACKAGE_SPEC.md)
- [AI 实现说明](docs/AI_IMPLEMENTATION.md)
- [传感器 Helper 设计](docs/SENSOR_HELPER_DESIGN.md)
- [项目需求](PROJECT_REQUIREMENTS.md)

## 项目边界

本项目不复用鲁大师的代码、商标或视觉资源。小鲁不会静默安装第三方软件、驱动或后台服务。
