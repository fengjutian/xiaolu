import { invoke, isTauri } from "@tauri-apps/api/core";
import type { BenchmarkResult, DriverInventory, HardwareDetails, SensorSnapshot, StartupItem, SystemSnapshot } from "./types";

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

export async function getSensorSnapshot(): Promise<SensorSnapshot> {
  if (!isTauri()) return {
    capturedAtUnixMs: Date.now(),
    readings: [
      { id: "demo-gpu-temp", name: "演示 GPU 温度", category: "temperature", value: 54, unit: "°C", source: "浏览器演示" },
      { id: "demo-gpu-fan", name: "演示 GPU 风扇", category: "fan", value: 32, unit: "%", source: "浏览器演示" },
      { id: "demo-gpu-power", name: "演示 GPU 功耗", category: "power", value: 48, unit: "W", source: "浏览器演示" },
    ],
    storage: [{ name: "演示 NVMe SSD", healthStatus: "Healthy", operationalStatus: "OK", temperatureC: 39, wearPercent: 4, powerOnHours: 1260, readErrorsTotal: 0, writeErrorsTotal: 0, source: "浏览器演示" }],
    smartDevices: [{ instanceName: "演示 SATA SSD", predictFailure: false, source: "浏览器演示", attributes: [{ id: 5, name: "重映射扇区数", current: 100, worst: 100, raw: 0 }, { id: 9, name: "通电时间", current: 98, worst: 98, raw: 1260 }, { id: 194, name: "温度", current: 61, worst: 48, raw: 39 }] }],
    limitations: [],
  };
  return invoke<SensorSnapshot>("get_sensor_snapshot");
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

export async function runBenchmark(kind: BenchmarkResult["kind"]): Promise<BenchmarkResult> {
  if (!isTauri()) return { schemaVersion: 1, kind, label: { "cpu-single": "CPU 单核", "cpu-multi": "CPU 多核", memory: "内存带宽", disk: "磁盘顺序读写" }[kind], score: 8560, unit: "分", durationMs: 1260, details: kind === "disk" ? { writeMiBPerSecond: 850, readMiBPerSecond: 2200, testFileMiB: 128 } : { throughput: 85.6 }, note: "浏览器演示结果" };
  return invoke<BenchmarkResult>("run_benchmark", { kind });
}

export function isDesktopRuntime(): boolean {
  return isTauri();
}
