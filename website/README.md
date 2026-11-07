# 小鲁产品官网

基于 Astro + React 构建的静态产品官网，与仓库根目录的 Tauri 应用相互独立。

## 页面结构

| 路径 | 内容 |
|---|---|
| `/` | 首页：能力概览、性能趋势示意、工具箱与安全要点、下载入口 |
| `/features/` | 功能总览：实时监控、硬件与健康、性能测试、问题诊断、空间与文件、日常效率、报告与数据 |
| `/toolbox/` | 工具箱：25 个独立工具进程，按能力分组并标注风险等级 |
| `/ai/` | AI 解读：默认关闭、工作流程、受控能力、密钥与费用说明 |
| `/security/` | 安全与隐私：设计原则、写操作约束链路、数据处理、明确不做的事 |
| `/download/` | 下载：版本、系统要求、安装包格式、安装步骤与首次使用提示 |

站点内的版本号与应用版本同步，当前为 `0.57.0`（来源：`package.json` 与 `src-tauri/tauri.conf.json`）。

## 本地开发

```powershell
cd website
npm install
npm run dev
```

## 生产构建

```powershell
npm run build
```

构建产物位于 `website/dist`。`npm run build` 会先跑 `astro check`（类型与模板检查），再生成静态页面。

## Netlify

连接仓库后设置：

- Base directory：`website`
- Build command：`npm run build`
- Publish directory：`dist`
- 环境变量 `SITE_URL`：正式网站地址，例如 `https://example.com`

`astro.config.mjs` 已开启 `trailingSlash: "always"`，站内链接统一带尾斜杠，避免访问时产生额外重定向。正式域名确定后，只需设置 `SITE_URL` 环境变量（`sitemap` 集成会据此生成站点地图）。

仓库中的 `netlify.toml` 已包含 Node.js 版本、缓存和基础安全响应头配置。
