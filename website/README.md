# 小鲁产品官网

基于 Astro + React 构建的静态产品官网，与仓库根目录的 Tauri 应用相互独立。

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

构建产物位于 `website/dist`。

## Netlify

连接仓库后设置：

- Base directory：`website`
- Build command：`npm run build`
- Publish directory：`dist`
- 环境变量 `SITE_URL`：正式网站地址，例如 `https://example.com`

仓库中的 `netlify.toml` 已包含 Node.js 版本、缓存和基础安全响应头配置。
