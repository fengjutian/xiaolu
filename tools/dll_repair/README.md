# dll_repair · DLL 修复

检查常见运行库 DLL 是否存在，并读取 VC++ 2015-2022 与 .NET Framework 安装状态；可打开微软官方下载页手动安装。不自动下载、不替换 DLL。

## 运行方式

由 System Lens 工具箱宿主以固定参数启动：

```text
dll_repair.exe --stdio
```

## 支持的动作

| 动作 | 写操作 | 说明 |
|---|---|---|
| `preview` | 否 | 检查 System32/SysWOW64 DLL，读取 VC++/.NET 安装状态 |
| `open_official` | 否 | 用默认浏览器打开白名单中的微软官方下载页 |

`open_official` 参数：

```json
{ "id": "vcpp-x86" }
```

可接受 ID：`vcpp-x64`、`vcpp-x86`、`dotnet48`。

## 检查项

| 项目 | 来源 |
|---|---|
| DLL 存在性 | `%SystemRoot%\System32` 与 `SysWOW64` 下的 11 个常见运行库文件 |
| VC++ x64/x86 | `HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\{x64,x86}` 的 `Installed`/`Version` |
| .NET Framework 4.8 | `HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full\Release` ≥ 528040 |

## 安全机制

- `open_official` 只接受三个固定 ID，URL 由程序内白名单决定，不接受任意 URL。
- 只调用 `rundll32.exe url.dll,FileProtocolHandler <固定 URL>` 打开浏览器。
- 不下载、不执行安装包、不替换系统 DLL、不写入系统目录。
- 打开动作写入本地操作日志。

## 边界

- 缺失 DLL 不等于系统损坏：部分组件由应用自带或按需安装。
- 本工具不自动修复，只提供检测结果与官方入口。
- `.NET Framework` 只识别 4.x 的 `Release` 值，不覆盖 3.5/2.0。
- 安装运行库可能需要管理员权限，由用户在官方安装程序中确认。

## 构建与测试

```powershell
cargo build --release --manifest-path tools/dll_repair/Cargo.toml
cargo test --manifest-path tools/dll_repair/Cargo.toml
```
