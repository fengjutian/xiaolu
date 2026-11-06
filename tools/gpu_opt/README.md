
## 按应用 GPU 偏好

`set_gpu_preference` 写当前用户 `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`：

- 1 = 省电，2 = 高性能，0 = 由 Windows 决定（删除该应用的值）。
- 只接受绝对 `.exe` 路径，写入后回读确认。
- 不修改驱动、不超频、不改全局显卡设置；删除值即恢复默认。
