# 鲁大师 SuperApp 模块 REA 静态分析

分析日期：2026-11-06  
分析方式：REA 目录制品图、PE 导入表、PE 证书；辅以只读文件清单与模块清单 JSON。未启动任何目标程序，未执行优化、清理、分区或网络过滤操作。

## 1. 样本与证据边界

本次确认的实际样本根目录：

- 主程序：`D:\github\xiaolu\reference-input\ludashi`，795 个文件，约 675 MB。REA Evidence：`ev_bb3c05bb4990e7a59e8012b06bc79ec9140623ed461afd5b57dc1400039105d8`
- 用户模块数据：`C:\Users\Charles.feng\AppData\Roaming\LuDaShi`，546 个文件，约 225 MB。REA Evidence：`ev_46de43a02ef50f214d2de2de2465f434c4b871205e91372fb24e31e059dc8169`
- 独立压缩工具：`C:\Program Files (x86)\LdsOmniZip`，116 个文件，约 229 MB。REA Evidence：`ev_7c4c72c7352b36ae24da7e597bfd144e39254c6b082f55262a8319cb8bcc4436`

旧目录 `D:\github\xiaolu\鲁大师\ludashi` 当前仅剩少量资源文件，不再作为完整样本。

结论仅代表静态可观察能力。导入某个 Windows API 不等于运行时必然调用它；`.dat` 的具体规则、服务端策略、付费校验及真实修改项仍需受控动态验证。

## 2. 21 个功能的模块映射

| ID | 已确认入口/载荷 | 静态判断 | Tauri 复刻建议 |
|---|---|---|---|
| `defrag` | `SuperApp\defrag\defrag.exe`、`defrag.dat`、`defrag.json` | 独立磁盘整理程序；导入卷信息、磁盘空间、`DeviceIoControl`、注册表写入与下载 API | 首版只做 HDD/SSD/卷类型、碎片分析和 TRIM 状态；整理调用 Windows 官方能力，并对 SSD 禁止传统碎片整理 |
| `gpu_opt` | `gpu_opt_ui.exe`、`gpu_opt.dat`、`game.json` | 带大型游戏识别库；可创建进程、访问设备、调整令牌权限和注册表 | 首版只读展示 GPU/驱动/电源计划及逐项建议；不要实现“神秘一键优化” |
| `ai_cooling` | `ai_cooling_ui.exe`、`ai_cooling.dat` | 清单称“实时智能调节温度、降低功耗和发热”；具备设备、进程、注册表和下载接口 | 使用温度传感器 + CPU/GPU 负载生成建议；写操作限定为可回滚电源计划，不直接控制未知 EC/风扇寄存器 |
| `pc_optimizer` | `pc_optimizer_ui.exe`、`pc_optimizer.dat`、`game_info.json` | 明确导入服务查询/启动/停止/改配置、注册表删除/写入、提权、进程和电源 API | 分成扫描、建议、应用、回滚四步；每项展示原值与预期影响，默认只读 |
| `memopt_pro` | `memory_optimizer_setting.exe`、`memopt_pro.dat` | 存在独立设置程序和规则载荷；具体释放算法未由本轮证实 | 做进程内存排行、提交量/缓存/分页诊断；不以强制清空工作集冒充“释放内存” |
| `sys_boost` | `sys_boost_ui.exe`、`sys_boost.dat` | 清单称“一键优化系统性能”，规则封装在 DAT | 复用优化事务框架；禁止无差别套用注册表模板，所有更改都要形成恢复点/回滚日志 |
| `winopt_assistant` | `winopt_assistant_ui.exe`、`winopt_assistant_monitor.exe`、两个 DAT | 除 UI 外还有常驻 monitor，表明存在后台监测 | 首版仅做逐项 Windows 设置建议；后台监测必须可关闭、低频且明确说明采集项 |
| `clean_master` | `clean_master.exe`、`cclean.exe`、`dclean.exe`、DAT | 聚合多个清理执行器；具备文件、进程、注册表修改能力 | 先扫描后勾选；路径白名单、占用检测、回收站/隔离区、清理前后明细不可缺少 |
| `privacy_incognito` | `privacy_incognito.exe`、`privacy_incognito.dat`；清单实际通过 `computercenter.exe --PluginName=privacy_client_v2` 打开 | 是主程序插件入口加独立组件，不是简单浏览器无痕开关 | 必须先定义痕迹边界；按浏览器历史、最近文件、剪贴板、DNS 等分类，默认只预览 |
| `c_expand` | `c_expand.exe`、`c_expand_p.dat` | 导入卷枚举、卷路径、挂载点删除、`DeviceIoControl` 和提权 API，属于高风险系统工具 | Tauri 主应用只做磁盘布局诊断；真正分区操作放独立提权 helper，需备份、供电检查、BitLocker 检查和恢复介质 |
| `software_pm` | `software_pm_ui.exe`、`software_pm.dat` | 清单描述为软件权限管理，具体权限模型在 DAT/运行时中 | 先实现启动项、计划任务、服务、通知和防火墙规则的只读视图；修改逐项确认 |
| `eye_care` | `eye_care.exe`、`eye_care.dat` | 独立 UI/常驻能力，清单提到蓝光和颜色校准 | 用系统夜间模式/色温覆盖层；提供暂停、定时和全屏应用例外 |
| `lockscreen` | `lockscreen.exe`、`cssdk.exe`、`cssdk.dll`、`Utils\LockScreen.dll` | 不只是调用一次系统锁屏，包含额外 SDK 组件 | 复刻只调用 `LockWorkStation`；不复制未知 SDK 或自定义锁屏注入行为 |
| `omni_zip` | 主目录和独立安装目录均有 `omni_zip.exe`；另有托盘、预览和 32/64 位 Shell DLL | 独立产品，带资源管理器 Shell/预览集成；主 EXE 可写注册表安全项并启动进程 | 采用许可清晰的压缩库；Shell 扩展作为可选独立组件，首版先做应用内解压/压缩 |
| `master_pdf` | 主目录和 Roaming 中仅观察到 `master_pdf.dat` | 当前样本未见对应 EXE，可能是下载占位、外部产品入口或尚未完整安装 | 不声称已复刻；先做 PDF 查看、合并、拆分、旋转，OCR/编辑另行选库与许可审查 |
| `multi_wechat` | `multi_wechat.exe`、`multi_wechat.dat`、用户信息数据 | 独立启动器/规则载荷；真实多开机制未由静态清单证实 | 先研究微信版本兼容和账号安全；不得绕过安全校验或修改第三方二进制 |
| `start_menu_pro` | `start_menu_pro.exe`、helper、32/64 位 taskbar DLL、DAT | 有常驻/任务栏集成组件，明显超出普通页面功能 | 先实现应用内快捷启动器；Explorer/任务栏注入不进入首版 |
| `desktop_quick` | UI、卸载器、DAT，清单称整理桌面和快捷访问 | 是可安装/卸载的常驻桌面组件 | 可做无注入侧边栏窗口；自动启动必须显式选择，提供一键退出和卸载 |
| `adblock_master` | UI、弹窗进程及两份 DAT | UI EXE 可读其他进程内存、访问设备、注册表和网络 API；过滤后端未在本轮直接定位 | 首版只做浏览器扩展/Hosts 规则审计或 DNS 建议；不要直接实现内核/WFP 过滤驱动 |
| `intercept_bs` | `intercept_bs_ui.exe`、`intercept_bs.dat` | 具备进程、设备、注册表与下载接口；真正安装监控机制未由本轮证实 | 先实现下载/安装包信誉与捆绑项提示；若做安装监控，应采用受控事件源而非进程注入 |
| `push_accelerator` | `push_accelerator.exe`、DAT，Roaming 有约 282 KB 云配置 | 第三方推送/云规则驱动的独立组件 | 不纳入核心优化器；作为可选集成，先确定协议、数据流和隐私边界 |
| `push_open_claw` | `push_open_claw.exe`、`push_open_claw_p.dat` | 本地存在可执行组件，而非仅有名称登记；实际协议未知 | 维持“仅登记、暂不集成”；在协议、签名、权限和数据去向明确前不执行 |

## 3. 关键 REA PE 证据

代表性主程序均包含 Authenticode 证书表，样本证书链显示签名主体为“成都奇鲁科技有限公司”（`defrag` 的旧签名为英文 `Chengdu Qilu Technology Co. Ltd.`）。这说明文件带签名材料，不等同于已完成联网吊销检查，也不代表功能安全。

| 模块 | SHA-256 | 关键静态能力 | REA Evidence（导入 / 证书） |
|---|---|---|---|
| `defrag` | `1ac66edc2cb7f3421517acb0855919c5bc7a8c9aea090dfc14dde805b1f73c68` | 卷信息、磁盘空间、设备控制、注册表写删、下载 | `ev_92890712b82bc292db7fb64ff35ad198fc1ff8808f0be7b756928d6068af3290` / `ev_e103e91eccc0fc55bf506e769799a550ec0266fb8bc6e4cbb98b70479d299019` |
| `gpu_opt` | `96a6135c8beba3498208d3d3a2b8b6f5bcd5aa7b9dc99ad747e8c8b2b8a580e9` | 进程启动、设备控制、权限调整、注册表写删、D3D9 | `ev_0ad07e68cf83d7620ac5695263ae06a6702c0723b5cb10d912612da67674ea2b` / `ev_d4a3479817a40a1d88458f68afedd47e149fd4e0b4b36b7ccd881813f53a0361` |
| `ai_cooling` | `ac1b323fecfd193ff2e149b0a8ab422b1786f0d4c6a32e9ab7f2e16893060eee` | 进程启动、设备控制、注册表写删、联网下载 | `ev_15ee35c714e2432ef36636e439fdd9ccea489a17d60066640aa9713d0812764f` / `ev_3b54099a7a2fe13bda926d7da7ae4ee83130babd46c686746a7f56f492556d16` |
| `pc_optimizer` | `7af1ed8470af92953545a05547cb970eb96368287cd0950a28b00ce44688f664` | 服务控制、提权、注册表删改、进程/内存、电源事件 | `ev_9eee7dcb3b60a3f4349db85eba26635e80413156be62951c7beabcb337c25e58` / `ev_a2c2595f21f973d8ecddf4622d4fbe0fdeb530546ae993768c2ff18ca69640e1` |
| `clean_master` | `342ad3091474762ce3e4b3ee89ecad41af50ed682b3e4a6c2bf1788b347ce263` | 文件/进程、设备控制、注册表写删、下载 | `ev_7ce85b37bdb064c9b9acca3ea4ca2784b63ec050d94475db6d1926d79315e58c` / `ev_d5d409f91c0a9945fdafc8963108ac6d5715b032c7fd9e94431135498e0a17fc` |
| `c_expand` | `85a23eecd66d293b8902d902c0ea3ce21da1c7a44e3c242aaf6d0b1e02e82c85` | 卷枚举、挂载点删除、设备控制、提权、进程启动 | `ev_398b4fcf0871c55a3e886539e8715c844cdf52fb213be108fb7ffd99786dd4d5` / `ev_8f6f72eddfadfe488d3d14dffa068c7405d2f131507bedec701ea02fa6d7b259` |
| `adblock_master` | `0d42b1bac993d9a64098e8e6d484b6f099c04ed73185cc26fdfbb820d965071c` | 读进程内存、设备控制、进程启动、注册表写删 | `ev_1eef340706f2bde79808d72938a555b1349505e7b62cd28c29f680257d71b53e` / `ev_967babed08491850c27eb464bad1c48f7ccd2cba863946369ed9bda896718c88` |
| `intercept_bs` | `39cf85ffe8b7f1615cbab101141e20ea473b681efcaea19875e7e7cf67adc236` | 进程查询、设备控制、注册表写删、下载 | `ev_cd0bb139e61189f932d83b7bbc6e65058b78b4ec70b3de9a6051fe37b1ab9ec2` / `ev_4132b1938be90cb227817b4eec7ea9576765cbc71ce24e71fa91faf3955bab6d` |
| `omni_zip` | `3eb4fbda9f5287d30a84d584f12e77fd4efb27e59a13354f99f84e10a72cdc48` | Shell 集成相关注册表操作、进程启动、卷信息、设备控制 | `ev_593f6870d856fabf58eee80dc5dad3112a41ed87c5787a42d7c686064805abb7` / `ev_970c362fd1b8558200fabff1feab066067065ff1bfdc122922a4d6acb432608c` |

## 4. 建议的 Tauri 实施顺序

### P0：只读诊断与通用安全框架

1. 建立统一 `ScanResult`、`Recommendation`、`PlannedChange`、`RollbackRecord` 数据模型。
2. 完成 `defrag` 只读卷/TRIM、`gpu_opt` 只读 GPU/驱动/电源、`ai_cooling` 温度建议、`memopt_pro` 内存分析、`software_pm` 权限/启动行为查看。
3. 所有系统命令采用固定参数 Rust API；禁止前端传入任意 PowerShell/命令行。
4. UI 明确区分“检测结果”“建议”“将执行的更改”和“已执行结果”。

### P1：低风险、可回滚写操作

实现护眼、锁屏、缓存清理、逐项电源设置和用户明确选择的启动项修改。每次写操作记录原值、时间、执行主体、结果和恢复方式。

### P2：独立工具与常驻能力

压缩/PDF、桌面侧边栏和应用内快捷启动器使用独立 feature crate；托盘和自启动默认关闭。Shell 扩展、Explorer 集成和后台 monitor 分包安装。

### P3：高风险系统能力

分区扩容、网络过滤、安装拦截、服务批量修改必须使用独立签名 helper，并通过 IPC 白名单接受结构化请求。先完成恢复方案和故障注入测试，再开放 UI 入口。

### 暂缓

`multi_wechat`、`push_accelerator`、`push_open_claw`、内核级广告过滤和 Explorer 注入。当前证据不足以安全、兼容地复刻这些行为。

## 5. 仍需补证的内容

- DAT 载荷格式及各优化项的准确键值、服务名和回滚方式。
- `ai_cooling` 是否调用鲁大师已有硬件驱动，还是仅调整电源/进程策略。
- `adblock_master` 和 `intercept_bs` 的过滤/监控后端是否位于共享服务或驱动中。
- `master_pdf` 的实际产品入口与所用 PDF 引擎许可证。
- `multi_wechat` 的实现机制及对当前微信版本的影响。
- 所有模块在隔离虚拟机中的进程树、文件/注册表/服务/计划任务差分和网络端点。

后续动态分析必须在快照虚拟机进行，先记录基线，再逐个模块启动和操作；不得在当前开发机直接执行 `c_expand`、批量优化、清理、网络过滤或安装拦截。

## 6. 实施状态（2026-11-06）

按第 4 节的 P0 顺序，只读诊断工具已全部落地为 `tools/<id>/` 独立进程（23 个可运行工具：17 只读 + 6 写，另含 3 个应用内置工具页）：

| REA 功能 | 本仓库工具 | 当前能力 | 边界 |
|---|---|---|---|
| `defrag` | `defrag` | 固定卷、物理介质、容量与 TRIM 状态 | 不整理、不优化、不写注册表 |
| `gpu_opt` | `gpu_opt` | 显卡、驱动版本/日期、电源计划与建议 | 不切换电源计划、不改注册表 |
| `ai_cooling` | `ai_cooling` | ACPI 热区温度、CPU 负载与降温建议 | 不控制风扇/EC、不改电源策略 |
| `memopt_pro` | `memopt_pro` | 物理内存、提交量、系统缓存、内核池、进程工作集排行 | 不强制清空工作集、不结束进程 |
| `software_pm` | `software_pm` | 启动项、自动服务、用户计划任务、通知开关、防火墙规则统计 | 不启用/禁用任何启动行为或规则 |

其余已登记但未复刻的模块保持在工具箱“规划中”状态，按 P1–P3 顺序逐项评审后再实现：

- P1 低风险写操作：`lockscreen`（会话锁定）、`pc_optimizer`（电源计划切换 + 逐项电源设置）、`clean_master`（白名单清理、回收站回滚）与 `software_pm`（当前用户启动项启停、备份可还原）已实现确认令牌、审计日志与原值/回滚记录；`winopt_assistant` 已提供 11 项设置的只读体检与手动建议；`eye_care` 已以应用内页面实现（Gamma Ramp 色温滤镜 + 休息提醒）；`privacy_incognito` 已提供只读痕迹扫描（清除未实现）；`sys_boost` 已提供只读优化候选汇总（一键修改未实现）；系统级启动项修改仍待实现。
- P2 独立工具：`omni_zip`（压缩/解压/列表）与 `master_pdf`（信息/合并/拆分/旋转）已完成；`start_menu_pro`（应用内快捷启动器）与 `desktop_quick`（桌面助手）已完成。
- P3 高风险能力：`c_expand`、`intercept_bs`、服务批量修改；`adblock_master` 已完成只读审计首版。
- 暂缓：`multi_wechat`、`push_accelerator`、`push_open_claw`。

写操作框架已落地：`preview → confirmationToken → apply`、一次性令牌（宿主记录）与本地操作日志已用于上述两个写工具。清单哈希校验、恢复点与批量回滚仍待后续完成。
