import { invoke, isTauri } from "@tauri-apps/api/core";
import type { SystemSnapshot } from "./types";

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
      },
    ],
    processes: [
      { pid: 4120, name: "Code.exe", cpuPercent: 8.2, memoryBytes: 1.2 * gib },
      { pid: 1736, name: "msedge.exe", cpuPercent: 5.4, memoryBytes: 824 * 1024 ** 2 },
      { pid: 988, name: "explorer.exe", cpuPercent: 1.8, memoryBytes: 286 * 1024 ** 2 },
      { pid: 4, name: "System", cpuPercent: 1.1, memoryBytes: 12 * 1024 ** 2 },
    ],
  };
}

export async function getSystemSnapshot(): Promise<SystemSnapshot> {
  if (!isTauri()) return mockSnapshot();
  return invoke<SystemSnapshot>("get_system_snapshot");
}

export function isDesktopRuntime(): boolean {
  return isTauri();
}
