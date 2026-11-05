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
  disks: DiskInfo[];
  processes: ProcessInfo[];
}
