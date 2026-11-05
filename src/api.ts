import { invoke, isTauri } from "@tauri-apps/api/core";
import type { HardwareDetails, StartupItem, SystemSnapshot } from "./types";

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

export function isDesktopRuntime(): boolean {
  return isTauri();
}
