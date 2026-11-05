import { useMemo, useState } from "react";
import {
  Alert,
  Badge,
  Box,
  Button,
  Card,
  Group,
  Loader,
  ScrollArea,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Text,
  Title,
} from "@mantine/core";
import {
  IconAlertCircle,
  IconDownload,
  IconFileText,
  IconShieldCheck,
} from "@tabler/icons-react";
import {
  getDiagnosticReport,
  getDriverInventory,
  getHardwareDetails,
  getNetworkDiagnostics,
  getScheduledTasks,
  getSensorSnapshot,
  getSoftwareInventory,
  getStartupItems,
  getSystemSnapshot,
  scanCleanupCandidates,
} from "./api";
import type {
  CleanupPreview,
  DiagnosticReport,
  DriverInventory,
  HardwareDetails,
  NetworkDiagnosticReport,
  ScheduledTaskInventory,
  SensorSnapshot,
  SoftwareInventory,
  StartupItem,
  SystemSnapshot,
} from "./types";

const APP_VERSION = "0.26.0";
const REDACTED = "[已脱敏]";

type RedactionOptions = {
  hostName: boolean;
  userName: boolean;
  serials: boolean;
  macAddresses: boolean;
  ipAddresses: boolean;
  localPaths: boolean;
};

type SectionOptions = {
  hardware: boolean;
  sensors: boolean;
  drivers: boolean;
  diagnostics: boolean;
  network: boolean;
  software: boolean;
  startup: boolean;
  processes: boolean;
  tasks: boolean;
  cleanup: boolean;
};

type ReportError = { section: string; message: string };

type ReportPayload = {
  schemaVersion: number;
  generatedAt: string;
  application: { name: string; version: string };
  privacy: { redaction: RedactionOptions; note: string };
  system?: SystemSnapshot;
  hardware?: HardwareDetails;
  sensors?: SensorSnapshot;
  drivers?: DriverInventory;
  diagnostics?: DiagnosticReport;
  network?: NetworkDiagnosticReport;
  software?: SoftwareInventory;
  startup?: StartupItem[];
  tasks?: ScheduledTaskInventory;
  cleanup?: CleanupPreview;
  errors: ReportError[];
};

type FieldRow = { path: string; value: string };

const DEFAULT_REDACTION: RedactionOptions = {
  hostName: true,
  userName: true,
  serials: true,
  macAddresses: true,
  ipAddresses: true,
  localPaths: true,
};

const DEFAULT_SECTIONS: SectionOptions = {
  hardware: true,
  sensors: true,
  drivers: true,
  diagnostics: true,
  network: true,
  software: true,
  startup: true,
  processes: true,
  tasks: true,
  cleanup: false,
};

const bytes = (value: number) =>
  value >= 1024 ** 3
    ? `${(value / 1024 ** 3).toFixed(1)} GB`
    : value >= 1024 ** 2
      ? `${(value / 1024 ** 2).toFixed(1)} MB`
      : `${(value / 1024).toFixed(0)} KB`;

const duration = (seconds: number) =>
  `${Math.floor(seconds / 86400)} 天 ${Math.floor((seconds % 86400) / 3600)} 小时`;

const looksLikeMac = (value: string) =>
  /^([0-9a-f]{2}[:-]){5}[0-9a-f]{2}$/i.test(value.trim());

const looksLikeIp = (value: string) =>
  /^(\d{1,3}\.){3}\d{1,3}$/.test(value.trim()) ||
  /^[0-9a-f]{0,4}(:[0-9a-f]{0,4}){2,7}$/i.test(value.trim());

function maskUserPath(value: string): string {
  return value
    .replace(/(\\Users\\)[^\\/]+/gi, `$1${REDACTED}`)
    .replace(/(\\Documents and Settings\\)[^\\/]+/gi, `$1${REDACTED}`)
    .replace(/%USERPROFILE%/gi, REDACTED);
}

function redactValue(
  value: unknown,
  key: string,
  options: RedactionOptions,
): unknown {
  if (typeof value === "string") {
    let output = value;
    if (
      options.serials &&
      /serial|service[_\s-]?tag/i.test(key)
    ) {
      return REDACTED;
    }
    if (options.macAddresses && (/mac/i.test(key) || looksLikeMac(output))) {
      return REDACTED;
    }
    if (options.ipAddresses && looksLikeIp(output)) {
      return REDACTED;
    }
    if (
      options.hostName &&
      /^(hostname|host_name|computername|computer|csname)$/i.test(key)
    ) {
      return REDACTED;
    }
    if (
      options.userName &&
      /^(username|user_name|user|owner|registereduser)$/i.test(key)
    ) {
      return REDACTED;
    }
    if (options.localPaths) output = maskUserPath(output);
    return output;
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactValue(item, key, options));
  }
  if (value && typeof value === "object") {
    const result: Record<string, unknown> = {};
    for (const [childKey, child] of Object.entries(value)) {
      // PID 属于准标识符，始终不进入报告。
      if (childKey.toLowerCase() === "pid") continue;
      result[childKey] = redactValue(child, childKey, options);
    }
    return result;
  }
  return value;
}

function flattenFields(
  value: unknown,
  prefix: string,
  rows: FieldRow[],
  depth: number,
): FieldRow[] {
  if (rows.length >= 300) return rows;
  if (value === null || typeof value !== "object") {
    rows.push({
      path: prefix || "(root)",
      value:
        value === null
          ? "null"
          : String(value).slice(0, 80) || '""',
    });
    return rows;
  }
  if (depth >= 3) {
    rows.push({
      path: prefix,
      value: Array.isArray(value) ? `[${value.length} 项]` : "{…}",
    });
    return rows;
  }
  if (Array.isArray(value)) {
    value.slice(0, 5).forEach((item, index) => {
      flattenFields(item, `${prefix}[${index}]`, rows, depth + 1);
    });
    if (value.length > 5) {
      rows.push({ path: `${prefix}[…]`, value: `共 ${value.length} 项` });
    }
    return rows;
  }
  for (const [key, child] of Object.entries(value)) {
    flattenFields(child, prefix ? `${prefix}.${key}` : key, rows, depth + 1);
  }
  return rows;
}

function cell(value: unknown): string {
  if (value === null || value === undefined) return "—";
  return String(value).replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
}

const healthText = (level: string) =>
  level === "critical"
    ? "危险"
    : level === "caution"
      ? "注意"
      : level === "healthy"
        ? "健康"
        : "未知";

function table(headers: string[], rows: unknown[][]): string[] {
  const lines = [
    `| ${headers.map(cell).join(" | ")} |`,
    `| ${headers.map(() => "---").join(" | ")} |`,
  ];
  for (const row of rows) {
    lines.push(`| ${row.map(cell).join(" | ")} |`);
  }
  return lines;
}

function buildMarkdown(
  payload: ReportPayload,
  options: RedactionOptions,
): string {
  const lines: string[] = [];
  lines.push("# 小鲁 · 系统透镜 诊断报告", "");
  lines.push(
    `- 生成时间：${payload.generatedAt}`,
    `- 应用版本：${payload.application.version}`,
    `- 数据来源：本机只读采集，未上传`,
    "",
    "## 脱敏设置",
    "",
    ...table(
      ["字段", "处理"],
      [
        ["主机名", options.hostName ? REDACTED : "保留"],
        ["用户名", options.userName ? REDACTED : "保留"],
        ["序列号", options.serials ? REDACTED : "保留"],
        ["MAC 地址", options.macAddresses ? REDACTED : "保留"],
        ["IP 地址", options.ipAddresses ? REDACTED : "保留"],
        ["本地路径", options.localPaths ? "抹除用户目录名" : "保留"],
        ["进程 PID", "始终移除"],
      ],
    ),
    "",
  );

  if (payload.system) {
    const system = payload.system;
    lines.push(
      "## 系统概览",
      "",
      ...table(
        ["项目", "值"],
        [
          ["主机名", system.hostName],
          ["系统", system.osVersion.toLowerCase().startsWith(system.osName.toLowerCase()) ? system.osVersion : `${system.osName} ${system.osVersion}`],
          ["内核", system.kernelVersion],
          ["架构", system.architecture],
          ["运行时长", duration(system.uptimeSeconds)],
          [
            "CPU",
            `${system.cpu.brand} · ${system.cpu.logicalCores} 线程 · ${system.cpu.usagePercent.toFixed(1)}%`,
          ],
          [
            "内存",
            `${bytes(system.memory.usedBytes)} / ${bytes(system.memory.totalBytes)} · ${system.memory.usagePercent.toFixed(1)}%`,
          ],
        ],
      ),
      "",
      "### 磁盘",
      "",
      ...table(
        ["盘符", "文件系统", "已用 / 总量", "可用"],
        system.disks.map((disk) => [
          disk.mountPoint,
          disk.fileSystem,
          `${bytes(disk.totalBytes - disk.availableBytes)} / ${bytes(disk.totalBytes)}`,
          bytes(disk.availableBytes),
        ]),
      ),
      "",
    );
    if (system.processes.length > 0) {
      lines.push(
        "### 进程占用（前 20）",
        "",
        ...table(
          ["进程", "CPU", "内存"],
          [...system.processes]
            .sort((left, right) => right.memoryBytes - left.memoryBytes)
            .slice(0, 20)
            .map((process) => [
              process.name,
              `${process.cpuPercent.toFixed(1)}%`,
              bytes(process.memoryBytes),
            ]),
        ),
        "",
      );
    }
  }

  if (payload.hardware) {
    lines.push(
      "## 硬件信息",
      "",
      "```json",
      JSON.stringify(payload.hardware, null, 2),
      "```",
      "",
    );
  }

  if (payload.sensors) {
    const sensors = payload.sensors;
    const available = sensors.values.filter(
      (value) => value.availability === "ok" && value.value != null,
    );
    lines.push(
      "## 传感器与温度",
      "",
      `可用传感器 ${available.length} / ${sensors.values.length}，权限模式：${sensors.elevated ? "管理员" : "普通用户"}。`,
      "",
    );
    if (available.length > 0) {
      lines.push(
        ...table(
          ["传感器", "数值", "来源", "采集时间"],
          available.map((value) => [
            value.name,
            `${value.value?.toFixed(1)} ${value.unit}`,
            value.source,
            new Date(value.capturedAtUnixMs).toLocaleString(),
          ]),
        ),
        "",
      );
    }
    if (sensors.alerts.length > 0) {
      lines.push(
        "### 活动告警",
        "",
        ...table(
          ["传感器", "级别", "数值", "阈值"],
          sensors.alerts.map((alert) => [
            alert.sensorName,
            alert.level,
            `${alert.value.toFixed(1)} °C`,
            `${alert.threshold.toFixed(0)} °C`,
          ]),
        ),
        "",
      );
    }
    if (sensors.storage.length > 0) {
      lines.push(
        "### 磁盘健康",
        "",
        ...table(
          ["磁盘", "评估", "健康状态", "温度", "磨损", "通电时间"],
          sensors.storage.map((disk) => [
            disk.name,
            healthText(disk.assessment.level),
            `${disk.healthStatus} / ${disk.operationalStatus}`,
            disk.temperatureC == null ? "不可用" : `${disk.temperatureC} °C`,
            disk.wearPercent == null ? "不可用" : `${disk.wearPercent}%`,
            disk.powerOnHours == null ? "不可用" : `${disk.powerOnHours} 小时`,
          ]),
        ),
        "",
      );
    }
    const healthSignals = sensors.storage.flatMap((disk) =>
      disk.assessment.signals.map((signal) => `${disk.name}：${signal.message}`),
    );
    if (healthSignals.length > 0) {
      lines.push("### 健康信号", "", ...healthSignals.map((text) => `> ${text}`), "");
    }
    for (const limitation of sensors.limitations) {
      lines.push(`> ${limitation.code}：${limitation.message} ${limitation.remediation}`, "");
    }
  }

  if (payload.drivers) {
    const drivers = payload.drivers;
    lines.push(
      "## 驱动摘要",
      "",
      `设备 ${drivers.total} · 已签名 ${drivers.signed} · 未签名 ${drivers.unsigned} · 缺失 ${drivers.missing} · 异常 ${drivers.problems}`,
      "",
    );
    const problemItems = drivers.items.filter(
      (item) => item.errorCode !== 0 || item.missingDriver || !item.signed,
    );
    if (problemItems.length > 0) {
      lines.push(
        ...table(
          ["设备", "状态", "错误码", "驱动版本", "INF"],
          problemItems.slice(0, 30).map((item) => [
            item.deviceName,
            item.status,
            item.errorCode,
            item.driverVersion || "未知",
            item.infName || "无",
          ]),
        ),
        "",
      );
    }
  }

  if (payload.diagnostics) {
    const diagnostics = payload.diagnostics;
    lines.push(
      "## 系统诊断",
      "",
      ...table(
        ["项目", "值"],
        [
          ["待重启", diagnostics.pendingReboot ? "是" : "否"],
          ["Windows Update 请求重启", diagnostics.pendingWindowsUpdateReboot ? "是" : "否"],
          [
            "电池",
            diagnostics.battery.present
              ? `${diagnostics.battery.name} · ${diagnostics.battery.chargePercent?.toFixed(0) ?? "?"}% · 健康 ${diagnostics.battery.healthPercent?.toFixed(1) ?? "?"}%`
              : "无电池或接口不可用",
          ],
          ["最近 24 小时错误事件", String(diagnostics.events.length)],
          ["最近 7 天崩溃/意外关机", String(diagnostics.crashEvents.length)],
          ["自动启动服务异常", String(diagnostics.serviceIssues.length)],
        ],
      ),
      "",
    );
    if (diagnostics.serviceIssues.length > 0) {
      lines.push(
        ...table(
          ["服务", "状态", "启动模式", "退出码"],
          diagnostics.serviceIssues.map((service) => [
            service.displayName,
            service.state,
            service.startMode,
            service.exitCode,
          ]),
        ),
        "",
      );
    }
  }

  if (payload.network) {
    const network = payload.network;
    lines.push(
      "## 网络摘要",
      "",
      ...table(
        ["项目", "值"],
        [
          ["默认网关", network.defaultGateways.join(" · ") || "未检测到"],
          ["DNS", network.dns.map((item) => `${item.alias}: ${item.servers.join(" / ")}`).join("；") || "未读取到"],
          ["系统代理", network.proxyEnabled ? network.proxyServer || "已启用" : "未启用"],
          ["Hosts 自定义项", String(network.hostsCustomEntries)],
        ],
      ),
      "",
      "> 远程连接明细不写入报告。",
      "",
    );
  }

  if (payload.software) {
    const software = payload.software;
    lines.push(
      "## 软件清单",
      "",
      `共 ${software.total} 个程序，登记占用 ${bytes(software.estimatedSizeBytes)}。`,
      "",
      ...table(
        ["软件", "版本", "发布者", "范围"],
        [...software.items]
          .sort((left, right) => right.installDate.localeCompare(left.installDate))
          .slice(0, 20)
          .map((item) => [item.name, item.version, item.publisher, item.scope]),
      ),
      "",
    );
  }

  if (payload.startup && payload.startup.length > 0) {
    lines.push(
      "## 启动项",
      "",
      ...table(
        ["名称", "范围", "状态", "命令"],
        payload.startup.map((item) => [
          item.name,
          item.scope,
          item.enabled ? "已启用" : "已禁用",
          item.command,
        ]),
      ),
      "",
    );
  }

  if (payload.tasks && payload.tasks.tasks.length > 0) {
    const inventory = payload.tasks;
    const rows = inventory.tasks
      .filter((task) => !task.path.toLowerCase().startsWith("\\microsoft\\"))
      .slice(0, 50)
      .map((task) => [
        task.name,
        task.state,
        task.triggers.join(" / ") || "无",
        task.lastRunUnixMs ? new Date(task.lastRunUnixMs).toLocaleString() : "—",
        task.lastResult == null
          ? "—"
          : task.lastResult === 0
            ? "成功"
            : `0x${(task.lastResult >>> 0).toString(16).toUpperCase()}`,
        task.runAsUser || "未指定",
      ]);
    lines.push(
      "## 计划任务",
      "",
      `共 ${inventory.total} 个任务：启用 ${inventory.enabled}，禁用 ${inventory.disabled}，运行中 ${inventory.running}，登录/开机触发 ${inventory.logonOrBoot}。`,
      "",
    );
    if (rows.length > 0) {
      lines.push(
        "仅列出非 Microsoft 任务（最多 50 个）。",
        "",
        ...table(["任务", "状态", "触发", "上次运行", "结果", "运行身份"], rows),
        "",
      );
    }
  }

  if (payload.cleanup) {
    const preview = payload.cleanup;
    lines.push(
      "## 清理候选",
      "",
      "只读统计，不删除任何文件。",
      "",
      ...table(
        ["位置", "路径", "占用", "文件数", "权限"],
        preview.candidates.map((candidate) => [
          candidate.label,
          candidate.path || "—",
          bytes(candidate.bytes),
          `${candidate.fileCount}${candidate.truncated ? "（已截断）" : ""}`,
          candidate.requiresAdmin ? "需要管理员" : "普通权限",
        ]),
      ),
      "",
      `合计 ${bytes(preview.totalBytes)}，${preview.totalFiles} 个文件。`,
      "",
    );
  }

  if (payload.errors.length > 0) {
    lines.push(
      "## 采集异常",
      "",
      ...table(
        ["区块", "错误"],
        payload.errors.map((error) => [error.section, error.message]),
      ),
      "",
    );
  }

  lines.push("---", "", `由小鲁 · 系统透镜 v${payload.application.version} 在本机生成。`);
  return lines.join("\n");
}

function downloadFile(filename: string, content: string, type: string) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

const REDACTION_LABELS: Array<[keyof RedactionOptions, string]> = [
  ["hostName", "隐藏主机名"],
  ["userName", "隐藏用户名"],
  ["serials", "隐藏序列号"],
  ["macAddresses", "隐藏 MAC 地址"],
  ["ipAddresses", "隐藏 IP 地址"],
  ["localPaths", "隐藏本地路径"],
];

const SECTION_LABELS: Array<[keyof SectionOptions, string]> = [
  ["hardware", "硬件详情"],
  ["sensors", "传感器与温度"],
  ["drivers", "驱动清单"],
  ["diagnostics", "系统诊断"],
  ["network", "网络摘要"],
  ["software", "软件清单"],
  ["startup", "启动项"],
  ["processes", "进程列表"],
  ["tasks", "计划任务"],
  ["cleanup", "清理候选（较慢）"],
];

export default function ReportPanel() {
  const [redaction, setRedaction] = useState(DEFAULT_REDACTION);
  const [sections, setSections] = useState(DEFAULT_SECTIONS);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [payload, setPayload] = useState<ReportPayload | null>(null);
  const [json, setJson] = useState("");
  const [markdown, setMarkdown] = useState("");

  const fields = useMemo(() => {
    if (!payload) return [];
    return flattenFields(
      redactValue(payload, "", redaction),
      "",
      [],
      0,
    );
  }, [payload, redaction]);

  const generate = async () => {
    setLoading(true);
    setError(null);
    setPayload(null);
    setJson("");
    setMarkdown("");
    try {
      const results = await Promise.allSettled([
        getSystemSnapshot(),
        sections.hardware ? getHardwareDetails() : Promise.resolve(undefined),
        sections.sensors ? getSensorSnapshot() : Promise.resolve(undefined),
        sections.drivers ? getDriverInventory() : Promise.resolve(undefined),
        sections.diagnostics ? getDiagnosticReport() : Promise.resolve(undefined),
        sections.network ? getNetworkDiagnostics() : Promise.resolve(undefined),
        sections.software ? getSoftwareInventory() : Promise.resolve(undefined),
        sections.startup ? getStartupItems() : Promise.resolve(undefined),
        sections.tasks ? getScheduledTasks() : Promise.resolve(undefined),
        sections.cleanup ? scanCleanupCandidates() : Promise.resolve(undefined),
      ]);
      const errors: ReportError[] = [];
      const pick = <T,>(index: number, name: string): T | undefined => {
        const result = results[index];
        if (result.status === "fulfilled") {
          return result.value as T | undefined;
        }
        errors.push({ section: name, message: String(result.reason) });
        return undefined;
      };

      let system = pick<SystemSnapshot>(0, "系统概览");
      if (system && !sections.processes) {
        system = { ...system, processes: [] };
      }

      const next: ReportPayload = {
        schemaVersion: 4,
        generatedAt: new Date().toISOString(),
        application: { name: "小鲁 · 系统透镜", version: APP_VERSION },
        privacy: {
          redaction,
          note: "报告仅在本机生成；导出前按所选选项处理敏感字段。进程 PID 始终移除。",
        },
        system,
        hardware: pick<HardwareDetails>(1, "硬件详情"),
        sensors: pick<SensorSnapshot>(2, "传感器与温度"),
        drivers: pick<DriverInventory>(3, "驱动清单"),
        diagnostics: pick<DiagnosticReport>(4, "系统诊断"),
        network: pick<NetworkDiagnosticReport>(5, "网络摘要"),
        software: pick<SoftwareInventory>(6, "软件清单"),
        startup: pick<StartupItem[]>(7, "启动项"),
        tasks: pick<ScheduledTaskInventory>(8, "计划任务"),
        cleanup: pick<CleanupPreview>(9, "清理候选"),
        errors,
      };

      const redacted = redactValue(next, "", redaction) as ReportPayload;
      setPayload(redacted);
      setJson(JSON.stringify(redacted, null, 2));
      setMarkdown(buildMarkdown(redacted, redaction));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  const stamp = new Date().toISOString().slice(0, 10);
  const exportJson = () => {
    if (!json) return;
    downloadFile(`system-lens-${stamp}.json`, json, "application/json");
  };
  const exportMarkdown = () => {
    if (!markdown) return;
    downloadFile(`system-lens-${stamp}.md`, markdown, "text/markdown");
  };

  return (
    <Stack gap="md">
      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start">
          <Box>
            <Group gap="xs">
              <IconShieldCheck size={22} color="#748ffc" />
              <Title order={2}>导出前预览与脱敏</Title>
            </Group>
            <Text c="dimmed" mt={6} size="sm">
              报告在本机采集和生成。先选择要隐藏的字段和要包含的区块，再生成预览；
              导出内容与预览完全一致。
            </Text>
          </Box>
          <Badge variant="light">Schema 4</Badge>
        </Group>

        <SimpleGrid cols={{ base: 1, md: 2 }} mt="lg">
          <Box>
            <Text fw={700} mb="xs">
              脱敏选项
            </Text>
            <SimpleGrid cols={{ base: 1, sm: 2 }}>
              {REDACTION_LABELS.map(([key, label]) => (
                <Switch
                  key={key}
                  checked={redaction[key]}
                  onChange={(event) =>
                    setRedaction({
                      ...redaction,
                      [key]: event.currentTarget.checked,
                    })
                  }
                  label={label}
                />
              ))}
            </SimpleGrid>
          </Box>
          <Box>
            <Text fw={700} mb="xs">
              包含区块
            </Text>
            <SimpleGrid cols={{ base: 1, sm: 2 }}>
              {SECTION_LABELS.map(([key, label]) => (
                <Switch
                  key={key}
                  checked={sections[key]}
                  onChange={(event) =>
                    setSections({
                      ...sections,
                      [key]: event.currentTarget.checked,
                    })
                  }
                  label={label}
                />
              ))}
            </SimpleGrid>
          </Box>
        </SimpleGrid>

        <Group mt="lg">
          <Button
            leftSection={<IconFileText size={18} />}
            loading={loading}
            onClick={() => void generate()}
          >
            生成预览
          </Button>
          <Button
            variant="light"
            leftSection={<IconDownload size={18} />}
            disabled={!json}
            onClick={exportJson}
          >
            导出 JSON
          </Button>
          <Button
            variant="light"
            leftSection={<IconDownload size={18} />}
            disabled={!markdown}
            onClick={exportMarkdown}
          >
            导出 Markdown
          </Button>
          {loading && <Loader size="sm" />}
        </Group>
      </Card>

      {error && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {error}
        </Alert>
      )}

      {payload && payload.errors.length > 0 && (
        <Alert color="yellow" icon={<IconAlertCircle />} title="部分区块采集失败">
          {payload.errors.map((item) => `${item.section}：${item.message}`).join("；")}
        </Alert>
      )}

      {payload && (
        <>
          <SimpleGrid cols={{ base: 1, xl: 2 }}>
            <Card className="glass-card" radius="lg" p={0}>
              <Box p="lg" pb="sm">
                <Group justify="space-between">
                  <Text fw={800}>字段预览</Text>
                  <Badge variant="light">{fields.length} 个字段</Badge>
                </Group>
                <Text size="xs" c="dimmed">
                  已按当前脱敏选项处理后展示
                </Text>
              </Box>
              <ScrollArea h={420}>
                <Table highlightOnHover verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>字段路径</Table.Th>
                      <Table.Th>值</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {fields.map((field, index) => (
                      <Table.Tr key={`${field.path}-${index}`}>
                        <Table.Td>
                          <Text size="xs" ff="monospace">
                            {field.path}
                          </Text>
                        </Table.Td>
                        <Table.Td>
                          <Text
                            size="xs"
                            c={field.value === REDACTED ? "orange" : undefined}
                          >
                            {field.value}
                          </Text>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </ScrollArea>
            </Card>

            <Card className="glass-card" radius="lg" p={0}>
              <Box p="lg" pb="sm">
                <Group justify="space-between">
                  <Text fw={800}>Markdown 预览</Text>
                  <Badge variant="light">{markdown.length} 字符</Badge>
                </Group>
                <Text size="xs" c="dimmed">
                  导出的 .md 文件与此内容一致
                </Text>
              </Box>
              <ScrollArea h={420}>
                <pre className="report" style={{ margin: 0 }}>
                  {markdown}
                </pre>
              </ScrollArea>
            </Card>
          </SimpleGrid>

          <Card className="glass-card" radius="lg">
            <Group justify="space-between">
              <Box>
                <Text fw={800}>JSON 预览</Text>
                <Text size="xs" c="dimmed">
                  Schema 4 · 共 {json.length} 字符
                </Text>
              </Box>
              <Button variant="light" onClick={exportJson}>
                导出 JSON
              </Button>
            </Group>
            <ScrollArea h={240} mt="md">
              <pre className="report" style={{ margin: 0 }}>
                {json}
              </pre>
            </ScrollArea>
          </Card>
        </>
      )}
    </Stack>
  );
}
