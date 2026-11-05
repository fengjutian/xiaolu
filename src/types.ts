export interface CpuInfo {
  brand: string;
  logicalCores: number;
  usagePercent: number;
  frequencyMhz: number;
}

export interface MemoryInfo {
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
}

export interface DiskInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  availableBytes: number;
  removable: boolean;
  readBytesPerSec: number;
  writtenBytesPerSec: number;
}

export interface NetworkInfo {
  downloadBytesPerSec: number;
  uploadBytesPerSec: number;
  totalReceivedBytes: number;
  totalTransmittedBytes: number;
  interfaces: number;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  cpuPercent: number;
  memoryBytes: number;
}

export interface SystemSnapshot {
  hostName: string;
  osName: string;
  osVersion: string;
  kernelVersion: string;
  architecture: string;
  uptimeSeconds: number;
  capturedAtUnixMs: number;
  cpu: CpuInfo;
  memory: MemoryInfo;
  network: NetworkInfo;
  disks: DiskInfo[];
  processes: ProcessInfo[];
}

export interface StartupItem {
  name: string;
  command: string;
  scope: string;
  enabled: boolean;
  editable: boolean;
}

export type HardwareDetails = Record<string, unknown>;

export interface SensorReading {
  id: string;
  name: string;
  category: "temperature" | "fan" | "power" | "clock" | string;
  value: number;
  unit: string;
  source: string;
}

export interface StorageHealth {
  name: string;
  healthStatus: string;
  operationalStatus: string;
  temperatureC: number | null;
  wearPercent: number | null;
  powerOnHours: number | null;
  readErrorsTotal: number | null;
  writeErrorsTotal: number | null;
  source: string;
}

export interface SensorSnapshot {
  capturedAtUnixMs: number;
  readings: SensorReading[];
  storage: StorageHealth[];
  limitations: string[];
}
