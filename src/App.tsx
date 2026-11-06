import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Card,
  Center,
  Checkbox,
  Drawer,
  Group,
  Loader,
  NavLink,
  NumberInput,
  Paper,
  Progress,
  ScrollArea,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Tabs,
  Text,
  TextInput,
  ThemeIcon,
  Title,
  Tooltip,
  useMantineColorScheme,
} from "@mantine/core";
import {
  IconActivity,
  IconAlertCircle,
  IconArrowDown,
  IconArrowUp,
  IconApps,
  IconBolt,
  IconCalendarTime,
  IconCamera,
  IconCpu,
  IconDeviceDesktop,
  IconDownload,
  IconFolder,
  IconGauge,
  IconHeartbeat,
  IconNetwork,
  IconPackage,
  IconPlayerPause,
  IconRefresh,
  IconRocket,
  IconSearch,
  IconServer,
  IconShieldCheck,
  IconSpeakerphone,
  IconTemperature,
  IconTool,
  IconUsb,
  IconBluetooth,
  IconChevronDown,
  IconChevronUp,
  IconMoon,
  IconSun,
  IconWind,
} from "@tabler/icons-react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  Treemap,
  XAxis,
  YAxis,
} from "recharts";
import {
  getBenchmarkHistory,
  benchmarkPreflight,
  cancelBenchmark,
  cancelGpuBenchmark,
  getDriverInventory,
  getDiagnosticReport,
  getDeviceDetails,
  getNotificationSettings,
  getStorageDrives,
  getHardwareDetails,
  getNetworkDiagnostics,
  getNvmeHealth,
  getSensorHistory,
  listToolboxTools,
  notifySensorAlert,
  openPathInExplorer,
  runToolAction,
  sendTestNotification,
  saveNotificationSettings,
  getSensorSettings,
  getSensorSnapshot,
  getScheduledTasks,
  saveSensorSettings,
  getSoftwareInventory,
  getStartupItems,
  getSystemSnapshot,
  getSystemHistory,
  queryEventLog,
  isDesktopRuntime,
  runBenchmark,
  scanCleanupCandidates,
  scanDirectory,
  assessDriverUpdates,
  scanDuplicates,
  scanLargeFiles,
  scanStorage,
  setStartupEnabled,
} from "./api";
import type {
  BenchmarkRecord,
  BenchmarkResult,
  CleanupPreview,
  DriverInventory,
  DriverUpdateAssessment,
  DriverUpdate,
  DiagnosticReport,
  DeviceDetails,
  DirectoryListing,
  DuplicateScan,
  EventLogResult,
  HardwareDetails,
  HealthLevel,
  LargeFileScan,
  NetworkDiagnosticReport,
  NvmeHealthReport,
  NotificationSettings,
  ProcessInfo,
  ScheduledTaskInventory,
  SensorHistorySeries,
  SensorSettings,
  SensorSnapshot,
  SensorValue,
  SlimScan,
  SoftwareInventory,
  StorageScan,
  StorageDrive,
  StartupItem,
  SystemHistorySeries,
  SystemSnapshot,
  PowerSettingOption,
  ToolActionDescriptor,
  ToolDescriptor,
  ToolResult,
} from "./types";
import "./styles.css";
import ReportPanel from "./report";

const logoUrl = "/logo.png";

type Tab =
  | "overview"
  | "benchmark"
  | "sensors"
  | "drivers"
  | "diagnostics"
  | "network"
  | "software"
  | "storage"
  | "hardware"
  | "devices"
  | "processes"
  | "startup"
  | "tasks"
  | "toolbox"
  | "report";
type Sample = {
  time: string;
  cpu: number;
  memory: number;
  download: number;
  upload: number;
  diskRead: number;
  diskWrite: number;
};

const nav = [
  ["overview", "性能概览", IconGauge],
  ["benchmark", "性能跑分", IconActivity],
  ["sensors", "温度与健康", IconTemperature],
  ["drivers", "驱动检测", IconTool],
  ["diagnostics", "系统诊断", IconHeartbeat],
  ["network", "网络诊断", IconNetwork],
  ["software", "软件清单", IconApps],
  ["storage", "空间分析", IconFolder],
  ["hardware", "硬件详情", IconCpu],
  ["devices", "设备详情", IconDeviceDesktop],
  ["processes", "运行进程", IconActivity],
  ["startup", "启动项管理", IconRocket],
  ["tasks", "计划任务", IconCalendarTime],
  ["toolbox", "工具箱", IconPackage],
  ["report", "诊断报告", IconShieldCheck],
] as const;

const bytes = (value: number) =>
  value >= 1024 ** 3
    ? `${(value / 1024 ** 3).toFixed(1)} GB`
    : value >= 1024 ** 2
      ? `${(value / 1024 ** 2).toFixed(1)} MB`
      : `${(value / 1024).toFixed(0)} KB`;
const speed = (value: number) => `${bytes(value)}/s`;
const duration = (seconds: number) =>
  `${Math.floor(seconds / 86400)} 天 ${Math.floor((seconds % 86400) / 3600)} 小时`;

function Stat({
  icon: Icon,
  label,
  value,
  hint,
  color,
}: {
  icon: typeof IconCpu;
  label: string;
  value: string;
  hint: string;
  color: string;
}) {
  return (
    <Card className="stat-card" padding="lg" radius="lg">
      <Group justify="space-between" align="flex-start">
        <Box>
          <Text size="xs" c="dimmed" fw={700}>
            {label}
          </Text>
          <Title order={2} mt={6}>
            {value}
          </Title>
          <Text size="xs" c="dimmed" mt={6}>
            {hint}
          </Text>
        </Box>
        <ThemeIcon size={42} radius="xl" variant="light" color={color}>
          <Icon size={22} />
        </ThemeIcon>
      </Group>
    </Card>
  );
}

function PerformanceChart({
  data,
  mode,
}: {
  data: Sample[];
  mode: "system" | "network";
}) {
  const system = mode === "system";
  return (
    <ResponsiveContainer width="100%" height={245}>
      <AreaChart data={data} margin={{ left: -18, right: 8, top: 10 }}>
        <defs>
          <linearGradient id="cpu" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#748ffc" stopOpacity={0.45} />
            <stop offset="100%" stopColor="#748ffc" stopOpacity={0} />
          </linearGradient>
          <linearGradient id="net" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#38d9a9" stopOpacity={0.4} />
            <stop offset="100%" stopColor="#38d9a9" stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke="#293042" strokeDasharray="4 4" />
        <XAxis
          dataKey="time"
          tick={{ fill: "#7d879b", fontSize: 11 }}
          axisLine={false}
          tickLine={false}
        />
        <YAxis
          domain={system ? [0, 100] : [0, "auto"]}
          tick={{ fill: "#7d879b", fontSize: 11 }}
          axisLine={false}
          tickLine={false}
        />
        <ChartTooltip
          contentStyle={{
            background: "#171b25",
            border: "1px solid #30384b",
            borderRadius: 12,
          }}
        />
        {system ? (
          <>
            <Area
              type="monotone"
              dataKey="cpu"
              name="CPU %"
              stroke="#748ffc"
              fill="url(#cpu)"
              strokeWidth={2}
            />
            <Area
              type="monotone"
              dataKey="memory"
              name="内存 %"
              stroke="#da77f2"
              fillOpacity={0}
              strokeWidth={2}
            />
          </>
        ) : (
          <>
            <Area
              type="monotone"
              dataKey="download"
              name="下载 MB/s"
              stroke="#38d9a9"
              fill="url(#net)"
              strokeWidth={2}
            />
            <Area
              type="monotone"
              dataKey="upload"
              name="上传 MB/s"
              stroke="#ffd43b"
              fillOpacity={0}
              strokeWidth={2}
            />
          </>
        )}
      </AreaChart>
    </ResponsiveContainer>
  );
}

function ProcessPanel({ processes }: { processes: ProcessInfo[] }) {
  const [metric, setMetric] = useState<"cpu" | "memory">("cpu");
  const [query, setQuery] = useState("");
  const [sortBy, setSortBy] = useState<"cpu" | "memory" | "name">("cpu");

  const chartData = useMemo(() => {
    const grouped = new Map<
      string,
      { name: string; cpu: number; memory: number; instances: number }
    >();
    processes.forEach((process) => {
      const key = process.name.toLocaleLowerCase();
      const current = grouped.get(key) ?? {
        name: process.name,
        cpu: 0,
        memory: 0,
        instances: 0,
      };
      current.cpu += process.cpuPercent;
      current.memory += process.memoryBytes;
      current.instances += 1;
      grouped.set(key, current);
    });
    return [...grouped.values()]
      .sort((a, b) => b[metric] - a[metric])
      .slice(0, 8)
      .map((item) => ({
        ...item,
        value: metric === "cpu" ? item.cpu : item.memory / 1024 ** 2,
      }));
  }, [metric, processes]);

  const visibleProcesses = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return processes
      .filter((process) => process.name.toLocaleLowerCase().includes(needle))
      .sort((a, b) => {
        if (sortBy === "name") return a.name.localeCompare(b.name);
        if (sortBy === "memory") return b.memoryBytes - a.memoryBytes;
        return b.cpuPercent - a.cpuPercent;
      });
  }, [processes, query, sortBy]);

  const activeProcesses = processes.filter(
    (process) => process.cpuPercent > 0,
  ).length;
  const sampledMemory = processes.reduce(
    (total, process) => total + process.memoryBytes,
    0,
  );

  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 1, sm: 3 }}>
        <Card className="glass-card" radius="lg">
          <Text size="xs" c="dimmed">已采集进程</Text>
          <Title order={3} mt={4}>{processes.length}</Title>
        </Card>
        <Card className="glass-card" radius="lg">
          <Text size="xs" c="dimmed">CPU 活跃进程</Text>
          <Title order={3} mt={4}>{activeProcesses}</Title>
        </Card>
        <Card className="glass-card" radius="lg">
          <Text size="xs" c="dimmed">采集进程内存合计</Text>
          <Title order={3} mt={4}>{bytes(sampledMemory)}</Title>
        </Card>
      </SimpleGrid>

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" mb="md">
          <Box>
            <Text fw={700}>资源占用排行</Text>
            <Text size="xs" c="dimmed">
              同名进程合并统计 · 每 2 秒刷新
            </Text>
          </Box>
          <SegmentedControl
            size="xs"
            value={metric}
            onChange={(value) => setMetric(value as "cpu" | "memory")}
            data={[
              { label: "CPU", value: "cpu" },
              { label: "内存", value: "memory" },
            ]}
          />
        </Group>
        <ResponsiveContainer width="100%" height={300}>
          <BarChart
            data={chartData}
            layout="vertical"
            margin={{ top: 0, right: 34, bottom: 0, left: 24 }}
          >
            <CartesianGrid stroke="#293042" strokeDasharray="4 4" horizontal={false} />
            <XAxis
              type="number"
              axisLine={false}
              tickLine={false}
              tick={{ fill: "#7d879b", fontSize: 11 }}
              tickFormatter={(value) =>
                metric === "cpu" ? `${value}%` : `${value} MB`
              }
            />
            <YAxis
              type="category"
              dataKey="name"
              width={150}
              axisLine={false}
              tickLine={false}
              tick={{ fill: "#7d879b", fontSize: 12 }}
            />
            <ChartTooltip
              cursor={{ fill: "rgba(116, 143, 252, 0.08)" }}
              contentStyle={{
                background: "#171b25",
                border: "1px solid #30384b",
                borderRadius: 12,
              }}
              formatter={(value) => [
                metric === "cpu"
                  ? `${Number(value).toFixed(1)}%`
                  : bytes(Number(value) * 1024 ** 2),
                metric === "cpu" ? "CPU" : "内存",
              ]}
            />
            <Bar dataKey="value" fill="#4c6ef5" radius={[0, 6, 6, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </Card>

      <Card className="glass-card" radius="lg" p={0}>
        <Group p="md" justify="space-between" align="flex-end">
          <Box>
            <Text fw={700}>进程明细</Text>
            <Text size="xs" c="dimmed">
              显示 {visibleProcesses.length} / {processes.length} 个进程
            </Text>
          </Box>
          <Group gap="sm">
            <TextInput
              size="xs"
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              placeholder="搜索进程"
              leftSection={<IconSearch size={15} />}
              w={220}
            />
            <Select
              size="xs"
              value={sortBy}
              onChange={(value) =>
                setSortBy((value as "cpu" | "memory" | "name") ?? "cpu")
              }
              allowDeselect={false}
              w={140}
              data={[
                { value: "cpu", label: "按 CPU 排序" },
                { value: "memory", label: "按内存排序" },
                { value: "name", label: "按名称排序" },
              ]}
            />
          </Group>
        </Group>
        <ScrollArea h={430}>
          <Table stickyHeader highlightOnHover verticalSpacing="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>进程</Table.Th>
                <Table.Th>PID</Table.Th>
                <Table.Th>CPU</Table.Th>
                <Table.Th>内存</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {visibleProcesses.map((process) => (
                <Table.Tr key={process.pid}>
                  <Table.Td><Text fw={600}>{process.name}</Text></Table.Td>
                  <Table.Td>{process.pid}</Table.Td>
                  <Table.Td>{process.cpuPercent.toFixed(1)}%</Table.Td>
                  <Table.Td>{bytes(process.memoryBytes)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>
    </Stack>
  );
}

function HardwarePanel({ details }: { details: HardwareDetails | null }) {
  if (!details)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  return (
    <SimpleGrid cols={{ base: 1, md: 2 }} spacing="md">
      {Object.entries(details).map(([section, value]) => (
        <Card key={section} className="glass-card" radius="lg">
          <Text tt="uppercase" size="xs" c="indigo.3" fw={800} mb="md">
            {section}
          </Text>
          <pre className="hardware-json">{JSON.stringify(value, null, 2)}</pre>
        </Card>
      ))}
    </SimpleGrid>
  );
}

function sensorValueText(sensor: SensorValue): string {
  if (sensor.availability !== "ok" || sensor.value == null) return "不可用";
  return `${sensor.value.toFixed(sensor.category === "temperature" ? 0 : 1)} ${sensor.unit}`;
}

const healthColor = (level: HealthLevel) =>
  level === "critical"
    ? "red"
    : level === "caution"
      ? "orange"
      : level === "healthy"
        ? "teal"
        : "gray";

const healthLabel = (level: HealthLevel) =>
  level === "critical"
    ? "危险"
    : level === "caution"
      ? "注意"
      : level === "healthy"
        ? "健康"
        : "未知";

const sensorCategoryLabels: Record<string, string> = {
  temperature: "温度",
  fan: "风扇",
  power: "功耗",
  clock: "频率",
};

function sensorCategoryOrder(category: string): number {
  const order = ["temperature", "fan", "power", "clock"];
  const index = order.indexOf(category);
  return index === -1 ? order.length : index;
}

function SensorsPanel({ data }: { data: SensorSnapshot | null }) {
  const [draft, setDraft] = useState<SensorSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [settingsStatus, setSettingsStatus] = useState<string | null>(null);
  const [settingsError, setSettingsError] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [hours, setHours] = useState("1");
  const [history, setHistory] = useState<SensorHistorySeries | null>(null);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [nvme, setNvme] = useState<NvmeHealthReport | null>(null);
  const [nvmeLoading, setNvmeLoading] = useState(false);
  const [nvmeError, setNvmeError] = useState<string | null>(null);
  const [notifySettings, setNotifySettings] =
    useState<NotificationSettings | null>(null);
  const [notifySaving, setNotifySaving] = useState(false);
  const [notifyStatus, setNotifyStatus] = useState<string | null>(null);
  const [notifyError, setNotifyError] = useState(false);

  useEffect(() => {
    void getNotificationSettings()
      .then(setNotifySettings)
      .catch(() => undefined);
  }, []);

  const saveNotifySettings = async () => {
    if (!notifySettings) return;
    setNotifySaving(true);
    try {
      setNotifySettings(await saveNotificationSettings(notifySettings));
      setNotifyStatus("通知设置已保存。");
      setNotifyError(false);
    } catch (error) {
      setNotifyStatus(error instanceof Error ? error.message : String(error));
      setNotifyError(true);
    } finally {
      setNotifySaving(false);
    }
  };

  const testNotification = async () => {
    try {
      const outcome = await sendTestNotification();
      setNotifyStatus(outcome.message);
      setNotifyError(outcome.status !== "sent");
    } catch (error) {
      setNotifyStatus(error instanceof Error ? error.message : String(error));
      setNotifyError(true);
    }
  };

  const temperatures = useMemo(
    () =>
      (data?.values ?? []).filter((value) => value.category === "temperature"),
    [data],
  );

  useEffect(() => {
    void getSensorSettings()
      .then((value) => {
        setDraft(value);
      })
      .catch((error) =>
        setSettingsStatus(error instanceof Error ? error.message : String(error)),
      );
  }, []);

  useEffect(() => {
    if (temperatures.length === 0) {
      setSelected(null);
      return;
    }
    if (!selected || !temperatures.some((value) => value.id === selected)) {
      setSelected(temperatures[0].id);
    }
  }, [temperatures, selected]);

  useEffect(() => {
    if (!selected) {
      setHistory(null);
      return;
    }
    let cancelled = false;
    setLoadingHistory(true);
    void getSensorHistory(selected, Number(hours))
      .then((series) => {
        if (cancelled) return;
        setHistory(series);
        setHistoryError(null);
      })
      .catch((error) => {
        if (cancelled) return;
        setHistoryError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (!cancelled) setLoadingHistory(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selected, hours, data?.capturedAtUnixMs]);

  const chartData = useMemo(
    () =>
      (history?.points ?? []).map((point) => ({
        time: new Date(point.t).toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        }),
        avg: +point.avg.toFixed(1),
        max: +point.max.toFixed(1),
        min: +point.min.toFixed(1),
      })),
    [history],
  );

  const grouped = useMemo(() => {
    const map = new Map<string, SensorValue[]>();
    for (const value of data?.values ?? []) {
      const list = map.get(value.category) ?? [];
      list.push(value);
      map.set(value.category, list);
    }
    return [...map.entries()].sort(
      (left, right) => sensorCategoryOrder(left[0]) - sensorCategoryOrder(right[0]),
    );
  }, [data]);

  const hottest = useMemo(
    () =>
      [...temperatures]
        .filter((value) => value.availability === "ok" && value.value != null)
        .sort((left, right) => (right.value ?? 0) - (left.value ?? 0))[0],
    [temperatures],
  );

  const updateThreshold = (
    hardwareClass: keyof SensorSettings["thresholds"],
    field: "warningC" | "criticalC",
    value: number | string,
  ) => {
    const parsed = typeof value === "number" ? value : Number.parseFloat(value);
    if (!Number.isFinite(parsed)) return;
    setDraft((current) =>
      current
        ? {
            ...current,
            thresholds: {
              ...current.thresholds,
              [hardwareClass]: {
                ...current.thresholds[hardwareClass],
                [field]: parsed,
              },
            },
          }
        : current,
    );
  };

  const saveSettings = async () => {
    if (!draft) return;
    setSaving(true);
    setSettingsStatus(null);
    try {
      const saved = await saveSensorSettings(draft);
      setDraft(saved);
      setSettingsError(false);
      setSettingsStatus("阈值与保留策略已保存");
    } catch (error) {
      setSettingsError(true);
      setSettingsStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };
  const loadNvme = async () => {
    setNvmeLoading(true);
    setNvmeError(null);
    try {
      setNvme(await getNvmeHealth());
    } catch (error) {
      setNvmeError(error instanceof Error ? error.message : String(error));
    } finally {
      setNvmeLoading(false);
    }
  };

  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );

  const available = data.values.filter((value) => value.availability === "ok");

  return (
    <Stack gap="md">
      {data.alerts.map((alert) => (
        <Alert
          key={alert.id}
          color={alert.level === "critical" ? "red" : "orange"}
          icon={<IconAlertCircle />}
          title={
            alert.level === "critical"
              ? `温度危险：${alert.sensorName}`
              : `温度警告：${alert.sensorName}`
          }
        >
          {alert.message}
          <Text size="xs" c="dimmed" mt={4}>
            自 {new Date(alert.sinceUnixMs).toLocaleTimeString()} 起持续越限
          </Text>
        </Alert>
      ))}

      <SimpleGrid cols={{ base: 2, xl: 4 }}>
        <Stat
          icon={IconTemperature}
          label="可用传感器"
          value={`${available.length} / ${data.values.length}`}
          hint={`${data.sources.filter((source) => source.ok).length} 个数据源在线`}
          color="cyan"
        />
        <Stat
          icon={IconGauge}
          label="最高温度"
          value={hottest?.value == null ? "不可用" : `${hottest.value.toFixed(0)} °C`}
          hint={hottest?.name ?? "没有可用的温度读数"}
          color={hottest && hottest.value != null && hottest.value >= 85 ? "red" : "teal"}
        />
        <Stat
          icon={IconAlertCircle}
          label="活动告警"
          value={String(data.alerts.length)}
          hint={data.alerts.length ? "连续 3 个样本越限才升级" : "所有温度在阈值内"}
          color={data.alerts.some((alert) => alert.level === "critical") ? "red" : "orange"}
        />
        <Stat
          icon={IconShieldCheck}
          label="权限模式"
          value={data.elevated ? "管理员" : "普通用户"}
          hint={data.elevated ? "可读取受限 SMART" : "部分接口需要管理员"}
          color="indigo"
        />
      </SimpleGrid>

      {data.limitations.map((limitation) => (
        <Alert
          key={`${limitation.code}-${limitation.message}`}
          color="yellow"
          icon={<IconAlertCircle />}
          title={limitation.code}
        >
          {limitation.message}
          <Text size="xs" c="dimmed" mt={4}>
            {limitation.remediation}
          </Text>
        </Alert>
      ))}

      <Tabs defaultValue="sensors" className="sensor-tabs" keepMounted={false}>
        <Tabs.List className="sensor-tabs-list">
          <Tabs.Tab value="sensors" leftSection={<IconActivity size={17} />}>
            实时与趋势
          </Tabs.Tab>
          <Tabs.Tab value="storage" leftSection={<IconServer size={17} />}>
            磁盘健康
          </Tabs.Tab>
          <Tabs.Tab value="settings" leftSection={<IconTool size={17} />}>
            告警设置
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="sensors" pt="md">
          <Stack gap="md">
      <Card className="glass-card history-card" radius="lg">
        <Group className="history-toolbar" justify="space-between" align="flex-start" wrap="wrap">
          <Box className="history-heading">
            <Text fw={800} size="lg">温度历史</Text>
            <Text size="xs" c="dimmed">
              样本保存在本机 SQLite，按范围自动降采样
            </Text>
          </Box>
          <Group className="history-controls" gap="sm">
            <Select
              className="history-sensor-select"
              size="xs"
              placeholder="选择温度传感器"
              data={temperatures.map((value) => ({
                value: value.id,
                label: `${value.hardwareName} · ${value.name}`,
              }))}
              value={selected}
              onChange={setSelected}
              disabled={temperatures.length === 0}
              allowDeselect={false}
            />
            <SegmentedControl
              size="xs"
              value={hours}
              onChange={setHours}
              data={[
                { label: "1 小时", value: "1" },
                { label: "6 小时", value: "6" },
                { label: "24 小时", value: "24" },
              ]}
            />
          </Group>
        </Group>

        {historyError && (
          <Alert color="red" mt="md">
            {historyError}
          </Alert>
        )}
        {temperatures.length === 0 && (
          <Alert color="gray" mt="md">
            当前没有可记录的温度传感器，历史曲线暂不可用。
          </Alert>
        )}
        {temperatures.length > 0 && (
          <>
            <div className="history-metrics">
              <Box className="history-metric history-metric-low">
                <Text size="xs" c="dimmed">
                  最低
                </Text>
                <Text fw={800} size="lg">
                  {history?.min == null ? "—" : `${history.min.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box className="history-metric history-metric-average">
                <Text size="xs" c="dimmed">
                  平均
                </Text>
                <Text fw={800} size="lg">
                  {history?.avg == null ? "—" : `${history.avg.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box className="history-metric history-metric-high">
                <Text size="xs" c="dimmed">
                  最高
                </Text>
                <Text fw={800} size="lg">
                  {history?.max == null ? "—" : `${history.max.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box className="history-metric">
                <Text size="xs" c="dimmed">
                  样本
                </Text>
                <Text fw={800} size="lg">{history?.sampleCount ?? 0}</Text>
              </Box>
              {loadingHistory && <Loader className="history-loader" size="xs" />}
            </div>
            {chartData.length === 0 ? (
              <Alert color="gray" mt="md">
                这个传感器还没有历史样本；桌面应用运行一段时间后会自动累积。
              </Alert>
            ) : (
              <Box className="history-chart">
                <ResponsiveContainer width="100%" height={250}>
                  <LineChart data={chartData} margin={{ left: -10, right: 10, top: 12, bottom: 2 }}>
                    <CartesianGrid stroke="var(--chart-grid)" strokeDasharray="4 5" vertical={false} />
                    <XAxis
                      dataKey="time"
                      tick={{ fill: "#7d879b", fontSize: 11 }}
                      axisLine={false}
                      tickLine={false}
                      minTickGap={24}
                    />
                    <YAxis
                      domain={["auto", "auto"]}
                      tick={{ fill: "#7d879b", fontSize: 11 }}
                      axisLine={false}
                      tickLine={false}
                      unit="°C"
                    />
                    <ChartTooltip
                      contentStyle={{
                        background: "var(--chart-tooltip-bg)",
                        border: "1px solid var(--chart-tooltip-border)",
                        borderRadius: 12,
                        boxShadow: "0 12px 30px rgba(15, 23, 42, .18)",
                      }}
                    />
                    <Line type="monotone" dataKey="avg" name="平均" stroke="#ff922b" strokeWidth={2} dot={false} />
                    <Line type="monotone" dataKey="max" name="最高" stroke="#fa5252" strokeDasharray="4 4" dot={false} />
                    <Line type="monotone" dataKey="min" name="最低" stroke="#4dabf7" strokeDasharray="4 4" dot={false} />
                  </LineChart>
                </ResponsiveContainer>
              </Box>
            )}
          </>
        )}
      </Card>

      {grouped.map(([category, values]) => (
        <Box key={category}>
          <Title order={3} mt="sm">
            {sensorCategoryLabels[category] ?? category}
          </Title>
          <SimpleGrid cols={{ base: 2, xl: 4 }} mt="sm">
            {values.map((sensor) => (
              <Card key={sensor.id} className="stat-card" radius="lg">
                <Group justify="space-between" align="flex-start" wrap="nowrap">
                  <Box maw="75%">
                    <Text size="xs" c="dimmed" fw={700} truncate>
                      {sensor.name}
                    </Text>
                    <Title order={3} mt={6}>
                      {sensorValueText(sensor)}
                    </Title>
                    <Text size="xs" c="dimmed" mt={6} truncate>
                      {sensor.source}
                    </Text>
                  </Box>
                  <ThemeIcon
                    size={38}
                    radius="xl"
                    variant="light"
                    color={
                      sensor.availability !== "ok"
                        ? "gray"
                        : sensor.category === "temperature" && (sensor.value ?? 0) >= 85
                          ? "red"
                          : "cyan"
                    }
                  >
                    {sensor.category === "fan" ? (
                      <IconWind size={20} />
                    ) : sensor.category === "power" ? (
                      <IconBolt size={20} />
                    ) : sensor.category === "clock" ? (
                      <IconGauge size={20} />
                    ) : (
                      <IconTemperature size={20} />
                    )}
                  </ThemeIcon>
                </Group>
                {sensor.availability !== "ok" && (
                  <Tooltip label={sensor.errorCode ?? "接口未提供数据"}>
                    <Badge color="gray" variant="light" mt="sm">
                      {sensor.availability}
                    </Badge>
                  </Tooltip>
                )}
              </Card>
            ))}
          </SimpleGrid>
        </Box>
      ))}

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" mb="sm">
          <Text fw={800}>采集来源</Text>
          <Text size="xs" c="dimmed">
            每个值都可追溯到具体接口
          </Text>
        </Group>
        <Group gap="xs">
          {data.sources.map((source) => (
            <Tooltip
              key={source.source}
              label={source.lastError ?? "正常"}
              multiline
              w={280}
            >
              <Badge color={source.ok ? "teal" : "gray"} variant="light">
                {source.source}
              </Badge>
            </Tooltip>
          ))}
        </Group>
      </Card>

          </Stack>
        </Tabs.Panel>

        <Tabs.Panel value="storage" pt="md">
          <Stack gap="md">

      <Title order={3} mt="sm">
        物理磁盘健康
      </Title>
      {data.storage.length === 0 ? (
        <Alert color="gray">
          Windows Storage API 没有返回物理磁盘健康数据。
        </Alert>
      ) : (
        data.storage.map((disk) => (
          <Card key={disk.name} className="glass-card" radius="lg">
            <Group justify="space-between" mb="md">
              <Box>
                <Text fw={800}>{disk.name}</Text>
                <Text size="xs" c="dimmed">
                  {disk.source}
                </Text>
              </Box>
              <Badge color={healthColor(disk.assessment.level)}>
                {healthLabel(disk.assessment.level)} · {disk.healthStatus}
              </Badge>
            </Group>
            <SimpleGrid cols={{ base: 2, md: 5 }}>
              <Box>
                <Text size="xs" c="dimmed">
                  温度
                </Text>
                <Text fw={700}>
                  {disk.temperatureC == null
                    ? "不可用"
                    : `${disk.temperatureC} °C`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  磨损
                </Text>
                <Text fw={700}>
                  {disk.wearPercent == null ? "不可用" : `${disk.wearPercent}%`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  通电时间
                </Text>
                <Text fw={700}>
                  {disk.powerOnHours == null
                    ? "不可用"
                    : `${disk.powerOnHours} 小时`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  读取错误
                </Text>
                <Text fw={700}>{disk.readErrorsTotal ?? "不可用"}</Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  写入错误
                </Text>
                <Text fw={700}>{disk.writeErrorsTotal ?? "不可用"}</Text>
              </Box>
            </SimpleGrid>
            {disk.assessment.signals.length > 0 && (
              <Stack gap={4} mt="md">
                {disk.assessment.signals.map((signal) => (
                  <Text
                    key={signal.code}
                    size="xs"
                    c={signal.severity === "critical" ? "red" : "orange"}
                  >
                    • {signal.message}
                  </Text>
                ))}
              </Stack>
            )}
          </Card>
        ))
      )}

      {data.smartDevices.length > 0 && (
        <>
          <Title order={3} mt="sm">
            ATA SMART 原始属性
          </Title>
          {data.smartDevices.map((device) => (
            <Card
              key={device.instanceName}
              className="glass-card"
              radius="lg"
              p={0}
            >
              <Box p="lg">
                <Group justify="space-between">
                  <Box>
                    <Text fw={800}>{device.instanceName}</Text>
                    <Text size="xs" c="dimmed">
                      {device.source}
                    </Text>
                  </Box>
                  <Group gap="xs">
                    <Badge color={healthColor(device.assessment.level)} variant="light">
                      {healthLabel(device.assessment.level)}
                    </Badge>
                    <Badge
                      color={
                        device.predictFailure === true
                          ? "red"
                          : device.predictFailure === false
                            ? "teal"
                            : "gray"
                      }
                      variant="light"
                    >
                      {device.predictFailure === true
                        ? "预测故障"
                        : device.predictFailure === false
                          ? "未预测故障"
                          : "预测状态未知"}
                    </Badge>
                  </Group>
                </Group>
              </Box>
              {device.assessment.signals.length > 0 && (
                <Stack gap={4} px="lg" pb="md">
                  {device.assessment.signals.map((signal) => (
                    <Text
                      key={signal.code}
                      size="xs"
                      c={signal.severity === "critical" ? "red" : "orange"}
                    >
                      • {signal.message}
                    </Text>
                  ))}
                </Stack>
              )}
              <Table striped withTableBorder={false}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>ID</Table.Th>
                    <Table.Th>属性</Table.Th>
                    <Table.Th>当前值</Table.Th>
                    <Table.Th>最差值</Table.Th>
                    <Table.Th>原始值</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {device.attributes.map((attribute) => (
                    <Table.Tr key={attribute.id}>
                      <Table.Td>{attribute.id}</Table.Td>
                      <Table.Td>{attribute.name}</Table.Td>
                      <Table.Td>{attribute.current}</Table.Td>
                      <Table.Td>{attribute.worst}</Table.Td>
                      <Table.Td>{attribute.raw}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Card>
          ))}
        </>
      )}

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start" wrap="wrap">
          <Box>
            <Text fw={800}>NVMe 健康日志（直读）</Text>
            <Text size="xs" c="dimmed">
              IOCTL_STORAGE_QUERY_PROPERTY · 读取固件 Health Information Log
            </Text>
          </Box>
          <Button
            size="xs"
            loading={nvmeLoading}
            onClick={() => void loadNvme()}
          >
            读取 NVMe 健康日志
          </Button>
        </Group>
        {nvmeError && (
          <Alert color="red" icon={<IconAlertCircle />} mt="md">
            {nvmeError}
          </Alert>
        )}
        {nvme && (
          <>
            <Alert
              color={
                nvme.accessDenied
                  ? "orange"
                  : nvme.unsupported
                    ? "gray"
                    : "blue"
              }
              icon={<IconShieldCheck />}
              mt="md"
            >
              {nvme.note}
              {nvme.lastError && (
                <Text size="xs" c="dimmed" mt={4}>
                  {nvme.lastError}
                </Text>
              )}
            </Alert>
            {nvme.devices.length > 0 && (
              <Table mt="md" highlightOnHover verticalSpacing="xs">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>设备</Table.Th>
                    <Table.Th>温度</Table.Th>
                    <Table.Th>寿命消耗</Table.Th>
                    <Table.Th>备用空间</Table.Th>
                    <Table.Th>累计写入</Table.Th>
                    <Table.Th>通电 / 次数</Table.Th>
                    <Table.Th>介质错误</Table.Th>
                    <Table.Th>严重警告</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {nvme.devices.map((device) => (
                    <Table.Tr key={device.device}>
                      <Table.Td>
                        <Text size="sm" fw={600}>
                          {device.device}
                        </Text>
                        <Text size="xs" c="dimmed" truncate maw={220}>
                          {device.source}
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        {device.temperatureC == null
                          ? "不可用"
                          : `${device.temperatureC.toFixed(0)} °C`}
                      </Table.Td>
                      <Table.Td>{device.percentageUsed}%</Table.Td>
                      <Table.Td>
                        {device.availableSparePercent}%
                        <Text size="xs" c="dimmed">
                          阈值 {device.spareThresholdPercent}%
                        </Text>
                      </Table.Td>
                      <Table.Td>{bytes(device.dataWrittenBytes)}</Table.Td>
                      <Table.Td>
                        {device.powerOnHours} 小时
                        <Text size="xs" c="dimmed">
                          {device.powerCycles} 次上电
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        {device.mediaErrors}
                        <Text size="xs" c="dimmed">
                          日志 {device.errorLogEntries} 条
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        <Badge
                          color={device.criticalWarning === 0 ? "teal" : "red"}
                          variant="light"
                        >
                          {device.criticalWarning === 0
                            ? "正常"
                            : `0x${device.criticalWarning.toString(16).toUpperCase()}`}
                        </Badge>
                        {device.unsafeShutdowns > 0 && (
                          <Text size="xs" c="dimmed">
                            异常断电 {device.unsafeShutdowns}
                          </Text>
                        )}
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            )}
          </>
        )}
      </Card>

          </Stack>
        </Tabs.Panel>

        <Tabs.Panel value="settings" pt="md">
          <Stack gap="md">

      {draft && (
        <Card className="glass-card" radius="lg">
          <Group justify="space-between" align="flex-start">
            <Box>
              <Text fw={800}>告警阈值与历史保留</Text>
              <Text size="xs" c="dimmed">
                连续 3 个样本越限才升级；回落需低于阈值 5 °C 才解除
              </Text>
            </Box>
            <Switch
              checked={draft.alertsEnabled}
              onChange={(event) =>
                setDraft({ ...draft, alertsEnabled: event.currentTarget.checked })
              }
              label="启用温度告警"
            />
          </Group>
          <SimpleGrid cols={{ base: 1, md: 3 }} mt="md">
            {(
              [
                ["cpu", "CPU"],
                ["gpu", "GPU"],
                ["disk", "磁盘"],
              ] as const
            ).map(([hardwareClass, label]) => (
              <Paper key={hardwareClass} p="md" radius="md" withBorder>
                <Text fw={700} mb="sm">
                  {label}
                </Text>
                <Group grow>
                  <NumberInput
                    size="xs"
                    label="警告 °C"
                    min={30}
                    max={110}
                    value={draft.thresholds[hardwareClass].warningC}
                    onChange={(value) =>
                      updateThreshold(hardwareClass, "warningC", value)
                    }
                  />
                  <NumberInput
                    size="xs"
                    label="危险 °C"
                    min={31}
                    max={120}
                    value={draft.thresholds[hardwareClass].criticalC}
                    onChange={(value) =>
                      updateThreshold(hardwareClass, "criticalC", value)
                    }
                  />
                </Group>
              </Paper>
            ))}
          </SimpleGrid>
          <Group mt="md" align="flex-end">
            <NumberInput
              size="xs"
              label="历史保留（小时）"
              min={1}
              max={720}
              w={180}
              value={draft.retentionHours}
              onChange={(value) => {
                const parsed =
                  typeof value === "number" ? value : Number.parseFloat(value);
                if (Number.isFinite(parsed)) {
                  setDraft({ ...draft, retentionHours: Math.round(parsed) });
                }
              }}
            />
            <Button loading={saving} onClick={() => void saveSettings()}>
              保存设置
            </Button>
            {settingsStatus && (
              <Text size="sm" c={settingsError ? "red" : "teal"}>
                {settingsStatus}
              </Text>
            )}
          </Group>
        </Card>
      )}
      {notifySettings && (
        <Card className="glass-card" radius="lg">
          <Group justify="space-between" align="flex-start" wrap="wrap">
            <Box>
              <Text fw={800}>桌面通知</Text>
              <Text size="xs" c="dimmed">
                温度告警通过 Windows 通知弹出；同一告警有冷却时间，免打扰时段只记录不弹窗
              </Text>
            </Box>
            <Switch
              checked={notifySettings.enabled}
              onChange={(event) =>
                setNotifySettings({
                  ...notifySettings,
                  enabled: event.currentTarget.checked,
                })
              }
              label="启用桌面通知"
            />
          </Group>
          <Group mt="md" align="flex-end" wrap="wrap">
            <NumberInput
              size="xs"
              label="冷却（分钟）"
              min={1}
              max={60}
              w={140}
              value={notifySettings.cooldownMinutes}
              onChange={(value) => {
                const parsed =
                  typeof value === "number" ? value : Number.parseFloat(value);
                if (Number.isFinite(parsed)) {
                  setNotifySettings({
                    ...notifySettings,
                    cooldownMinutes: Math.round(parsed),
                  });
                }
              }}
            />
            <NumberInput
              size="xs"
              label="免打扰开始（时）"
              min={0}
              max={23}
              w={150}
              value={notifySettings.quietStartHour}
              onChange={(value) => {
                const parsed =
                  typeof value === "number" ? value : Number.parseFloat(value);
                if (Number.isFinite(parsed)) {
                  setNotifySettings({
                    ...notifySettings,
                    quietStartHour: Math.round(parsed),
                  });
                }
              }}
            />
            <NumberInput
              size="xs"
              label="免打扰结束（时）"
              min={0}
              max={23}
              w={150}
              value={notifySettings.quietEndHour}
              onChange={(value) => {
                const parsed =
                  typeof value === "number" ? value : Number.parseFloat(value);
                if (Number.isFinite(parsed)) {
                  setNotifySettings({
                    ...notifySettings,
                    quietEndHour: Math.round(parsed),
                  });
                }
              }}
            />
            <Button
              size="xs"
              loading={notifySaving}
              onClick={() => void saveNotifySettings()}
            >
              保存通知设置
            </Button>
            <Button
              size="xs"
              variant="light"
              onClick={() => void testNotification()}
            >
              发送测试通知
            </Button>
          </Group>
          {notifyStatus && (
            <Text size="sm" mt="sm" c={notifyError ? "red" : "teal"}>
              {notifyStatus}
            </Text>
          )}
        </Card>
      )}

          {!draft && !notifySettings && (
            <Alert color="gray">设置正在加载，请稍候。</Alert>
          )}
          </Stack>
        </Tabs.Panel>
      </Tabs>
    </Stack>
  );
}


function driverStatus(status: string): { label: string; color: string } {
  if (status === "applicable") return { label: "可更新", color: "teal" };
  if (status === "current") return { label: "已是最新", color: "gray" };
  if (status === "no_match") return { label: "无匹配设备", color: "orange" };
  return { label: "未知", color: "gray" };
}

function DriversPanel({ data }: { data: DriverInventory | null }) {
  const [query, setQuery] = useState("");
  const [updates, setUpdates] = useState<DriverUpdateAssessment[] | null>(null);
  const [scanningUpdates, setScanningUpdates] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [assessmentNote, setAssessmentNote] = useState<string>("");
  const scanUpdates = async () => {
    setScanningUpdates(true);
    setUpdateError(null);
    try {
      const report = await assessDriverUpdates();
      setUpdates(report.updates);
      setAssessmentNote(report.note);
    } catch (error) {
      setUpdateError(error instanceof Error ? error.message : String(error));
    } finally {
      setScanningUpdates(false);
    }
  };
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const needle = query.trim().toLowerCase();
  const items = data.items.filter(
    (item) =>
      !needle ||
      [
        item.deviceName,
        item.deviceClass,
        item.manufacturer,
        item.driverProvider,
        item.driverVersion,
        item.infName,
        item.deviceId,
      ].some((value) => value.toLowerCase().includes(needle)),
  );
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 2, xl: 5 }}>
        <Stat
          icon={IconTool}
          label="设备总数"
          value={String(data.total)}
          hint={data.source}
          color="indigo"
        />
        <Stat
          icon={IconShieldCheck}
          label="已签名驱动"
          value={String(data.signed)}
          hint="Windows 报告为已签名"
          color="teal"
        />
        <Stat
          icon={IconAlertCircle}
          label="未签名"
          value={String(data.unsigned)}
          hint="建议核对发布者"
          color="orange"
        />
        <Stat
          icon={IconDownload}
          label="缺少驱动"
          value={String(data.missing)}
          hint="没有匹配驱动记录"
          color="red"
        />
        <Stat
          icon={IconActivity}
          label="设备异常"
          value={String(data.problems)}
          hint="PnP 错误码非 0"
          color="yellow"
        />
      </SimpleGrid>
      <Alert color="blue" icon={<IconShieldCheck />}>
        检测结果来自 Windows CIM
        和即插即用数据库。当前版本只读，不会自动下载或安装驱动。
      </Alert>
      <Tabs defaultValue="updates" keepMounted={false}>
        <Tabs.List className="module-tabs-list">
          <Tabs.Tab value="updates" leftSection={<IconDownload size={17} />}>
            官方更新
          </Tabs.Tab>
          <Tabs.Tab value="inventory" leftSection={<IconTool size={17} />}>
            驱动清单 <Badge size="xs" variant="light">{data.total}</Badge>
          </Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="updates" pt="md">
      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start">
          <Box>
            <Text fw={800}>Windows Update 驱动候选</Text>
            <Text size="xs" c="dimmed">
              只读查询 Microsoft Update；按硬件 ID 匹配本机设备并比较版本，同时检查签名状态。不下载、不安装、不回滚。
            </Text>
          </Box>
          <Button
            variant="light"
            leftSection={<IconSearch size={17} />}
            loading={scanningUpdates}
            onClick={() => void scanUpdates()}
          >
            扫描官方更新
          </Button>
        </Group>
        {updateError && (
          <Alert color="red" mt="md">
            {updateError}
          </Alert>
        )}
        {updates && updates.length === 0 && (
          <Alert color="teal" mt="md">
            Windows Update 当前没有返回驱动更新候选。
          </Alert>
        )}
        {updates && updates.length > 0 && (
          <Stack mt="md" gap="sm">
            {updates.map((assessment) => {
              const update = assessment.update;
              const status = driverStatus(assessment.status);
              return (
                <Paper
                  key={`${update.updateId}-${update.revisionNumber}`}
                  p="md"
                  radius="md"
                  withBorder
                >
                  <Group justify="space-between" align="flex-start">
                    <Box maw="75%">
                      <Group gap="xs">
                        <Text fw={700}>{update.title}</Text>
                        <Badge color={status.color} variant="light">
                          {status.label}
                        </Badge>
                        <Badge
                          color={
                            assessment.signatureOk == null
                              ? "gray"
                              : assessment.signatureOk
                                ? "teal"
                                : "red"
                          }
                          variant="light"
                        >
                          {assessment.signatureOk == null
                            ? "签名未知"
                            : assessment.signatureOk
                              ? "已签名"
                              : "未签名"}
                        </Badge>
                      </Group>
                      <Text size="xs" c="dimmed" mt={4}>
                        {update.driverManufacturer ||
                          update.driverProvider ||
                          "发布者未知"}{" "}
                        ·{" "}
                        {update.driverModel ||
                          update.driverClass ||
                          "设备类型未知"}
                      </Text>
                      <Text size="xs" c="dimmed" mt={5}>
                        {update.description || "没有附加说明"}
                      </Text>
                      {assessment.matchingDevices.length > 0 && (
                        <Text size="xs" mt={5}>
                          匹配设备：{assessment.matchingDevices.join("；")}
                        </Text>
                      )}
                      {assessment.notes.map((note) => (
                        <Text key={note} size="xs" c="dimmed">
                          {note}
                        </Text>
                      ))}
                    </Box>
                    <Box ta="right">
                      <Text size="xs" c="dimmed">
                        候选版本 {update.driverVersion || "未知"}
                      </Text>
                      <Text size="xs" c="dimmed" mt={4} maw={220} truncate>
                        {update.driverHardwareId || "未提供硬件 ID"}
                      </Text>
                      <Text size="xs" c="dimmed" mt={4}>
                        {update.downloaded ? "已下载" : "未下载"}
                      </Text>
                    </Box>
                  </Group>
                </Paper>
              );
            })}
            {assessmentNote && (
              <Text size="xs" c="dimmed">
                {assessmentNote}
              </Text>
            )}
          </Stack>
        )}
      </Card>
        </Tabs.Panel>
        <Tabs.Panel value="inventory" pt="md">
          <Stack gap="md">
      <TextInput
        value={query}
        onChange={(event) => setQuery(event.currentTarget.value)}
        leftSection={<IconSearch size={17} />}
        placeholder="搜索设备、厂商、版本、INF 或硬件 ID"
      />
      <Card className="glass-card" radius="lg" p={0}>
        <ScrollArea h="calc(100vh - 390px)">
          <Table stickyHeader highlightOnHover verticalSpacing="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>设备</Table.Th>
                <Table.Th>驱动</Table.Th>
                <Table.Th>签名</Table.Th>
                <Table.Th>状态</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {items.map((item) => (
                <Table.Tr key={item.deviceId}>
                  <Table.Td>
                    <Text fw={700} size="sm">
                      {item.deviceName}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {item.deviceClass || "未分类"} ·{" "}
                      {item.manufacturer || "未知厂商"}
                    </Text>
                    <Text size="xs" c="dimmed" truncate maw={380}>
                      {item.deviceId}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">
                      {item.missingDriver
                        ? "缺少驱动"
                        : item.driverVersion || "版本未知"}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {item.driverProvider || "提供商未知"} ·{" "}
                      {item.infName || "无 INF"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Badge
                      color={
                        item.signed
                          ? "teal"
                          : item.missingDriver
                            ? "red"
                            : "orange"
                      }
                      variant="light"
                    >
                      {item.signed
                        ? "已签名"
                        : item.missingDriver
                          ? "缺失"
                          : "未签名"}
                    </Badge>
                  </Table.Td>
                  <Table.Td>
                    <Badge color={item.errorCode === 0 ? "gray" : "red"}>
                      {item.status} · {item.errorCode}
                    </Badge>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>
          </Stack>
        </Tabs.Panel>
      </Tabs>
    </Stack>
  );
}

function DevicePanel({ data }: { data: DeviceDetails | null }) {
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const maskedSerial = (serial: string) =>
    serial
      ? `${serial.slice(0, 2)}${"*".repeat(Math.max(3, serial.length - 2))}`
      : "未提供";
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        只读枚举：显示器 EDID、音频、USB、蓝牙与摄像头。不打开设备、不配对、不安装或修改驱动；显示器序列号已掩码。
      </Alert>
      {data.unavailable.map((message) => (
        <Alert key={message} color="orange" icon={<IconAlertCircle />}>
          {message}
        </Alert>
      ))}
      <Tabs defaultValue="monitors" keepMounted={false}>
        <Tabs.List className="module-tabs-list">
          <Tabs.Tab value="monitors" leftSection={<IconDeviceDesktop size={17} />}>显示器 <Badge size="xs" variant="light">{data.monitors.length}</Badge></Tabs.Tab>
          <Tabs.Tab value="audio" leftSection={<IconSpeakerphone size={17} />}>音频 <Badge size="xs" variant="light">{data.audio.length}</Badge></Tabs.Tab>
          <Tabs.Tab value="usb" leftSection={<IconUsb size={17} />}>USB <Badge size="xs" variant="light">{data.usb.length}</Badge></Tabs.Tab>
          <Tabs.Tab value="bluetooth" leftSection={<IconBluetooth size={17} />}>蓝牙 <Badge size="xs" variant="light">{data.bluetooth.length}</Badge></Tabs.Tab>
          <Tabs.Tab value="cameras" leftSection={<IconCamera size={17} />}>摄像头 <Badge size="xs" variant="light">{data.cameras.length}</Badge></Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="monitors" pt="md">
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Text fw={800}>显示器</Text>
            <Badge variant="light">{data.monitors.length} 台</Badge>
          </Group>
          {data.monitors.length === 0 ? (
            <Text size="sm" c="dimmed" mt="sm">
              未读取到 WmiMonitorID 数据（部分虚拟机和远程会话不提供 EDID）。
            </Text>
          ) : (
            <Table mt="sm" verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>名称</Table.Th>
                  <Table.Th>厂商</Table.Th>
                  <Table.Th>型号</Table.Th>
                  <Table.Th>年份</Table.Th>
                  <Table.Th>序列号</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.monitors.map((monitor) => (
                  <Table.Tr key={`${monitor.name}-${monitor.serial}`}>
                    <Table.Td>
                      <Text size="sm" fw={600}>
                        {monitor.name}
                      </Text>
                    </Table.Td>
                    <Table.Td>{monitor.manufacturer || "—"}</Table.Td>
                    <Table.Td>{monitor.product || "—"}</Table.Td>
                    <Table.Td>{monitor.year || "—"}</Table.Td>
                    <Table.Td>
                      <Text size="xs" c="dimmed" ff="monospace">
                        {maskedSerial(monitor.serial)}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        </Tabs.Panel>

        <Tabs.Panel value="audio" pt="md">
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Text fw={800}>音频设备</Text>
            <Badge variant="light">{data.audio.length} 个</Badge>
          </Group>
          {data.audio.length === 0 ? (
            <Text size="sm" c="dimmed" mt="sm">
              未发现音频设备。
            </Text>
          ) : (
            <Table mt="sm" verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>名称</Table.Th>
                  <Table.Th>厂商</Table.Th>
                  <Table.Th>状态</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.audio.map((device) => (
                  <Table.Tr key={device.name}>
                    <Table.Td>{device.name}</Table.Td>
                    <Table.Td>{device.manufacturer || "—"}</Table.Td>
                    <Table.Td>
                      <Badge size="xs" variant="light">
                        {device.status || "未知"}
                      </Badge>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        </Tabs.Panel>

        <Tabs.Panel value="usb" pt="md">
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Text fw={800}>USB 设备</Text>
            <Badge variant="light">{data.usb.length} 个</Badge>
          </Group>
          {data.usb.length === 0 ? (
            <Text size="sm" c="dimmed" mt="sm">
              未发现 USB 设备。
            </Text>
          ) : (
            <Table mt="sm" verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>名称</Table.Th>
                  <Table.Th>VID / PID</Table.Th>
                  <Table.Th>状态</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.usb.map((device) => (
                  <Table.Tr key={`${device.name}-${device.vid}-${device.pid}`}>
                    <Table.Td>
                      <Text size="sm" fw={600} truncate maw={260}>
                        {device.name}
                      </Text>
                      <Text size="xs" c="dimmed" truncate maw={260}>
                        {device.manufacturer || "厂商未知"}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs" ff="monospace">
                        {device.vid ? `${device.vid}:${device.pid || "????"}` : "—"}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Badge size="xs" variant="light">
                        {device.status || "未知"}
                      </Badge>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        </Tabs.Panel>

        <Tabs.Panel value="bluetooth" pt="md">
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Text fw={800}>蓝牙</Text>
            <Badge variant="light">{data.bluetooth.length} 个</Badge>
          </Group>
          {data.bluetooth.length === 0 ? (
            <Text size="sm" c="dimmed" mt="sm">
              未发现蓝牙适配器或已配对设备。
            </Text>
          ) : (
            <Table mt="sm" verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>名称</Table.Th>
                  <Table.Th>类型</Table.Th>
                  <Table.Th>状态</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.bluetooth.map((device) => (
                  <Table.Tr key={`${device.name}-${device.kind}`}>
                    <Table.Td>
                      <Text size="sm" fw={600}>
                        {device.name}
                      </Text>
                      <Text size="xs" c="dimmed">
                        {device.manufacturer || "厂商未知"}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Badge size="xs" variant="light">
                        {device.kind}
                      </Badge>
                    </Table.Td>
                    <Table.Td>{device.status || "未知"}</Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        </Tabs.Panel>

        <Tabs.Panel value="cameras" pt="md">
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Text fw={800}>摄像头</Text>
            <Badge variant="light">{data.cameras.length} 个</Badge>
          </Group>
          {data.cameras.length === 0 ? (
            <Text size="sm" c="dimmed" mt="sm">
              未发现摄像头设备。
            </Text>
          ) : (
            <Table mt="sm" verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>名称</Table.Th>
                  <Table.Th>厂商</Table.Th>
                  <Table.Th>类型</Table.Th>
                  <Table.Th>状态</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.cameras.map((camera) => (
                  <Table.Tr key={camera.name}>
                    <Table.Td>{camera.name}</Table.Td>
                    <Table.Td>{camera.manufacturer || "—"}</Table.Td>
                    <Table.Td>
                      <Badge size="xs" variant="light">
                        {camera.pnpClass}
                      </Badge>
                    </Table.Td>
                    <Table.Td>{camera.status || "未知"}</Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        </Tabs.Panel>
      </Tabs>
      <Text size="xs" c="dimmed">
        来源：{data.source} · 采集于{" "}
        {new Date(data.capturedAtUnixMs).toLocaleString()}
      </Text>
    </Stack>
  );
}

function StoragePanel() {
  const [running, setRunning] = useState<StorageScan["kind"] | null>(null);
  const [results, setResults] = useState<Record<string, StorageScan>>({});
  const [failure, setFailure] = useState<string | null>(null);
  const [deepKind, setDeepKind] = useState<StorageScan["kind"]>("downloads");
  const [minMb, setMinMb] = useState("10");
  const [large, setLarge] = useState<LargeFileScan | null>(null);
  const [dupes, setDupes] = useState<DuplicateScan | null>(null);
  const [loadingLarge, setLoadingLarge] = useState(false);
  const [loadingDupes, setLoadingDupes] = useState(false);
  const [deepFailure, setDeepFailure] = useState<string | null>(null);
  const [cleanup, setCleanup] = useState<CleanupPreview | null>(null);
  const [loadingCleanup, setLoadingCleanup] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [loadingDirectory, setLoadingDirectory] = useState(false);
  const [directoryError, setDirectoryError] = useState<string | null>(null);
  const [drives, setDrives] = useState<StorageDrive[]>([]);
  const [expandedCategories, setExpandedCategories] = useState<
    Record<string, boolean>
  >({});
  useEffect(() => {
    void getStorageDrives()
      .then(setDrives)
      .catch(() => undefined);
  }, []);
  const scan = async (kind: StorageScan["kind"]) => {
    setRunning(kind);
    setFailure(null);
    try {
      const result = await scanStorage(kind);
      setResults((old) => ({ ...old, [kind]: result }));
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setRunning(null);
    }
  };
  const runLarge = async () => {
    setLoadingLarge(true);
    setDeepFailure(null);
    try {
      setLarge(await scanLargeFiles(deepKind, Number(minMb) * 1024 ** 2, 50));
    } catch (error) {
      setDeepFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingLarge(false);
    }
  };
  const runDupes = async () => {
    setLoadingDupes(true);
    setDeepFailure(null);
    try {
      setDupes(await scanDuplicates(deepKind, 50));
    } catch (error) {
      setDeepFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingDupes(false);
    }
  };
  const runCleanup = async () => {
    setLoadingCleanup(true);
    setCleanupError(null);
    try {
      setCleanup(await scanCleanupCandidates());
    } catch (error) {
      setCleanupError(error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingCleanup(false);
    }
  };
  const openDirectory = async (relative: string) => {
    setLoadingDirectory(true);
    setDirectoryError(null);
    try {
      setListing(await scanDirectory(deepKind, relative, 200));
    } catch (error) {
      setDirectoryError(error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingDirectory(false);
    }
  };
  const maxDirectoryBytes = Math.max(
    1,
    ...(large?.directories.map((item) => item.bytes) ?? []),
  );
  const storageLabel = deepKind.startsWith("drive:")
    ? `${deepKind.slice("drive:".length).toUpperCase()} 盘`
    : (({ downloads: "下载", documents: "文档", desktop: "桌面" }) as Record<
        string,
        string
      >)[deepKind] ?? deepKind;
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        默认只扫描下载、文档和桌面三个标准目录；在“大文件与重复文件”中可切换到整盘。结果按扩展名聚合，不显示文件内容，也不会删除文件。每次最多统计
        25 万个文件；整盘扫描会跳过 Windows、回收站、System Volume Information 等系统目录。
      </Alert>

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start" wrap="wrap">
          <Box>
            <Text fw={800}>清理候选（只读预览）</Text>
            <Text size="xs" c="dimmed">
              统计常见可清理位置的占用，不会删除任何文件；本版本不提供清理动作。
            </Text>
          </Box>
          <Button
            size="xs"
            loading={loadingCleanup}
            onClick={() => void runCleanup()}
          >
            扫描清理候选
          </Button>
        </Group>
        {cleanupError && (
          <Alert color="red" icon={<IconAlertCircle />} mt="md">
            {cleanupError}
          </Alert>
        )}
        {cleanup && (
          <>
            <Group gap="lg" mt="md">
              <Badge variant="light">合计 {bytes(cleanup.totalBytes)}</Badge>
              <Text size="xs" c="dimmed">
                {cleanup.totalFiles} 个文件 · {cleanup.source}
              </Text>
            </Group>
            <Table mt="sm" highlightOnHover verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>位置</Table.Th>
                  <Table.Th>占用</Table.Th>
                  <Table.Th>文件</Table.Th>
                  <Table.Th>权限</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {cleanup.candidates.map((candidate) => (
                  <Table.Tr key={candidate.id}>
                    <Table.Td>
                      <Text fw={600} size="sm">
                        {candidate.label}
                      </Text>
                      <Text size="xs" c="dimmed" truncate maw={520}>
                        {candidate.path || "无法解析路径"}
                      </Text>
                      <Text size="xs" c="dimmed">
                        {candidate.note}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      {candidate.exists ? bytes(candidate.bytes) : "不存在"}
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">{candidate.fileCount}</Text>
                      {candidate.errorCount > 0 && (
                        <Text size="xs" c="orange">
                          {candidate.errorCount} 个读取错误
                        </Text>
                      )}
                      {candidate.truncated && (
                        <Badge size="xs" color="orange">
                          已截断
                        </Badge>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <Badge
                        size="xs"
                        variant="light"
                        color={candidate.requiresAdmin ? "orange" : "gray"}
                      >
                        {candidate.requiresAdmin ? "需要管理员" : "普通权限"}
                      </Badge>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </>
        )}
      </Card>
      {failure && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {failure}
        </Alert>
      )}
      <SimpleGrid cols={{ base: 1, md: 3 }}>
        {(["downloads", "documents", "desktop"] as const).map((kind) => {
          const result = results[kind];
          const label = {
            downloads: "下载",
            documents: "文档",
            desktop: "桌面",
          }[kind];
          return (
            <Card key={kind} className="glass-card" radius="lg">
              <Group justify="space-between">
                <Box>
                  <Text fw={800}>{label}</Text>
                  <Text size="xs" c="dimmed">
                    {result?.path || "尚未扫描"}
                  </Text>
                </Box>
                <ThemeIcon variant="light" size={40}>
                  <IconFolder />
                </ThemeIcon>
              </Group>
              {result && (
                <Box mt="lg">
                  <Title order={2}>{bytes(result.totalBytes)}</Title>
                  <Text size="xs" c="dimmed">
                    {result.fileCount} 个文件 · {result.directoryCount} 个目录 ·{" "}
                    {result.errorCount} 个读取错误
                  </Text>
                  {result.truncated && (
                    <Badge color="orange" mt="sm">
                      已达到 25 万文件上限
                    </Badge>
                  )}
                </Box>
              )}
              <Button
                fullWidth
                mt="lg"
                variant={result ? "light" : "filled"}
                loading={running === kind}
                disabled={running !== null && running !== kind}
                onClick={() => void scan(kind)}
              >
                {result ? "重新扫描" : "开始扫描"}
              </Button>
            </Card>
          );
        })}
      </SimpleGrid>
      {Object.values(results).map((result) => (
        <Card key={result.kind} className="glass-card" radius="lg">
          <Group justify="space-between" mb="md">
            <Box>
              <Text fw={800}>{result.label} · 文件类型占用</Text>
              <Text size="xs" c="dimmed">
                仅显示占用最大的 30 种扩展名
              </Text>
            </Box>
            <Badge variant="light">{bytes(result.totalBytes)}</Badge>
          </Group>
          <SimpleGrid cols={{ base: 1, xl: 2 }} spacing="sm" verticalSpacing="sm">
            {result.categories
              .slice(0, expandedCategories[result.kind] ? undefined : 8)
              .map((category) => {
              const percent = result.totalBytes
                ? (category.totalBytes / result.totalBytes) * 100
                : 0;
              return (
                <Box key={category.extension}>
                  <Group justify="space-between">
                    <Text size="sm" fw={600}>
                      {category.extension}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {category.fileCount} 个 · {bytes(category.totalBytes)}
                    </Text>
                  </Group>
                  <Progress value={percent} mt={5} />
                </Box>
              );
            })}
          </SimpleGrid>
          {result.categories.length > 8 && (
            <Button
              fullWidth
              mt="md"
              size="xs"
              variant="subtle"
              leftSection={
                expandedCategories[result.kind] ? (
                  <IconChevronUp size={16} />
                ) : (
                  <IconChevronDown size={16} />
                )
              }
              onClick={() =>
                setExpandedCategories((current) => ({
                  ...current,
                  [result.kind]: !current[result.kind],
                }))
              }
            >
              {expandedCategories[result.kind]
                ? "收起"
                : `查看全部 ${result.categories.length} 种文件类型`}
            </Button>
          )}
        </Card>
      ))}

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start" wrap="wrap">
          <Box>
            <Text fw={800}>大文件与重复文件</Text>
            <Text size="xs" c="dimmed">
              只读扫描，不删除任何文件；重复文件按大小分组后做全量哈希
            </Text>
          </Box>
          <Group gap="sm">
            <Select
              size="xs"
              w={200}
              allowDeselect={false}
              value={deepKind}
              onChange={(value) => {
                if (!value) return;
                setDeepKind(value);
                setLarge(null);
                setDupes(null);
                setListing(null);
              }}
              data={[
                { value: "downloads", label: "下载" },
                { value: "documents", label: "文档" },
                { value: "desktop", label: "桌面" },
                ...drives.map((drive) => ({
                  value: `drive:${drive.letter}`,
                  label: `${drive.letter} 盘 · ${bytes(drive.freeBytes)} 可用`,
                })),
              ]}
            />
            <Select
              size="xs"
              w={140}
              allowDeselect={false}
              value={minMb}
              onChange={(value) => setMinMb(value ?? "10")}
              data={[
                { value: "10", label: "≥ 10 MB" },
                { value: "100", label: "≥ 100 MB" },
                { value: "1024", label: "≥ 1 GB" },
              ]}
            />
            <Button size="xs" loading={loadingLarge} onClick={() => void runLarge()}>
              扫描大文件
            </Button>
            <Button
              size="xs"
              variant="light"
              loading={loadingDupes}
              onClick={() => void runDupes()}
            >
              查找重复文件
            </Button>
          </Group>
        </Group>

        {deepFailure && (
          <Alert color="red" icon={<IconAlertCircle />} mt="md">
            {deepFailure}
          </Alert>
        )}

        {large && (
          <Box mt="lg">
            <Group gap="lg" mb="sm">
              <Badge variant="light">{large.label}</Badge>
              <Text size="xs" c="dimmed">
                {large.fileCount} 个文件 · 总计 {bytes(large.totalBytes)}
              </Text>
              {large.truncated && <Badge color="orange">已达 25 万文件上限</Badge>}
            </Group>
            <SimpleGrid cols={{ base: 1, lg: 2 }}>
              <Box>
                <Text fw={700} size="sm" mb="xs">
                  目录占用
                </Text>
                <Stack gap="sm">
                  {large.directories.map((directory) => (
                    <Box key={directory.path}>
                      <Group justify="space-between">
                        <Text size="sm" fw={600}>
                          {directory.name}
                        </Text>
                        <Text size="xs" c="dimmed">
                          {directory.fileCount} 个 · {bytes(directory.bytes)}
                        </Text>
                      </Group>
                      <Progress
                        value={(directory.bytes / maxDirectoryBytes) * 100}
                        mt={5}
                      />
                    </Box>
                  ))}
                </Stack>
              </Box>
              <Box>
                <Text fw={700} size="sm" mb="xs">
                  最大的文件
                </Text>
                {large.files.length === 0 ? (
                  <Text c="dimmed" size="sm">
                    没有超过阈值的文件。
                  </Text>
                ) : (
                  <Table highlightOnHover verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>文件</Table.Th>
                        <Table.Th>大小</Table.Th>
                        <Table.Th>修改时间</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {large.files.map((file) => (
                        <Table.Tr key={file.path}>
                          <Table.Td>
                            <Text size="sm" fw={600} truncate maw={220}>
                              {file.name}
                            </Text>
                            <Text size="xs" c="dimmed" truncate maw={260}>
                              {file.path}
                            </Text>
                          </Table.Td>
                          <Table.Td>{bytes(file.bytes)}</Table.Td>
                          <Table.Td>
                            <Text size="xs" c="dimmed">
                              {file.modifiedUnixMs
                                ? new Date(file.modifiedUnixMs).toLocaleDateString()
                                : "未知"}
                            </Text>
                          </Table.Td>
                        </Table.Tr>
                      ))}
                    </Table.Tbody>
                  </Table>
                )}
              </Box>
            </SimpleGrid>
          </Box>
        )}

        {dupes && (
          <Box mt="lg">
            <Group gap="lg" mb="sm">
              <Badge variant="light">{dupes.label}</Badge>
              <Text size="xs" c="dimmed">
                已哈希 {dupes.candidateFiles} 个文件 · {bytes(dupes.scannedBytes)} ·{" "}
                {dupes.groups.length} 组重复
              </Text>
              {dupes.truncated && (
                <Badge color="orange">已达哈希上限，结果可能不完整</Badge>
              )}
            </Group>
            {dupes.groups.length === 0 ? (
              <Alert color="teal" icon={<IconShieldCheck />}>
                未发现重复文件（只比较 ≥1 MiB 且内容哈希一致的文件）。
              </Alert>
            ) : (
              <Stack gap="sm">
                {dupes.groups.map((group) => (
                  <Paper
                    key={`${group.hash}-${group.sizeBytes}`}
                    p="md"
                    radius="md"
                    withBorder
                  >
                    <Group justify="space-between">
                      <Text fw={700} size="sm">
                        {group.files.length} 个相同文件 · 每个 {bytes(group.sizeBytes)}
                      </Text>
                      <Badge color="orange">可释放 {bytes(group.wastedBytes)}</Badge>
                    </Group>
                    <Stack gap={4} mt="xs">
                      {group.files.map((file) => (
                        <Text key={file.path} size="xs" c="dimmed" truncate>
                          {file.path}
                        </Text>
                      ))}
                    </Stack>
                  </Paper>
                ))}
              </Stack>
            )}
          </Box>
        )}
      </Card>

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start" wrap="wrap">
          <Box>
            <Text fw={800}>目录钻取</Text>
            <Text size="xs" c="dimmed">
              在当前范围（{storageLabel}）内导航；Treemap 面积代表占用，不显示文件内容。
            </Text>
          </Box>
          <Group gap="sm">
            <Button
              size="xs"
              loading={loadingDirectory}
              onClick={() => void openDirectory(listing?.relativePath ?? "")}
            >
              打开目录
            </Button>
            {listing?.parentRelativePath != null && (
              <Button
                size="xs"
                variant="light"
                onClick={() => void openDirectory(listing.parentRelativePath ?? "")}
              >
                返回上级
              </Button>
            )}
          </Group>
        </Group>
        {listing && (
          <Text size="xs" c="dimmed" ff="monospace" mt="xs" truncate>
            {listing.root}\{listing.relativePath}
          </Text>
        )}
        {directoryError && (
          <Alert color="red" icon={<IconAlertCircle />} mt="md">
            {directoryError}
          </Alert>
        )}
        {listing && (
          <>
            <Group gap="lg" mt="md">
              <Badge variant="light">{bytes(listing.totalBytes)}</Badge>
              <Text size="xs" c="dimmed">
                {listing.totalFiles} 个文件 · {listing.children.length} 项
              </Text>
              {listing.truncated && <Badge color="orange">已截断</Badge>}
              {listing.errorCount > 0 && (
                <Text size="xs" c="orange">
                  {listing.errorCount} 个读取错误
                </Text>
              )}
            </Group>
            {listing.exclusions && listing.exclusions.length > 0 && (
              <Text size="xs" c="dimmed" mt={4}>
                已跳过系统目录与文件：{listing.exclusions.join("、")}
              </Text>
            )}
            <SimpleGrid cols={{ base: 1, lg: 2 }} mt="md">
              <Box h={300}>
                {listing.children.length === 0 ? (
                  <Center h={300}>
                    <Text c="dimmed" size="sm">
                      目录为空
                    </Text>
                  </Center>
                ) : (
                  <ResponsiveContainer width="100%" height="100%">
                    <Treemap
                      data={listing.children.map((child) => ({
                        name: child.name,
                        size: Math.max(child.bytes, 1),
                        isDir: child.isDir,
                      }))}
                      dataKey="size"
                      nameKey="name"
                      stroke="#161a24"
                      fill="#4c6ef5"
                      aspectRatio={4 / 3}
                    />
                  </ResponsiveContainer>
                )}
              </Box>
              <Table highlightOnHover verticalSpacing="xs">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>名称</Table.Th>
                    <Table.Th>类型</Table.Th>
                    <Table.Th>占用</Table.Th>
                    <Table.Th>文件</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {listing.children.map((child) => (
                    <Table.Tr
                      key={child.path}
                      style={{ cursor: child.isDir ? "pointer" : "default" }}
                      onClick={() => {
                        if (child.isDir) void openDirectory(child.relativePath);
                      }}
                    >
                      <Table.Td>
                        <Text
                          size="sm"
                          fw={child.isDir ? 700 : 400}
                          truncate
                          maw={260}
                        >
                          {child.name}
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        <Badge
                          size="xs"
                          variant="light"
                          color={child.isDir ? "indigo" : "gray"}
                        >
                          {child.isDir ? "目录" : "文件"}
                        </Badge>
                      </Table.Td>
                      <Table.Td>{bytes(child.bytes)}</Table.Td>
                      <Table.Td>{child.fileCount}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </SimpleGrid>
          </>
        )}
      </Card>
    </Stack>
  );
}

function SoftwarePanel({ data }: { data: SoftwareInventory | null }) {
  const [query, setQuery] = useState("");
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const needle = query.trim().toLowerCase();
  const items = data.items.filter(
    (item) =>
      !needle ||
      [item.name, item.version, item.publisher, item.installLocation].some(
        (value) => value.toLowerCase().includes(needle),
      ),
  );
  const dated = [...data.items]
    .filter((item) => item.installDate)
    .sort((a, b) => b.installDate.localeCompare(a.installDate));
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 1, md: 3 }}>
        <Stat
          icon={IconApps}
          label="已安装程序"
          value={String(data.total)}
          hint={data.source}
          color="indigo"
        />
        <Stat
          icon={IconServer}
          label="登记占用空间"
          value={bytes(data.estimatedSizeBytes)}
          hint="仅统计提供 EstimatedSize 的程序"
          color="teal"
        />
        <Stat
          icon={IconActivity}
          label="最近登记安装"
          value={dated[0]?.installDate || "日期不可用"}
          hint={dated[0]?.name || "没有安装日期记录"}
          color="blue"
        />
      </SimpleGrid>
      <Alert color="blue" icon={<IconShieldCheck />}>
        清单直接读取 HKLM/HKCU 卸载注册表，不调用可能触发 MSI 修复的
        Win32_Product，也不会执行卸载。
      </Alert>
      <TextInput
        value={query}
        onChange={(event) => setQuery(event.currentTarget.value)}
        leftSection={<IconSearch size={17} />}
        placeholder="搜索软件、版本、发布者或安装位置"
      />
      <Card className="glass-card" radius="lg" p={0}>
        <ScrollArea h="calc(100vh - 360px)">
          <Table stickyHeader highlightOnHover verticalSpacing="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>软件</Table.Th>
                <Table.Th>版本与发布者</Table.Th>
                <Table.Th>范围</Table.Th>
                <Table.Th>登记大小</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {items.map((item, index) => (
                <Table.Tr
                  key={`${item.name}-${item.version}-${item.scope}-${index}`}
                >
                  <Table.Td>
                    <Text fw={700}>{item.name}</Text>
                    <Text size="xs" c="dimmed" truncate maw={360}>
                      {item.installLocation || "未登记安装位置"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">{item.version || "版本未知"}</Text>
                    <Text size="xs" c="dimmed">
                      {item.publisher || "发布者未知"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Badge variant="light">
                      {item.scope} · {item.architecture}
                    </Badge>
                    <Text size="xs" c="dimmed" mt={5}>
                      {item.uninstallAvailable ? "提供卸载入口" : "无卸载入口"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    {item.estimatedSizeBytes
                      ? bytes(item.estimatedSizeBytes)
                      : "未登记"}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>
    </Stack>
  );
}

function NetworkPanel({ data }: { data: NetworkDiagnosticReport | null }) {
  const [query, setQuery] = useState("");
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const needle = query.trim().toLowerCase();
  const connections = data.connections.filter(
    (item) =>
      !needle ||
      [
        item.processName,
        String(item.pid),
        item.localAddress,
        item.remoteAddress,
        String(item.remotePort),
      ].some((value) => value.toLowerCase().includes(needle)),
  );
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 2, xl: 4 }}>
        <Stat
          icon={IconNetwork}
          label="活动 TCP 连接"
          value={String(data.connections.length)}
          hint="最多展示 200 条已建立连接"
          color="indigo"
        />
        <Stat
          icon={IconServer}
          label="默认网关"
          value={data.defaultGateways[0] || "未检测到"}
          hint={
            data.defaultGateways.length > 1
              ? `${data.defaultGateways.length} 个网关`
              : data.source
          }
          color="teal"
        />
        <Stat
          icon={IconShieldCheck}
          label="系统代理"
          value={data.proxyEnabled ? "已启用" : "未启用"}
          hint={data.proxyServer || data.autoConfigUrl || "没有代理服务器"}
          color={data.proxyEnabled ? "orange" : "teal"}
        />
        <Stat
          icon={IconTool}
          label="Hosts 自定义项"
          value={String(data.hostsCustomEntries)}
          hint="仅统计非注释记录，不展示内容"
          color="blue"
        />
      </SimpleGrid>
      <Tabs defaultValue="connections" keepMounted={false}>
        <Tabs.List className="module-tabs-list">
          <Tabs.Tab value="connections" leftSection={<IconNetwork size={17} />}>
            活动连接 <Badge size="xs" variant="light">{data.connections.length}</Badge>
          </Tabs.Tab>
          <Tabs.Tab value="dns" leftSection={<IconServer size={17} />}>
            DNS 配置 <Badge size="xs" variant="light">{data.dns.length}</Badge>
          </Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="dns" pt="md">
      <Card className="glass-card" radius="lg">
        <Text fw={800} mb="md">
          DNS 服务器
        </Text>
        <SimpleGrid cols={{ base: 1, md: 2 }}>
          {data.dns.map((item) => (
            <Paper key={item.alias} p="md" radius="md" withBorder>
              <Text fw={700}>{item.alias}</Text>
              <Text size="sm" c="dimmed">
                {item.servers.join(" · ")}
              </Text>
            </Paper>
          ))}
        </SimpleGrid>
        {data.dns.length === 0 && (
          <Text c="dimmed">没有读取到 IPv4 DNS 服务器。</Text>
        )}
      </Card>
        </Tabs.Panel>
        <Tabs.Panel value="connections" pt="md">
          <Stack gap="md">
      <Group justify="space-between">
        <Title order={3}>已建立连接</Title>
        <TextInput
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          leftSection={<IconSearch size={17} />}
          placeholder="搜索进程、PID、地址或端口"
          w={360}
        />
      </Group>
      <Alert color="blue" icon={<IconShieldCheck />}>
        远程地址仅在本机展示，不会写入诊断报告或上传。
      </Alert>
      <Card className="glass-card" radius="lg" p={0}>
        <ScrollArea h="calc(100vh - 470px)">
          <Table stickyHeader highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>进程</Table.Th>
                <Table.Th>本地端点</Table.Th>
                <Table.Th>远程端点</Table.Th>
                <Table.Th>状态</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {connections.map((item, index) => (
                <Table.Tr
                  key={`${item.pid}-${item.localPort}-${item.remoteAddress}-${item.remotePort}-${index}`}
                >
                  <Table.Td>
                    <Text fw={700}>{item.processName}</Text>
                    <Text size="xs" c="dimmed">
                      PID {item.pid}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    {item.localAddress}:{item.localPort}
                  </Table.Td>
                  <Table.Td>
                    {item.remoteAddress}:{item.remotePort}
                  </Table.Td>
                  <Table.Td>
                    <Badge color="teal" variant="light">
                      {item.state}
                    </Badge>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>
          </Stack>
        </Tabs.Panel>
      </Tabs>
    </Stack>
  );
}

function DiagnosticsPanel({ data }: { data: DiagnosticReport | null }) {
  const [logName, setLogName] = useState("System");
  const [logLevel, setLogLevel] = useState("error");
  const [logHours, setLogHours] = useState("24");
  const [logSearch, setLogSearch] = useState("");
  const [logResult, setLogResult] = useState<EventLogResult | null>(null);
  const [logLoading, setLogLoading] = useState(false);
  const [logError, setLogError] = useState<string | null>(null);
  const runLogQuery = async () => {
    setLogLoading(true);
    setLogError(null);
    try {
      setLogResult(
        await queryEventLog(logName, logLevel, Number(logHours), logSearch, 100),
      );
    } catch (error) {
      setLogError(error instanceof Error ? error.message : String(error));
    } finally {
      setLogLoading(false);
    }
  };
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const battery = data.battery;
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 1, md: 3 }}>
        <Stat
          icon={IconHeartbeat}
          label="待重启状态"
          value={data.pendingReboot ? "需要重启" : "无需重启"}
          hint={
            data.pendingWindowsUpdateReboot
              ? "Windows Update 请求重启"
              : "未发现更新重启标记"
          }
          color={data.pendingReboot ? "orange" : "teal"}
        />
        <Stat
          icon={IconBolt}
          label="电池电量"
          value={
            battery.present && battery.chargePercent != null
              ? `${battery.chargePercent.toFixed(0)}%`
              : "无电池"
          }
          hint={
            battery.present
              ? `${battery.name} · ${battery.status}`
              : "台式机或电池接口不可用"
          }
          color="blue"
        />
        <Stat
          icon={IconShieldCheck}
          label="电池健康"
          value={
            battery.healthPercent == null
              ? "不可用"
              : `${battery.healthPercent.toFixed(1)}%`
          }
          hint={
            battery.cycleCount == null
              ? "循环次数不可用"
              : `${battery.cycleCount} 次循环`
          }
          color={
            battery.healthPercent != null && battery.healthPercent < 70
              ? "red"
              : "teal"
          }
        />
      </SimpleGrid>
      <Tabs defaultValue="errors" keepMounted={false}>
        <Tabs.List className="module-tabs-list">
          <Tabs.Tab value="errors" leftSection={<IconAlertCircle size={17} />}>
            系统错误 <Badge size="xs" color={data.events.length ? "orange" : "teal"}>{data.events.length}</Badge>
          </Tabs.Tab>
          <Tabs.Tab value="crashes" leftSection={<IconHeartbeat size={17} />}>
            崩溃与关机 <Badge size="xs" color={data.crashEvents.length ? "red" : "teal"}>{data.crashEvents.length}</Badge>
          </Tabs.Tab>
          <Tabs.Tab value="services" leftSection={<IconTool size={17} />}>
            服务异常 <Badge size="xs" color={data.serviceIssues.length ? "orange" : "teal"}>{data.serviceIssues.length}</Badge>
          </Tabs.Tab>
          <Tabs.Tab value="event-log" leftSection={<IconSearch size={17} />}>
            事件日志查询
          </Tabs.Tab>
          {battery.present && (
            <Tabs.Tab value="battery" leftSection={<IconBolt size={17} />}>
              电池详情
            </Tabs.Tab>
          )}
        </Tabs.List>
        <Tabs.Panel value="battery" pt="md">
      {battery.present && (
        <Card className="glass-card" radius="lg">
          <Group justify="space-between">
            <Box>
              <Text fw={800}>电池容量</Text>
              <Text size="xs" c="dimmed">
                {data.source}
              </Text>
            </Box>
            <Badge
              color={
                battery.healthPercent != null && battery.healthPercent < 70
                  ? "red"
                  : "teal"
              }
            >
              {battery.status}
            </Badge>
          </Group>
          <SimpleGrid cols={3} mt="lg">
            <Box>
              <Text size="xs" c="dimmed">
                设计容量
              </Text>
              <Text fw={700}>
                {battery.designCapacityMwh == null
                  ? "不可用"
                  : `${battery.designCapacityMwh} mWh`}
              </Text>
            </Box>
            <Box>
              <Text size="xs" c="dimmed">
                满充容量
              </Text>
              <Text fw={700}>
                {battery.fullChargeCapacityMwh == null
                  ? "不可用"
                  : `${battery.fullChargeCapacityMwh} mWh`}
              </Text>
            </Box>
            <Box>
              <Text size="xs" c="dimmed">
                损耗率
              </Text>
              <Text fw={700}>
                {battery.healthPercent == null
                  ? "不可用"
                  : `${Math.max(0, 100 - battery.healthPercent).toFixed(1)}%`}
              </Text>
            </Box>
          </SimpleGrid>
        </Card>
      )}
        </Tabs.Panel>
        <Tabs.Panel value="errors" pt="md">
          <Stack gap="md">
      <Group justify="space-between" mt="sm">
        <Title order={3}>最近 24 小时系统错误</Title>
        <Badge variant="light" color={data.events.length ? "orange" : "teal"}>
          {data.events.length} 条
        </Badge>
      </Group>
      {data.events.length === 0 ? (
        <Alert color="teal" icon={<IconShieldCheck />}>
          系统日志中没有找到最近 24 小时的严重或错误事件。
        </Alert>
      ) : (
        data.events.map((event, index) => (
          <Card
            key={`${event.timeCreated}-${event.eventId}-${index}`}
            className="glass-card"
            radius="lg"
          >
            <Group justify="space-between" align="flex-start">
              <Box>
                <Text fw={700}>
                  {event.provider} · 事件 {event.eventId}
                </Text>
                <Text size="xs" c="dimmed">
                  {new Date(event.timeCreated).toLocaleString()}
                </Text>
              </Box>
              <Badge
                color={
                  event.level.includes("严重") ||
                  event.level.toLowerCase().includes("critical")
                    ? "red"
                    : "orange"
                }
              >
                {event.level}
              </Badge>
            </Group>
            <Text size="sm" c="dimmed" mt="md" className="event-message">
              {event.message || "事件没有提供说明。"}
            </Text>
          </Card>
        ))
      )}
          </Stack>
        </Tabs.Panel>
        <Tabs.Panel value="crashes" pt="md">
          <Stack gap="md">
      <Group justify="space-between" mt="sm">
        <Title order={3}>最近 7 天崩溃与意外关机</Title>
        <Badge
          color={data.crashEvents.length || data.minidumpCount ? "red" : "teal"}
          variant="light"
        >
          {data.crashEvents.length} 个事件 · {data.minidumpCount} 个转储
        </Badge>
      </Group>
      {data.latestMinidump && (
        <Alert color="orange" icon={<IconAlertCircle />}>
          最近蓝屏转储时间：{new Date(data.latestMinidump).toLocaleString()}
          。应用只读取转储文件元数据，不打开转储内容。
        </Alert>
      )}
      {data.crashEvents.length === 0 ? (
        <Alert color="teal" icon={<IconShieldCheck />}>
          没有找到最近 7 天的意外关机或应用崩溃事件。
        </Alert>
      ) : (
        data.crashEvents.map((event, index) => (
          <Card
            key={`crash-${event.timeCreated}-${event.eventId}-${index}`}
            className="glass-card"
            radius="lg"
          >
            <Group justify="space-between">
              <Box>
                <Text fw={700}>
                  {event.provider} · 事件 {event.eventId}
                </Text>
                <Text size="xs" c="dimmed">
                  {new Date(event.timeCreated).toLocaleString()}
                </Text>
              </Box>
              <Badge color="red">{event.level}</Badge>
            </Group>
            <Text size="sm" c="dimmed" mt="md">
              {event.message || "事件没有提供说明。"}
            </Text>
          </Card>
        ))
      )}
          </Stack>
        </Tabs.Panel>
        <Tabs.Panel value="services" pt="md">
          <Stack gap="md">
      <Group justify="space-between" mt="sm">
        <Title order={3}>自动启动服务异常</Title>
        <Badge
          color={data.serviceIssues.length ? "orange" : "teal"}
          variant="light"
        >
          {data.serviceIssues.length} 项
        </Badge>
      </Group>
      {data.serviceIssues.length === 0 ? (
        <Alert color="teal" icon={<IconShieldCheck />}>
          没有发现应自动启动但当前未运行的服务。
        </Alert>
      ) : (
        <Card className="glass-card" radius="lg" p={0}>
          <Table highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>服务</Table.Th>
                <Table.Th>状态</Table.Th>
                <Table.Th>启动模式</Table.Th>
                <Table.Th>退出码</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {data.serviceIssues.map((service) => (
                <Table.Tr key={service.name}>
                  <Table.Td>
                    <Text fw={700}>{service.displayName}</Text>
                    <Text size="xs" c="dimmed">
                      {service.name}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Badge color="orange">{service.state}</Badge>
                  </Table.Td>
                  <Table.Td>{service.startMode}</Table.Td>
                  <Table.Td>{service.exitCode}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>
      )}
          </Stack>
        </Tabs.Panel>

        <Tabs.Panel value="event-log" pt="md">
          <Stack gap="md">
      <Group justify="space-between" mt="md">
        <Title order={3}>事件日志查询</Title>
        <Text size="xs" c="dimmed">
          {logResult ? `${logResult.total} 条匹配` : "只读，不修改日志"}
        </Text>
      </Group>
      <Alert color="blue" icon={<IconShieldCheck />}>
        直接读取 Windows 事件日志；Security 日志通常需要管理员权限，普通权限下会返回空结果。
      </Alert>
      <Group align="flex-end" wrap="wrap">
        <Select
          size="xs"
          label="日志"
          w={150}
          allowDeselect={false}
          value={logName}
          onChange={(value) => setLogName(value ?? "System")}
          data={[
            { value: "System", label: "System" },
            { value: "Application", label: "Application" },
            { value: "Setup", label: "Setup" },
            { value: "Security", label: "Security" },
          ]}
        />
        <Select
          size="xs"
          label="级别"
          w={150}
          allowDeselect={false}
          value={logLevel}
          onChange={(value) => setLogLevel(value ?? "error")}
          data={[
            { value: "all", label: "全部" },
            { value: "critical", label: "严重" },
            { value: "error", label: "错误" },
            { value: "warning", label: "警告" },
            { value: "information", label: "信息" },
          ]}
        />
        <SegmentedControl
          size="xs"
          value={logHours}
          onChange={setLogHours}
          data={[
            { label: "1 小时", value: "1" },
            { label: "24 小时", value: "24" },
            { label: "7 天", value: "168" },
          ]}
        />
        <TextInput
          size="xs"
          label="关键词"
          w={240}
          value={logSearch}
          onChange={(event) => setLogSearch(event.currentTarget.value)}
          placeholder="按消息或来源过滤"
        />
        <Button size="xs" loading={logLoading} onClick={() => void runLogQuery()}>
          查询
        </Button>
      </Group>
      {logError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {logError}
        </Alert>
      )}
      {logResult && (
        <Card className="glass-card" radius="lg" p={0}>
          <Group justify="space-between" p="md">
            <Text fw={700} size="sm">
              {logResult.logName} · 显示 {logResult.events.length} / 共 {logResult.total} 条
            </Text>
            <Group gap="xs">
              {logResult.truncated && <Badge color="orange">已截断到 100 条</Badge>}
              <Badge variant="light">{logResult.source}</Badge>
            </Group>
          </Group>
          <ScrollArea h={420}>
            <Table stickyHeader highlightOnHover verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>时间</Table.Th>
                  <Table.Th>级别</Table.Th>
                  <Table.Th>来源</Table.Th>
                  <Table.Th>事件</Table.Th>
                  <Table.Th>说明</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {logResult.events.map((entry, index) => (
                  <Table.Tr key={`${entry.timeCreatedUnixMs}-${entry.eventId}-${index}`}>
                    <Table.Td>
                      <Text size="xs">
                        {new Date(entry.timeCreatedUnixMs).toLocaleString()}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Badge
                        size="xs"
                        color={
                          entry.level.includes("严重") || entry.level.includes("Critical")
                            ? "red"
                            : entry.level.includes("错误") || entry.level.includes("Error")
                              ? "orange"
                              : entry.level.includes("警告") || entry.level.includes("Warning")
                                ? "yellow"
                                : "gray"
                        }
                      >
                        {entry.level}
                      </Badge>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs" truncate maw={160}>
                        {entry.provider}
                      </Text>
                    </Table.Td>
                    <Table.Td>{entry.eventId}</Table.Td>
                    <Table.Td>
                      <Text size="xs" lineClamp={3} maw={620}>
                        {entry.message || "事件没有提供说明。"}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </ScrollArea>
        </Card>
      )}
          </Stack>
        </Tabs.Panel>
      </Tabs>
    </Stack>
  );
}

const benchmarkKinds: Array<{
  kind: BenchmarkResult["kind"];
  label: string;
  description: string;
}> = [
  {
    kind: "cpu-single",
    label: "CPU 单核",
    description: "固定整数混合工作负载",
  },
  {
    kind: "cpu-multi",
    label: "CPU 多核",
    description: "最多使用 32 个逻辑线程",
  },
  { kind: "memory", label: "内存带宽", description: "64 MiB 缓冲区复制测试" },
  {
    kind: "disk",
    label: "磁盘顺序读写",
    description: "临时创建并删除测试文件（默认 1 GiB，可配置）",
  },
  { kind: "gpu", label: "GPU 计算", description: "WebGPU 计算（Windows 走 D3D12 后端），失败回退 WebGL2" },
];

function BenchmarkPanel() {
  const [benchmarkTab, setBenchmarkTab] = useState<string | null>("cpu");
  const [running, setRunning] = useState<BenchmarkResult["kind"] | null>(null);
  const [results, setResults] = useState<Record<string, BenchmarkResult>>({});
  const [benchmarkHistory, setBenchmarkHistory] = useState<BenchmarkRecord[]>(
    [],
  );
  const [failure, setFailure] = useState<string | null>(null);
  const [diskSize, setDiskSize] = useState("1024");
  const [cancelling, setCancelling] = useState(false);
  useEffect(() => {
    void getBenchmarkHistory()
      .then(setBenchmarkHistory)
      .catch((error) => setFailure(String(error)));
  }, []);
  const run = async (kind: BenchmarkResult["kind"]) => {
    setRunning(kind);
    setFailure(null);
    try {
      const preflight = await benchmarkPreflight();
      if (preflight.warnings.length > 0) {
        const proceed = window.confirm(
          [
            ...preflight.warnings,
            "",
            "跑分期间请勿运行其他高负载程序。仍要继续吗？",
          ].join("\n"),
        );
        if (!proceed) return;
      }
      const result = await runBenchmark(
        kind,
        kind === "disk" ? Number(diskSize) : undefined,
      );
      setResults((old) => ({ ...old, [kind]: result }));
      setBenchmarkHistory(await getBenchmarkHistory());
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setFailure(
        message.includes("BENCHMARK_CANCELLED")
          ? "已取消本次跑分。"
          : message,
      );
    } finally {
      setRunning(null);
    }
  };
  const complete = benchmarkKinds.every((item) => results[item.kind]);
  const weights: Record<BenchmarkResult["kind"], number> = {
    "cpu-single": 0.2,
    "cpu-multi": 0.3,
    memory: 0.15,
    disk: 0.15,
    gpu: 0.2,
  };
  const composite = complete
    ? Math.exp(
        benchmarkKinds.reduce(
          (sum, item) =>
            sum +
            weights[item.kind] *
              Math.log(Math.max(1, results[item.kind].score)),
          0,
        ),
      )
    : null;
  const visibleBenchmarkKinds = benchmarkKinds.filter((item) =>
    benchmarkTab === "cpu"
      ? item.kind === "cpu-single" || item.kind === "cpu-multi"
      : item.kind === benchmarkTab,
  );
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconGauge />}>
        跑分会在短时间内提高 CPU、内存、磁盘或 GPU
        负载。综合分采用加权几何平均：单核 20%、多核 30%、内存 15%、磁盘
        15%、GPU 20%。
      </Alert>
      <Tabs value={benchmarkTab} onChange={setBenchmarkTab} variant="pills">
        <Tabs.List>
          <Tabs.Tab value="cpu" leftSection={<IconCpu size={16} />}>
            CPU
          </Tabs.Tab>
          <Tabs.Tab value="memory">内存</Tabs.Tab>
          <Tabs.Tab value="disk">磁盘</Tabs.Tab>
          <Tabs.Tab value="gpu">GPU</Tabs.Tab>
          <Tabs.Tab value="history">历史记录</Tabs.Tab>
        </Tabs.List>
      </Tabs>
      {benchmarkTab === "disk" && (
      <Group justify="space-between" align="flex-end" wrap="wrap">
        <Group align="flex-end">
          <Box>
            <Text size="xs" c="dimmed" mb={4}>
              磁盘测试大小
            </Text>
            <Select
              size="xs"
              w={170}
              allowDeselect={false}
              value={diskSize}
              onChange={(value) => setDiskSize(value ?? "1024")}
              data={[
                { value: "128", label: "128 MiB" },
                { value: "512", label: "512 MiB" },
                { value: "1024", label: "1 GiB（默认）" },
                { value: "2048", label: "2 GiB" },
                { value: "4096", label: "4 GiB" },
              ]}
            />
          </Box>
          <Text size="xs" c="dimmed" mb={6}>
            仅影响磁盘顺序读写；文件在系统临时目录创建并自动删除。
          </Text>
        </Group>
        {running && (
          <Button
            size="xs"
            color="red"
            variant="light"
            loading={cancelling}
            onClick={() => {
              setCancelling(true);
              cancelGpuBenchmark();
              void cancelBenchmark().finally(() => setCancelling(false));
            }}
          >
            取消测试
          </Button>
        )}
      </Group>
      )}
      {benchmarkTab !== "disk" && running && (
        <Group justify="flex-end">
          <Button
            size="xs"
            color="red"
            variant="light"
            loading={cancelling}
            onClick={() => {
              setCancelling(true);
              cancelGpuBenchmark();
              void cancelBenchmark().finally(() => setCancelling(false));
            }}
          >
            取消测试
          </Button>
        </Group>
      )}
      {failure && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {failure}
        </Alert>
      )}
      {composite != null && (
        <Card className="glass-card" radius="lg">
          <Text size="xs" c="dimmed" fw={800}>
            SYSTEM LENS 综合分 · 公式 v3
          </Text>
          <Title order={1} mt="xs">
            {composite.toFixed(0)} 分
          </Title>
          <Text size="sm" c="dimmed">
            仅用于相同 Schema 和应用版本之间比较。
          </Text>
        </Card>
      )}
      {benchmarkTab !== "history" && (
      <SimpleGrid cols={{ base: 1, md: benchmarkTab === "cpu" ? 2 : 1 }}>
        {visibleBenchmarkKinds.map((item) => {
          const result = results[item.kind];
          return (
            <Card key={item.kind} className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Text fw={800}>{item.label}</Text>
                  <Text size="xs" c="dimmed">
                    {item.description}
                  </Text>
                </Box>
                {result && (
                  <Badge variant="light" color="teal">
                    Schema {result.schemaVersion}
                  </Badge>
                )}
              </Group>
              {result ? (
                <Box mt="xl">
                  <Title order={1}>
                    {result.score.toFixed(0)}{" "}
                    <Text component="span" size="sm" c="dimmed">
                      {result.unit}
                    </Text>
                  </Title>
                  <SimpleGrid cols={2} mt="md">
                    {Object.entries(result.details)
                      .filter(([key]) => key !== "checksum")
                      .map(([key, value]) => (
                        <Box key={key}>
                          <Text size="xs" c="dimmed">
                            {key}
                          </Text>
                          <Text fw={700}>{Number(value).toFixed(2)}</Text>
                        </Box>
                      ))}
                  </SimpleGrid>
                  <Text size="xs" c="dimmed" mt="md">
                    耗时 {result.durationMs} ms · {result.note}
                  </Text>
                </Box>
              ) : (
                <Text c="dimmed" mt="xl">
                  尚未运行
                </Text>
              )}
              <Button
                fullWidth
                mt="xl"
                variant={result ? "light" : "filled"}
                loading={running === item.kind}
                disabled={running !== null && running !== item.kind}
                onClick={() => void run(item.kind)}
              >
                {result ? "重新测试" : "开始测试"}
              </Button>
            </Card>
          );
        })}
      </SimpleGrid>
      )}
      {benchmarkTab === "history" && (
      <>
      <Title order={3} mt="md">
        本机历史记录
      </Title>
      {benchmarkHistory.length === 0 ? (
        <Alert color="gray">还没有保存的跑分结果。</Alert>
      ) : (
        <Card className="glass-card" radius="lg" p={0}>
          <Table highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>时间</Table.Th>
                <Table.Th>项目</Table.Th>
                <Table.Th>分数</Table.Th>
                <Table.Th>耗时</Table.Th>
                <Table.Th>环境</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {benchmarkHistory.slice(0, 20).map((record) => (
                <Table.Tr
                  key={`${record.recordedAtUnixMs}-${record.result.kind}`}
                >
                  <Table.Td>
                    {new Date(record.recordedAtUnixMs).toLocaleString()}
                  </Table.Td>
                  <Table.Td>
                    {record.result.label}
                    <Badge size="xs" variant="light" ml={6}>
                      Schema {record.result.schemaVersion}
                    </Badge>
                    {record.result.kind === "gpu" &&
                      record.result.schemaVersion < 3 && (
                        <Tooltip label="旧 WebGL2 GPU 工作负载，分数不能与 Schema 3 直接比较">
                          <Badge size="xs" color="gray" variant="light" ml={4}>
                            旧 GPU
                          </Badge>
                        </Tooltip>
                      )}
                  </Table.Td>
                  <Table.Td>{record.result.score.toFixed(0)} 分</Table.Td>
                  <Table.Td>{record.result.durationMs} ms</Table.Td>
                  <Table.Td>
                    {record.environment ? (
                      <Tooltip
                        multiline
                        w={320}
                        label={`${record.environment.osName} ${record.environment.osVersion} · ${(
                          record.environment.memoryBytes / 1024 ** 3
                        ).toFixed(1)} GB 内存 · 应用 v${record.environment.appVersion}`}
                      >
                        <Box>
                          <Text size="xs" truncate maw={220}>
                            {record.environment.cpuBrand}
                          </Text>
                          <Text size="xs" c="dimmed">
                            {record.environment.logicalCores} 线程 · v
                            {record.environment.appVersion}
                          </Text>
                        </Box>
                      </Tooltip>
                    ) : (
                      <Text size="xs" c="dimmed">
                        旧记录（无环境信息）
                      </Text>
                    )}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>
      )}
      </>
      )}
    </Stack>
  );
}

const formatTaskTime = (value: number | null) =>
  value ? new Date(value).toLocaleString() : "—";

const formatTaskResult = (value: number | null) => {
  if (value == null) return "—";
  if (value === 0) return "成功 (0)";
  return `0x${(value >>> 0).toString(16).toUpperCase()}`;
};

function ScheduledTasksPanel({
  data,
}: {
  data: ScheduledTaskInventory | null;
}) {
  const [query, setQuery] = useState("");
  const [hideMicrosoft, setHideMicrosoft] = useState(true);
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  const needle = query.trim().toLowerCase();
  const items = data.tasks.filter((task) => {
    if (hideMicrosoft && task.path.toLowerCase().startsWith("\\microsoft\\")) {
      return false;
    }
    if (!needle) return true;
    return [
      task.name,
      task.path,
      task.author,
      task.runAsUser,
      task.description,
      ...task.triggers,
    ].some((value) => value.toLowerCase().includes(needle));
  });
  const statusColor = (state: string) =>
    state === "Running"
      ? "teal"
      : state === "Disabled"
        ? "gray"
        : state === "Queued"
          ? "orange"
          : "blue";
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 2, xl: 4 }}>
        <Stat
          icon={IconCalendarTime}
          label="任务总数"
          value={String(data.total)}
          hint={data.source}
          color="indigo"
        />
        <Stat
          icon={IconShieldCheck}
          label="已启用"
          value={String(data.enabled)}
          hint="状态不是 Disabled"
          color="teal"
        />
        <Stat
          icon={IconPlayerPause}
          label="已禁用"
          value={String(data.disabled)}
          hint="不会自动触发"
          color="gray"
        />
        <Stat
          icon={IconRocket}
          label="登录/开机触发"
          value={String(data.logonOrBoot)}
          hint="含登录、开机启动的任务"
          color="orange"
        />
      </SimpleGrid>
      <Alert color="blue" icon={<IconShieldCheck />}>
        只读枚举 Windows 计划任务，不启用、禁用或删除任何任务。触发条件来自任务计划程序；
        隐藏任务也可能被其他程序调用。
      </Alert>
      <Group justify="space-between" align="flex-end" wrap="wrap">
        <TextInput
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          leftSection={<IconSearch size={17} />}
          placeholder="搜索任务、路径、作者、运行身份或触发条件"
          w={420}
        />
        <Switch
          checked={hideMicrosoft}
          onChange={(event) => setHideMicrosoft(event.currentTarget.checked)}
          label="隐藏 Microsoft 系统任务"
        />
      </Group>
      {items.length === 0 ? (
        <Alert color="gray">
          没有匹配的计划任务。
        </Alert>
      ) : (
        <Card className="glass-card" radius="lg" p={0}>
          <ScrollArea h="calc(100vh - 400px)">
            <Table stickyHeader highlightOnHover verticalSpacing="sm">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>任务</Table.Th>
                  <Table.Th>状态</Table.Th>
                  <Table.Th>触发条件</Table.Th>
                  <Table.Th>运行记录</Table.Th>
                  <Table.Th>运行身份</Table.Th>
                  <Table.Th>作者</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {items.map((task) => (
                  <Table.Tr key={`${task.path}${task.name}`}>
                    <Table.Td>
                      <Text fw={700} size="sm">
                        {task.name}
                        {task.hidden && (
                          <Badge size="xs" variant="light" color="gray" ml={6}>
                            隐藏
                          </Badge>
                        )}
                      </Text>
                      <Text size="xs" c="dimmed" truncate maw={380}>
                        {task.path}
                      </Text>
                      {task.description && (
                        <Text size="xs" c="dimmed" truncate maw={420}>
                          {task.description}
                        </Text>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <Badge color={statusColor(task.state)} variant="light">
                        {task.state}
                      </Badge>
                    </Table.Td>
                    <Table.Td>
                      <Group gap={4}>
                        {task.triggers.length === 0 ? (
                          <Text size="xs" c="dimmed">
                            无触发器
                          </Text>
                        ) : (
                          task.triggers.map((trigger) => (
                            <Badge key={trigger} size="xs" variant="outline">
                              {trigger}
                            </Badge>
                          ))
                        )}
                      </Group>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs">上次：{formatTaskTime(task.lastRunUnixMs)}</Text>
                      <Text size="xs" c="dimmed">
                        下次：{formatTaskTime(task.nextRunUnixMs)}
                      </Text>
                      <Text
                        size="xs"
                        c={
                          task.lastResult == null
                            ? "dimmed"
                            : task.lastResult === 0
                              ? "dimmed"
                              : "orange"
                        }
                      >
                        结果：{formatTaskResult(task.lastResult)}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs">{task.runAsUser || "未指定"}</Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs" c="dimmed" truncate maw={160}>
                        {task.author || "未知"}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </ScrollArea>
        </Card>
      )}
    </Stack>
  );
}

const riskLabel = (risk: string) =>
  risk === "read_only" ? "只读" : risk === "write" ? "写操作" : "系统级";

const riskColor = (risk: string) =>
  risk === "read_only" ? "teal" : risk === "write" ? "orange" : "red";

const slimCategoryLabel = (category: string) =>
  category === "safe"
    ? "可安全清理"
    : category === "caution"
      ? "需确认/管理员"
      : "系统托管";

const slimCategoryColor = (category: string) =>
  category === "safe" ? "teal" : category === "caution" ? "orange" : "gray";

function ToolboxPanel() {
  const [tools, setTools] = useState<ToolDescriptor[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [slim, setSlim] = useState<SlimScan | null>(null);
  const [slimDrawerOpen, setSlimDrawerOpen] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [openPathError, setOpenPathError] = useState<string | null>(null);
  const [generic, setGeneric] = useState<{
    tool: ToolDescriptor;
    data: ToolResult;
  } | null>(null);
  const [genericDrawerOpen, setGenericDrawerOpen] = useState(false);
  const [genericSuccess, setGenericSuccess] = useState("");
  const [runningTool, setRunningTool] = useState<string | null>(null);
  const [genericError, setGenericError] = useState<string | null>(null);
  const [applying, setApplying] = useState<string | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [selection, setSelection] = useState<string[]>([]);
  const [powerDraft, setPowerDraft] = useState<
    Record<string, { ac: number | null; dc: number | null }>
  >({});

  const updatePowerValue = (
    id: string,
    field: "ac" | "dc",
    value: number | string,
  ) => {
    const parsed = typeof value === "number" ? value : Number.parseFloat(value);
    if (!Number.isFinite(parsed)) return;
    setPowerDraft((current) => ({
      ...current,
      [id]: {
        ...(current[id] ?? { ac: null, dc: null }),
        [field]: Math.round(parsed),
      },
    }));
  };

  useEffect(() => {
    void listToolboxTools()
      .then(setTools)
      .catch((reason) => setError(String(reason)));
  }, []);

  const runGeneric = async (tool: ToolDescriptor) => {
    setRunningTool(tool.id);
    setGenericError(null);
    setApplyError(null);
    try {
      const data = (await runToolAction(tool.id, "preview")) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess(`${tool.name}预览生成完成`);
      setGenericDrawerOpen(true);
      if (tool.id === "clean_master") {
        setSelection(
          (data.rows ?? [])
            .filter(
              (row) =>
                row.status === "可用" &&
                Number(row.files) > 0 &&
                row.requiresAdmin !== true,
            )
            .map((row) => String(row.id)),
        );
      }
      if (tool.id === "pc_optimizer") {
        const settings = (data as ToolResult).powerSettings ?? [];
        setPowerDraft(
          Object.fromEntries(
            settings.map((setting) => [
              setting.id,
              { ac: setting.ac, dc: setting.dc },
            ]),
          ),
        );
      }
    } catch (reason) {
      setGenericError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRunningTool(null);
    }
  };

  const runApply = async (
    tool: ToolDescriptor,
    action: ToolActionDescriptor,
    parameters?: Record<string, unknown>,
  ) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新运行预览。");
      return;
    }
    if (!window.confirm(`确认执行「${action.label}」？该操作会立即生效。`))
      return;
    setApplying(`${tool.id}:${action.id}`);
    setApplyError(null);
    try {
      const data = (await runToolAction(
        tool.id,
        action.id,
        token,
        parameters,
      )) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess(`${action.label}执行完成`);
      setSelection([]);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const toggleToolStartup = async (
    tool: ToolDescriptor,
    item: NonNullable<ToolResult["startupItems"]>[number],
  ) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新运行预览。");
      return;
    }
    const actionLabel = item.enabled
      ? `停用「${item.name}」`
      : `启用「${item.name}」`;
    if (
      !window.confirm(
        `确认${actionLabel}？只影响当前用户启动项，下次登录/触发时生效。`,
      )
    )
      return;
    setApplying(`${tool.id}:toggle_startup`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "toggle_startup", token, {
        key: item.key,
        name: item.name,
        enable: !item.enabled,
      });
      setGenericSuccess(`${actionLabel}完成，正在刷新预览…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const scan = async () => {
    setScanning(true);
    setScanError(null);
    try {
      const result = (await runToolAction("cclean", "preview")) as SlimScan;
      setSlim(result);
      setSlimDrawerOpen(true);
    } catch (reason) {
      setScanError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setScanning(false);
    }
  };

  const openScanPath = async (path: string) => {
    setOpenPathError(null);
    try {
      await openPathInExplorer(path);
    } catch (reason) {
      setOpenPathError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  if (!tools)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );

  const available = tools.filter((tool) => tool.status !== "planned");
  const planned = tools.filter((tool) => tool.status === "planned");
  const usedBytes = slim
    ? Math.max(0, slim.driveTotalBytes - slim.driveFreeBytes)
    : 0;
  const powerSettings: PowerSettingOption[] = generic?.data.powerSettings ?? [];
  const powerChanges = powerSettings.flatMap((setting) => {
    const draft = powerDraft[setting.id];
    if (
      !draft ||
      setting.ac == null ||
      setting.dc == null ||
      draft.ac == null ||
      draft.dc == null
    ) {
      return [] as { id: string; ac: number; dc: number }[];
    }
    if (draft.ac === setting.ac && draft.dc === setting.dc) {
      return [] as { id: string; ac: number; dc: number }[];
    }
    return [{ id: setting.id, ac: draft.ac, dc: draft.dc }];
  });

  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        已接入 {available.length} 个工具。只读工具直接生成预览；涉及写操作时会先展示影响范围，
        经确认后执行并写入本地操作日志。
      </Alert>
      {error && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {error}
        </Alert>
      )}

      <Group justify="space-between" align="flex-end">
        <Box>
          <Title order={3}>已接入工具</Title>
          <Text size="xs" c="dimmed" mt={3}>
            这里只展示当前版本能够实际运行或生成预览的功能
          </Text>
        </Box>
        <Badge variant="light">{available.length} 个</Badge>
      </Group>
      <Box className="toolbox-grid">
        {available.map((tool) => (
          <Card key={tool.id} className="glass-card" radius="lg">
            <Group justify="space-between" align="flex-start">
              <Box>
                <Text fw={800}>{tool.name}</Text>
                <Text size="xs" c="dimmed" mt={3}>
                  {tool.category} ·{" "}
                  <Text component="span" c={riskColor(tool.risk)}>
                    {riskLabel(tool.risk)}
                  </Text>
                  {tool.requiresAdmin ? " · 需要管理员" : ""}
                </Text>
              </Box>
              <Badge
                color={tool.status === "available" ? "teal" : "orange"}
                variant="light"
              >
                {tool.status === "available" ? "可用" : "部分可用"}
              </Badge>
            </Group>
            <Text size="sm" mt="sm">
              {tool.description}
            </Text>
            <Text size="xs" c="dimmed" mt={4}>
              {tool.note}
            </Text>
            {tool.id === "cclean" && (
              <Button
                size="xs"
                mt="md"
                loading={scanning}
                onClick={() => void scan()}
              >
                扫描 C 盘
              </Button>
            )}
            {tool.id !== "cclean" && tool.uiKind === "table" && (
              <Button
                size="xs"
                mt="md"
                variant="light"
                loading={runningTool === tool.id}
                onClick={() => void runGeneric(tool)}
              >
                {tool.id === "clean_master" ? "扫描清理位置" : "运行预览"}
              </Button>
            )}
            {tool.id === "clean_master" && tool.uiKind === "table" && (
              <Text size="xs" c="dimmed" mt={4}>
                扫描后勾选位置，文件会移入回收站
              </Text>
            )}
          </Card>
        ))}
      </Box>

      {scanError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {scanError}
        </Alert>
      )}

      {openPathError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {openPathError}
        </Alert>
      )}

      {genericError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {genericError}
        </Alert>
      )}

      <Drawer
        opened={genericDrawerOpen}
        onClose={() => setGenericDrawerOpen(false)}
        position="right"
        size="min(1180px, 94vw)"
        title={generic ? `${generic.tool.name} · 运行结果` : "工具运行结果"}
        overlayProps={{ backgroundOpacity: 0.35, blur: 3 }}
      >
      {generic && (
        <Stack gap="md">
          <Alert color="teal" icon={<IconShieldCheck />}>
            {genericSuccess}
            {(generic.data.rows ?? []).length > 0
              ? `，共返回 ${generic.data.rows?.length} 条记录。`
              : "。"}
          </Alert>
          <Group gap="xs" mt="xs">
            {(generic.data.summary ?? []).map((entry) => (
              <Badge key={entry.label} variant="light">
                {entry.label} {entry.value}
              </Badge>
            ))}
          </Group>
          {(generic.data.rows ?? []).length === 0 ? (
            <Alert color="gray" mt="md">
              没有可显示的数据。
            </Alert>
          ) : (
            <ScrollArea type="auto">
            <Table miw={720} highlightOnHover verticalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  {(generic.data.columns ?? []).map((column) => (
                    <Table.Th key={column.key}>{column.label}</Table.Th>
                  ))}
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {(generic.data.rows ?? []).map((row, index) => (
                  <Table.Tr key={index}>
                    {(generic.data.columns ?? []).map((column) => (
                      <Table.Td key={column.key}>
                        {String(
                          (row as Record<string, unknown>)[column.key] ?? "",
                        )}
                      </Table.Td>
                    ))}
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
            </ScrollArea>
          )}
          {(generic.data.notes ?? []).map((note) => (
            <Text key={note} size="xs" c="dimmed" mt="xs">
              {note}
            </Text>
          ))}
          {(generic.tool.actions ?? []).some((action) => action.write) && (
            <Group gap="xs" mt="md">
              {generic.tool.id !== "pc_optimizer" &&
                generic.tool.id !== "clean_master" &&
                generic.tool.id !== "software_pm" &&
                (generic.tool.actions ?? [])
                  .filter((action) => action.write)
                  .map((action) => (
                    <Button
                      key={action.id}
                      size="xs"
                      color="orange"
                      variant="light"
                      loading={applying === `${generic.tool.id}:${action.id}`}
                      onClick={() => void runApply(generic.tool, action)}
                    >
                      {action.label}
                    </Button>
                  ))}
              {generic.tool.id === "pc_optimizer" &&
                (generic.data.rows ?? [])
                  .filter(
                    (row) =>
                      row.status !== "当前使用" && typeof row.guid === "string",
                  )
                  .map((row) => (
                    <Button
                      key={String(row.guid)}
                      size="xs"
                      variant="light"
                      loading={applying === `${generic.tool.id}:apply`}
                      onClick={() =>
                        void runApply(
                          generic.tool,
                          {
                            id: "apply",
                            label: `切换到 ${String(row.name)}`,
                            write: true,
                          },
                          { planGuid: row.guid },
                        )
                      }
                    >
                      切换到 {String(row.name)}
                    </Button>
                  ))}
            </Group>
          )}
          {generic.tool.id === "clean_master" &&
            (generic.data.rows ?? []).some(
              (row) => typeof row.id === "string",
            ) && (
              <Box mt="md">
                <Text size="sm" fw={600}>
                  勾选要移入回收站的位置
                </Text>
                <Stack gap={6} mt="xs">
                  {(generic.data.rows ?? []).map((row) => {
                    const id = String(row.id ?? "");
                    const disabled =
                      row.status === "不存在" || Number(row.files) === 0;
                    return (
                      <Checkbox
                        key={id}
                        checked={selection.includes(id)}
                        disabled={disabled}
                        onChange={(event) =>
                          setSelection((current) =>
                            event.currentTarget.checked
                              ? [...current, id]
                              : current.filter((item) => item !== id),
                          )
                        }
                        label={`${row.label} · ${row.size} · ${row.files} 个文件${
                          Number(row.locked) > 0
                            ? ` · ${row.locked} 个占用/只读`
                            : ""
                        }${row.truncated ? " · 已截断" : ""}${
                          row.requiresAdmin ? " · 需要管理员" : ""
                        }${disabled ? ` · ${row.status}` : ""}`}
                      />
                    );
                  })}
                </Stack>
                <Group mt="sm" align="center">
                  <Button
                    size="xs"
                    color="orange"
                    variant="light"
                    disabled={selection.length === 0}
                    loading={applying === `${generic.tool.id}:apply`}
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply",
                          label: "清理所选位置",
                          write: true,
                        },
                        { candidateIds: selection },
                      )
                    }
                  >
                    清理所选（移入回收站）
                  </Button>
                  <Text size="xs" c="dimmed">
                    已选 {selection.length} 个位置；清理后可到回收站还原
                  </Text>
                </Group>
              </Box>
            )}
          {generic.tool.id === "software_pm" &&
            (generic.data.startupItems?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  当前用户启动项（仅 Run / RunOnce）
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  停用项会保存到 HKCU\Software\SystemLens\DisabledStartup，可随时还原
                </Text>
                <ScrollArea type="auto" mt="xs">
                  <Table miw={760} verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>启动项</Table.Th>
                        <Table.Th>状态</Table.Th>
                        <Table.Th>命令</Table.Th>
                        <Table.Th>操作</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {(generic.data.startupItems ?? []).map((item) => (
                        <Table.Tr key={`${item.key}::${item.name}`}>
                          <Table.Td>
                            <Text size="sm" fw={600}>
                              {item.name}
                            </Text>
                            <Text size="xs" c="dimmed">
                              {item.location}
                            </Text>
                          </Table.Td>
                          <Table.Td>
                            <Badge
                              size="xs"
                              variant="light"
                              color={item.enabled ? "teal" : "gray"}
                            >
                              {item.enabled ? "已启用" : "已停用"}
                            </Badge>
                          </Table.Td>
                          <Table.Td>
                            <Text size="xs" c="dimmed" truncate maw={340}>
                              {item.command}
                            </Text>
                          </Table.Td>
                          <Table.Td>
                            <Button
                              size="xs"
                              variant="light"
                              color={item.enabled ? "orange" : "teal"}
                              loading={
                                applying ===
                                `${generic.tool.id}:toggle_startup`
                              }
                              onClick={() =>
                                void toggleToolStartup(generic.tool, item)
                              }
                            >
                              {item.enabled ? "停用" : "启用（还原）"}
                            </Button>
                          </Table.Td>
                        </Table.Tr>
                      ))}
                    </Table.Tbody>
                  </Table>
                </ScrollArea>
                {generic.data.startupRegistryError && (
                  <Alert color="orange" mt="sm" icon={<IconAlertCircle />}>
                    {generic.data.startupRegistryError}
                  </Alert>
                )}
              </Box>
            )}
          {generic.tool.id === "pc_optimizer" && powerSettings.length > 0 && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                逐项电源设置（当前计划 · 交流 / 电池）
              </Text>
              <ScrollArea type="auto" mt="xs">
                <Table miw={760} verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>设置项</Table.Th>
                      <Table.Th>交流电</Table.Th>
                      <Table.Th>电池</Table.Th>
                      <Table.Th>范围</Table.Th>
                      <Table.Th>说明</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {powerSettings.map((setting) => {
                      const draft = powerDraft[setting.id] ?? {
                        ac: setting.ac,
                        dc: setting.dc,
                      };
                      const unavailable =
                        setting.ac == null || setting.dc == null;
                      return (
                        <Table.Tr key={setting.id}>
                          <Table.Td>
                            <Text size="sm" fw={600}>
                              {setting.label}
                            </Text>
                            {unavailable && (
                              <Text size="xs" c="orange">
                                当前计划不可读
                              </Text>
                            )}
                          </Table.Td>
                          <Table.Td>
                            <NumberInput
                              size="xs"
                              w={110}
                              min={setting.min}
                              max={setting.max}
                              disabled={unavailable}
                              value={draft.ac ?? ""}
                              onChange={(value) =>
                                updatePowerValue(setting.id, "ac", value)
                              }
                            />
                          </Table.Td>
                          <Table.Td>
                            <NumberInput
                              size="xs"
                              w={110}
                              min={setting.min}
                              max={setting.max}
                              disabled={unavailable}
                              value={draft.dc ?? ""}
                              onChange={(value) =>
                                updatePowerValue(setting.id, "dc", value)
                              }
                            />
                          </Table.Td>
                          <Table.Td>
                            <Text size="xs" c="dimmed">
                              {setting.min}–{setting.max} {setting.unit}
                            </Text>
                          </Table.Td>
                          <Table.Td>
                            <Text size="xs" c="dimmed">
                              {setting.note}
                            </Text>
                          </Table.Td>
                        </Table.Tr>
                      );
                    })}
                  </Table.Tbody>
                </Table>
              </ScrollArea>
              <Group mt="sm" align="center">
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  disabled={powerChanges.length === 0}
                  loading={applying === `${generic.tool.id}:apply_settings`}
                  onClick={() =>
                    void runApply(
                      generic.tool,
                      {
                        id: "apply_settings",
                        label: "应用电源设置",
                        write: true,
                      },
                      { changes: powerChanges },
                    )
                  }
                >
                  应用电源设置
                </Button>
                <Text size="xs" c="dimmed">
                  {powerChanges.length > 0
                    ? `将提交 ${powerChanges.length} 项修改；原值写入本地操作日志`
                    : "修改数值后可应用；0 表示永不"}
                </Text>
              </Group>
            </Box>
          )}
          {applyError && (
            <Alert color="red" mt="sm" icon={<IconAlertCircle />}>
              {applyError}
            </Alert>
          )}
        </Stack>
      )}
      </Drawer>

      <Drawer
        opened={slimDrawerOpen}
        onClose={() => setSlimDrawerOpen(false)}
        position="right"
        size="min(1180px, 94vw)"
        title="C 盘瘦身扫描结果"
        overlayProps={{ backgroundOpacity: 0.35, blur: 3 }}
      >
        {slim && (
        <Stack gap="md">
          <Alert color="teal" icon={<IconShieldCheck />}>
            扫描完成：发现 {slim.items.length} 个位置、{slim.fileCount} 个文件，预计可安全清理 {bytes(slim.safeBytes)}。
          </Alert>
          <Group justify="space-between" align="flex-start" wrap="wrap">
            <Box>
              <Text fw={800}>C 盘瘦身预览</Text>
              <Text size="xs" c="dimmed">
                {slim.driveLetter} 已用 {bytes(usedBytes)} / {bytes(slim.driveTotalBytes)} · 可用{" "}
                {bytes(slim.driveFreeBytes)}
              </Text>
            </Box>
            <Group gap="xs">
              <Badge color="teal" variant="light">
                可安全清理 {bytes(slim.safeBytes)}
              </Badge>
              <Badge color="orange" variant="light">
                需确认 {bytes(slim.cautionBytes)}
              </Badge>
              <Badge color="gray" variant="light">
                系统托管 {bytes(slim.systemBytes)}
              </Badge>
            </Group>
          </Group>
          <Progress
            mt="md"
            value={
              slim.driveTotalBytes
                ? (usedBytes / slim.driveTotalBytes) * 100
                : 0
            }
          />
          <Text size="xs" c="dimmed" mt="sm">
            {slim.items.length} 个位置 · {slim.fileCount} 个文件
          </Text>
          <ScrollArea type="auto">
          <Table miw={900} highlightOnHover verticalSpacing="xs">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>位置</Table.Th>
                <Table.Th>占用</Table.Th>
                <Table.Th>类别</Table.Th>
                <Table.Th>权限</Table.Th>
                <Table.Th>说明</Table.Th>
                <Table.Th w={112}>操作</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {slim.items.map((item) => (
                <Table.Tr key={item.id}>
                  <Table.Td>
                    <Text size="sm" fw={600}>
                      {item.label}
                    </Text>
                    <Text size="xs" c="dimmed" truncate maw={360}>
                      {item.path}
                    </Text>
                  </Table.Td>
                  <Table.Td>{bytes(item.bytes)}</Table.Td>
                  <Table.Td>
                    <Badge
                      size="xs"
                      color={slimCategoryColor(item.category)}
                      variant="light"
                    >
                      {slimCategoryLabel(item.category)}
                    </Badge>
                  </Table.Td>
                  <Table.Td>
                    <Text size="xs">
                      {item.requiresAdmin ? "需要管理员" : "普通权限"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="xs" c="dimmed" maw={320}>
                      {item.note}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Button
                      size="compact-xs"
                      variant="light"
                      leftSection={<IconFolder size={14} />}
                      onClick={() => void openScanPath(item.path)}
                    >
                      打开目录
                    </Button>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
          </ScrollArea>
          <Text size="xs" c="dimmed" mt="md">
            {slim.note}
          </Text>
        </Stack>
        )}
      </Drawer>

      <Group justify="space-between" align="flex-end" mt="md">
        <Box>
          <Title order={3}>规划中</Title>
          <Text size="xs" c="dimmed" mt={3}>
            已登记但当前版本尚不可运行的工具
          </Text>
        </Box>
        <Badge variant="light" color="gray">
          {planned.length} 个
        </Badge>
      </Group>
      <Card className="glass-card" radius="lg" p={0}>
        <ScrollArea h={320}>
          <Table stickyHeader highlightOnHover verticalSpacing="xs">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>工具</Table.Th>
                <Table.Th>分类</Table.Th>
                <Table.Th>风险</Table.Th>
                <Table.Th>说明</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {planned.map((tool) => (
                <Table.Tr key={tool.id}>
                  <Table.Td>
                    <Text size="sm" fw={600}>{tool.name}</Text>
                    <Text size="xs" c="dimmed">{tool.id}</Text>
                  </Table.Td>
                  <Table.Td>{tool.category}</Table.Td>
                  <Table.Td>
                    <Badge size="xs" color={riskColor(tool.risk)} variant="light">
                      {riskLabel(tool.risk)}
                    </Badge>
                  </Table.Td>
                  <Table.Td>
                    <Text size="xs" c="dimmed">
                      {tool.description} · {tool.note}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>

    </Stack>
  );
}

export default function App() {
  const { colorScheme, setColorScheme } = useMantineColorScheme();
  const [tab, setTab] = useState<Tab>("overview");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [systemHistory, setSystemHistory] = useState<SystemHistorySeries | null>(
    null,
  );
  const [historyRange, setHistoryRange] = useState("1");
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [hardware, setHardware] = useState<HardwareDetails | null>(null);
  const [sensors, setSensors] = useState<SensorSnapshot | null>(null);
  const [deviceDetails, setDeviceDetails] = useState<DeviceDetails | null>(null);
  const [drivers, setDrivers] = useState<DriverInventory | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [networkDiagnostics, setNetworkDiagnostics] =
    useState<NetworkDiagnosticReport | null>(null);
  const [software, setSoftware] = useState<SoftwareInventory | null>(null);
  const [startup, setStartup] = useState<StartupItem[]>([]);
  const [scheduledTasks, setScheduledTasks] =
    useState<ScheduledTaskInventory | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await getSystemSnapshot();
      setSnapshot(next);
      setError(null);
      const diskRead = next.disks.reduce(
        (sum, x) => sum + x.readBytesPerSec,
        0,
      );
      const diskWrite = next.disks.reduce(
        (sum, x) => sum + x.writtenBytesPerSec,
        0,
      );
      setHistory((old) =>
        [
          ...old,
          {
            time: new Date().toLocaleTimeString([], {
              minute: "2-digit",
              second: "2-digit",
            }),
            cpu: +next.cpu.usagePercent.toFixed(1),
            memory: +next.memory.usagePercent.toFixed(1),
            download: +(next.network.downloadBytesPerSec / 1024 ** 2).toFixed(
              2,
            ),
            upload: +(next.network.uploadBytesPerSec / 1024 ** 2).toFixed(2),
            diskRead,
            diskWrite,
          },
        ].slice(-30),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [refresh]);
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      void getSystemHistory(Number(historyRange))
        .then((series) => {
          if (cancelled) return;
          setSystemHistory(series);
          setHistoryError(null);
        })
        .catch((reason) => {
          if (!cancelled) setHistoryError(String(reason));
        });
    };
    load();
    const timer = window.setInterval(load, 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [historyRange]);
  useEffect(() => {
    if (tab === "hardware" && !hardware)
      void getHardwareDetails()
        .then(setHardware)
        .catch((e) => setError(String(e)));
  }, [tab, hardware]);
  useEffect(() => {
    if (tab === "devices" && !deviceDetails)
      void getDeviceDetails()
        .then(setDeviceDetails)
        .catch((e) => setError(String(e)));
  }, [tab, deviceDetails]);
  useEffect(() => {
    // 传感器采样与页面无关：即使停留在概览页，告警状态和托盘摘要也要持续更新。
    let cancelled = false;
    const load = () =>
      void getSensorSnapshot()
        .then((next) => {
          if (!cancelled) setSensors(next);
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
    load();
    const timer = window.setInterval(load, 10_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    // 桌面通知由后端做冷却与免打扰判断，这里只负责把活动告警交给后端。
    for (const alert of sensors?.alerts ?? []) {
      void notifySensorAlert({
        alertId: alert.id,
        level: alert.level,
        title:
          alert.level === "critical"
            ? `小鲁 · 危险：${alert.sensorName}`
            : `小鲁 · 温度警告：${alert.sensorName}`,
        message: alert.message,
      }).catch(() => undefined);
    }
  }, [sensors]);
  useEffect(() => {
    if (tab === "drivers" && !drivers)
      void getDriverInventory()
        .then(setDrivers)
        .catch((e) => setError(String(e)));
  }, [tab, drivers]);
  useEffect(() => {
    if (tab === "diagnostics" && !diagnostics)
      void getDiagnosticReport()
        .then(setDiagnostics)
        .catch((e) => setError(String(e)));
  }, [tab, diagnostics]);
  useEffect(() => {
    if (tab === "network" && !networkDiagnostics)
      void getNetworkDiagnostics()
        .then(setNetworkDiagnostics)
        .catch((e) => setError(String(e)));
  }, [tab, networkDiagnostics]);
  useEffect(() => {
    if (tab === "software" && !software)
      void getSoftwareInventory()
        .then(setSoftware)
        .catch((e) => setError(String(e)));
  }, [tab, software]);
  const loadStartup = useCallback(
    () =>
      getStartupItems()
        .then(setStartup)
        .catch((e) => setError(String(e))),
    [],
  );
  useEffect(() => {
    if (tab === "startup") void loadStartup();
  }, [tab, loadStartup]);
  useEffect(() => {
    if (tab === "tasks" && !scheduledTasks)
      void getScheduledTasks()
        .then(setScheduledTasks)
        .catch((e) => setError(String(e)));
  }, [tab, scheduledTasks]);

  const toggleStartup = async (item: StartupItem, enabled: boolean) => {
    if (
      !window.confirm(
        `确定要${enabled ? "启用" : "禁用"}启动项“${item.name}”吗？`,
      )
    )
      return;
    try {
      await setStartupEnabled(item.name, enabled);
      await loadStartup();
    } catch (e) {
      setError(String(e));
    }
  };

  if (!snapshot)
    return (
      <div
        className="boot-splash"
        role="status"
        aria-live="polite"
        aria-label="小鲁系统透镜正在启动"
      >
        <div className="boot-splash__inner">
          <div className="boot-logo-shell">
            <img
              className="boot-logo"
              src={logoUrl}
              alt="小鲁 System Lens Logo"
              width="104"
              height="104"
            />
          </div>
          <div className="boot-title">小鲁</div>
          <div className="boot-subtitle">SYSTEM LENS</div>
          <div className="boot-progress" aria-hidden="true">
            <span />
          </div>
          <div className="boot-status">正在建立本地系统视图…</div>
          {error && (
            <Alert color="red" mt="md" style={{ maxWidth: 520 }}>
              {error}
            </Alert>
          )}
        </div>
      </div>
    );
  const diskRead = snapshot.disks.reduce(
    (sum, x) => sum + x.readBytesPerSec,
    0,
  );
  const diskWrite = snapshot.disks.reduce(
    (sum, x) => sum + x.writtenBytesPerSec,
    0,
  );
  const historySamples: Sample[] = (systemHistory?.points ?? []).map((point) => ({
    time: new Date(point.t).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    }),
    cpu: +point.cpuPercent.toFixed(1),
    memory: +point.memoryPercent.toFixed(1),
    download: +(point.downloadBytesPerSec / 1024 ** 2).toFixed(2),
    upload: +(point.uploadBytesPerSec / 1024 ** 2).toFixed(2),
    diskRead: point.diskReadBytesPerSec,
    diskWrite: point.diskWriteBytesPerSec,
  }));

  return (
    <div className="shell">
      <aside className="side">
        <Group gap="sm" className="logo">
          <img
            className="brand-logo"
            src={logoUrl}
            alt="小鲁 System Lens Logo"
          />
          <Box>
            <Text fw={800}>小鲁</Text>
            <Text size="xs" c="dimmed">
              SYSTEM LENS
            </Text>
          </Box>
        </Group>
        <Stack gap={5} mt="xl">
          {nav.map(([id, label, Icon]) => (
            <NavLink
              key={id}
              active={tab === id}
              label={label}
              leftSection={<Icon size={19} />}
              onClick={() => setTab(id)}
            />
          ))}
        </Stack>
        <Paper className="privacy" radius="lg" p="md">
          <Group gap="xs">
            <IconShieldCheck size={18} />
            <Text fw={700} size="sm">
              本地优先
            </Text>
          </Group>
          <Text size="xs" c="dimmed" mt="xs">
            数据仅在本机采集和处理。
          </Text>
        </Paper>
        <Text size="xs" c="dimmed">
          v0.38.0 · 18 个工具 · 逐项电源设置
        </Text>
      </aside>
      <main className="content">
        <Group justify="space-between" mb="xl">
          <Box>
            <Text size="sm" c="dimmed">
              {snapshot.hostName} · {snapshot.osName}
            </Text>
            <Title order={1}>{nav.find((x) => x[0] === tab)?.[1]}</Title>
          </Box>
          <Group>
            {!isDesktopRuntime() && <Badge variant="light">浏览器演示</Badge>}
            <Tooltip
              label={
                colorScheme === "dark" ? "切换到浅色模式" : "切换到深色模式"
              }
            >
              <ActionIcon
                size="lg"
                variant="light"
                aria-label={
                  colorScheme === "dark" ? "切换到浅色模式" : "切换到深色模式"
                }
                onClick={() =>
                  setColorScheme(colorScheme === "dark" ? "light" : "dark")
                }
              >
                {colorScheme === "dark" ? (
                  <IconSun size={19} />
                ) : (
                  <IconMoon size={19} />
                )}
              </ActionIcon>
            </Tooltip>
            <Tooltip label="立即刷新">
              <ActionIcon
                size="lg"
                variant="light"
                onClick={() => void refresh()}
                loading={busy}
              >
                <IconRefresh size={19} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Group>
        {error && (
          <Alert
            icon={<IconAlertCircle />}
            color="red"
            withCloseButton
            onClose={() => setError(null)}
            mb="md"
          >
            {error}
          </Alert>
        )}
        {tab === "overview" && (
          <Stack gap="md">
            <SimpleGrid cols={{ base: 2, xl: 4 }}>
              <Stat
                icon={IconCpu}
                label="CPU 使用率"
                value={`${snapshot.cpu.usagePercent.toFixed(1)}%`}
                hint={`${snapshot.cpu.logicalCores} 线程 · ${snapshot.cpu.frequencyMhz} MHz`}
                color="indigo"
              />
              <Stat
                icon={IconServer}
                label="内存使用率"
                value={`${snapshot.memory.usagePercent.toFixed(1)}%`}
                hint={`${bytes(snapshot.memory.usedBytes)} / ${bytes(snapshot.memory.totalBytes)}`}
                color="grape"
              />
              <Stat
                icon={IconArrowDown}
                label="实时下载"
                value={speed(snapshot.network.downloadBytesPerSec)}
                hint={`累计 ${bytes(snapshot.network.totalReceivedBytes)}`}
                color="teal"
              />
              <Stat
                icon={IconArrowUp}
                label="实时上传"
                value={speed(snapshot.network.uploadBytesPerSec)}
                hint={`累计 ${bytes(snapshot.network.totalTransmittedBytes)}`}
                color="yellow"
              />
            </SimpleGrid>
            <SimpleGrid cols={{ base: 1, lg: 2 }}>
              <Card className="glass-card" radius="lg">
                <Group justify="space-between" mb="sm">
                  <Box>
                    <Text fw={700}>系统负载</Text>
                    <Text size="xs" c="dimmed">
                      最近 60 秒
                    </Text>
                  </Box>
                  <Badge variant="dot">实时</Badge>
                </Group>
                <PerformanceChart data={history} mode="system" />
              </Card>
              <Card className="glass-card" radius="lg">
                <Group justify="space-between" mb="sm">
                  <Box>
                    <Text fw={700}>网络吞吐</Text>
                    <Text size="xs" c="dimmed">
                      MB/s · {snapshot.network.interfaces} 个接口
                    </Text>
                  </Box>
                  <IconNetwork size={21} />
                </Group>
                <PerformanceChart data={history} mode="network" />
              </Card>
            </SimpleGrid>
            <Card className="glass-card" radius="lg">
              <Group justify="space-between" mb="md">
                <Box>
                  <Text fw={700}>历史趋势</Text>
                  <Text size="xs" c="dimmed">
                    {systemHistory
                      ? `${systemHistory.sampleCount} 个样本 · ${systemHistory.points.length} 个数据点`
                      : "加载中…"}
                  </Text>
                </Box>
                <SegmentedControl
                  size="xs"
                  value={historyRange}
                  onChange={setHistoryRange}
                  data={[
                    { label: "1 小时", value: "1" },
                    { label: "6 小时", value: "6" },
                    { label: "24 小时", value: "24" },
                  ]}
                />
              </Group>
              {historyError && (
                <Alert color="red" icon={<IconAlertCircle />} mb="md">
                  {historyError}
                </Alert>
              )}
              {historySamples.length === 0 ? (
                <Alert color="gray">
                  还没有足够的指标历史；应用运行一段时间后会自动累积。
                </Alert>
              ) : (
                <SimpleGrid cols={{ base: 1, lg: 2 }}>
                  <Box>
                    <Text size="sm" fw={600} mb={4}>
                      CPU / 内存
                    </Text>
                    <PerformanceChart data={historySamples} mode="system" />
                  </Box>
                  <Box>
                    <Text size="sm" fw={600} mb={4}>
                      网络吞吐
                    </Text>
                    <PerformanceChart data={historySamples} mode="network" />
                  </Box>
                </SimpleGrid>
              )}
            </Card>
            <Card className="glass-card" radius="lg">
              <Group justify="space-between" mb="md">
                <Box>
                  <Text fw={700}>磁盘活动</Text>
                  <Text size="xs" c="dimmed">
                    读取 {speed(diskRead)} · 写入 {speed(diskWrite)}
                  </Text>
                </Box>
                <Badge color="gray">
                  {snapshot.disks.length} 个分区
                </Badge>
              </Group>
              {snapshot.disks.map((d) => {
                const used = d.totalBytes - d.availableBytes;
                const mountPoint = d.mountPoint.replace(/[\\/]$/, "");
                const volumeName = d.name.trim();
                const nameIsMountPoint =
                  volumeName.replace(/[\\/]$/, "").toLocaleLowerCase() ===
                  mountPoint.toLocaleLowerCase();
                const diskLabel = volumeName && !nameIsMountPoint
                  ? `${volumeName} (${mountPoint || "无盘符"})`
                  : mountPoint
                    ? `本地磁盘 (${mountPoint})`
                    : `${volumeName || "未命名卷"} (无盘符)`;
                return (
                  <Box key={d.mountPoint} mb="md">
                    <Group justify="space-between">
                      <Text size="sm" fw={600}>
                        {diskLabel} · {d.fileSystem || "未知文件系统"}
                      </Text>
                      <Text size="xs" c="dimmed">
                        {bytes(used)} / {bytes(d.totalBytes)}
                      </Text>
                    </Group>
                    <Progress
                      mt={8}
                      value={d.totalBytes ? (used / d.totalBytes) * 100 : 0}
                    />
                  </Box>
                );
              })}
            </Card>
          </Stack>
        )}
        {tab === "benchmark" && <BenchmarkPanel />}
        {tab === "hardware" && <HardwarePanel details={hardware} />}
        {tab === "devices" && <DevicePanel data={deviceDetails} />}
        {tab === "sensors" && <SensorsPanel data={sensors} />}
        {tab === "drivers" && <DriversPanel data={drivers} />}
        {tab === "diagnostics" && <DiagnosticsPanel data={diagnostics} />}
        {tab === "network" && <NetworkPanel data={networkDiagnostics} />}
        {tab === "software" && <SoftwarePanel data={software} />}
        {tab === "storage" && <StoragePanel />}
        {tab === "processes" && <ProcessPanel processes={snapshot.processes} />}
        {tab === "startup" && (
          <Stack>
            <Alert color="blue" icon={<IconPlayerPause />}>
              当前用户启动项可以启停；系统级启动项只读，避免未经授权修改全局配置。
            </Alert>
            {startup.map((item) => (
              <Card
                key={`${item.scope}-${item.name}-${item.enabled}`}
                className="glass-card"
                radius="lg"
              >
                <Group justify="space-between" wrap="nowrap">
                  <Box className="startup-command">
                    <Group gap="xs">
                      <Text fw={700}>{item.name}</Text>
                      <Badge size="xs" variant="light">
                        {item.scope}
                      </Badge>
                    </Group>
                    <Text size="xs" c="dimmed" truncate>
                      {item.command}
                    </Text>
                  </Box>
                  <Switch
                    checked={item.enabled}
                    disabled={!item.editable}
                    onChange={(e) =>
                      void toggleStartup(item, e.currentTarget.checked)
                    }
                    label={item.enabled ? "已启用" : "已禁用"}
                  />
                </Group>
              </Card>
            ))}
          </Stack>
        )}
        {tab === "tasks" && <ScheduledTasksPanel data={scheduledTasks} />}
        {tab === "toolbox" && <ToolboxPanel />}
        {tab === "report" && <ReportPanel />}
      </main>
    </div>
  );
}
