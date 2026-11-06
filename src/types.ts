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
  driverHardwareId: string;
  signed: boolean | null;
}

export interface DriverUpdateAssessment {
  update: DriverUpdate;
  signatureOk: boolean | null;
  status: "applicable" | "current" | "no_match" | "unknown" | string;
  matchingDevices: string[];
  notes: string[];
}

export interface DriverAssessmentReport {
  capturedAtUnixMs: number;
  updates: DriverUpdateAssessment[];
  inventorySource: string;
  note: string;
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

export interface SystemHistoryPoint {
  t: number;
  cpuPercent: number;
  memoryPercent: number;
  downloadBytesPerSec: number;
  uploadBytesPerSec: number;
  diskReadBytesPerSec: number;
  diskWriteBytesPerSec: number;
}

export interface SystemHistorySeries {
  hours: number;
  bucketMs: number;
  points: SystemHistoryPoint[];
  sampleCount: number;
}

export interface ToolActionDescriptor {
  id: string;
  label: string;
  write: boolean;
}

export interface ToolDescriptor {
  id: string;
  name: string;
  category: string;
  description: string;
  risk: "read_only" | "write" | "system" | string;
  requiresAdmin: boolean;
  status: "available" | "partial" | "planned" | string;
  note: string;
  uiKind: string;
  actions?: ToolActionDescriptor[];
}

export interface EyeCareSettings {
  enabled: boolean;
  warmth: number;
  breakIntervalMinutes: number;
  breakDurationSeconds: number;
  scheduleEnabled: boolean;
  scheduleStartHour: number;
  scheduleEndHour: number;
}

export interface EyeCareStatus {
  applied: boolean;
  displays: string[];
  failed: string[];
  message: string;
}

export interface AppSettings {
  snapshotIntervalSeconds: number;
  metricsRetentionDays: number;
}

export interface AppPaths {
  dataDir: string;
  configDir: string;
}
export interface DesktopItem {
  name: string;
  path: string;
  isDir: boolean;
  extension: string;
  bytes: number;
  modifiedUnixMs: number | null;
}

export interface DesktopOverview {
  desktopPath: string;
  documentsPath: string;
  downloadsPath: string;
  items: DesktopItem[];
  total: number;
  errorCount: number;
}
export interface LaunchEntry {
  name: string;
  source: string;
  target: string;
  appId: string;
  targetPath: string;
  arguments: string;
}
export interface PowerSettingOption {
  id: string;
  label: string;
  unit: string;
  min: number;
  max: number;
  ac: number | null;
  dc: number | null;
  note: string;
}

export interface StartupToolItem {
  key: string;
  name: string;
  command: string;
  enabled: boolean;
  location: string;
}

export interface WinoptSetting {
  id: string;
  label: string;
  current: number | null;
  min: number;
  max: number;
  options: { value: number; label: string }[];
  requiresAdmin?: boolean;
}

export interface PrivacySetting {
  id: string;
  label: string;
  current: string | null;
  options: { value: string; label: string }[];
  requiresAdmin?: boolean;
}

export interface DuplicateToolGroup {
  id: string;
  sizeBytes: number;
  size: string;
  reclaimBytes: number;
  reclaim: string;
  files: string[];
}

export interface ServiceItem {
  name: string;
  display: string;
  start: number;
  startLabel: string;
  path: string;
}

export interface VolumeItem {
  drive: string;
  name: string;
  media: string;
  size: string;
  free: string;
  canOptimize: boolean;
  canTrim: boolean;
}

export interface ScheduledTaskItem {
  path: string;
  name: string;
  state: string;
  enabled: boolean;
}

export interface ProcessItem {
  name: string;
  pid: number;
  startTime: number;
  cpu: string;
  memory: string;
  priority: string;
  priorityLabel: string;
  canEnd: boolean;
  endToken: string;
  priorityToken: string;
}

export interface ToolResult {
  summary?: { label: string; value: string }[];
  columns?: { key: string; label: string }[];
  rows?: Record<string, unknown>[];
  notes?: string[];
  confirmationToken?: string;
  confirmationTokenExpiresAt?: number;
  powerSettings?: PowerSettingOption[];
  undoChanges?: { id: string; ac: number; dc: number }[];
  winoptSettings?: WinoptSetting[];
  winoptUndo?: { id: string; value: number | null }[];
  startupItems?: StartupToolItem[];
  systemStartupItems?: StartupToolItem[];
  serviceItems?: ServiceItem[];
  duplicateFileGroups?: DuplicateToolGroup[];
  privacySettings?: PrivacySetting[];
  privacyUndo?: { id: string; value: string }[];
  processItems?: ProcessItem[];
  taskItems?: ScheduledTaskItem[];
  taskToken?: string;
  volumeItems?: VolumeItem[];
  journalPath?: string;
  journalDir?: string;
  startupRegistryError?: string;
}

export interface SlimItem {
  id: string;
  label: string;
  path: string;
  bytes: number;
  fileCount: number;
  category: "safe" | "caution" | "system" | string;
  requiresAdmin: boolean;
  truncated: boolean;
  note: string;
}

export interface SlimScan {
  capturedAtUnixMs: number;
  driveLetter: string;
  driveTotalBytes: number;
  driveFreeBytes: number;
  items: SlimItem[];
  safeBytes: number;
  cautionBytes: number;
  systemBytes: number;
  fileCount: number;
  note: string;
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
  residueCount: number;
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
    key: string;
    uninstallString: string;
    quietUninstallString: string;
    residue: boolean;
  }>;
  source: string;
}

export interface StorageScan {
  kind: string;
  label: string;
  path: string;
  fileCount: number;
  directoryCount: number;
  totalBytes: number;
  errorCount: number;
  truncated: boolean;
  categories: Array<{ extension: string; fileCount: number; totalBytes: number }>;
}

export type StorageKind = string;

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
  exclusions?: string[];
}

export interface StorageDrive {
  letter: string;
  label: string;
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  freeBytes: number;
  removable: boolean;
}

export interface MonitorInfo {
  name: string;
  manufacturer: string;
  product: string;
  serial: string;
  year: number;
}

export interface AudioDevice {
  name: string;
  manufacturer: string;
  status: string;
}

export interface UsbDevice {
  name: string;
  manufacturer: string;
  status: string;
  vid: string;
  pid: string;
}

export interface BluetoothDevice {
  name: string;
  manufacturer: string;
  status: string;
  kind: string;
}

export interface CameraDevice {
  name: string;
  manufacturer: string;
  status: string;
  pnpClass: string;
}

export interface DeviceDetails {
  capturedAtUnixMs: number;
  monitors: MonitorInfo[];
  audio: AudioDevice[];
  usb: UsbDevice[];
  bluetooth: BluetoothDevice[];
  cameras: CameraDevice[];
  unavailable: string[];
  source: string;
}

export interface NotificationSettings {
  enabled: boolean;
  cooldownMinutes: number;
  quietStartHour: number;
  quietEndHour: number;
}

export interface NotificationOutcome {
  status: string;
  message: string;
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

export interface NvmeHealth {
  device: string;
  criticalWarning: number;
  temperatureC: number | null;
  availableSparePercent: number;
  spareThresholdPercent: number;
  percentageUsed: number;
  dataReadBytes: number;
  dataWrittenBytes: number;
  powerOnHours: number;
  powerCycles: number;
  unsafeShutdowns: number;
  mediaErrors: number;
  errorLogEntries: number;
  source: string;
}

export interface NvmeHealthReport {
  capturedAtUnixMs: number;
  devices: NvmeHealth[];
  accessDenied: boolean;
  unsupported: boolean;
  lastError: string | null;
  note: string;
}

export type AiSeverity = "info" | "warning" | "critical";
export type AiConfidence = "low" | "medium" | "high";
export type AiAvailability = "available" | "partial" | "unavailable";

export interface AiRecommendation {
  label: string;
  kind: "read_only_check" | "manual_action" | "controlled_write_preview" | string;
  toolId?: string | null;
  risk: "none" | "low" | "medium" | "high" | string;
  requiresConfirmation: boolean;
}

export interface AiFinding {
  id: string;
  title: string;
  severity: AiSeverity | string;
  summary: string;
  confidence: AiConfidence | string;
  evidenceRefs: string[];
  recommendations: AiRecommendation[];
  limitations: string[];
}

export interface AiSourceStatus {
  id: string;
  label: string;
  ok: boolean;
  detail: string;
}

export interface DiagnosticFact {
  id: string;
  snapshotId: string;
  category: string;
  severity: AiSeverity;
  title: string;
  value: unknown;
  source: string;
  capturedAtUnixMs: number;
  confidence: number;
  availability: AiAvailability;
  evidenceRefs: string[];
  sensitive: boolean;
}

export interface DiagnosticSnapshot {
  schemaVersion: number;
  snapshotId: string;
  capturedAtUnixMs: number;
  facts: DiagnosticFact[];
  localFindings: AiFinding[];
  limitations: string[];
  sources: AiSourceStatus[];
  excludedFactCount: number;
}

export interface AiSettings {
  enabled: boolean;
  provider: string;
  baseUrl: string;
  model: string;
  maxOutputTokens: number;
  timeoutSeconds: number;
  nativeTools: boolean;
  inputPricePerMillion: number;
  outputPricePerMillion: number;
}

export interface AiCapabilityReport {
  reachable: boolean;
  structuredOutput: boolean;
  nativeTools: boolean;
  model: string;
  latencyMs: number;
  note: string;
}

export interface AiSettingsView {
  settings: AiSettings;
  hasApiKey: boolean;
  keyStorage: string;
}

export interface AiRedactionSummary {
  removedFields: number;
  appliedRules: string[];
  excludedFacts: number;
}

export interface AiPayloadPreview {
  snapshotId: string;
  baseUrl: string;
  model: string;
  payload: unknown;
  estimatedInputTokens: number;
  redaction: AiRedactionSummary;
}

export interface AiUsage {
  promptTokens: number | null;
  completionTokens: number | null;
  totalTokens: number | null;
}

export interface AiToolRun {
  toolId: string;
  label: string;
  reason: string;
  ok: boolean;
  durationMs: number;
  summary: unknown;
  error: string | null;
}

export interface AiRequestedCheck {
  toolId: string;
  reason: string;
  parameters: Record<string, unknown>;
}

export interface AiAnswer {
  requestId: string;
  snapshotId: string;
  generatedAtUnixMs: number;
  provider: string;
  model: string;
  promptVersion: string;
  degraded: boolean;
  degradedReason: string | null;
  summary: string;
  findings: AiFinding[];
  limitations: string[];
  localFindings: AiFinding[];
  usage: AiUsage | null;
  checks?: AiToolRun[];
  estimatedCost?: number | null;
  repairSuggestions?: AiRepairSuggestion[];
}

export interface AiRepairSuggestion {
  toolId: string;
  reason: string;
  evidenceRefs: string[];
}

export interface AiRepairTarget {
  label: string;
  detail: string;
  bytes: number | null;
  fileCount: number | null;
  requiresAdmin: boolean;
}

export interface AiRepairPreview {
  toolId: string;
  label: string;
  risk: "low" | "medium" | "high" | string;
  reversible: boolean;
  requiresAdmin: boolean;
  status: string;
  summary: string;
  targets: AiRepairTarget[];
  rollback: string;
  notes: string[];
  evidenceRefs: string[];
  generatedAtUnixMs: number;
}

export interface AiTurn {
  snapshotId: string;
  role: "user" | "assistant" | string;
  content: string;
}

export interface AiStreamEvent {
  requestId: string;
  stage: "status" | "delta" | string;
  delta: string;
  charCount: number;
  message: string;
}

export interface AiSessionRecord {
  id: string;
  savedAtUnixMs: number;
  question: string | null;
  answer: AiAnswer;
  feedback?: string | null;
}

export interface AiToolAuditEntry {
  savedAtUnixMs: number;
  requestId: string;
  toolId: string;
  ok: boolean;
  durationMs: number;
  reason: string;
  parameters: unknown;
  error: string | null;
}
