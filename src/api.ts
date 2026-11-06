import { invoke, isTauri } from "@tauri-apps/api/core";
import type { BenchmarkPreflight, BenchmarkRecord, BenchmarkResult, CleanupPreview, DeviceDetails, DiagnosticReport, DirectoryListing, DriverAssessmentReport, DriverInventory, DriverUpdate, DuplicateScan, EventLogResult, EyeCareSettings, EyeCareStatus, HardwareDetails, LargeFileScan, NetworkDiagnosticReport, NotificationOutcome, NotificationSettings, ProcessInfo, NvmeHealthReport, ScheduledTaskInventory, SensorHistorySeries, SensorSettings, SensorSnapshot, SensorValue, SlimScan, SoftwareInventory, StartupItem, StorageDrive, StorageKind, StorageScan, SystemHistorySeries, SystemSnapshot, ToolDescriptor } from "./types";

const gib = 1024 ** 3;

const storageLabel = (kind: string): string => {
  if (kind.startsWith("drive:")) {
    return `${kind.slice("drive:".length).toUpperCase()} 盘`;
  }
  return (
    { downloads: "下载", documents: "文档", desktop: "桌面" } as Record<
      string,
      string
    >
  )[kind] ?? kind;
};

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

export async function getProcessInventory(): Promise<ProcessInfo[]> {
  if (!isTauri()) return mockSnapshot().processes;
  return invoke<ProcessInfo[]>("get_process_inventory");
}

export async function getSystemHistory(hours: number): Promise<SystemHistorySeries> {
  if (!isTauri()) {
    const now = Date.now();
    const count = 60;
    const bucketMs = Math.max(10_000, Math.floor((hours * 3_600_000) / count));
    const points = Array.from({ length: count }, (_, index) => {
      const cpu = 45 + Math.sin(index / 6) * 10;
      return {
        t: now - (count - 1 - index) * bucketMs,
        cpuPercent: cpu,
        memoryPercent: 60 + Math.cos(index / 8) * 5,
        downloadBytesPerSec: 2 * 1024 ** 2 + Math.sin(index) * 1024 ** 2,
        uploadBytesPerSec: 300 * 1024 + Math.cos(index) * 100 * 1024,
        diskReadBytesPerSec: 5 * 1024 ** 2,
        diskWriteBytesPerSec: 2 * 1024 ** 2,
      };
    });
    return { hours, bucketMs, points, sampleCount: count };
  }
  return invoke<SystemHistorySeries>("get_system_history", { hours });
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
      storage: [{ name: "演示 NVMe SSD", healthStatus: "Healthy", operationalStatus: "OK", temperatureC: 39, wearPercent: 4, powerOnHours: 1260, readErrorsTotal: 0, writeErrorsTotal: 0, source: "浏览器演示", assessment: { level: "healthy", signals: [] } }],
      smartDevices: [{ instanceName: "演示 SATA SSD", predictFailure: false, source: "浏览器演示", assessment: { level: "healthy", signals: [] }, attributes: [{ id: 5, name: "重映射扇区数", current: 100, worst: 100, raw: 0 }, { id: 9, name: "通电时间", current: 98, worst: 98, raw: 1260 }, { id: 194, name: "温度", current: 61, worst: 48, raw: 39 }] }],
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
  if (!isTauri()) return [{ title: "演示厂商 - Display - 32.0.2", description: "来自 Windows Update 的演示驱动候选。", updateId: "demo-update-id", revisionNumber: 1, driverClass: "Display", driverManufacturer: "演示厂商", driverModel: "演示显卡", driverProvider: "演示厂商", driverVersion: "32.0.2", driverDate: "2026-06-01", categories: ["Drivers"], downloaded: false, driverHardwareId: "PCI\\VEN_DEMO&DEV_0001", signed: true }];
  return invoke<DriverUpdate[]>("scan_driver_updates");
}

export async function assessDriverUpdates(): Promise<DriverAssessmentReport> {
  if (!isTauri()) {
    return {
      capturedAtUnixMs: Date.now(),
      updates: [
        {
          update: {
            title: "演示厂商 - Display - 32.0.2",
            description: "来自 Windows Update 的演示驱动候选。",
            updateId: "demo-update-id",
            revisionNumber: 1,
            driverClass: "Display",
            driverManufacturer: "演示厂商",
            driverModel: "演示显卡",
            driverProvider: "演示厂商",
            driverVersion: "32.0.2",
            driverDate: "2026-06-01",
            categories: ["Drivers"],
            downloaded: false,
            driverHardwareId: "PCI\\VEN_DEMO&DEV_0001",
            signed: true,
          },
          signatureOk: true,
          status: "applicable",
          matchingDevices: ["演示显卡（当前 32.0.1）"],
          notes: ["候选版本 32.0.2 高于已安装的 32.0.1。"],
        },
        {
          update: {
            title: "演示厂商 - Net - 1.0.0",
            description: "无匹配设备的演示候选。",
            updateId: "demo-update-id-2",
            revisionNumber: 2,
            driverClass: "Net",
            driverManufacturer: "演示厂商",
            driverModel: "演示网卡",
            driverProvider: "演示厂商",
            driverVersion: "1.0.0",
            driverDate: "2026-03-01",
            categories: ["Drivers"],
            downloaded: false,
            driverHardwareId: "PCI\\VEN_DEMO&DEV_9999",
            signed: true,
          },
          signatureOk: true,
          status: "no_match",
          matchingDevices: [],
          notes: ["候选硬件 ID 未匹配到本机设备。"],
        },
      ],
      inventorySource: "浏览器演示",
      note: "只读评估：按硬件 ID 匹配设备并比较版本；不会下载、安装或回滚驱动。",
    };
  }
  return invoke<DriverAssessmentReport>("assess_driver_updates");
}

export async function runBenchmark(kind: BenchmarkResult["kind"], diskSizeMiB?: number): Promise<BenchmarkResult> {
  if (kind === "gpu") return runGpuBenchmark();
  if (!isTauri()) return { schemaVersion: 1, kind, label: { "cpu-single": "CPU 单核", "cpu-multi": "CPU 多核", memory: "内存带宽", disk: "磁盘顺序读写" }[kind], score: 8560, unit: "分", durationMs: 1260, details: kind === "disk" ? { writeMiBPerSecond: 850, readMiBPerSecond: 2200, testFileMiB: diskSizeMiB ?? 1024 } : { throughput: 85.6 }, note: "浏览器演示结果" };
  return invoke<BenchmarkResult>("run_benchmark", { kind, diskSizeMiB: diskSizeMiB ?? null });
}

export async function benchmarkPreflight(): Promise<BenchmarkPreflight> {
  if (!isTauri()) {
    return { maxTemperatureC: 61, onBattery: false, warnings: [], source: "浏览器演示" };
  }
  return invoke<BenchmarkPreflight>("benchmark_preflight");
}

export async function cancelBenchmark(): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_benchmark");
}

export async function getBenchmarkHistory(): Promise<BenchmarkRecord[]> {
  if (!isTauri()) {
    return [
      {
        recordedAtUnixMs: Date.now() - 3_600_000,
        result: {
          schemaVersion: 1,
          kind: "cpu-single",
          label: "CPU 单核",
          score: 8560,
          unit: "分",
          durationMs: 1260,
          details: { throughput: 85.6 },
          note: "浏览器演示结果",
        },
        environment: {
          appVersion: "0.23.0",
          cpuBrand: "演示处理器",
          logicalCores: 16,
          memoryBytes: 32 * gib,
          osName: "Windows",
          osVersion: "11 Pro",
        },
      },
    ];
  }
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

export async function scanStorage(kind: string): Promise<StorageScan> {
  if (!isTauri()) return { kind, label: storageLabel(kind), path: `C:\\Users\\Demo\\${kind}`, fileCount: 1280, directoryCount: 94, totalBytes: 12.4 * gib, errorCount: 0, truncated: false, categories: [{ extension: ".zip", fileCount: 18, totalBytes: 4.2 * gib }, { extension: ".mp4", fileCount: 12, totalBytes: 3.8 * gib }, { extension: ".exe", fileCount: 21, totalBytes: 2.1 * gib }] };
  return invoke<StorageScan>("scan_storage", { kind });
}

export async function getEyeCareSettings(): Promise<EyeCareSettings> {
  if (!isTauri()) {
    return {
      enabled: false,
      warmth: 40,
      breakIntervalMinutes: 45,
      breakDurationSeconds: 20,
      scheduleEnabled: false,
      scheduleStartHour: 20,
      scheduleEndHour: 8,
    };
  }
  return invoke<EyeCareSettings>("get_eye_care_settings");
}

export async function saveEyeCareSettings(
  settings: EyeCareSettings,
): Promise<EyeCareSettings> {
  if (!isTauri()) return settings;
  return invoke<EyeCareSettings>("save_eye_care_settings", { settings });
}

export async function setEyeCareFilter(
  enabled: boolean,
  warmth: number,
): Promise<EyeCareStatus> {
  if (!isTauri()) {
    return {
      applied: enabled,
      displays: enabled ? ["演示显示器"] : [],
      failed: [],
      message: "浏览器演示不调整真实色温。",
    };
  }
  return invoke<EyeCareStatus>("set_eye_care_filter", { enabled, warmth });
}

export async function sendBreakReminder(): Promise<NotificationOutcome> {
  if (!isTauri()) {
    return { status: "disabled", message: "浏览器演示不发送系统通知。" };
  }
  return invoke<NotificationOutcome>("send_break_reminder");
}
export async function getStorageDrives(): Promise<StorageDrive[]> {
  if (!isTauri()) {
    return [
      { letter: "C", label: "系统盘", mountPoint: "C:\\", fileSystem: "NTFS", totalBytes: 512 * gib, freeBytes: 120 * gib, removable: false },
      { letter: "D", label: "数据盘", mountPoint: "D:\\", fileSystem: "NTFS", totalBytes: 1024 * gib, freeBytes: 640 * gib, removable: false },
    ];
  }
  return invoke<StorageDrive[]>("get_storage_drives");
}

export async function getDeviceDetails(): Promise<DeviceDetails> {
  if (!isTauri()) {
    return {
      capturedAtUnixMs: Date.now(),
      monitors: [{ name: "DELL U2723QE", manufacturer: "DEL", product: "U2723QE", serial: "DEMO1234", year: 2023 }],
      audio: [
        { name: "扬声器 (Realtek(R) Audio)", manufacturer: "Realtek", status: "OK" },
        { name: "麦克风阵列 (Intel Smart Sound)", manufacturer: "Intel", status: "OK" },
      ],
      usb: [
        { name: "USB Composite Device", manufacturer: "(标准 USB 主机控制器)", status: "OK", vid: "046D", pid: "C52B" },
        { name: "USB 大容量存储设备", manufacturer: "Generic", status: "OK", vid: "0781", pid: "5581" },
      ],
      bluetooth: [
        { name: "Intel(R) Wireless Bluetooth(R)", manufacturer: "Intel", status: "OK", kind: "适配器" },
        { name: "WH-1000XM5", manufacturer: "Sony", status: "OK", kind: "已配对/近场设备" },
      ],
      cameras: [
        { name: "Integrated Camera", manufacturer: "SunplusIT", status: "OK", pnpClass: "Camera" },
      ],
      unavailable: [],
      source: "浏览器演示",
    };
  }
  return invoke<DeviceDetails>("get_device_details");
}

export async function getNotificationSettings(): Promise<NotificationSettings> {
  if (!isTauri()) {
    return { enabled: true, cooldownMinutes: 10, quietStartHour: 23, quietEndHour: 8 };
  }
  return invoke<NotificationSettings>("get_notification_settings");
}

export async function saveNotificationSettings(
  settings: NotificationSettings,
): Promise<NotificationSettings> {
  if (!isTauri()) return settings;
  return invoke<NotificationSettings>("save_notification_settings", { settings });
}

export async function notifySensorAlert(payload: {
  alertId: string;
  level: string;
  title: string;
  message: string;
}): Promise<NotificationOutcome> {
  if (!isTauri()) {
    return { status: "disabled", message: "浏览器演示不发送系统通知。" };
  }
  return invoke<NotificationOutcome>("notify_sensor_alert", payload);
}

export async function sendTestNotification(): Promise<NotificationOutcome> {
  if (!isTauri()) {
    return { status: "disabled", message: "浏览器演示不发送系统通知。" };
  }
  return invoke<NotificationOutcome>("send_test_notification");
}

export async function scanLargeFiles(kind: StorageKind, minBytes: number, limit = 50): Promise<LargeFileScan> {
  if (!isTauri()) {
    return {
      kind,
      label: storageLabel(kind),
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
      label: storageLabel(kind),
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

export async function scanDirectory(
  kind: StorageKind,
  relative = "",
  limit = 200,
): Promise<DirectoryListing> {
  if (!isTauri()) {
    const children = relative
      ? [
          { name: "installer.exe", path: `C:\\Users\\Demo\\${kind}\\${relative}\\installer.exe`, relativePath: `${relative}\\installer.exe`, isDir: false, bytes: 1.2 * gib, fileCount: 1 },
          { name: "notes.txt", path: `C:\\Users\\Demo\\${kind}\\${relative}\\notes.txt`, relativePath: `${relative}\\notes.txt`, isDir: false, bytes: 12 * 1024, fileCount: 1 },
        ]
      : [
          { name: "Videos", path: `C:\\Users\\Demo\\${kind}\\Videos`, relativePath: "Videos", isDir: true, bytes: 3.8 * gib, fileCount: 12 },
          { name: "Installers", path: `C:\\Users\\Demo\\${kind}\\Installers`, relativePath: "Installers", isDir: true, bytes: 2.1 * gib, fileCount: 6 },
          { name: "readme.txt", path: `C:\\Users\\Demo\\${kind}\\readme.txt`, relativePath: "readme.txt", isDir: false, bytes: 4 * 1024, fileCount: 1 },
        ];
    const totalBytes = children.reduce((sum, item) => sum + item.bytes, 0);
    const totalFiles = children.reduce((sum, item) => sum + item.fileCount, 0);
    return {
      kind,
      label: storageLabel(kind),
      root: `C:\\Users\\Demo\\${kind}`,
      relativePath: relative,
      path: relative ? `C:\\Users\\Demo\\${kind}\\${relative}` : `C:\\Users\\Demo\\${kind}`,
      parentRelativePath: relative
        ? relative.split("\\").slice(0, -1).join("\\")
        : null,
      children,
      totalBytes,
      totalFiles,
      errorCount: 0,
      truncated: false,
    };
  }
  return invoke<DirectoryListing>("scan_directory", { kind, relative, limit });
}

export async function getScheduledTasks(): Promise<ScheduledTaskInventory> {
  if (!isTauri()) {
    const tasks = [
      { name: "DemoUpdate", path: "\\Demo\\", state: "Ready", author: "演示厂商", description: "演示更新任务", runAsUser: "DEMO\\user", hidden: false, enabled: true, triggers: ["登录", "定时"], lastRunUnixMs: Date.now() - 3_600_000, nextRunUnixMs: Date.now() + 3_600_000, lastResult: 0 },
      { name: "DemoBackup", path: "\\Demo\\", state: "Disabled", author: "演示厂商", description: "演示备份任务", runAsUser: "SYSTEM", hidden: false, enabled: false, triggers: ["每天"], lastRunUnixMs: null, nextRunUnixMs: null, lastResult: null },
      { name: "DemoMaintenance", path: "\\Microsoft\\Windows\\", state: "Running", author: "Microsoft", description: "系统维护", runAsUser: "SYSTEM", hidden: true, enabled: true, triggers: ["维护"], lastRunUnixMs: null, nextRunUnixMs: null, lastResult: null },
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

export async function queryEventLog(
  logName: string,
  level: string,
  hours: number,
  search: string,
  limit = 100,
): Promise<EventLogResult> {
  if (!isTauri()) {
    const events = [
      { timeCreatedUnixMs: Date.now() - 3_600_000, logName, provider: "Demo-Provider", eventId: 41, level: "错误", message: "演示事件：系统服务启动失败。" },
      { timeCreatedUnixMs: Date.now() - 7_200_000, logName, provider: "Demo-Driver", eventId: 219, level: "警告", message: "演示事件：驱动加载耗时过长。" },
      { timeCreatedUnixMs: Date.now() - 10_800_000, logName, provider: "Demo-App", eventId: 1000, level: "信息", message: "演示事件：应用正常启动。" },
    ].filter((event) => !search || event.message.includes(search));
    return {
      logName,
      level,
      hours,
      total: events.length,
      events,
      truncated: false,
      source: "浏览器演示",
    };
  }
  return invoke<EventLogResult>("query_event_log", {
    logName,
    level,
    hours,
    search,
    limit,
  });
}

export async function getNvmeHealth(): Promise<NvmeHealthReport> {
  if (!isTauri()) {
    return {
      capturedAtUnixMs: Date.now(),
      devices: [
        {
          device: "\\\\.\\PhysicalDrive0",
          criticalWarning: 0,
          temperatureC: 41,
          availableSparePercent: 100,
          spareThresholdPercent: 10,
          percentageUsed: 3,
          dataReadBytes: 1.8 * 1024 ** 4,
          dataWrittenBytes: 640 * gib,
          powerOnHours: 1260,
          powerCycles: 128,
          unsafeShutdowns: 2,
          mediaErrors: 0,
          errorLogEntries: 4,
          source: "浏览器演示",
        },
      ],
      accessDenied: false,
      unsupported: false,
      lastError: null,
      note: "浏览器演示数据",
    };
  }
  return invoke<NvmeHealthReport>("get_nvme_health");
}

function compileShader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("无法创建 GPU 着色器");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) || "GPU 着色器编译失败");
  return shader;
}

let gpuCancelRequested = false;

/** 请求取消正在运行的 GPU 跑分（WebGPU 分批提交，在批次之间检查）。 */
export function cancelGpuBenchmark() {
  gpuCancelRequested = true;
}

async function runWebGpuBenchmark(gpu: {
  requestAdapter: (options?: unknown) => Promise<unknown>;
}): Promise<BenchmarkResult> {
  const adapter = (await gpu.requestAdapter({
    powerPreference: "high-performance",
  })) as {
    info?: {
      vendor?: string;
      architecture?: string;
      device?: string;
      description?: string;
    };
    requestDevice: () => Promise<unknown>;
  } | null;
  if (!adapter) throw new Error("WebGPU 适配器不可用");
  const device = (await adapter.requestDevice()) as {
    createBuffer: (options: unknown) => {
      size: number;
      mapAsync: (mode: number) => Promise<void>;
      getMappedRange: () => ArrayBuffer;
      unmap: () => void;
      destroy: () => void;
    };
    createShaderModule: (options: unknown) => unknown;
    createComputePipeline: (options: unknown) => {
      getBindGroupLayout: (index: number) => unknown;
    };
    createBindGroup: (options: unknown) => unknown;
    createCommandEncoder: () => {
      beginComputePass: () => {
        setPipeline: (pipeline: unknown) => void;
        setBindGroup: (index: number, group: unknown) => void;
        dispatchWorkgroups: (count: number) => void;
        end: () => void;
      };
      copyBufferToBuffer: (
        source: unknown,
        sourceOffset: number,
        destination: unknown,
        destinationOffset: number,
        size: number,
      ) => void;
      finish: () => unknown;
    };
    queue: {
      submit: (commands: unknown[]) => void;
      onSubmittedWorkDone: () => Promise<void>;
    };
    destroy: () => void;
  };

  const BUFFER_USAGE_STORAGE = 0x0080;
  const BUFFER_USAGE_COPY_SRC = 0x0004;
  const BUFFER_USAGE_COPY_DST = 0x0008;
  const BUFFER_USAGE_MAP_READ = 0x0001;
  const MAP_MODE_READ = 0x0001;

  const elements = 1 << 21;
  const bytes = elements * 4;
  const iterations = 1024;
  const batches = 8;
  const workgroups = Math.ceil(elements / 64);

  const buffer = device.createBuffer({
    size: bytes,
    usage: BUFFER_USAGE_STORAGE | BUFFER_USAGE_COPY_SRC | BUFFER_USAGE_COPY_DST,
  });
  const readback = device.createBuffer({
    size: bytes,
    usage: BUFFER_USAGE_COPY_DST | BUFFER_USAGE_MAP_READ,
  });

  const shader = device.createShaderModule({
    code: `
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  let index = id.x;
  if (index >= arrayLength(&data)) { return; }
  var value: u32 = data[index];
  for (var i: u32 = 0u; i < ${iterations}u; i = i + 1u) {
    value = value * 1664525u + 1013904223u;
    value = value ^ (value >> 13u);
    value = value + id.x;
  }
  data[index] = value;
}`,
  });
  const pipeline = device.createComputePipeline({
    layout: "auto",
    compute: { module: shader, entryPoint: "main" },
  });
  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer } }],
  });

  const dispatch = () => {
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.dispatchWorkgroups(workgroups);
    pass.end();
    device.queue.submit([encoder.finish()]);
  };

  try {
    dispatch();
    await device.queue.onSubmittedWorkDone();
    gpuCancelRequested = false;
    const started = performance.now();
    for (let batch = 0; batch < batches; batch += 1) {
      if (gpuCancelRequested) {
        throw new Error("BENCHMARK_CANCELLED");
      }
      dispatch();
      await device.queue.onSubmittedWorkDone();
    }
    const durationMs = performance.now() - started;

    let checksum = 0;
    try {
      const encoder = device.createCommandEncoder();
      encoder.copyBufferToBuffer(buffer, 0, readback, 0, bytes);
      device.queue.submit([encoder.finish()]);
      await readback.mapAsync(MAP_MODE_READ);
      const mapped = new Uint32Array(readback.getMappedRange().slice(0));
      for (let index = 0; index < mapped.length; index += 4096) {
        checksum ^= mapped[index];
      }
      readback.unmap();
    } catch {
      checksum = 0;
    }

    const operations = elements * iterations * batches * 4;
    const opsPerSecond = (operations / Math.max(durationMs, 1)) * 1000;
    const info = adapter.info ?? {};
    const renderer =
      info.description || info.device || info.vendor || "WebGPU 设备";
    return {
      schemaVersion: 3,
      kind: "gpu",
      label: "GPU 计算（WebGPU / D3D12）",
      score: opsPerSecond / 40_000_000,
      unit: "分",
      durationMs: Math.round(durationMs),
      details: {
        elements,
        iterations,
        batches,
        workgroups,
        opsPerSecond,
        checksum,
        backend: 1,
      },
      note: `WebGPU 计算管线（Windows 由 D3D12 后端执行）；分数已归一化，仅同版本比较；${renderer}`,
    };
  } finally {
    device.destroy();
  }
}

async function runWebGlBenchmark(): Promise<BenchmarkResult> {
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
  return { schemaVersion: 2, kind: "gpu", label: "GPU 着色器", score: megapixelsPerSecond * 10, unit: "分", durationMs: Math.round(durationMs), details: { megapixelsPerSecond, width, height, frames, shaderIterations: 48, backend: 0 }, note: `WebGL2 离屏着色器（WebGPU 不可用时的回退）；${renderer}` };
}

export async function runGpuBenchmark(): Promise<BenchmarkResult> {
  gpuCancelRequested = false;
  const gpu = (navigator as unknown as {
    gpu?: { requestAdapter: (options?: unknown) => Promise<unknown> };
  }).gpu;
  let result: BenchmarkResult;
  if (gpu) {
    try {
      result = await runWebGpuBenchmark(gpu);
    } catch (error) {
      if (
        error instanceof Error &&
        error.message.includes("BENCHMARK_CANCELLED")
      ) {
        throw error;
      }
      console.warn("WebGPU 跑分失败，回退 WebGL2：", error);
      result = await runWebGlBenchmark();
    }
  } else {
    result = await runWebGlBenchmark();
  }
  if (isTauri()) await invoke("save_external_benchmark", { result });
  return result;
}


export async function listToolboxTools(): Promise<ToolDescriptor[]> {
  if (!isTauri()) {
    return [
      { id: "cclean", name: "C 盘瘦身", category: "清理", description: "只读统计 C 盘可清理位置与系统托管文件", risk: "read_only", requiresAdmin: false, status: "available", note: "当前仅预览，不删除", uiKind: "table" },
      { id: "sys_clean", name: "Windows 系统瘦身", category: "清理", description: "只读列出已安装应用包与系统内置候选", risk: "read_only", requiresAdmin: false, status: "available", note: "只读清单", uiKind: "table" },
      { id: "privacy_clean", name: "隐私清理", category: "隐私", description: "只读统计浏览器记录、缓存与最近使用痕迹的占用", risk: "read_only", requiresAdmin: false, status: "available", note: "只读统计", uiKind: "table" },
      { id: "dll_repair", name: "DLL 修复", category: "修复", description: "只读检查常见运行库 DLL 是否存在", risk: "read_only", requiresAdmin: false, status: "available", note: "只读检查", uiKind: "table" },
      { id: "DuplicateFileClean", name: "重复文件清理", category: "清理", description: "重复文件检测与清理（规划）", risk: "read_only", requiresAdmin: false, status: "partial", note: "已具备只读检测", uiKind: "none" },
      { id: "driver_manager", name: "驱动管理", category: "驱动", description: "驱动检测、更新预检与安装（规划）", risk: "system", requiresAdmin: true, status: "partial", note: "已具备检测与只读预检", uiKind: "none" },
      { id: "defrag", name: "磁盘碎片整理", category: "优化", description: "HDD 碎片整理与 SSD TRIM（规划）", risk: "system", requiresAdmin: true, status: "planned", note: "调用系统 API，需确认磁盘类型", uiKind: "none" },
    ];
  }
  return invoke<ToolDescriptor[]>("list_toolbox_tools");
}

export async function runToolAction(
  toolId: string,
  action: string,
  confirmation?: string | null,
  parameters?: Record<string, unknown> | null,
): Promise<unknown> {
  if (!isTauri()) {
    if (toolId === "sys_clean") {
      return {
        summary: [
          { label: "已安装应用包", value: "42" },
          { label: "系统内置候选", value: "28" },
        ],
        columns: [
          { key: "name", label: "应用包" },
          { key: "version", label: "版本" },
          { key: "type", label: "类型" },
        ],
        rows: [
          { name: "Microsoft.WindowsCalculator", version: "11.0.0", type: "系统内置", location: "" },
          { name: "Microsoft.Paint", version: "11.0.0", type: "系统内置", location: "" },
        ],
        notes: ["只读清单：不会卸载或修改任何应用包。"],
      };
    }
    if (toolId === "privacy_clean") {
      return {
        summary: [{ label: "合计占用", value: "1.8 GB" }],
        columns: [
          { key: "label", label: "位置" },
          { key: "size", label: "占用" },
        ],
        rows: [
          { label: "Edge 缓存", size: "820.0 MB" },
          { label: "最近使用文件", size: "12.0 MB" },
        ],
        notes: ["只读统计：不读取、不上传、不删除任何浏览记录或 Cookie。"],
      };
    }
    if (toolId === "dll_repair") {
      return {
        summary: [{ label: "缺失项", value: "0" }],
        columns: [
          { key: "dll", label: "DLL" },
          { key: "arch", label: "架构" },
          { key: "status", label: "状态" },
        ],
        rows: [
          { dll: "vcruntime140.dll", arch: "System32", status: "存在" },
          { dll: "msvcp140.dll", arch: "System32", status: "存在" },
        ],
        notes: ["只读检查：不下载、不替换任何 DLL。"],
      };
    }
    const items = [
      { id: "user-temp", label: "用户临时文件", path: "C:\\Users\\Demo\\AppData\\Local\\Temp", bytes: 3.4 * gib, fileCount: 8421, category: "safe", requiresAdmin: false, truncated: false, note: "应用运行时可能仍在写入。" },
      { id: "windows-update", label: "Windows Update 下载缓存", path: "C:\\Windows\\SoftwareDistribution\\Download", bytes: 1.2 * gib, fileCount: 10, category: "caution", requiresAdmin: true, truncated: false, note: "更新缓存由系统管理。" },
      { id: "recycle-bin", label: "回收站", path: "C:\\$Recycle.Bin", bytes: 900 * 1024 ** 2, fileCount: 64, category: "caution", requiresAdmin: true, truncated: false, note: "普通权限只能看到部分内容。" },
      { id: "system-hiberfil.sys", label: "休眠文件 hiberfil.sys", path: "C:\\hiberfil.sys", bytes: 6.4 * gib, fileCount: 1, category: "system", requiresAdmin: false, truncated: false, note: "可在电源设置中关闭休眠以释放空间。" },
    ];
    return {
      capturedAtUnixMs: Date.now(),
      driveLetter: "C:",
      driveTotalBytes: 512 * gib,
      driveFreeBytes: 120 * gib,
      items,
      safeBytes: 3.4 * gib,
      cautionBytes: 2.1 * gib,
      systemBytes: 6.4 * gib,
      fileCount: 8496,
      note: "只读预览：只统计占用，不删除任何文件。",
    };
  }
  return invoke<unknown>("run_tool_action", {
    toolId,
    action,
    confirmation: confirmation ?? null,
    parameters: parameters ?? null,
  });
}

export function isDesktopRuntime(): boolean {
  return isTauri();
}

export async function openPathInExplorer(path: string): Promise<void> {
  if (!isTauri()) {
    throw new Error("打开目录仅在 Windows 桌面版中可用");
  }
  await invoke("open_path_in_explorer", { path });
}
