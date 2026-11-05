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

export type SensorAvailability =
  | "ok"
  | "unavailable"
  | "denied"
  | "unsupported"
  | "stale";

export type SensorQuality = "measured" | "derived";

export interface SensorValue {
  id: string;
  hardwareId: string;
  hardwareName: string;
  hardwareClass: "cpu" | "gpu" | "disk" | "system" | string;
  category: "temperature" | "fan" | "power" | "clock" | string;
  name: string;
  value: number | null;
  unit: string;
  source: string;
  capturedAtUnixMs: number;
  availability: SensorAvailability;
  errorCode: string | null;
  quality: SensorQuality;
}

export interface SensorLimitation {
  code: string;
  message: string;
  remediation: string;
}

export interface SensorSourceStatus {
  source: string;
  ok: boolean;
  lastError: string | null;
  lastSuccessAtUnixMs: number | null;
}

export type SensorAlertLevel = "normal" | "warning" | "critical";

export interface SensorAlert {
  id: string;
  sensorId: string;
  sensorName: string;
  level: SensorAlertLevel;
  value: number;
  threshold: number;
  message: string;
  sinceUnixMs: number;
  lastNotifiedUnixMs: number;
}

export interface SensorThresholdClass {
  warningC: number;
  criticalC: number;
}

export interface SensorThresholds {
  cpu: SensorThresholdClass;
  gpu: SensorThresholdClass;
  disk: SensorThresholdClass;
}

export interface SensorSettings {
  samplingIntervalMs: number;
  retentionHours: number;
  alertsEnabled: boolean;
  thresholds: SensorThresholds;
}

export interface SensorHistoryPoint {
  t: number;
  min: number;
  max: number;
  avg: number;
  count: number;
}

export interface SensorHistorySeries {
  sensorId: string;
  bucketMs: number;
  points: SensorHistoryPoint[];
  min: number | null;
  max: number | null;
  avg: number | null;
  sampleCount: number;
}

export type HealthLevel = "unknown" | "healthy" | "caution" | "critical";

export interface HealthSignal {
  code: string;
  severity: HealthLevel;
  message: string;
}

export interface HealthAssessment {
  level: HealthLevel;
  signals: HealthSignal[];
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
  assessment: HealthAssessment;
}

export interface SensorSnapshot {
  schemaVersion: number;
  capturedAtUnixMs: number;
  values: SensorValue[];
  storage: StorageHealth[];
  smartDevices: SmartDevice[];
  limitations: SensorLimitation[];
  sources: SensorSourceStatus[];
  alerts: SensorAlert[];
  elevated: boolean;
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
  assessment: HealthAssessment;
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
  environment: BenchmarkEnvironment | null;
}

export interface BenchmarkEnvironment {
  appVersion: string;
  cpuBrand: string;
  logicalCores: number;
  memoryBytes: number;
  osName: string;
  osVersion: string;
}

export interface BenchmarkPreflight {
  maxTemperatureC: number | null;
  onBattery: boolean | null;
  warnings: string[];
  source: string;
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
  crashEvents: Array<{
    timeCreated: string;
    provider: string;
    eventId: number;
    level: string;
    message: string;
  }>;
  serviceIssues: Array<{
    name: string;
    displayName: string;
    state: string;
    startMode: string;
    exitCode: number;
  }>;
  minidumpCount: number;
  latestMinidump: string;
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

export type StorageKind = StorageScan["kind"];

export interface LargeFileEntry {
  path: string;
  name: string;
  bytes: number;
  modifiedUnixMs: number | null;
}

export interface DirectoryUsage {
  path: string;
  name: string;
  bytes: number;
  fileCount: number;
}

export interface LargeFileScan {
  kind: StorageKind;
  label: string;
  root: string;
  minBytes: number;
  files: LargeFileEntry[];
  directories: DirectoryUsage[];
  totalBytes: number;
  fileCount: number;
  errorCount: number;
  truncated: boolean;
}

export interface DuplicateFile {
  path: string;
  name: string;
  bytes: number;
}

export interface DuplicateGroup {
  sizeBytes: number;
  hash: string;
  files: DuplicateFile[];
  wastedBytes: number;
}

export interface DuplicateScan {
  kind: StorageKind;
  label: string;
  root: string;
  groups: DuplicateGroup[];
  candidateFiles: number;
  scannedBytes: number;
  errorCount: number;
  truncated: boolean;
}

export interface DirectoryChild {
  name: string;
  path: string;
  relativePath: string;
  isDir: boolean;
  bytes: number;
  fileCount: number;
}

export interface DirectoryListing {
  kind: StorageKind;
  label: string;
  root: string;
  relativePath: string;
  path: string;
  parentRelativePath: string | null;
  children: DirectoryChild[];
  totalBytes: number;
  totalFiles: number;
  errorCount: number;
  truncated: boolean;
}

export interface ScheduledTask {
  name: string;
  path: string;
  state: string;
  author: string;
  description: string;
  runAsUser: string;
  hidden: boolean;
  enabled: boolean;
  triggers: string[];
  lastRunUnixMs: number | null;
  nextRunUnixMs: number | null;
  lastResult: number | null;
}

export interface ScheduledTaskInventory {
  capturedAtUnixMs: number;
  total: number;
  enabled: number;
  disabled: number;
  running: number;
  logonOrBoot: number;
  tasks: ScheduledTask[];
  source: string;
}

export interface CleanupCandidate {
  id: string;
  label: string;
  path: string;
  exists: boolean;
  bytes: number;
  fileCount: number;
  errorCount: number;
  requiresAdmin: boolean;
  truncated: boolean;
  note: string;
  error: string | null;
}

export interface CleanupPreview {
  capturedAtUnixMs: number;
  candidates: CleanupCandidate[];
  totalBytes: number;
  totalFiles: number;
  source: string;
}

export interface EventLogEntry {
  timeCreatedUnixMs: number;
  logName: string;
  provider: string;
  eventId: number;
  level: string;
  message: string;
}

export interface EventLogResult {
  logName: string;
  level: string;
  hours: number;
  total: number;
  events: EventLogEntry[];
  truncated: boolean;
  source: string;
}
