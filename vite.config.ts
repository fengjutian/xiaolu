import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    // 不要让 Vite 监听 Rust 编译产物和参考数据：`src-tauri/target` 有大量
    // 高频写入的文件，监听它会让 dev server 一直忙于处理文件事件，首屏
    // 转译（App.tsx 约 7.7k 行）会明显变慢。
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/dist/**",
        "**/reference-input/**",
        "**/reference-installer.exe",
        "**/analysis/**",
      ],
    },
    // 启动时先把入口和几个大模块转译好，避免 tauri 窗口打开后才开始
    // 逐级转译造成长时间停留在静态启动页。
    warmup: {
      clientFiles: [
        "./src/main.tsx",
        "./src/App.tsx",
        "./src/AiPanel.tsx",
        "./src/report.tsx",
      ],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : "oxc",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
});
