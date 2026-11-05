import { invoke, isTauri } from "@tauri-apps/api/core";
import type { BenchmarkRecord, BenchmarkResult, CleanupPreview, DiagnosticReport, DriverInventory, DriverUpdate, DuplicateScan, HardwareDetails, LargeFileScan, NetworkDiagnosticReport, ScheduledTaskInventory, SensorHistorySeries, SensorSettings, SensorSnapshot, SensorValue, SoftwareInventory, StartupItem, StorageKind, StorageScan, SystemSnapshot } from "./types";

const gib = 1024 ** 3;

function mockSnapshot(): SystemSnapshot {
  return {
    hostName: "DESKTOP-DEMO",
    osName: "Windows",
    osVersion: "11 Pro",
    kernelVersion: "10.0.26100",
    architecture: "x86_64",
    uptimeSeconds: 126_440,
    capturedAtUnixMs: Date.now(),
    cpu: {
      brand: "演示处理器（浏览器预览）",
      logicalCores: 16,
      usagePercent: 27.4,
      frequencyMhz: 3600,
    },
    memory: {
      totalBytes: 32 * gib,
      usedBytes: 13.6 * gib,
      usagePercent: 42.5,
    },
    disks: [
      {
        name: "系统盘",
        mountPoint: "C:\\",
        fileSystem: "NTFS",
        totalBytes: 953.9 * gib,
        availableBytes: 412.8 * gib,
        removable: false,
        readBytesPerSec: 24 * 1024 ** 2,
        writtenBytesPerSec: 8 * 1024 ** 2,
      },
    ],
    processes: [
      { pid: 4120, name: "Code.exe", cpuPercent: 8.2, memoryBytes: 1.2 * gib },
      { pid: 1736, name: "msedge.exe", cpuPercent: 5.4, memoryBytes: 824 * 1024 ** 2 },
      { pid: 988, name: "explorer.exe", cpuPercent: 1.8, memoryBytes: 286 * 1024 ** 2 },
      { pid: 4, name: "System", cpuPercent: 1.1, memoryBytes: 12 * 1024 ** 2 },
    ],
    network: { downloadBytesPerSec: 4.8 * 1024 ** 2, uploadBytesPerSec: 720 * 1024, totalReceivedBytes: 26 * gib, totalTransmittedBytes: 3.8 * gib, interfaces: 3 },
  };
}

export async function getSystemSnapshot(): Promise<SystemSnapshot> {
  if (!isTauri()) return mockSnapshot();
  return invoke<SystemSnapshot>("get_system_snapshot");
}

export async function getHardwareDetails(): Promise<HardwareDetails> {
  if (!isTauri()) return { computer: { Manufacturer: "演示厂商", Model: "System Lens Preview" }, board: { Manufacturer: "演示厂商", Product: "Preview Board" }, bios: { SMBIOSBIOSVersion: "1.0.0" }, gpus: [{ Name: "演示显卡", AdapterRAM: 8 * gib }], memory: [{ Manufacturer: "演示内存", Capacity: 16 * gib, Speed: 5600, DeviceLocator: "DIMM 0" }], drives: [{ Model: "演示 NVMe SSD", Size: 1024 * gib, InterfaceType: "NVMe" }] };
  return invoke<HardwareDetails>("get_hardware_details");
}

export async function getStartupItems(): Promise<StartupItem[]> {
  if (!isTauri()) return [{ name: "System Lens Preview", command: "C:\\Apps\\preview.exe", scope: "当前用户", enabled: true, editable: true }];
  return invoke<StartupItem[]>("get_startup_items");
}

export async function setStartupEnabled(name: string, enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  return invoke("set_startup_enabled", { name, enabled });
}

const demoSensorValue = (
  id: string,
  hardwareId: string,
  hardwareName: string,
  hardwareClass: string,
  category: string,
  name: string,
  value: number,
  unit: string,
): SensorValue => ({
  id,
  hardwareId,
  hardwareName,
  hardwareClass,
  category,
  name,
  value,
  unit,
  source: "浏览器演示",
  capturedAtUnixMs: Date.now(),
  availability: "ok",
  errorCode: null,
  quality: "measured",
});

export async function getSensorSnapshot(): Promise<SensorSnapshot> {
  if (!isTauri()) {
    const gpu = "gpu-demo";
    return {
      schemaVersion: 2,
      capturedAtUnixMs: Date.now(),
      values: [
        demoSensorValue(`${gpu}-temp`, gpu, "演示显卡", "gpu", "temperature", "演示显卡 温度", 54, "°C"),
        demoSensorValue(`${gpu}-fan`, gpu, "演示显卡", "gpu", "fan", "演示显卡 风扇", 32, "%"),
        demoSensorValue(`${gpu}-power`, gpu, "演示显卡", "gpu", "power", "演示显卡 功耗", 48, "W"),
        demoSensorValue("disk-demo-temp", "disk-demo", "演示 NVMe SSD", "disk", "temperature", "演示 NVMe SSD 温度", 39, "°C"),
      ],
      storage: [{ name: "演示 NVMe SSD", healthStatus: "Healthy", operationalStatus: "OK", temperatureC: 39, wearPercent: 4, powerOnHours: 1260, readErrorsTotal: 0, writeErrorsTotal: 0, source: "浏览器演示" }],
      smartDevices: [{ instanceName: "演示 SATA SSD", predictFailure: false, source: "浏览器演示", attributes: [{ id: 5, name: "重映射扇区数", current: 100, worst: 100, raw: 0 }, { id: 9, name: "通电时间", current: 98, worst: 98, raw: 1260 }, { id: 194, name: "温度", current: 61, worst: 48, raw: 39 }] }],
      limitations: [
        { code: "SENSOR_UNAVAILABLE", message: "浏览器演示不包含真实 CPU/主板温度。", remediation: "在桌面应用中运行以读取本机传感器。" },
      ],
      sources: [
        { source: "nvidia-smi", ok: true, lastError: null, lastSuccessAtUnixMs: Date.now() },
        { source: "windows-acpi", ok: false, lastError: null, lastSuccessAtUnixMs: null },
      ],
      alerts: [],
      elevated: false,
    };
  }
  return invoke<SensorSnapshot>("get_sensor_snapshot");
}

export async function getSensorHistory(sensorId: string, hours: number): Promise<SensorHistorySeries> {
  if (!isTauri()) {
    const now = Date.now();
    const points = Array.from({ length: 30 }, (_, index) => {
      const avg = 45 + Math.sin(index / 3) * 6;
      return { t: now - (29 - index) * 60_000, min: avg - 2, max: avg + 2, avg, count: 6 };
    });
    return {
      sensorId,
      bucketMs: 60_000,
      points,
      min: Math.min(...points.map((point) => point.min)),
      max: Math.max(...points.map((point) => point.max)),
      avg: points.reduce((sum, point) => sum + point.avg, 0) / points.length,
      sampleCount: points.reduce((sum, point) => sum + point.count, 0),
    };
  }
  return invoke<SensorHistorySeries>("get_sensor_history", { sensorId, hours });
}

export async function getSensorSettings(): Promise<SensorSettings> {
  if (!isTauri()) {
    return {
      samplingIntervalMs: 10_000,
      retentionHours: 24,
      alertsEnabled: true,
      thresholds: {
        cpu: { warningC: 85, criticalC: 95 },
        gpu: { warningC: 80, criticalC: 90 },
        disk: { warningC: 55, criticalC: 65 },
      },
    };
  }
  return invoke<SensorSettings>("get_sensor_settings");
}

export async function saveSensorSettings(settings: SensorSettings): Promise<SensorSettings> {
  if (!isTauri()) return settings;
  return invoke<SensorSettings>("save_sensor_settings", { settings });
}

export async function getDriverInventory(): Promise<DriverInventory> {
  if (!isTauri()) return {
    capturedAtUnixMs: Date.now(), total: 3, signed: 2, unsigned: 0, missing: 1, problems: 1, source: "浏览器演示",
    items: [
      { deviceName: "未知 PCI 设备", deviceClass: "System", manufacturer: "", driverProvider: "", driverVersion: "", driverDate: "", infName: "", signer: "", signed: false, deviceId: "PCI\\VEN_DEMO", status: "Error", errorCode: 28, missingDriver: true },
      { deviceName: "演示显卡", deviceClass: "Display", manufacturer: "演示厂商", driverProvider: "演示厂商", driverVersion: "32.0.1", driverDate: "2026-01-01", infName: "oem1.inf", signer: "Microsoft Windows Hardware Compatibility Publisher", signed: true, deviceId: "PCI\\VEN_0000", status: "OK", errorCode: 0, missingDriver: false },
      { deviceName: "演示网卡", deviceClass: "Net", manufacturer: "演示厂商", driverProvider: "演示厂商", driverVersion: "1.0.0", driverDate: "2025-08-01", infName: "oem2.inf", signer: "Microsoft Windows Hardware Compatibility Publisher", signed: true, deviceId: "PCI\\VEN_0001", status: "OK", errorCode: 0, missingDriver: false },
    ],
  };
  return invoke<DriverInventory>("get_driver_inventory");
}

export async function scanDriverUpdates(): Promise<DriverUpdate[]> {
  if (!isTauri()) return [{ title: "演示厂商 - Display - 32.0.2", description: "来自 Windows Update 的演示驱动候选。", updateId: "demo-update-id", revisionNumber: 1, driverClass: "Display", driverManufacturer: "演示厂商", driverModel: "演示显卡", driverProvider: "演示厂商", driverVersion: "32.0.2", driverDate: "2026-06-01", categories: ["Drivers"], downloaded: false }];
  return invoke<DriverUpdate[]>("scan_driver_updates");
}

export async function runBenchmark(kind: BenchmarkResult["kind"]): Promise<BenchmarkResult> {
  if (kind === "gpu") return runGpuBenchmark();
  if (!isTauri()) return { schemaVersion: 1, kind, label: { "cpu-single": "CPU 单核", "cpu-multi": "CPU 多核", memory: "内存带宽", disk: "磁盘顺序读写" }[kind], score: 8560, unit: "分", durationMs: 1260, details: kind === "disk" ? { writeMiBPerSecond: 850, readMiBPerSecond: 2200, testFileMiB: 128 } : { throughput: 85.6 }, note: "浏览器演示结果" };
  return invoke<BenchmarkResult>("run_benchmark", { kind });
}

export async function getBenchmarkHistory(): Promise<BenchmarkRecord[]> {
  if (!isTauri()) return [];
  return invoke<BenchmarkRecord[]>("get_benchmark_history");
}

export async function getDiagnosticReport(): Promise<DiagnosticReport> {
  if (!isTauri()) return { capturedAtUnixMs: Date.now(), pendingReboot: false, pendingWindowsUpdateReboot: false, battery: { present: true, name: "演示电池", status: "OK", chargePercent: 72, designCapacityMwh: 86000, fullChargeCapacityMwh: 77400, healthPercent: 90, cycleCount: 128 }, events: [{ timeCreated: new Date().toISOString(), provider: "演示事件源", eventId: 1001, level: "错误", message: "浏览器预览事件。" }], crashEvents: [], serviceIssues: [{ name: "DemoService", displayName: "演示服务", state: "Stopped", startMode: "Auto", exitCode: 0 }], minidumpCount: 0, latestMinidump: "", source: "浏览器演示" };
  return invoke<DiagnosticReport>("get_diagnostic_report");
}

export async function getNetworkDiagnostics(): Promise<NetworkDiagnosticReport> {
  if (!isTauri()) return { capturedAtUnixMs: Date.now(), defaultGateways: ["192.168.1.1"], dns: [{ alias: "Wi-Fi", servers: ["1.1.1.1", "8.8.8.8"] }], proxyEnabled: false, proxyServer: "", autoConfigUrl: "", hostsCustomEntries: 2, connections: [{ processName: "browser", pid: 1234, localAddress: "192.168.1.10", localPort: 50123, remoteAddress: "93.184.216.34", remotePort: 443, state: "Established" }], source: "浏览器演示" };
  return invoke<NetworkDiagnosticReport>("get_network_diagnostics");
}

export async function getSoftwareInventory(): Promise<SoftwareInventory> {
  if (!isTauri()) return { capturedAtUnixMs: Date.now(), total: 2, estimatedSizeBytes: 2.4 * gib, source: "浏览器演示", items: [{ name: "演示应用", version: "1.2.0", publisher: "演示厂商", installDate: "20260101", installLocation: "C:\\Apps\\Demo", estimatedSizeBytes: 1.4 * gib, scope: "所有用户", architecture: "64-bit", uninstallAvailable: true }, { name: "示例工具", version: "3.0", publisher: "示例组织", installDate: "", installLocation: "", estimatedSizeBytes: 1 * gib, scope: "当前用户", architecture: "当前用户", uninstallAvailable: true }] };
  return invoke<SoftwareInventory>("get_software_inventory");
}

export async function scanStorage(kind: StorageScan["kind"]): Promise<StorageScan> {
  if (!isTauri()) return { kind, label: { downloads: "下载", documents: "文档", desktop: "桌面" }[kind], path: `C:\\Users\\Demo\\${kind}`, fileCount: 1280, directoryCount: 94, totalBytes: 12.4 * gib, errorCount: 0, truncated: false, categories: [{ extension: ".zip", fileCount: 18, totalBytes: 4.2 * gib }, { extension: ".mp4", fileCount: 12, totalBytes: 3.8 * gib }, { extension: ".exe", fileCount: 21, totalBytes: 2.1 * gib }] };
  return invoke<StorageScan>("scan_storage", { kind });
}

export async function scanLargeFiles(kind: StorageKind, minBytes: number, limit = 50): Promise<LargeFileScan> {
  if (!isTauri()) {
    return {
      kind,
      label: { downloads: "下载", documents: "文档", desktop: "桌面" }[kind],
      root: `C:\\Users\\Demo\\${kind}`,
      minBytes,
      files: [
        { path: `C:\\Users\\Demo\\${kind}\\demo-setup.exe`, name: "demo-setup.exe", bytes: 2.4 * gib, modifiedUnixMs: Date.now() - 86400000 },
        { path: `C:\\Users\\Demo\\${kind}\\movie.mp4`, name: "movie.mp4", bytes: 1.8 * gib, modifiedUnixMs: Date.now() - 3 * 86400000 },
        { path: `C:\\Users\\Demo\\${kind}\\archive.zip`, name: "archive.zip", bytes: 900 * 1024 ** 2, modifiedUnixMs: Date.now() - 10 * 86400000 },
      ],
      directories: [
        { path: `C:\\Users\\Demo\\${kind}\\Videos`, name: "Videos", bytes: 1.8 * gib, fileCount: 4 },
        { path: `C:\\Users\\Demo\\${kind}`, name: "(根目录)", bytes: 3.3 * gib, fileCount: 12 },
      ],
      totalBytes: 12.4 * gib,
      fileCount: 1280,
      errorCount: 0,
      truncated: false,
    };
  }
  return invoke<LargeFileScan>("scan_large_files", { kind, minBytes, limit });
}

export async function scanDuplicates(kind: StorageKind, limit = 50): Promise<DuplicateScan> {  if (!isTauri()) {
    return {
      kind,
      label: { downloads: "下载", documents: "文档", desktop: "桌面" }[kind],
      root: `C:\\Users\\Demo\\${kind}`,
      groups: [
        {
          sizeBytes: 1.2 * gib,
          hash: "demo",
          wastedBytes: 1.2 * gib,
          files: [
            { path: `C:\\Users\\Demo\\${kind}\\setup.exe`, name: "setup.exe", bytes: 1.2 * gib },
            { path: `C:\\Users\\Demo\\${kind}\\setup (1).exe`, name: "setup (1).exe", bytes: 1.2 * gib },
          ],
        },
      ],
      candidateFiles: 24,
      scannedBytes: 3.6 * gib,
      errorCount: 0,
      truncated: false,
    };
  }
  return invoke<DuplicateScan>("scan_duplicates", { kind, limit });
}

export async function getScheduledTasks(): Promise<ScheduledTaskInventory> {
  if (!isTauri()) {
    const tasks = [
      { name: "DemoUpdate", path: "\\Demo\\", state: "Ready", author: "演示厂商", description: "演示更新任务", runAsUser: "DEMO\\user", hidden: false, enabled: true, triggers: ["登录", "定时"] },
      { name: "DemoBackup", path: "\\Demo\\", state: "Disabled", author: "演示厂商", description: "演示备份任务", runAsUser: "SYSTEM", hidden: false, enabled: false, triggers: ["每天"] },
      { name: "DemoMaintenance", path: "\\Microsoft\\Windows\\", state: "Running", author: "Microsoft", description: "系统维护", runAsUser: "SYSTEM", hidden: true, enabled: true, triggers: ["维护"] },
    ];
    return {
      capturedAtUnixMs: Date.now(),
      total: tasks.length,
      enabled: 2,
      disabled: 1,
      running: 1,
      logonOrBoot: 1,
      tasks,
      source: "浏览器演示",
    };
  }
  return invoke<ScheduledTaskInventory>("get_scheduled_tasks");
}

export async function scanCleanupCandidates(): Promise<CleanupPreview> {
  if (!isTauri()) {
    const candidates = [
      { id: "user-temp", label: "用户临时文件", path: "C:\\Users\\Demo\\AppData\\Local\\Temp", exists: true, bytes: 3.4 * gib, fileCount: 8421, errorCount: 0, requiresAdmin: false, truncated: false, note: "应用运行时可能仍在写入，统计值会波动。", error: null },
      { id: "windows-temp", label: "Windows 临时文件", path: "C:\\Windows\\Temp", exists: true, bytes: 1.2 * gib, fileCount: 412, errorCount: 3, requiresAdmin: true, truncated: false, note: "部分系统临时文件需要管理员权限才能完整统计。", error: null },
      { id: "recycle-bin", label: "回收站", path: "C:\\$Recycle.Bin", exists: true, bytes: 900 * 1024 ** 2, fileCount: 64, errorCount: 1, requiresAdmin: true, truncated: false, note: "普通权限只能看到当前用户的部分内容。", error: null },
      { id: "edge-cache", label: "Edge 浏览器缓存", path: "C:\\Users\\Demo\\AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache", exists: true, bytes: 640 * 1024 ** 2, fileCount: 2100, errorCount: 0, requiresAdmin: false, truncated: false, note: "浏览器运行时缓存文件可能被占用，统计值会波动。", error: null },
    ];
    return {
      capturedAtUnixMs: Date.now(),
      candidates,
      totalBytes: candidates.reduce((sum, item) => sum + item.bytes, 0),
      totalFiles: candidates.reduce((sum, item) => sum + item.fileCount, 0),
      source: "浏览器演示",
    };
  }
  return invoke<CleanupPreview>("scan_cleanup_candidates");
}

function compileShader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("无法创建 GPU 着色器");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) || "GPU 着色器编译失败");
  return shader;
}

export async function runGpuBenchmark(): Promise<BenchmarkResult> {
  const width = 1024, height = 1024, frames = 90;
  const canvas = document.createElement("canvas");
  canvas.width = width; canvas.height = height;
  const gl = canvas.getContext("webgl2", { antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false, powerPreference: "high-performance" });
  if (!gl) throw new Error("当前 WebView2 或显卡驱动不支持 WebGL2");
  const vertex = compileShader(gl, gl.VERTEX_SHADER, `#version 300 es\nprecision highp float;\nconst vec2 p[3]=vec2[3](vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));\nout vec2 uv;\nvoid main(){gl_Position=vec4(p[gl_VertexID],0.,1.);uv=p[gl_VertexID]*.5+.5;}`);
  const fragment = compileShader(gl, gl.FRAGMENT_SHADER, `#version 300 es\nprecision highp float;\nin vec2 uv;uniform float frame;out vec4 color;\nvoid main(){vec2 z=uv*2.-1.;float a=frame*.001;for(int i=0;i<48;i++){z=vec2(sin(z.x*z.y*3.1+a)+cos(z.y*1.7),cos(z.x*2.3-a)-sin(z.y*z.x));a+=dot(z,z)*.0007;}color=vec4(fract(z.x),fract(z.y),fract(a),1.);}`);
  const program = gl.createProgram();
  if (!program) throw new Error("无法创建 GPU 测试程序");
  gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) || "GPU 测试程序链接失败");
  gl.useProgram(program); gl.viewport(0, 0, width, height);
  const frameLocation = gl.getUniformLocation(program, "frame");
  for (let frame = 0; frame < 3; frame++) { gl.uniform1f(frameLocation, frame); gl.drawArrays(gl.TRIANGLES, 0, 3); gl.finish(); }
  const started = performance.now();
  for (let frame = 0; frame < frames; frame++) { gl.uniform1f(frameLocation, frame + 3); gl.drawArrays(gl.TRIANGLES, 0, 3); gl.finish(); }
  const durationMs = performance.now() - started;
  const megapixelsPerSecond = width * height * frames / durationMs / 1000;
  const debug = gl.getExtension("WEBGL_debug_renderer_info");
  const renderer = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : String(gl.getParameter(gl.RENDERER));
  gl.deleteProgram(program); gl.deleteShader(vertex); gl.deleteShader(fragment);
  const result: BenchmarkResult = { schemaVersion: 2, kind: "gpu", label: "GPU 着色器", score: megapixelsPerSecond * 10, unit: "分", durationMs: Math.round(durationMs), details: { megapixelsPerSecond, width, height, frames, shaderIterations: 48 }, note: `WebGL2 离屏着色器；${renderer}` };
  if (isTauri()) await invoke("save_external_benchmark", { result });
  return result;
}

export function isDesktopRuntime(): boolean {
  return isTauri();
}
