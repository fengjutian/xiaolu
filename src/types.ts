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
  smartDevices: SmartDevice[];
  limitations: string[];
}

export interface SmartAttribute {
  id: number;
  name: string;
  current: number;
  worst: number;
  raw: number;
}

export interface SmartDevice {
  instanceName: string;
  predictFailure: boolean | null;
  attributes: SmartAttribute[];
  source: string;
}

export interface DriverInfo {
  deviceName: string;
  deviceClass: string;
  manufacturer: string;
  driverProvider: string;
  driverVersion: string;
  driverDate: string;
  infName: string;
  signer: string;
  signed: boolean;
  deviceId: string;
  status: string;
  errorCode: number;
  missingDriver: boolean;
}

export interface DriverInventory {
  capturedAtUnixMs: number;
  total: number;
  signed: number;
  unsigned: number;
  missing: number;
  problems: number;
  items: DriverInfo[];
  source: string;
}

export interface DriverUpdate {
  title: string;
  description: string;
  updateId: string;
  revisionNumber: number;
  driverClass: string;
  driverManufacturer: string;
  driverModel: string;
  driverProvider: string;
  driverVersion: string;
  driverDate: string;
  categories: string[];
  downloaded: boolean;
}

export interface BenchmarkResult {
  schemaVersion: number;
  kind: "cpu-single" | "cpu-multi" | "memory" | "disk" | "gpu";
  label: string;
  score: number;
  unit: string;
  durationMs: number;
  details: Record<string, number>;
  note: string;
}

export interface BenchmarkRecord {
  recordedAtUnixMs: number;
  result: BenchmarkResult;
}

export interface DiagnosticReport {
  capturedAtUnixMs: number;
  pendingReboot: boolean;
  pendingWindowsUpdateReboot: boolean;
  battery: {
    present: boolean;
    name: string;
    status: string;
    chargePercent: number | null;
    designCapacityMwh: number | null;
    fullChargeCapacityMwh: number | null;
    healthPercent: number | null;
    cycleCount: number | null;
  };
  events: Array<{
    timeCreated: string;
    provider: string;
    eventId: number;
    level: string;
    message: string;
  }>;
  source: string;
}

export interface NetworkDiagnosticReport {
  capturedAtUnixMs: number;
  defaultGateways: string[];
  dns: Array<{ alias: string; servers: string[] }>;
  proxyEnabled: boolean;
  proxyServer: string;
  autoConfigUrl: string;
  hostsCustomEntries: number;
  connections: Array<{
    processName: string;
    pid: number;
    localAddress: string;
    localPort: number;
    remoteAddress: string;
    remotePort: number;
    state: string;
  }>;
  source: string;
}

export interface SoftwareInventory {
  capturedAtUnixMs: number;
  total: number;
  estimatedSizeBytes: number;
  items: Array<{
    name: string;
    version: string;
    publisher: string;
    installDate: string;
    installLocation: string;
    estimatedSizeBytes: number;
    scope: string;
    architecture: string;
    uninstallAvailable: boolean;
  }>;
  source: string;
}

export interface StorageScan {
  kind: "downloads" | "documents" | "desktop";
  label: string;
  path: string;
  fileCount: number;
  directoryCount: number;
  totalBytes: number;
  errorCount: number;
  truncated: boolean;
  categories: Array<{ extension: string; fileCount: number; totalBytes: number }>;
}
