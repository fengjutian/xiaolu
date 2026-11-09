import { lazy, memo, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  Radio,
  ScrollArea,
  SegmentedControl,
  Slider,
  Skeleton,
  Select,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Tabs,
  Text,
  TextInput,
  Textarea,
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
  IconEye,
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
  IconSettings,
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
  IconSparkles,
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
  getBenchmarkBaselines,
  benchmarkPreflight,
  cancelBenchmark,
  cancelGpuBenchmark,
  getDriverInventory,
  getDiagnosticReport,
  getDeviceDetails,
  getDesktopOverview,
  openDesktopItem,
  getAppSettings,
  saveAppSettings,
  getAppPaths,
  clearSystemHistory,
  getEyeCareSettings,
  saveEyeCareSettings,
  setEyeCareFilter,
  sendBreakReminder,
  getNotificationSettings,
  getStorageDrives,
  getHardwareDetails,
  getNetworkDiagnostics,
  getNvmeHealth,
  getSensorHistory,
  listToolboxTools,
  notifySensorAlert,
  openPathInExplorer,
  openRecycleBin,
  runToolAction,
  listenToolProgress,
  getToolboxStats,
  cancelToolAction,
  newToolJobId,
  sendTestNotification,
  saveNotificationSettings,
  getSensorSettings,
  getSensorSnapshot,
  getScheduledTasks,
  saveSensorSettings,
  getSoftwareInventory,
  listStartMenuApps,
  launchStartMenuEntry,
  launchSoftwareUninstall,
  getStartupItems,
  getSystemSnapshot,
  getProcessInventory,
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
  DesktopItem,
  DesktopOverview,
  DeviceDetails,
  DirectoryListing,
  DuplicateScan,
  EventLogResult,
  AppSettings,
  AppPaths,
  EyeCareSettings,
  HardwareDetails,
  HealthLevel,
  LargeFileScan,
  LaunchEntry,
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
  PowerPresetOption,
  CleanupDiffRow,
  WinoptSetting,
  WinoptAdminSetting,
  PrivacySetting,
  ProcessItem,
  AppxItem,
  RuntimeItem,
  ExpandItem,
  ScheduledTaskItem,
  ToolActionDescriptor,
  ToolDescriptor,
  ToolResult,
  BenchmarkBaselineReport,
  ToolboxStatsReport,
} from "./types";
import "./styles.css";
import { Stat } from "./Stat";

// 诊断 / AI / 报告页面体积大且只在切换到对应标签时才需要，拆成独立 chunk，
// 避免它们的代码进入首屏关键路径。
const DiagnosticsPanel = lazy(() => import("./DiagnosticsPanel"));
const AiPanel = lazy(() => import("./AiPanel"));
const ReportPanel = lazy(() => import("./report"));

const logoUrl = "/logo.png";

type Tab =
  | "overview"
  | "benchmark"
  | "sensors"
  | "ai"
  | "eye-care"
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
  | "settings"
  | "desktop"
  | "launcher"
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

// 应用内页面（护眼模式 / 快捷启动 / 桌面助手）不单独占用侧栏入口，
// 统一从「工具箱」里对应的工具卡片进入，避免同一页面出现两个导航项。
const nav = [
  ["overview", "性能概览", IconGauge],
  ["benchmark", "性能跑分", IconActivity],
  ["sensors", "温度与健康", IconTemperature],
  ["ai", "AI 解读", IconSparkles],
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
  ["settings", "设置", IconSettings],
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
  const [uninstallStatus, setUninstallStatus] = useState<string | null>(null);
  const [uninstallError, setUninstallError] = useState<string | null>(null);
  const [uninstallingKey, setUninstallingKey] = useState<string | null>(null);

  const uninstall = async (item: SoftwareInventory["items"][number]) => {
    const command = item.uninstallString || item.quietUninstallString;
    if (
      !window.confirm(
        `确认启动“${item.name}”的系统卸载程序？\n\n${command}\n\n卸载不可回滚，建议先创建还原点。`,
      )
    )
      return;
    setUninstallingKey(item.key);
    setUninstallStatus(null);
    setUninstallError(null);
    try {
      const message = await launchSoftwareUninstall(item.key);
      const detail = message.trim().replace(/[。.]$/, "");
      setUninstallStatus(
        `${item.name}：${detail}。请在系统弹出的卸载窗口中继续，本页不会跟踪卸载进度。`,
      );
    } catch (error) {
      setUninstallError(
        `${item.name}：${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setUninstallingKey(null);
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
      {uninstallError && (
        <Alert color="red" icon={<IconAlertCircle />} title="卸载程序没有启动">
          {uninstallError}
        </Alert>
      )}
      {uninstallStatus && (
        <Alert color="teal" icon={<IconShieldCheck />} title="已启动系统卸载程序">
          {uninstallStatus}
        </Alert>
      )}
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
                  <Table.Td>
                    <Group gap="xs" wrap="nowrap">
                      {item.residue && (
                        <Badge size="xs" color="orange" variant="light">
                          目录不存在
                        </Badge>
                      )}
                      <Button
                        size="xs"
                        variant="light"
                        loading={uninstallingKey === item.key}
                        disabled={
                          !item.uninstallAvailable ||
                          !item.key ||
                          (uninstallingKey != null &&
                            uninstallingKey !== item.key)
                        }
                        onClick={() => void uninstall(item)}
                      >
                        卸载
                      </Button>
                    </Group>
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
  const [benchmarkTab, setBenchmarkTab] = useState<string | null>("tests");
  const [running, setRunning] = useState<BenchmarkResult["kind"] | null>(null);
  const [results, setResults] = useState<Record<string, BenchmarkResult>>({});
  const [benchmarkHistory, setBenchmarkHistory] = useState<BenchmarkRecord[]>(
    [],
  );
  const [failure, setFailure] = useState<string | null>(null);
  const [diskSize, setDiskSize] = useState("1024");
  const [cancelling, setCancelling] = useState(false);
  const [cancelError, setCancelError] = useState<string | null>(null);
  const [baselineReport, setBaselineReport] =
    useState<BenchmarkBaselineReport | null>(null);
  const [baselineError, setBaselineError] = useState<string | null>(null);
  // 基线与可信度由 Rust 侧按本机历史计算，前端只负责展示与对比。
  const refreshBaselines = useCallback(
    async (current: Record<string, BenchmarkResult>) => {
      try {
        const report = await getBenchmarkBaselines(Object.values(current));
        setBaselineReport(report);
        setBaselineError(null);
      } catch (error) {
        setBaselineError(error instanceof Error ? error.message : String(error));
      }
    },
    [],
  );
  useEffect(() => {
    void getBenchmarkHistory()
      .then(setBenchmarkHistory)
      .catch((error) => setFailure(String(error)));
    void refreshBaselines({});
  }, [refreshBaselines]);
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
      const merged = { ...results, [kind]: result };
      setResults(merged);
      setBenchmarkHistory(await getBenchmarkHistory());
      await refreshBaselines(merged);
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
  const visibleBenchmarkKinds = benchmarkKinds;
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconGauge />}>
        跑分会在短时间内提高 CPU、内存、磁盘或 GPU
        负载。综合分采用加权几何平均：单核 20%、多核 30%、内存 15%、磁盘
        15%、GPU 20%。
      </Alert>
      <Tabs value={benchmarkTab} onChange={setBenchmarkTab} keepMounted={false}>
        <Tabs.List className="module-tabs-list">
          <Tabs.Tab value="tests" leftSection={<IconCpu size={16} />}>
            性能测试
          </Tabs.Tab>
          <Tabs.Tab value="history">历史记录</Tabs.Tab>
        </Tabs.List>
      </Tabs>
      {benchmarkTab === "tests" && (
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
          <Box>
            <Button
              size="xs"
              color="red"
              variant="light"
              loading={cancelling}
              onClick={() => {
                setCancelling(true);
                setCancelError(null);
                cancelGpuBenchmark();
                void cancelBenchmark()
                  .catch((error) =>
                    setCancelError(
                      error instanceof Error ? error.message : String(error),
                    ),
                  )
                  .finally(() => setCancelling(false));
              }}
            >
              取消测试
            </Button>
            {cancelError && (
              <Text size="xs" c="red" mt={4}>
                取消失败：{cancelError}
              </Text>
            )}
          </Box>
        )}
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
      {baselineError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          基线读取失败：{baselineError}
        </Alert>
      )}
      {baselineReport && (
        <Card className="glass-card" radius="lg" p={0}>
          <Box p="md" pb="xs">
            <Text size="xs" c="dimmed" fw={800}>
              本机基线与可信度
            </Text>
            <Text size="sm" c="dimmed">
              只统计本机同项目、同工作负载版本、同应用版本的历史记录；样本够了才会给可信度。
            </Text>
          </Box>
          {baselineReport.baselines.length === 0 ? (
            <Box p="md" pt={0}>
              <Alert color="gray">
                还没有可用于基线的历史记录。同一项目跑满 3 次后，这里会给出基线与可信度。
              </Alert>
            </Box>
          ) : (
            <ScrollArea type="auto">
              <Table miw={720} highlightOnHover verticalSpacing="xs">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>项目</Table.Th>
                    <Table.Th>样本</Table.Th>
                    <Table.Th>基线（中位数）</Table.Th>
                    <Table.Th>波动</Table.Th>
                    <Table.Th>可信度</Table.Th>
                    <Table.Th>说明</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {baselineReport.baselines.map((baseline) => (
                    <Table.Tr
                      key={`${baseline.kind}-${baseline.schemaVersion}`}
                    >
                      <Table.Td>{baseline.label}</Table.Td>
                      <Table.Td>{baseline.sampleCount} 次</Table.Td>
                      <Table.Td>{baseline.medianScore.toFixed(0)} 分</Table.Td>
                      <Table.Td>±{baseline.spreadPercent.toFixed(1)}%</Table.Td>
                      <Table.Td>
                        <Badge
                          size="xs"
                          variant="light"
                          color={
                            baseline.confidence === "高"
                              ? "teal"
                              : baseline.confidence === "中"
                                ? "yellow"
                                : baseline.confidence === "低"
                                  ? "red"
                                  : "gray"
                          }
                        >
                          {baseline.confidence}
                        </Badge>
                      </Table.Td>
                      <Table.Td>
                        <Text size="xs" c="dimmed">
                          {baseline.confidenceNote}
                        </Text>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </ScrollArea>
          )}
          {baselineReport.notes.length > 0 && (
            <Box p="md" pt="xs">
              {baselineReport.notes.map((note) => (
                <Text key={note} size="xs" c="dimmed">
                  · {note}
                </Text>
              ))}
            </Box>
          )}
        </Card>
      )}
      {benchmarkTab !== "history" && (
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        {visibleBenchmarkKinds.map((item) => {
          const result = results[item.kind];
          const baselineForKind = result
            ? baselineReport?.baselines.find(
                (baseline) =>
                  baseline.kind === result.kind &&
                  baseline.schemaVersion === result.schemaVersion,
              )
            : undefined;
          const comparisonForKind = result
            ? baselineReport?.comparisons.find(
                (comparison) => comparison.kind === result.kind,
              )
            : undefined;
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
                  {comparisonForKind ? (
                    <Alert
                      mt="md"
                      color={
                        comparisonForKind.withinBaselineBand ? "teal" : "orange"
                      }
                      icon={
                        comparisonForKind.withinBaselineBand ? (
                          <IconShieldCheck />
                        ) : (
                          <IconAlertCircle />
                        )
                      }
                      title={`对比本机基线 ${comparisonForKind.baselineScore.toFixed(0)} 分`}
                    >
                      {comparisonForKind.deltaPercent >= 0 ? "+" : ""}
                      {comparisonForKind.deltaPercent.toFixed(1)}% ·{" "}
                      {comparisonForKind.note}
                    </Alert>
                  ) : (
                    baselineForKind && (
                      <Text size="xs" c="dimmed" mt="md">
                        基线 {baselineForKind.medianScore.toFixed(0)} 分
                        （{baselineForKind.sampleCount} 次样本，可信度
                        {baselineForKind.confidence}）；复检后会给出本次与基线的偏差。
                      </Text>
                    )
                  )}
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
                {result
                  ? baselineForKind
                    ? "复检并对比基线"
                    : "重新测试"
                  : "开始测试"}
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

const rowPathToolIds = new Set(["privacy_clean"]);

const canOpenRowPath = (toolId: string) => rowPathToolIds.has(toolId);

function DesktopPanel() {
  const [overview, setOverview] = useState<DesktopOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setOverview(await getDesktopOverview());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const openItem = async (item: DesktopItem) => {
    try {
      setStatus(await openDesktopItem(item.path));
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const openFolder = async (path: string) => {
    try {
      await openPathInExplorer(path);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  if (!overview)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );

  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        应用内桌面助手：只读取桌面项目并交给系统打开；不常驻、不注入资源管理器、不移动或删除文件。
      </Alert>
      <Group justify="space-between" align="flex-start" wrap="wrap">
        <Group>
          <Button
            size="xs"
            variant="light"
            onClick={() => void openFolder(overview.desktopPath)}
          >
            桌面
          </Button>
          <Button
            size="xs"
            variant="light"
            onClick={() => void openFolder(overview.documentsPath)}
          >
            文档
          </Button>
          <Button
            size="xs"
            variant="light"
            onClick={() => void openFolder(overview.downloadsPath)}
          >
            下载
          </Button>
        </Group>
        <Button size="xs" loading={loading} onClick={() => void load()}>
          刷新
        </Button>
      </Group>
      {error && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {error}
        </Alert>
      )}
      {status && (
        <Alert color="teal" icon={<IconShieldCheck />}>
          {status}
        </Alert>
      )}
      <Card className="glass-card" radius="lg" p={0}>
        <ScrollArea h="calc(100vh - 380px)">
          <Table stickyHeader highlightOnHover verticalSpacing="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>名称</Table.Th>
                <Table.Th>类型</Table.Th>
                <Table.Th>大小</Table.Th>
                <Table.Th>修改时间</Table.Th>
                <Table.Th>操作</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {overview.items.map((item) => (
                <Table.Tr key={item.path}>
                  <Table.Td>
                    <Text fw={600} size="sm" truncate maw={360}>
                      {item.name}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Badge size="xs" variant="light">
                      {item.isDir ? "文件夹" : item.extension || "文件"}
                    </Badge>
                  </Table.Td>
                  <Table.Td>
                    {item.isDir ? "—" : bytes(item.bytes)}
                  </Table.Td>
                  <Table.Td>
                    <Text size="xs" c="dimmed">
                      {item.modifiedUnixMs
                        ? new Date(item.modifiedUnixMs).toLocaleString()
                        : "未知"}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Button
                      size="xs"
                      variant="light"
                      onClick={() => void openItem(item)}
                    >
                      {item.isDir ? "打开文件夹" : "打开"}
                    </Button>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>
      <Text size="xs" c="dimmed">
        共 {overview.total} 个项目
        {overview.errorCount > 0 ? ` · ${overview.errorCount} 个读取错误` : ""}
      </Text>
    </Stack>
  );
}

function StartMenuPanel() {
  const [entries, setEntries] = useState<LaunchEntry[] | null>(null);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await listStartMenuApps());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const launch = async (entry: LaunchEntry) => {
    setStatus(null);
    try {
      setStatus(
        await launchStartMenuEntry(
          entry.source === "应用" ? "app" : "shortcut",
          entry.target,
          entry.appId,
        ),
      );
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const needle = query.trim().toLowerCase();
  const filtered = (entries ?? []).filter(
    (entry) =>
      !needle ||
      [entry.name, entry.target, entry.appId, entry.targetPath].some((value) =>
        value.toLowerCase().includes(needle),
      ),
  );

  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        从开始菜单与应用列表启动程序，不注入 Explorer、不修改开始菜单；启动前请确认目标。
      </Alert>
      <Group>
        <TextInput
          style={{ flex: 1 }}
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          leftSection={<IconSearch size={17} />}
          placeholder="搜索应用或快捷方式"
        />
        <Button variant="light" loading={loading} onClick={() => void load()}>
          刷新列表
        </Button>
      </Group>
      {error && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {error}
        </Alert>
      )}
      {status && (
        <Alert color="teal" icon={<IconShieldCheck />}>
          {status}
        </Alert>
      )}
      {!entries ? (
        <Center h={260}>
          <Loader />
        </Center>
      ) : (
        <Card className="glass-card" radius="lg" p={0}>
          <ScrollArea h="calc(100vh - 360px)">
            <Table stickyHeader highlightOnHover verticalSpacing="sm">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>应用</Table.Th>
                  <Table.Th>来源</Table.Th>
                  <Table.Th>目标</Table.Th>
                  <Table.Th>操作</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {filtered.map((entry, index) => (
                  <Table.Tr key={`${entry.name}-${entry.target}-${entry.appId}-${index}`}>
                    <Table.Td>
                      <Text fw={700}>{entry.name}</Text>
                    </Table.Td>
                    <Table.Td>
                      <Badge size="xs" variant="light">
                        {entry.source}
                      </Badge>
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs" c="dimmed" truncate maw={420}>
                        {entry.source === "应用"
                          ? entry.appId
                          : entry.targetPath || entry.target}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Button
                        size="xs"
                        variant="light"
                        onClick={() => void launch(entry)}
                      >
                        启动
                      </Button>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </ScrollArea>
        </Card>
      )}
      <Text size="xs" c="dimmed">
        共 {entries?.length ?? 0} 项，显示 {filtered.length} 项；列表来自开始菜单快捷方式与系统应用列表。
      </Text>
    </Stack>
  );
}

function SettingsPanel({
  settings,
  onSaved,
  onNavigate,
}: {
  settings: AppSettings | null;
  onSaved: (settings: AppSettings) => void;
  onNavigate: (tab: Tab) => void;
}) {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [openPathError, setOpenPathError] = useState<string | null>(null);

  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    void getAppPaths()
      .then(setPaths)
      .catch(() => undefined);
  }, []);

  if (!draft)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );

  const saveSettings = async () => {
    setSaving(true);
    setFailed(false);
    try {
      const saved = await saveAppSettings(draft);
      setDraft(saved);
      onSaved(saved);
      setStatus("设置已保存，刷新间隔立即生效。");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
      setFailed(true);
    } finally {
      setSaving(false);
    }
  };

  const openDataPath = async (path: string) => {
    setOpenPathError(null);
    try {
      await openPathInExplorer(path);
    } catch (error) {
      setOpenPathError(error instanceof Error ? error.message : String(error));
    }
  };

  const clearHistory = async () => {
    if (!window.confirm("确认清空全部指标历史？该操作不可撤销。")) return;
    try {
      await clearSystemHistory();
      setStatus("指标历史已清空。");
      setFailed(false);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
      setFailed(true);
    }
  };

  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconSettings />}>
        设置保存在本机配置目录，不会上传；采样与历史只影响本应用。
      </Alert>

      <Card className="glass-card" radius="lg">
        <Text fw={800}>刷新与历史</Text>
        <Text size="xs" c="dimmed">
          刷新间隔影响概览页轮询；历史保留只影响指标数据库，不影响温度传感器历史。
        </Text>
        <Group mt="md" align="flex-end" wrap="wrap">
          <Select
            size="xs"
            w={180}
            label="概览刷新间隔"
            allowDeselect={false}
            value={String(draft.snapshotIntervalSeconds)}
            onChange={(value) =>
              value &&
              setDraft({ ...draft, snapshotIntervalSeconds: Number(value) })
            }
            data={[
              { value: "1", label: "1 秒" },
              { value: "2", label: "2 秒" },
              { value: "5", label: "5 秒" },
              { value: "10", label: "10 秒" },
            ]}
          />
          <NumberInput
            size="xs"
            label="指标历史保留（天）"
            min={1}
            max={30}
            w={180}
            value={draft.metricsRetentionDays}
            onChange={(value) => {
              const parsed =
                typeof value === "number" ? value : Number.parseFloat(value);
              if (Number.isFinite(parsed)) {
                setDraft({ ...draft, metricsRetentionDays: Math.round(parsed) });
              }
            }}
          />
          <Button loading={saving} onClick={() => void saveSettings()}>
            保存设置
          </Button>
        </Group>
        {status && (
          <Text size="sm" mt="sm" c={failed ? "red" : "teal"}>
            {status}
          </Text>
        )}
      </Card>

      <Card className="glass-card" radius="lg">
        <Text fw={800}>数据目录</Text>
        <Text size="xs" c="dimmed">
          指标数据库、工具操作日志与设置文件的本地位置
        </Text>
        <Stack gap="xs" mt="md">
          {[
            ["数据目录", paths?.dataDir],
            ["配置目录", paths?.configDir],
          ].map(([label, path]) => (
            <Group key={label} justify="space-between" wrap="nowrap">
              <Box className="startup-command">
                <Text size="sm" fw={600}>
                  {label}
                </Text>
                <Text size="xs" c="dimmed" truncate>
                  {path ?? "加载中…"}
                </Text>
              </Box>
              <Button
                size="xs"
                variant="light"
                disabled={!path}
                onClick={() => path && void openDataPath(path)}
              >
                打开
              </Button>
            </Group>
          ))}
          {openPathError && (
            <Text size="xs" c="red" mt="xs">
              打开目录失败：{openPathError}
            </Text>
          )}
        </Stack>
      </Card>

      <Card className="glass-card" radius="lg">
        <Text fw={800}>功能入口</Text>
        <Group mt="md">
          <Button size="xs" variant="light" onClick={() => onNavigate("sensors")}>
            通知与温度阈值
          </Button>
          <Button size="xs" variant="light" onClick={() => onNavigate("eye-care")}>
            护眼模式
          </Button>
          <Button size="xs" variant="light" onClick={() => onNavigate("toolbox")}>
            工具箱
          </Button>
        </Group>
      </Card>

      <Card className="glass-card" radius="lg">
        <Text fw={800}>维护</Text>
        <Text size="xs" c="dimmed">
          仅清空概览指标历史；温度传感器历史与工具审计日志不受影响。
        </Text>
        <Button size="xs" color="red" variant="light" mt="md" onClick={() => void clearHistory()}>
          清空指标历史
        </Button>
      </Card>
    </Stack>
  );
}

function EyeCarePanel({
  settings,
  onSaved,
}: {
  settings: EyeCareSettings | null;
  onSaved: (settings: EyeCareSettings) => void;
}) {
  const [draft, setDraft] = useState<EyeCareSettings | null>(settings);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => setDraft(settings), [settings]);

  if (!draft)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );

  const apply = async () => {
    setSaving(true);
    setFailed(false);
    try {
      const saved = await saveEyeCareSettings(draft);
      const result = await setEyeCareFilter(saved.enabled, saved.warmth);
      setDraft(saved);
      onSaved(saved);
      setStatus(result.message);
      setFailed(saved.enabled && !result.applied);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
      setFailed(true);
    } finally {
      setSaving(false);
    }
  };

  const swatchGreen = Math.round(255 - (draft.warmth / 100) * 30);
  const swatchBlue = Math.round(255 - (draft.warmth / 100) * 95);
  const numberFrom = (value: number | string) =>
    typeof value === "number" ? value : Number.parseFloat(value);

  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconEye />}>
        护眼模式通过调整显示器 Gamma Ramp 降低蓝光，不覆盖屏幕内容；HDR
        或部分驱动可能不支持。休息提醒只在应用运行时发送，系统通知失败时仍会显示应用内横幅。
      </Alert>

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start">
          <Box>
            <Text fw={800}>色温滤镜</Text>
            <Text size="xs" c="dimmed">
              暖色强度 {draft.warmth}%（0 为原色，100 为最暖）
            </Text>
          </Box>
          <Switch
            checked={draft.enabled}
            onChange={(event) =>
              setDraft({ ...draft, enabled: event.currentTarget.checked })
            }
            label="启用护眼模式"
          />
        </Group>
        <Slider
          mt="md"
          min={0}
          max={100}
          value={draft.warmth}
          onChange={(value) => setDraft({ ...draft, warmth: value })}
          marks={[
            { value: 0, label: "原色" },
            { value: 50, label: "适中" },
            { value: 100, label: "最暖" },
          ]}
        />
        <Group mt="lg" gap="sm" align="center">
          <Box
            w={120}
            h={48}
            style={{
              borderRadius: 10,
              border: "1px solid rgba(255,255,255,0.15)",
              background: `rgb(255, ${swatchGreen}, ${swatchBlue})`,
            }}
          />
          <Text size="xs" c="dimmed">
            近似效果预览；实际效果取决于显示器与色彩配置。
          </Text>
        </Group>
      </Card>

      <Card className="glass-card" radius="lg">
        <Text fw={800}>休息提醒</Text>
        <Text size="xs" c="dimmed">
          按间隔弹出应用内横幅并发送系统通知；应用在后台或全屏时跳过
        </Text>
        <Group mt="md" align="flex-end" wrap="wrap">
          <NumberInput
            size="xs"
            label="间隔（分钟）"
            min={5}
            max={180}
            w={150}
            value={draft.breakIntervalMinutes}
            onChange={(value) => {
              const parsed = numberFrom(value);
              if (Number.isFinite(parsed)) {
                setDraft({ ...draft, breakIntervalMinutes: Math.round(parsed) });
              }
            }}
          />
          <NumberInput
            size="xs"
            label="提醒时长（秒）"
            min={5}
            max={300}
            w={150}
            value={draft.breakDurationSeconds}
            onChange={(value) => {
              const parsed = numberFrom(value);
              if (Number.isFinite(parsed)) {
                setDraft({ ...draft, breakDurationSeconds: Math.round(parsed) });
              }
            }}
          />
          <Switch
            checked={draft.scheduleEnabled}
            onChange={(event) =>
              setDraft({
                ...draft,
                scheduleEnabled: event.currentTarget.checked,
              })
            }
            label="仅在时间段内生效"
          />
          <NumberInput
            size="xs"
            label="开始（时）"
            min={0}
            max={23}
            w={120}
            value={draft.scheduleStartHour}
            onChange={(value) => {
              const parsed = numberFrom(value);
              if (Number.isFinite(parsed)) {
                setDraft({ ...draft, scheduleStartHour: Math.round(parsed) });
              }
            }}
          />
          <NumberInput
            size="xs"
            label="结束（时）"
            min={0}
            max={23}
            w={120}
            value={draft.scheduleEndHour}
            onChange={(value) => {
              const parsed = numberFrom(value);
              if (Number.isFinite(parsed)) {
                setDraft({ ...draft, scheduleEndHour: Math.round(parsed) });
              }
            }}
          />
        </Group>
        <Text size="xs" c="dimmed" mt="sm">
          起止小时相同表示全天生效；跨零点按环形时间段计算。
        </Text>
      </Card>

      <Group align="center">
        <Button loading={saving} onClick={() => void apply()}>
          保存并应用
        </Button>
        {status && (
          <Text size="sm" c={failed ? "red" : "teal"}>
            {status}
          </Text>
        )}
      </Group>
    </Stack>
  );
}

/**
 * 清理前后对比表：按位置给出清理前后的占用与文件数。
 * 清理大师与 C 盘瘦身共用同一份清理引擎，因此两边的对比结果一致。
 */
function CleanupDiffTable({ rows }: { rows: CleanupDiffRow[] }) {
  if (!rows.length) return null;
  return (
    <Box mt="md">
      <Text size="sm" fw={600}>
        清理前后对比
      </Text>
      <ScrollArea type="auto" mt="xs">
        <Table miw={760} highlightOnHover verticalSpacing="xs">
          <Table.Thead>
            <Table.Tr>
              <Table.Th>位置</Table.Th>
              <Table.Th>清理前</Table.Th>
              <Table.Th>清理后</Table.Th>
              <Table.Th>释放</Table.Th>
              <Table.Th>结果</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {rows.map((row) => (
              <Table.Tr key={row.id}>
                <Table.Td>
                  <Text size="sm" fw={600}>
                    {row.label}
                  </Text>
                  <Text size="xs" c="dimmed" truncate maw={300}>
                    {row.path}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{row.beforeSize}</Text>
                  <Text size="xs" c="dimmed">
                    {row.beforeFiles} 个文件
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{row.afterSize}</Text>
                  <Text size="xs" c="dimmed">
                    {row.afterFiles} 个文件
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm" c={row.freedBytes > 0 ? "teal" : "dimmed"}>
                    {row.freedSize}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Badge
                    size="xs"
                    variant="light"
                    color={
                      row.status === "已清理"
                        ? "teal"
                        : row.status === "未变化"
                          ? "gray"
                          : "orange"
                    }
                  >
                    {row.status}
                  </Badge>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </ScrollArea>
    </Box>
  );
}

/** 取消与失败要分开回执：取消是用户主动操作，不该显示成报错。 */
function isCancelledMessage(message: string): boolean {
  return message.startsWith("CANCELLED:") || message.includes("操作已取消");
}

function ToolboxPanel({ onOpenEyeCare, onOpenStartMenu, onOpenDesktopQuick }: { onOpenEyeCare?: () => void; onOpenStartMenu?: () => void; onOpenDesktopQuick?: () => void }) {
  const [tools, setTools] = useState<ToolDescriptor[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [slim, setSlim] = useState<SlimScan | null>(null);
  const [slimDrawerOpen, setSlimDrawerOpen] = useState(false);
  const [slimSelection, setSlimSelection] = useState<string[]>([]);
  const [slimToken, setSlimToken] = useState<string | null>(null);
  const [slimApplying, setSlimApplying] = useState(false);
  const [slimResult, setSlimResult] = useState<ToolResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [openPathError, setOpenPathError] = useState<string | null>(null);
  const [rowPathError, setRowPathError] = useState<string | null>(null);
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
  const [toolboxStats, setToolboxStats] = useState<ToolboxStatsReport | null>(
    null,
  );
  const [statsError, setStatsError] = useState<string | null>(null);
  const [statsDetail, setStatsDetail] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const [zipMode, setZipMode] = useState<"compress" | "extract" | "list">(
    "compress",
  );
  const [zipSources, setZipSources] = useState("");
  const [zipArchive, setZipArchive] = useState("");
  const [zipDestination, setZipDestination] = useState("");
  const [zipOverwrite, setZipOverwrite] = useState(false);
  const [zipProgress, setZipProgress] = useState<{
    phase: string;
    done: number;
    total: number;
    message: string;
  } | null>(null);
  const [zipNotice, setZipNotice] = useState<string | null>(null);
  const [zipCancelling, setZipCancelling] = useState(false);
  const zipJobRef = useRef<string | null>(null);
  const [pdfMode, setPdfMode] = useState<
    "info" | "pages" | "merge" | "split" | "rotate"
  >("info");
  const [pdfInputs, setPdfInputs] = useState("");
  const [pdfOutput, setPdfOutput] = useState("");
  const [pdfPages, setPdfPages] = useState("");
  const [pdfAngle, setPdfAngle] = useState("90");
  const [pdfPassword, setPdfPassword] = useState("");
  const [pdfOutPassword, setPdfOutPassword] = useState("");
  const [pdfPermissions, setPdfPermissions] = useState<string[]>(["print"]);
  const [pdfConflict, setPdfConflict] = useState<"fail" | "overwrite" | "rename">(
    "fail",
  );
  // 只有合并/拆分/旋转会写文件；信息与页面预览都是只读。
  const pdfCanWrite =
    pdfMode === "merge" || pdfMode === "split" || pdfMode === "rotate";

  const runPdf = async (
    tool: ToolDescriptor,
    mode: "info" | "pages" | "merge" | "split" | "rotate",
  ) => {
    const inputs = pdfInputs
      .split("\n")
      .map((value) => value.trim())
      .filter(Boolean);
    if (inputs.length === 0) {
      setApplyError("请至少填写一个输入 PDF 的绝对路径。");
      return;
    }
    const params = {
      mode,
      inputs,
      output: pdfOutput.trim(),
      pages: pdfPages.trim(),
      angle: Number(pdfAngle),
      // 保留旧的 overwrite 布尔值，新逻辑统一走 conflict。
      overwrite: pdfConflict === "overwrite",
      conflict: pdfConflict,
      password: pdfPassword,
      outPassword: pdfOutPassword,
      permissions: pdfPermissions,
    };
    setApplying(`${tool.id}:${mode}`);
    setApplyError(null);
    try {
      if (mode === "info") {
        const data = (await runToolAction(tool.id, "info", null, {
          path: inputs[0],
          password: pdfPassword,
        })) as ToolResult;
        setGeneric({ tool, data });
        return;
      }
      if (mode === "pages") {
        const data = (await runToolAction(tool.id, "pages", null, {
          inputs,
          password: pdfPassword,
        })) as ToolResult;
        setGeneric({ tool, data });
        return;
      }
      const preview = (await runToolAction(tool.id, "preview", null, params)) as ToolResult;
      const summary = (preview.summary ?? [])
        .map((entry) => `${entry.label}：${entry.value}`)
        .join("\n");
      if (!window.confirm(`确认执行 PDF 操作？\n\n${summary}`)) return;
      const data = (await runToolAction(
        tool.id,
        "apply",
        preview.confirmationToken ?? null,
        params,
      )) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess("PDF 操作完成");
      void refreshStats();
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const runZip = async (
    tool: ToolDescriptor,
    mode: "compress" | "extract" | "list",
  ) => {
    const params = {
      mode,
      sources: zipSources
        .split("\n")
        .map((value) => value.trim())
        .filter(Boolean),
      archive: zipArchive.trim(),
      destination: zipDestination.trim(),
      overwrite: zipOverwrite,
    };
    setApplying(`${tool.id}:${mode}`);
    setApplyError(null);
    setZipNotice(null);
    try {
      if (mode === "list") {
        const data = (await runToolAction(tool.id, "list", null, {
          archive: params.archive,
        })) as ToolResult;
        setGeneric({ tool, data });
        return;
      }
      const preview = (await runToolAction(tool.id, "preview", null, params)) as ToolResult;
      const summary = (preview.summary ?? [])
        .map((entry) => `${entry.label}：${entry.value}`)
        .join("\n");
      if (
        !window.confirm(
          `${mode === "compress" ? "确认创建压缩包" : "确认解压"}？\n\n${summary}`,
        )
      )
        return;
      // 任务 id 必须在调用前同步记下：进度事件是异步到达的，靠它关联到这一次执行。
      const jobId = newToolJobId();
      zipJobRef.current = jobId;
      setZipProgress({
        phase: mode,
        done: 0,
        total: 0,
        message: mode === "compress" ? "正在准备压缩…" : "正在准备解压…",
      });
      setZipCancelling(false);
      const data = (await runToolAction(
        tool.id,
        "apply",
        preview.confirmationToken ?? null,
        params,
        jobId,
      )) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess(mode === "compress" ? "压缩完成" : "解压完成");
      void refreshStats();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      if (isCancelledMessage(message)) {
        setZipNotice(message.replace(/^CANCELLED:\s*/, ""));
      } else {
        setApplyError(message);
      }
    } finally {
      setApplying(null);
      setZipProgress(null);
      setZipCancelling(false);
      zipJobRef.current = null;
    }
  };

  const cancelZip = async () => {
    const jobId = zipJobRef.current;
    if (!jobId) return;
    setZipCancelling(true);
    try {
      const accepted = await cancelToolAction(jobId);
      if (!accepted) {
        setApplyError("取消请求未被接受：任务可能已经结束。");
        setZipCancelling(false);
      }
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
      setZipCancelling(false);
    }
  };
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

  const [winoptDraft, setWinoptDraft] = useState<Record<string, number>>({});
  const [winoptAdminDraft, setWinoptAdminDraft] = useState<
    Record<string, number>
  >({});
  const [dupKeep, setDupKeep] = useState<Record<string, string>>({});
  const [privacyDraft, setPrivacyDraft] = useState<Record<string, string>>({});
  const [procPriority, setProcPriority] = useState<Record<string, string>>({});
  const [adblockDomains, setAdblockDomains] = useState("");
  const [adblockFile, setAdblockFile] = useState("");
  const [gpuPath, setGpuPath] = useState("");
  const [gpuPref, setGpuPref] = useState(2);
  const [lockDelay, setLockDelay] = useState(5);
  const [lockRemaining, setLockRemaining] = useState(0);
  // 计时器由 effect 驱动：页面卸载时自动清理，不会「离开页面还锁屏」。
  const [lockArmed, setLockArmed] = useState(false);
  const [expandAck, setExpandAck] = useState<Record<string, boolean>>({});
  const [dumpDraft, setDumpDraft] = useState("");

  const updatePrivacyValue = (id: string, value: string | null) => {
    if (value == null) return;
    setPrivacyDraft((current) => ({ ...current, [id]: value }));
  };

  const updateWinoptValue = (id: string, value: number | string) => {
    const parsed = typeof value === "number" ? value : Number.parseInt(value, 10);
    if (!Number.isFinite(parsed)) return;
    setWinoptDraft((current) => ({ ...current, [id]: parsed }));
  };

  const updateWinoptAdminValue = (id: string, value: number | string) => {
    const parsed = typeof value === "number" ? value : Number.parseInt(value, 10);
    if (!Number.isFinite(parsed)) return;
    setWinoptAdminDraft((current) => ({ ...current, [id]: parsed }));
  };

  const refreshStats = useCallback(async () => {
    try {
      setToolboxStats(await getToolboxStats());
      setStatsError(null);
    } catch (reason) {
      setStatsError(reason instanceof Error ? reason.message : String(reason));
    }
  }, []);

  useEffect(() => {
    void listToolboxTools()
      .then(setTools)
      .catch((reason) => setError(String(reason)));
    void refreshStats();
  }, [refreshStats]);

  // 订阅工具进度：只有带 jobId 的执行才会有进度事件，按 jobId 过滤避免串台。
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenToolProgress((event) => {
      if (event.jobId !== zipJobRef.current) return;
      setZipProgress({
        phase: event.phase,
        done: event.done,
        total: event.total,
        message: event.message,
      });
    })
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const runGeneric = async (tool: ToolDescriptor) => {
    setRunningTool(tool.id);
    setGenericError(null);
    setApplyError(null);
    setRowPathError(null);
    try {
      const data = (await runToolAction(tool.id, "preview")) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess(`${tool.name}预览生成完成`);
      setGenericDrawerOpen(true);
      void refreshStats();
      if (tool.id === "clean_master") {
        setSelection(
          (data.rows ?? [])
            .filter(
              (row) =>
                row.cleanable !== false &&
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
      if (tool.id === "winopt_assistant") {
        setWinoptDraft({});
        setWinoptAdminDraft({});
      }
      if (tool.id === "DuplicateFileClean") {
        const groups = (data as ToolResult).duplicateFileGroups ?? [];
        setDupKeep(
          Object.fromEntries(
            groups
              .filter((group) => group.files.length > 0)
              .map((group) => [group.id, group.files[0]]),
          ),
        );
      }
      if (tool.id === "privacy_protection") {
        setPrivacyDraft({});
      }
      if (tool.id === "adblock_master") {
        setDumpDraft("");
      }
      if (tool.id === "bsod_repair") {
        setDumpDraft((data as ToolResult).dumpMode?.mode ?? "");
      }
      if (tool.id === "c_expand") {
        const items = (data as ToolResult).expandItems ?? [];
        setExpandAck(
          Object.fromEntries(items.map((item) => [item.drive, false])),
        );
      }
      if (tool.id === "proc_opt") {
        const items = (data as ToolResult).processItems ?? [];
        setProcPriority(
          Object.fromEntries(
            items.map((item) => [String(item.pid), item.priority]),
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
    tokenOverride?: string,
    confirmNote?: string,
  ) => {
    const token = tokenOverride ?? generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新运行预览。");
      return;
    }
    if (
      !window.confirm(
        `确认执行「${action.label}」？该操作会立即生效。${confirmNote ?? ""}`,
      )
    )
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
      void refreshStats();
      if (data.powerSettings) {
        setPowerDraft(
          Object.fromEntries(
            data.powerSettings.map((setting) => [
              setting.id,
              { ac: setting.ac, dc: setting.dc },
            ]),
          ),
        );
      }
      if (data.winoptSettings) {
        setWinoptDraft({});
      }
      if (data.winoptAdminSettings) {
        setWinoptAdminDraft({});
      }
      if (data.privacySettings) {
        setPrivacyDraft({});
      }
      if (data.dumpMode) {
        setDumpDraft(data.dumpMode.mode);
      }
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

  const restoreAllStartups = async (tool: ToolDescriptor) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新运行预览。");
      return;
    }
    if (!window.confirm("确认恢复全部已停用的当前用户启动项？")) return;
    setApplying(`${tool.id}:restore_all`);
    setApplyError(null);
    try {
      const data = (await runToolAction(
        tool.id,
        "restore_all",
        token,
        {},
      )) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess("已恢复全部停用启动项");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const toggleSystemStartup = async (
    tool: ToolDescriptor,
    item: NonNullable<ToolResult["systemStartupItems"]>[number],
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
        `确认${actionLabel}？这是系统级启动项，将弹出管理员授权（UAC）。`,
      )
    )
      return;
    setApplying(`${tool.id}:toggle_system_startup`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "toggle_system_startup", token, {
        key: item.key,
        name: item.name,
        enable: !item.enabled,
      });
      setGenericSuccess(`${actionLabel}完成，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const setServiceStart = async (
    tool: ToolDescriptor,
    item: NonNullable<ToolResult["serviceItems"]>[number],
    startType: number,
  ) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新运行预览。");
      return;
    }
    const label =
      startType === 2 ? "自动" : startType === 3 ? "手动" : "禁用";
    if (
      !window.confirm(
        `确认将服务「${item.display}」的启动类型改为「${label}」？将弹出管理员授权（UAC）。`,
      )
    )
      return;
    setApplying(`${tool.id}:set_service_start`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "set_service_start", token, {
        name: item.name,
        startType,
        expectCurrent: item.start,
      });
      setGenericSuccess(`「${item.display}」已改为${label}，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const setScheduledTaskEnabled = async (
    tool: ToolDescriptor,
    item: ScheduledTaskItem,
  ) => {
    const token = generic?.data.taskToken;
    if (!token) {
      setApplyError("任务令牌缺失或已使用，请重新预览。");
      return;
    }
    const label = item.enabled
      ? `禁用「${item.name}」`
      : `启用「${item.name}」`;
    if (
      !window.confirm(`确认${label}？将弹出管理员授权（UAC）。`)
    )
      return;
    setApplying(`${tool.id}:set_task_enabled`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "set_task_enabled", token, {
        taskPath: item.path,
        name: item.name,
        enable: !item.enabled,
      });
      setGenericSuccess(`${label}完成，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const setGpuPreference = async (
    tool: ToolDescriptor,
    path: string,
    preference: number,
  ) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新预览。");
      return;
    }
    const target = path.trim();
    if (!target) {
      setApplyError("请输入应用的 .exe 绝对路径。");
      return;
    }
    if (!window.confirm(`把 ${target} 的 GPU 偏好设为 ${preference}？`)) return;
    setApplying(`${tool.id}:set_gpu_preference`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "set_gpu_preference", token, {
        path: target,
        preference,
      });
      setGenericSuccess("GPU 偏好已更新");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const importAdblockRules = async (tool: ToolDescriptor) => {
    const path = adblockFile.trim();
    if (!path) {
      setApplyError("请输入规则文件路径。");
      return;
    }
    setApplying(`${tool.id}:import_rules`);
    setApplyError(null);
    try {
      const data = (await runToolAction(tool.id, "import_rules", null, {
        path,
      })) as ToolResult;
      const domains = data.domains ?? [];
      setAdblockDomains(domains.join("\n"));
      setGenericSuccess(`已导入 ${domains.length} 个域名`);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const applyAdblockHosts = async (tool: ToolDescriptor) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新审计。");
      return;
    }
    const domains = adblockDomains
      .split(/\r?\n/)
      .map((value) => value.trim())
      .filter(Boolean);
    if (domains.length === 0) {
      setApplyError("请输入至少一个域名。");
      return;
    }
    if (
      !window.confirm(
        `确认写入 ${domains.length} 条 Hosts 规则？将弹出管理员授权（UAC）。`,
      )
    )
      return;
    setApplying(`${tool.id}:apply_hosts`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "apply_hosts", token, { domains });
      setGenericSuccess("Hosts 规则已写入，正在刷新…");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const restoreAdblockHosts = async (tool: ToolDescriptor) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新审计。");
      return;
    }
    if (
      !window.confirm(
        "确认移除 SystemLens 管理的 Hosts 规则？将弹出管理员授权（UAC）。",
      )
    )
      return;
    setApplying(`${tool.id}:restore_hosts`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "restore_hosts", token, {});
      setGenericSuccess("SystemLens 规则已移除，正在刷新…");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const performLock = async () => {
    const tool = generic?.tool;
    if (!tool) return;
    setApplying(`${tool.id}:lock`);
    setApplyError(null);
    try {
      const preview = (await runToolAction(
        tool.id,
        "preview",
        null,
        {},
      )) as ToolResult;
      const token = preview.confirmationToken;
      if (!token) throw new Error("无法获取锁定令牌");
      await runToolAction(tool.id, "lock", token, {});
      setGenericSuccess("已锁定当前会话");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const startLockTimer = () => {
    setLockRemaining(Math.max(1, lockDelay) * 60);
    setLockArmed(true);
  };

  const cancelLockTimer = () => {
    setLockArmed(false);
    setLockRemaining(0);
  };

  // 倒计时：只在「已武装」时创建 interval，卸载/取消时由 cleanup 清掉。
  useEffect(() => {
    if (!lockArmed) return;
    const timer = window.setInterval(() => {
      setLockRemaining((previous) => (previous <= 1 ? 0 : previous - 1));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [lockArmed]);

  // 归零后触发锁定。用 effect 而不是在 setState 的 updater 里做副作用：
  // StrictMode 下 updater 会被重复调用，旧写法会发两次锁屏请求。
  useEffect(() => {
    if (!lockArmed || lockRemaining > 0) return;
    setLockArmed(false);
    void performLock();
    // performLock 依赖当前 generic，不能进依赖数组，否则每次渲染都会重新触发。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lockArmed, lockRemaining]);

  const runSysBoost = async (
    tool: ToolDescriptor,
    action: "boost" | "undo_boost",
  ) => {
    const token =
      action === "boost"
        ? generic?.data.boostToken
        : generic?.data.undoToken;
    if (!token) {
      setApplyError("加速令牌缺失，请重新预览。");
      return;
    }
    if (
      !window.confirm(
        action === "boost"
          ? "应用一键加速？会修改视觉效果、动画、透明与电源计划，并先保存快照。"
          : "撤销加速并恢复最近一次快照？",
      )
    )
      return;
    setApplying(`${tool.id}:${action}`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, action, token, {});
      setGenericSuccess(action === "boost" ? "加速已应用" : "已恢复加速前状态");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const setAiPowerLimit = async (tool: ToolDescriptor, percent: number) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新预览。");
      return;
    }
    if (
      !window.confirm(
        `把处理器最大状态限制为 ${percent}%？将弹出管理员授权（UAC），设回 100% 可恢复。`,
      )
    )
      return;
    setApplying(`${tool.id}:set_power_limit`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "set_power_limit", token, { percent });
      setGenericSuccess(`处理器上限已设为 ${percent}%`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const saveSoftwareRollback = async (tool: ToolDescriptor) => {
    setApplying(`${tool.id}:save_rollback`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "save_rollback", null, {});
      setGenericSuccess("回滚点已保存到本地");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const restoreSoftwareRollback = async (tool: ToolDescriptor) => {
    const token = generic?.data.rollbackToken;
    if (!token) {
      setApplyError("回滚点令牌缺失，请重新预览。");
      return;
    }
    if (
      !window.confirm(
        "恢复到最近一次回滚点？将逐项恢复第三方服务启动类型与计划任务，并弹出一次管理员授权（UAC）。",
      )
    )
      return;
    setApplying(`${tool.id}:restore_rollback`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "restore_rollback", token, {});
      setGenericSuccess("回滚点恢复完成");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const createRestorePoint = async (tool: ToolDescriptor) => {
    const token = generic?.data.restorePointToken;
    if (!token) {
      setApplyError("还原点令牌缺失，请重新预览。");
      return;
    }
    if (
      !window.confirm(
        "创建系统还原点？将弹出管理员授权（UAC），可能受 24 小时频率限制。",
      )
    )
      return;
    setApplying(`${tool.id}:create_restore_point`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "create_restore_point", token, {});
      setGenericSuccess("系统还原点已创建");
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const exportAppxList = async (tool: ToolDescriptor) => {
    setApplying(`${tool.id}:export_list`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "export_list", null, {});
      setGenericSuccess("可移除清单已导出到本地");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const openDriverTarget = async (tool: ToolDescriptor, id: string) => {
    setApplying(`${tool.id}:open_official`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "open_official", null, { id });
      setGenericSuccess("已打开 Windows 官方驱动界面");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const openOfficialRuntime = async (tool: ToolDescriptor, id: string) => {
    setApplying(`${tool.id}:open_official`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "open_official", null, { id });
      setGenericSuccess("已在默认浏览器打开微软官方下载页");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const removeAppx = async (tool: ToolDescriptor, item: AppxItem) => {
    const token = generic?.data.confirmationToken;
    if (!token) {
      setApplyError("确认令牌缺失或已使用，请重新预览。");
      return;
    }
    if (
      !window.confirm(
        `确认移除「${item.name}」？只影响当前用户，之后可从 Microsoft Store 重装。`,
      )
    )
      return;
    setApplying(`${tool.id}:remove_appx`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "remove_appx", token, {
        fullName: item.fullName,
      });
      setGenericSuccess(`已移除 ${item.name}，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const readElevatedSensors = async (tool: ToolDescriptor) => {
    setApplying(`${tool.id}:read_elevated`);
    setApplyError(null);
    try {
      const data = (await runToolAction(
        tool.id,
        "read_elevated",
        null,
        {},
      )) as ToolResult;
      setGeneric({ tool, data });
      setGenericSuccess("受限传感器读取完成");
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const endProcess = async (
    tool: ToolDescriptor,
    item: ProcessItem,
  ) => {
    if (!item.canEnd) return;
    if (
      !window.confirm(
        `确认结束「${item.name}」（PID ${item.pid}）？未保存的数据会丢失。`,
      )
    )
      return;
    setApplying(`${tool.id}:end_process`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "end_process", item.endToken, {
        pid: item.pid,
        name: item.name,
        startTime: item.startTime,
      });
      setGenericSuccess(`已结束 ${item.name}，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const applyProcessPriority = async (
    tool: ToolDescriptor,
    item: ProcessItem,
  ) => {
    const priority = procPriority[String(item.pid)] ?? item.priority;
    if (priority === item.priority) {
      setApplyError("优先级没有变化。");
      return;
    }
    if (
      !window.confirm(
        `确认把「${item.name}」（PID ${item.pid}）的优先级改为 ${priority}？`,
      )
    )
      return;
    setApplying(`${tool.id}:set_priority`);
    setApplyError(null);
    try {
      await runToolAction(tool.id, "set_priority", item.priorityToken, {
        pid: item.pid,
        name: item.name,
        startTime: item.startTime,
        priority,
      });
      setGenericSuccess(`已调整 ${item.name} 的优先级，正在刷新…`);
      await runGeneric(tool);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setApplying(null);
    }
  };

  const openCleanupRecycleBin = async () => {
    try {
      await openRecycleBin();
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const openCleanupJournal = async (dir: string) => {
    try {
      await openPathInExplorer(dir);
    } catch (reason) {
      setApplyError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const scan = async () => {
    setScanning(true);
    setScanError(null);
    setSlimResult(null);
    try {
      const result = (await runToolAction("cclean", "preview")) as SlimScan;
      setSlim(result);
      // 与清理大师共用同一份扫描结果：默认勾选可清理、占用明确且不需要提权的位置。
      setSlimSelection(
        result.items
          .filter(
            (item) =>
              item.cleanable !== false &&
              item.category === "safe" &&
              item.fileCount > 0 &&
              item.requiresAdmin !== true,
          )
          .map((item) => item.id),
      );
      setSlimToken(result.confirmationToken ?? null);
      setSlimDrawerOpen(true);
    } catch (reason) {
      setScanError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setScanning(false);
    }
  };

  const slimApply = async () => {
    if (!slimToken) {
      setScanError("确认令牌缺失或已使用，请重新扫描。");
      return;
    }
    if (
      !window.confirm(
        `确认把所选 ${slimSelection.length} 个位置的文件移入回收站？可从回收站还原。`,
      )
    )
      return;
    setSlimApplying(true);
    setScanError(null);
    try {
      const data = (await runToolAction("cclean", "apply", slimToken, {
        candidateIds: slimSelection,
      })) as ToolResult;
      setSlimResult(data);
      setSlimToken(null);
      setSlimSelection([]);
      // 清理后刷新数字；刷新失败就保留旧数字，不影响本次结果展示。
      try {
        const refreshed = (await runToolAction(
          "cclean",
          "preview",
        )) as SlimScan;
        setSlim(refreshed);
        setSlimToken(refreshed.confirmationToken ?? null);
      } catch {
        // 保留清理前的扫描结果。
      }
    } catch (reason) {
      setScanError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSlimApplying(false);
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

  const openRowPath = async (path: string) => {
    if (!path) {
      return;
    }
    setRowPathError(null);
    try {
      await openPathInExplorer(path);
    } catch (reason) {
      setRowPathError(reason instanceof Error ? reason.message : String(reason));
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
  const powerPresets: PowerPresetOption[] = generic?.data.powerPresets ?? [];
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
  const winoptSettings: WinoptSetting[] = generic?.data.winoptSettings ?? [];
  const winoptChanges = winoptSettings.flatMap((setting) => {
    const draft = winoptDraft[setting.id];
    if (draft == null || setting.current === draft) {
      return [] as { id: string; value: number }[];
    }
    return [{ id: setting.id, value: draft }];
  });
  const winoptUndo = generic?.data.winoptUndo ?? [];
  const winoptAdminSettings: WinoptAdminSetting[] =
    generic?.data.winoptAdminSettings ?? [];
  const winoptAdminChanges = winoptAdminSettings.flatMap((setting) => {
    const draft = winoptAdminDraft[setting.id];
    if (draft == null || draft === setting.current) {
      return [] as { id: string; value: number }[];
    }
    return [{ id: setting.id, value: draft }];
  });
  const winoptAdminUndo = generic?.data.winoptAdminUndo ?? [];
  const winoptAdminToken = generic?.data.winoptAdminToken;
  const privacySettings: PrivacySetting[] = generic?.data.privacySettings ?? [];
  const privacyChanges = privacySettings.flatMap((setting) => {
    const draft = privacyDraft[setting.id];
    if (draft == null || setting.current === draft) {
      return [] as { id: string; value: string }[];
    }
    return [{ id: setting.id, value: draft }];
  });
  const privacyUndo = generic?.data.privacyUndo ?? [];

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
      {statsError && (
        <Alert color="red" icon={<IconAlertCircle />}>
          工具运行统计读取失败：{statsError}
        </Alert>
      )}
      {toolboxStats && (
        <Card className="glass-card" radius="lg">
          <Group justify="space-between" align="flex-start" wrap="wrap">
            <Box>
              <Group gap="xs">
                <Text size="xs" c="dimmed" fw={800}>
                  工具运行统计
                </Text>
                <Badge
                  size="xs"
                  variant="light"
                  color={
                    toolboxStats.verdict.status === "pass"
                      ? "green"
                      : toolboxStats.verdict.status === "warn"
                        ? "red"
                        : "gray"
                  }
                >
                  {toolboxStats.verdict.status === "pass"
                    ? "达标"
                    : toolboxStats.verdict.status === "warn"
                      ? "未达标"
                      : "样本不足"}
                </Badge>
              </Group>
              <Group gap="xl" mt={8}>
                <Box>
                  <Text size="xs" c="dimmed">
                    握手成功率
                  </Text>
                  <Text fw={800} size="lg">
                    {toolboxStats.handshakeSuccessPercent === null
                      ? "暂无样本"
                      : `${toolboxStats.handshakeSuccessPercent.toFixed(2)}%`}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {toolboxStats.handshakes} / {toolboxStats.handshakeAttempts} 次 · 目标 ≥ 99%
                  </Text>
                </Box>
                <Box>
                  <Text size="xs" c="dimmed">
                    超时率
                  </Text>
                  <Text fw={800} size="lg">
                    {toolboxStats.timeoutPercent === null
                      ? "暂无样本"
                      : `${toolboxStats.timeoutPercent.toFixed(2)}%`}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {toolboxStats.timeouts} / {toolboxStats.attempts} 次 · 上限 &lt; 1%
                  </Text>
                </Box>
                <Box>
                  <Text size="xs" c="dimmed">
                    耗时 P95
                  </Text>
                  <Text fw={800} size="lg">
                    {toolboxStats.durationP95Ms === null
                      ? "暂无样本"
                      : `${toolboxStats.durationP95Ms} ms`}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {toolboxStats.toolCount} 个工具 · 共 {toolboxStats.attempts} 次执行
                  </Text>
                </Box>
              </Group>
              <Text size="xs" c="dimmed" mt={6}>
                {toolboxStats.verdict.summary}
              </Text>
            </Box>
            <Button
              size="xs"
              variant="subtle"
              onClick={() => setStatsDetail((current) => !current)}
            >
              {statsDetail ? "收起明细" : "查看明细"}
            </Button>
          </Group>
          {toolboxStats.verdict.status === "warn" && (
            <Alert color="red" icon={<IconAlertCircle />} mt="sm" title="有工具没达到验收线">
              <Stack gap={4}>
                {toolboxStats.tools
                  .filter((tool) => tool.status === "degraded")
                  .map((tool) => (
                    <Text size="xs" key={tool.toolId}>
                      <b>{tool.toolId}</b>：{tool.issues.join("；")}
                    </Text>
                  ))}
              </Stack>
            </Alert>
          )}
          {toolboxStats.verdict.status === "unknown" && (
            <Alert color="gray" mt="sm">
              执行次数还不够，暂时无法判定是否达标：每个工具至少执行 5 次才会参与判定。
            </Alert>
          )}
          {statsDetail && (
            <>
              {toolboxStats.tools.length === 0 ? (
                <Alert color="gray" mt="md">
                  还没有工具执行记录。跑一次工具后这里会出现逐工具明细。
                </Alert>
              ) : (
                <ScrollArea type="auto" mt="md">
                  <Table miw={760} highlightOnHover verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>工具</Table.Th>
                        <Table.Th>状态</Table.Th>
                        <Table.Th>执行</Table.Th>
                        <Table.Th>成功</Table.Th>
                        <Table.Th>超时</Table.Th>
                        <Table.Th>失败</Table.Th>
                        <Table.Th>握手成功率</Table.Th>
                        <Table.Th>P95</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {toolboxStats.tools.map((tool) => (
                        <Table.Tr key={tool.toolId}>
                          <Table.Td>
                            <Text size="xs">{tool.toolId}</Text>
                            {tool.issues.length > 0 && (
                              <Text size="xs" c="dimmed">
                                {tool.issues.join("；")}
                              </Text>
                            )}
                          </Table.Td>
                          <Table.Td>
                            <Badge
                              size="xs"
                              variant="light"
                              color={
                                tool.status === "ok"
                                  ? "green"
                                  : tool.status === "degraded"
                                    ? "red"
                                    : "gray"
                              }
                            >
                              {tool.status === "ok"
                                ? "达标"
                                : tool.status === "degraded"
                                  ? "未达标"
                                  : "样本不足"}
                            </Badge>
                          </Table.Td>
                          <Table.Td>{tool.attempts}</Table.Td>
                          <Table.Td>{tool.completed}</Table.Td>
                          <Table.Td>{tool.timeouts}</Table.Td>
                          <Table.Td>{tool.failed + tool.cancelled}</Table.Td>
                          <Table.Td>
                            {tool.handshakeSuccessPercent === null
                              ? "暂无样本"
                              : `${tool.handshakeSuccessPercent.toFixed(1)}%`}
                          </Table.Td>
                          <Table.Td>
                            {tool.durationP95Ms === null
                              ? "—"
                              : `${tool.durationP95Ms} ms`}
                          </Table.Td>
                        </Table.Tr>
                      ))}
                    </Table.Tbody>
                  </Table>
                </ScrollArea>
              )}
              <Text size="xs" c="dimmed" mt="sm">
                统计只记录本机工具执行，不上传任何数据；样本为 0 时显示「暂无样本」，不会用 100% 冒充成功。验收线为握手成功率 ≥ 99%、超时率 &lt; 1%，每个工具至少执行 5 次才参与判定。
              </Text>
            </>
          )}
        </Card>
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
            {tool.id === "desktop_quick" && onOpenDesktopQuick && (
              <Button
                size="xs"
                mt="md"
                variant="light"
                onClick={onOpenDesktopQuick}
              >
                打开桌面助手
              </Button>
            )}
            {tool.id === "start_menu_pro" && onOpenStartMenu && (
              <Button
                size="xs"
                mt="md"
                variant="light"
                onClick={onOpenStartMenu}
              >
                打开快捷启动
              </Button>
            )}
            {tool.id === "eye_care" && onOpenEyeCare && (
              <Button size="xs" mt="md" variant="light" onClick={onOpenEyeCare}>
                打开护眼设置
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
          {rowPathError && (
            <Alert color="red" icon={<IconAlertCircle />}>
              {rowPathError}
            </Alert>
          )}
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
          {(generic.data.diff ?? []).length > 0 && (
            <CleanupDiffTable rows={generic.data.diff ?? []} />
          )}
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
                  {canOpenRowPath(generic.tool.id) && (
                    <Table.Th>操作</Table.Th>
                  )}
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
                    {canOpenRowPath(generic.tool.id) && (
                      <Table.Td>
                        <Button
                          size="compact-xs"
                          variant="subtle"
                          leftSection={<IconFolder size={14} />}
                          disabled={
                            !String(
                              (row as Record<string, unknown>).path ?? "",
                            )
                          }
                          onClick={() =>
                            void openRowPath(
                              String(
                                (row as Record<string, unknown>).path ?? "",
                              ),
                            )
                          }
                        >
                          打开目录
                        </Button>
                      </Table.Td>
                    )}
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
                generic.tool.id !== "omni_zip" &&
                generic.tool.id !== "master_pdf" &&
                generic.tool.id !== "winopt_assistant" &&
                generic.tool.id !== "DuplicateFileClean" &&
                generic.tool.id !== "privacy_protection" &&
                generic.tool.id !== "proc_opt" &&
                generic.tool.id !== "defrag" &&
                generic.tool.id !== "adblock_master" &&
                generic.tool.id !== "bsod_repair" &&
                generic.tool.id !== "sys_clean" &&
                generic.tool.id !== "c_expand" &&
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
          {generic.tool.id === "DuplicateFileClean" &&
            (generic.data.duplicateFileGroups?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  重复组（选择每组要保留的文件）
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  删除前会做完整内容哈希复核；文件移入回收站可还原
                </Text>
                <Stack gap="sm" mt="sm">
                  {(generic.data.duplicateFileGroups ?? []).map(
                    (group, index) => {
                      const keep = dupKeep[group.id] ?? group.files[0] ?? "";
                      return (
                        <Card key={group.id} withBorder padding="sm">
                          <Group justify="space-between" align="center">
                            <Text size="sm" fw={600}>
                              组 {index + 1} · 单个 {group.size} · 可释放{" "}
                              {group.reclaim}
                            </Text>
                            <Button
                              size="xs"
                              color="orange"
                              variant="light"
                              loading={
                                applying ===
                                `${generic.tool.id}:clean_group`
                              }
                              onClick={() =>
                                void runApply(
                                  generic.tool,
                                  {
                                    id: "clean_group",
                                    label: "清理该组其余文件",
                                    write: true,
                                  },
                                  { groupId: group.id, keepPath: keep },
                                )
                              }
                            >
                              清理其余 {group.files.length - 1} 个
                            </Button>
                          </Group>
                          <Stack gap={4} mt="xs">
                            {group.files.map((file) => (
                              <Group key={file} gap="xs" wrap="nowrap">
                                <Radio
                                  value={file}
                                  checked={keep === file}
                                  onChange={() =>
                                    setDupKeep((current) => ({
                                      ...current,
                                      [group.id]: file,
                                    }))
                                  }
                                />
                                <Text
                                  size="xs"
                                  style={{ wordBreak: "break-all" }}
                                >
                                  {file}
                                </Text>
                              </Group>
                            ))}
                          </Stack>
                        </Card>
                      );
                    },
                  )}
                </Stack>
              </Box>
            )}
          {generic.tool.id === "clean_master" &&
            (generic.data.rows ?? []).some(
              (row) => typeof row.id === "string",
            ) && (
              <Box mt="md">
                <Text size="sm" fw={600}>
                  勾选要移入回收站的位置
                </Text>
                <Group gap="xs" mt="xs">
                  <Button
                    size="xs"
                    variant="subtle"
                    onClick={() =>
                      setSelection(
                        (generic.data.rows ?? [])
                          .filter(
                            (row) =>
                              row.cleanable !== false &&
                              row.status === "可用" &&
                              Number(row.files) > 0 &&
                              row.requiresAdmin !== true,
                          )
                          .map((row) => String(row.id)),
                      )
                    }
                  >
                    全选可清理
                  </Button>
                  <Button
                    size="xs"
                    variant="subtle"
                    onClick={() =>
                      setSelection(
                        (generic.data.rows ?? [])
                          .filter(
                            (row) =>
                              [
                                "user-temp",
                                "windows-temp",
                                "thumbnail-cache",
                                "crash-dumps",
                                "wer-user",
                                "edge-cache",
                              ].includes(String(row.id)) &&
                              row.cleanable !== false &&
                              row.status === "可用" &&
                              Number(row.files) > 0,
                          )
                          .map((row) => String(row.id)),
                      )
                    }
                  >
                    仅缓存/临时
                  </Button>
                  <Button
                    size="xs"
                    variant="subtle"
                    onClick={() => setSelection([])}
                  >
                    清空
                  </Button>
                </Group>
                <Stack gap={6} mt="xs">
                  {(generic.data.rows ?? []).map((row) => {
                    const id = String(row.id ?? "");
                    const statOnly = row.cleanable === false;
                    const disabled =
                      statOnly || row.status === "不存在" || Number(row.files) === 0;
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
                        }${statOnly ? " · 仅统计" : ""}${
                          disabled && !statOnly ? ` · ${row.status}` : ""
                        }`}
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
          {generic.tool.id === "clean_master" &&
            typeof generic.data.journalPath === "string" && (
              <Group mt="md" gap="xs" align="center">
                <Button
                  size="xs"
                  variant="light"
                  onClick={() => void openCleanupRecycleBin()}
                >
                  打开回收站还原
                </Button>
                {typeof generic.data.journalDir === "string" && (
                  <Button
                    size="xs"
                    variant="light"
                    onClick={() =>
                      void openCleanupJournal(String(generic.data.journalDir))
                    }
                  >
                    打开清单目录
                  </Button>
                )}
                <Text size="xs" c="dimmed" truncate maw={420}>
                  清单：{generic.data.journalPath}
                </Text>
              </Group>
            )}
          {generic.tool.id === "lockscreen" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                定时锁定
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                倒计时结束后自动锁定；点“取消”或关闭页面不会锁定
              </Text>
              <Group mt="xs" align="flex-end">
                <NumberInput
                  size="xs"
                  w={120}
                  min={1}
                  max={720}
                  label="延迟（分钟）"
                  value={lockDelay}
                  onChange={(value) =>
                    setLockDelay(
                      typeof value === "number" ? value : Number(value) || 1,
                    )
                  }
                />
                <Button size="xs" onClick={startLockTimer}>
                  开始倒计时
                </Button>
                <Button
                  size="xs"
                  variant="light"
                  disabled={lockRemaining <= 0}
                  onClick={cancelLockTimer}
                >
                  取消
                </Button>
                <Text size="sm" ff="monospace">
                  {String(Math.floor(lockRemaining / 60)).padStart(2, "0")}:
                  {String(lockRemaining % 60).padStart(2, "0")}
                </Text>
              </Group>
            </Box>
          )}
          {generic.tool.id === "software_pm" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                批量回滚点
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                保存当前第三方服务启动类型与非 Microsoft 计划任务状态；恢复时一次 UAC 逐项写回
              </Text>
              <Group mt="xs">
                <Button
                  size="xs"
                  variant="outline"
                  loading={applying === `${generic.tool.id}:save_rollback`}
                  onClick={() => void saveSoftwareRollback(generic.tool)}
                >
                  保存回滚点
                </Button>
                <Button
                  size="xs"
                  color="red"
                  variant="light"
                  disabled={!generic.data.rollbackToken}
                  loading={applying === `${generic.tool.id}:restore_rollback`}
                  onClick={() => void restoreSoftwareRollback(generic.tool)}
                >
                  恢复最近回滚点
                </Button>
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
                {(generic.data.startupItems ?? []).some(
                  (item) => !item.enabled,
                ) && (
                  <Button
                    size="xs"
                    mt="xs"
                    color="teal"
                    variant="light"
                    loading={applying === `${generic.tool.id}:restore_all`}
                    onClick={() => void restoreAllStartups(generic.tool)}
                  >
                    恢复全部停用项
                  </Button>
                )}
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
                {(generic.data.systemStartupItems?.length ?? 0) > 0 && (
                  <Box mt="lg">
                    <Text size="sm" fw={700}>
                      系统级启动项（HKLM）
                    </Text>
                    <Text size="xs" c="dimmed" mt={4}>
                      启停需要管理员授权（UAC），由独立 lens_helper.exe 执行
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
                          {(generic.data.systemStartupItems ?? []).map((item) => (
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
                                    `${generic.tool.id}:toggle_system_startup`
                                  }
                                  onClick={() =>
                                    void toggleSystemStartup(generic.tool, item)
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
                  </Box>
                )}
                {(generic.data.serviceItems?.length ?? 0) > 0 && (
                  <Box mt="lg">
                    <Text size="sm" fw={700}>
                      第三方服务启动类型
                    </Text>
                    <Text size="xs" c="dimmed" mt={4}>
                      修改需要管理员授权，重启后完全生效；不会停止正在运行的服务
                    </Text>
                    <ScrollArea type="auto" mt="xs">
                      <Table miw={820} verticalSpacing="xs">
                        <Table.Thead>
                          <Table.Tr>
                            <Table.Th>服务</Table.Th>
                            <Table.Th>当前</Table.Th>
                            <Table.Th>改为</Table.Th>
                          </Table.Tr>
                        </Table.Thead>
                        <Table.Tbody>
                          {(generic.data.serviceItems ?? []).map((item) => (
                            <Table.Tr key={item.name}>
                              <Table.Td>
                                <Text size="sm" fw={600}>
                                  {item.display}
                                </Text>
                                <Text size="xs" c="dimmed">
                                  {item.name}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Badge size="xs" variant="light">
                                  {item.startLabel}
                                </Badge>
                              </Table.Td>
                              <Table.Td>
                                <Group gap={4}>
                                  {[2, 3, 4].map((value) => (
                                    <Button
                                      key={value}
                                      size="xs"
                                      variant="light"
                                      color={
                                        value === 2
                                          ? "teal"
                                          : value === 3
                                            ? "blue"
                                            : "orange"
                                      }
                                      disabled={item.start === value}
                                      loading={
                                        applying ===
                                        `${generic.tool.id}:set_service_start`
                                      }
                                      onClick={() =>
                                        void setServiceStart(
                                          generic.tool,
                                          item,
                                          value,
                                        )
                                      }
                                    >
                                      {value === 2
                                        ? "自动"
                                        : value === 3
                                          ? "手动"
                                          : "禁用"}
                                    </Button>
                                  ))}
                                </Group>
                              </Table.Td>
                            </Table.Tr>
                          ))}
                        </Table.Tbody>
                      </Table>
                    </ScrollArea>
                  </Box>
                )}
                {(generic.data.taskItems?.length ?? 0) > 0 && (
                  <Box mt="lg">
                    <Text size="sm" fw={700}>
                      计划任务（非 Microsoft）
                    </Text>
                    <Text size="xs" c="dimmed" mt={4}>
                      启用/禁用需要管理员授权（UAC），由独立助手调用系统 schtasks 执行
                    </Text>
                    <ScrollArea type="auto" mt="xs">
                      <Table miw={820} verticalSpacing="xs">
                        <Table.Thead>
                          <Table.Tr>
                            <Table.Th>任务</Table.Th>
                            <Table.Th>路径</Table.Th>
                            <Table.Th>状态</Table.Th>
                            <Table.Th>操作</Table.Th>
                          </Table.Tr>
                        </Table.Thead>
                        <Table.Tbody>
                          {(generic.data.taskItems ?? []).map((item) => (
                            <Table.Tr key={item.path}>
                              <Table.Td>
                                <Text size="sm" fw={600}>
                                  {item.name}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Text size="xs" c="dimmed">
                                  {item.path}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Badge
                                  size="xs"
                                  variant="light"
                                  color={item.enabled ? "teal" : "gray"}
                                >
                                  {item.state}
                                </Badge>
                              </Table.Td>
                              <Table.Td>
                                <Button
                                  size="xs"
                                  variant="light"
                                  color={item.enabled ? "orange" : "teal"}
                                  loading={
                                    applying ===
                                    `${generic.tool.id}:set_task_enabled`
                                  }
                                  onClick={() =>
                                    void setScheduledTaskEnabled(
                                      generic.tool,
                                      item,
                                    )
                                  }
                                >
                                  {item.enabled ? "禁用" : "启用"}
                                </Button>
                              </Table.Td>
                            </Table.Tr>
                          ))}
                        </Table.Tbody>
                      </Table>
                    </ScrollArea>
                  </Box>
                )}
              </Box>
            )}
          {generic.tool.id === "driver_manager" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                驱动安装与回滚
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                本工具只做检测；安装、更新与回滚在 Windows 官方界面完成
              </Text>
              <Group mt="xs">
                <Button
                  size="xs"
                  variant="light"
                  loading={applying === `${generic.tool.id}:open_official`}
                  onClick={() =>
                    void openDriverTarget(generic.tool, "device_manager")
                  }
                >
                  打开设备管理器
                </Button>
                <Button
                  size="xs"
                  variant="light"
                  onClick={() =>
                    void openDriverTarget(generic.tool, "windows_update")
                  }
                >
                  Windows 更新
                </Button>
                <Button
                  size="xs"
                  variant="light"
                  onClick={() =>
                    void openDriverTarget(generic.tool, "optional_updates")
                  }
                >
                  可选更新
                </Button>
              </Group>
            </Box>
          )}
          {generic.tool.id === "c_expand" &&
            (generic.data.expandItems?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  扩容（仅向右扩展相邻未分配空间）
                </Text>
                <Alert color="red" mt="xs" icon={<IconAlertCircle />}>
                  分区操作有数据丢失风险。请先备份重要数据、确认电源稳定，并保留恢复介质。
                </Alert>
                <Stack gap="sm" mt="sm">
                  {(generic.data.expandItems ?? []).map(
                    (item: ExpandItem) => (
                      <Card key={item.drive} withBorder padding="sm">
                        <Text size="sm" fw={600}>
                          {item.drive}: 当前 {item.size} · 右侧未分配{" "}
                          {item.gap} · 最大可扩至{" "}
                          {Math.floor(item.maxBytes / 1024 ** 3)} GB
                        </Text>
                        <Text size="xs" c="dimmed" mt={2}>
                          文件系统 {item.filesystem}；不移动数据、不跨盘、不缩小
                        </Text>
                        <Checkbox
                          mt="xs"
                          checked={!!expandAck[item.drive]}
                          onChange={(event) =>
                            setExpandAck((current) => ({
                              ...current,
                              [item.drive]: event.currentTarget.checked,
                            }))
                          }
                          label="我已备份重要数据，并确认电源稳定"
                        />
                        <Button
                          mt="xs"
                          size="xs"
                          color="red"
                          variant="light"
                          disabled={!expandAck[item.drive]}
                          loading={
                            applying === `${generic.tool.id}:expand_volume`
                          }
                          onClick={() =>
                            void runApply(
                              generic.tool,
                              {
                                id: "expand_volume",
                                label: `扩展 ${item.drive}:`,
                                write: true,
                              },
                              {
                                drive: item.drive,
                                targetBytes: item.maxBytes,
                              },
                            )
                          }
                        >
                          扩展至最大
                        </Button>
                      </Card>
                    ),
                  )}
                </Stack>
              </Box>
            )}
          {generic.tool.id === "dll_repair" &&
            (generic.data.runtimeItems?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  VC++ / .NET 运行库
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  打开微软官方下载页手动安装；本工具不自动下载或安装
                </Text>
                <Table miw={760} mt="xs" verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>运行库</Table.Th>
                      <Table.Th>状态</Table.Th>
                      <Table.Th>版本</Table.Th>
                      <Table.Th>操作</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {(generic.data.runtimeItems ?? []).map((item) => (
                      <Table.Tr key={item.id}>
                        <Table.Td>
                          <Text size="sm" fw={600}>
                            {item.label}
                          </Text>
                          <Text size="xs" c="dimmed">
                            {item.note}
                          </Text>
                        </Table.Td>
                        <Table.Td>
                          <Badge
                            size="xs"
                            variant="light"
                            color={item.installed ? "teal" : "orange"}
                          >
                            {item.installed ? "已安装" : "未检测到"}
                          </Badge>
                        </Table.Td>
                        <Table.Td>{item.version}</Table.Td>
                        <Table.Td>
                          <Button
                            size="xs"
                            variant="light"
                            loading={
                              applying === `${generic.tool.id}:open_official`
                            }
                            onClick={() =>
                              void openOfficialRuntime(generic.tool, item.id)
                            }
                          >
                            打开官方下载页
                          </Button>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </Box>
            )}
          {generic.tool.id === "sys_clean" &&
            (generic.data.appxItems ?? []).some((item) => item.removable) && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  白名单可移除应用
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  仅影响当前用户，移除后可从 Microsoft Store 重装
                </Text>
                <Button
                  mt="xs"
                  size="xs"
                  variant="outline"
                  loading={applying === `${generic.tool.id}:export_list`}
                  onClick={() => void exportAppxList(generic.tool)}
                >
                  导出可移除清单
                </Button>
                <Button
                  mt="xs"
                  ml="xs"
                  size="xs"
                  variant="outline"
                  color="blue"
                  disabled={generic.data.restorePointEnabled === false}
                  loading={
                    applying === `${generic.tool.id}:create_restore_point`
                  }
                  onClick={() => void createRestorePoint(generic.tool)}
                >
                  创建系统还原点
                </Button>
                {generic.data.restorePointEnabled === false && (
                  <Text size="xs" c="orange" mt={4}>
                    系统保护已关闭，无法创建还原点；请先在系统属性中启用。
                  </Text>
                )}
                <ScrollArea type="auto" mt="xs">
                  <Table miw={760} verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>应用</Table.Th>
                        <Table.Th>版本</Table.Th>
                        <Table.Th>操作</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {(generic.data.appxItems ?? [])
                        .filter((item) => item.removable)
                        .map((item) => (
                          <Table.Tr key={item.fullName}>
                            <Table.Td>
                              <Text size="sm" fw={600}>
                                {item.name}
                              </Text>
                              <Text size="xs" c="dimmed">
                                {item.fullName}
                              </Text>
                            </Table.Td>
                            <Table.Td>{item.version}</Table.Td>
                            <Table.Td>
                              <Button
                                size="xs"
                                color="red"
                                variant="light"
                                loading={
                                  applying === `${generic.tool.id}:remove_appx`
                                }
                                onClick={() =>
                                  void removeAppx(generic.tool, item)
                                }
                              >
                                移除
                              </Button>
                            </Table.Td>
                          </Table.Tr>
                        ))}
                    </Table.Tbody>
                  </Table>
                </ScrollArea>
              </Box>
            )}
          {generic.tool.id === "bsod_repair" && generic.data.dumpMode && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                崩溃转储设置
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                当前：{generic.data.dumpMode.label}；切换需要管理员授权（UAC）
              </Text>
              <Group mt="xs" align="center">
                <Select
                  size="xs"
                  w={200}
                  data={[
                    { value: "automatic", label: "自动内存转储" },
                    { value: "kernel", label: "内核内存转储" },
                    { value: "small", label: "小内存转储" },
                    { value: "none", label: "不转储" },
                  ]}
                  value={dumpDraft}
                  allowDeselect={false}
                  onChange={(next) => setDumpDraft(next ?? "")}
                />
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  disabled={
                    !dumpDraft || dumpDraft === generic.data.dumpMode.mode
                  }
                  loading={applying === `${generic.tool.id}:set_dump_mode`}
                  onClick={() =>
                    void runApply(
                      generic.tool,
                      {
                        id: "set_dump_mode",
                        label: "切换崩溃转储模式",
                        write: true,
                      },
                      { mode: dumpDraft },
                    )
                  }
                >
                  应用转储模式
                </Button>
              </Group>
            </Box>
          )}
          {generic.tool.id === "adblock_master" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                SystemLens Hosts 规则
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                每行一个域名；写入需要管理员授权，自动备份，只改标记之间的内容
              </Text>
              <Group mt="xs" align="flex-end">
                <TextInput
                  size="xs"
                  w={360}
                  label="规则文件（纯域名或 hosts 格式）"
                  placeholder="C:\\rules.txt"
                  value={adblockFile}
                  onChange={(event) =>
                    setAdblockFile(event.currentTarget.value)
                  }
                />
                <Button
                  size="xs"
                  variant="light"
                  loading={applying === `${generic.tool.id}:import_rules`}
                  onClick={() => void importAdblockRules(generic.tool)}
                >
                  导入规则文件
                </Button>
              </Group>
              <Textarea
                mt="xs"
                minRows={4}
                placeholder={"ads.example.com\ntracker.example.com"}
                value={adblockDomains}
                onChange={(event) =>
                  setAdblockDomains(event.currentTarget.value)
                }
              />
              <Group mt="sm" align="center">
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() =>
                    setAdblockDomains(
                      (generic.data.builtinDomains ?? []).join("\n"),
                    )
                  }
                >
                  载入内置规则（{generic.data.builtinDomains?.length ?? 0}）
                </Button>
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  loading={applying === `${generic.tool.id}:apply_hosts`}
                  onClick={() => void applyAdblockHosts(generic.tool)}
                >
                  写入 Hosts
                </Button>
                <Button
                  size="xs"
                  variant="light"
                  color="red"
                  loading={applying === `${generic.tool.id}:restore_hosts`}
                  disabled={
                    !generic.data.hostsManagedDomains ||
                    generic.data.hostsManagedDomains === 0
                  }
                  onClick={() => void restoreAdblockHosts(generic.tool)}
                >
                  移除 SystemLens 规则
                </Button>
                <Text size="xs" c="dimmed">
                  当前管理段：{generic.data.hostsManagedDomains ?? 0} 条
                </Text>
              </Group>
            </Box>
          )}
          {generic.tool.id === "defrag" &&
            (generic.data.volumeItems?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  磁盘操作
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  分析/优化/TRIM 需要管理员授权；SSD 只允许分析与 TRIM
                </Text>
                <Table miw={760} mt="xs" verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>卷</Table.Th>
                      <Table.Th>介质</Table.Th>
                      <Table.Th>容量</Table.Th>
                      <Table.Th>可用</Table.Th>
                      <Table.Th>操作</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {(generic.data.volumeItems ?? []).map((volume) => (
                      <Table.Tr key={volume.drive}>
                        <Table.Td>
                          <Text size="sm" fw={600}>
                            {volume.name}
                          </Text>
                        </Table.Td>
                        <Table.Td>
                          <Badge size="xs" variant="light">
                            {volume.media}
                          </Badge>
                        </Table.Td>
                        <Table.Td>{volume.size}</Table.Td>
                        <Table.Td>{volume.free}</Table.Td>
                        <Table.Td>
                          <Group gap={4}>
                            <Button
                              size="xs"
                              variant="light"
                              loading={
                                applying ===
                                `${generic.tool.id}:optimize_disk`
                              }
                              onClick={() =>
                                void runApply(
                                  generic.tool,
                                  {
                                    id: "optimize_disk",
                                    label: `分析 ${volume.name}`,
                                    write: true,
                                  },
                                  { drive: volume.drive, mode: "analyze" },
                                )
                              }
                            >
                              分析
                            </Button>
                            <Button
                              size="xs"
                              variant="light"
                              color="teal"
                              disabled={!volume.canOptimize}
                              onClick={() =>
                                void runApply(
                                  generic.tool,
                                  {
                                    id: "optimize_disk",
                                    label: `优化 ${volume.name}`,
                                    write: true,
                                  },
                                  { drive: volume.drive, mode: "optimize" },
                                )
                              }
                            >
                              优化
                            </Button>
                            <Button
                              size="xs"
                              variant="light"
                              color="blue"
                              disabled={!volume.canTrim}
                              onClick={() =>
                                void runApply(
                                  generic.tool,
                                  {
                                    id: "optimize_disk",
                                    label: `TRIM ${volume.name}`,
                                    write: true,
                                  },
                                  { drive: volume.drive, mode: "trim" },
                                )
                              }
                            >
                              TRIM
                            </Button>
                          </Group>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </Box>
            )}
          {generic.tool.id === "sys_boost" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                一键加速
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                视觉性能优先 + 关闭窗口动画/透明 + 高性能电源计划；执行前自动保存快照
              </Text>
              <Group mt="xs">
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  loading={applying === `${generic.tool.id}:boost`}
                  onClick={() => void runSysBoost(generic.tool, "boost")}
                >
                  一键加速
                </Button>
                <Button
                  size="xs"
                  variant="outline"
                  loading={applying === `${generic.tool.id}:undo_boost`}
                  onClick={() => void runSysBoost(generic.tool, "undo_boost")}
                >
                  撤销加速
                </Button>
              </Group>
            </Box>
          )}
          {generic.tool.id === "gpu_opt" && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                按应用 GPU 偏好
              </Text>
              <Text size="xs" c="dimmed" mt={4}>
                只写当前用户 HKCU\UserGpuPreferences；选“由 Windows 决定”会删除该应用的值
              </Text>
              <Group mt="xs" align="flex-end">
                <TextInput
                  size="xs"
                  w={360}
                  label="应用 .exe 绝对路径"
                  placeholder="C:\\Games\\game.exe"
                  value={gpuPath}
                  onChange={(event) => setGpuPath(event.currentTarget.value)}
                />
                <Select
                  size="xs"
                  w={170}
                  label="偏好"
                  data={[
                    { value: "0", label: "由 Windows 决定" },
                    { value: "1", label: "省电" },
                    { value: "2", label: "高性能" },
                  ]}
                  value={String(gpuPref)}
                  allowDeselect={false}
                  onChange={(next) => setGpuPref(Number(next ?? 0))}
                />
                <Button
                  size="xs"
                  loading={applying === `${generic.tool.id}:set_gpu_preference`}
                  onClick={() =>
                    void setGpuPreference(generic.tool, gpuPath, gpuPref)
                  }
                >
                  应用
                </Button>
              </Group>
              {(generic.data.gpuPreferences?.length ?? 0) > 0 && (
                <Table miw={760} mt="xs" verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>应用</Table.Th>
                      <Table.Th>偏好</Table.Th>
                      <Table.Th>操作</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {(generic.data.gpuPreferences ?? []).map((item) => (
                      <Table.Tr key={item.path}>
                        <Table.Td>
                          <Text size="xs">{item.path}</Text>
                        </Table.Td>
                        <Table.Td>{item.label}</Table.Td>
                        <Table.Td>
                          <Button
                            size="xs"
                            variant="light"
                            onClick={() =>
                              void setGpuPreference(generic.tool, item.path, 0)
                            }
                          >
                            恢复默认
                          </Button>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              )}
            </Box>
          )}
          {generic.tool.id === "ai_cooling" && (
            <Box mt="md">
              <Group align="center">
                <Button
                  size="xs"
                  variant="light"
                  loading={applying === `${generic.tool.id}:read_elevated`}
                  onClick={() => void readElevatedSensors(generic.tool)}
                >
                  读取受限传感器（需管理员）
                </Button>
                <Text size="xs" c="dimmed">
                  ACPI 风扇/探针、磁盘 SMART；只读
                </Text>
              </Group>
              <Group mt="sm" align="center">
                <Text size="xs" fw={600}>
                  处理器上限（软件降温，非风扇控制）
                </Text>
                {[60, 80, 100].map((percent) => (
                  <Button
                    key={percent}
                    size="xs"
                    variant="outline"
                    loading={applying === `${generic.tool.id}:set_power_limit`}
                    onClick={() => void setAiPowerLimit(generic.tool, percent)}
                  >
                    {percent}%
                  </Button>
                ))}
              </Group>
            </Box>
          )}
          {generic.tool.id === "proc_opt" &&
            (generic.data.processItems?.length ?? 0) > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  进程操作
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  优先级只影响本次运行；结束进程不可撤销，系统关键进程已禁用
                </Text>
                <ScrollArea type="auto" mt="xs">
                  <Table miw={900} verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>进程</Table.Th>
                        <Table.Th>PID</Table.Th>
                        <Table.Th>CPU</Table.Th>
                        <Table.Th>内存</Table.Th>
                        <Table.Th>优先级</Table.Th>
                        <Table.Th>操作</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {(generic.data.processItems ?? []).map((item) => {
                        const priority =
                          procPriority[String(item.pid)] ?? item.priority;
                        return (
                          <Table.Tr key={`${item.pid}:${item.startTime}`}>
                            <Table.Td>
                              <Text size="sm" fw={600}>
                                {item.name}
                              </Text>
                            </Table.Td>
                            <Table.Td>
                              <Text size="xs" c="dimmed">
                                {item.pid}
                              </Text>
                            </Table.Td>
                            <Table.Td>{item.cpu}</Table.Td>
                            <Table.Td>{item.memory}</Table.Td>
                            <Table.Td>
                              <Select
                                size="xs"
                                w={120}
                                disabled={!item.canEnd}
                                data={[
                                  { value: "idle", label: "空闲" },
                                  { value: "below_normal", label: "低于正常" },
                                  { value: "normal", label: "正常" },
                                  { value: "above_normal", label: "高于正常" },
                                  { value: "high", label: "高" },
                                ]}
                                value={priority}
                                allowDeselect={false}
                                onChange={(next) => {
                                  if (next == null) return;
                                  setProcPriority((current) => ({
                                    ...current,
                                    [String(item.pid)]: next,
                                  }));
                                }}
                              />
                            </Table.Td>
                            <Table.Td>
                              <Group gap={4} wrap="nowrap">
                                <Button
                                  size="xs"
                                  variant="light"
                                  disabled={
                                    !item.canEnd || priority === item.priority
                                  }
                                  loading={
                                    applying ===
                                    `${generic.tool.id}:set_priority`
                                  }
                                  onClick={() =>
                                    void applyProcessPriority(
                                      generic.tool,
                                      item,
                                    )
                                  }
                                >
                                  应用优先级
                                </Button>
                                <Button
                                  size="xs"
                                  color="red"
                                  variant="light"
                                  disabled={!item.canEnd}
                                  loading={
                                    applying === `${generic.tool.id}:end_process`
                                  }
                                  onClick={() =>
                                    void endProcess(generic.tool, item)
                                  }
                                >
                                  结束进程
                                </Button>
                              </Group>
                            </Table.Td>
                          </Table.Tr>
                        );
                      })}
                    </Table.Tbody>
                  </Table>
                </ScrollArea>
              </Box>
            )}
          {generic.tool.id === "privacy_protection" &&
            privacySettings.length > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  可应用的隐私设置
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  仅写入 HKCU；写入后立即回读，可单项应用或一键撤销
                </Text>
                <Table miw={720} mt="xs" verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>设置项</Table.Th>
                      <Table.Th>当前</Table.Th>
                      <Table.Th>修改为</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {privacySettings.map((setting) => {
                      const currentLabel =
                        setting.current == null
                          ? "未记录"
                          : setting.options.find(
                              (option) => option.value === setting.current,
                            )?.label ?? setting.current;
                      const value =
                        privacyDraft[setting.id] ??
                        setting.current ??
                        setting.options[0]?.value ??
                        "";
                      return (
                        <Table.Tr key={setting.id}>
                          <Table.Td>
                            <Text size="sm" fw={600}>
                              {setting.label}
                            </Text>
                          </Table.Td>
                          <Table.Td>
                            <Badge size="xs" variant="light">
                              {currentLabel}
                            </Badge>
                          </Table.Td>
                          <Table.Td>
                            <Select
                              size="xs"
                              w={150}
                              data={setting.options.map((option) => ({
                                value: option.value,
                                label: option.label,
                              }))}
                              value={value}
                              allowDeselect={false}
                              onChange={(next) =>
                                updatePrivacyValue(setting.id, next)
                              }
                            />
                          </Table.Td>
                        </Table.Tr>
                      );
                    })}
                  </Table.Tbody>
                </Table>
                <Group mt="sm" align="center">
                  <Button
                    size="xs"
                    color="orange"
                    variant="light"
                    disabled={privacyChanges.length === 0}
                    loading={applying === `${generic.tool.id}:apply_settings`}
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply_settings",
                          label: "应用隐私设置",
                          write: true,
                        },
                        { changes: privacyChanges },
                      )
                    }
                  >
                    应用所选设置
                  </Button>
                  {privacyUndo.length > 0 && (
                    <Button
                      size="xs"
                      color="red"
                      variant="light"
                      loading={applying === `${generic.tool.id}:apply_settings`}
                      onClick={() =>
                        void runApply(
                          generic.tool,
                          {
                            id: "apply_settings",
                            label: "撤销本次修改",
                            write: true,
                          },
                          { changes: privacyUndo },
                        )
                      }
                    >
                      撤销本次修改
                    </Button>
                  )}
                  <Text size="xs" c="dimmed">
                    {privacyChanges.length > 0
                      ? `将修改 ${privacyChanges.length} 项；原值写入本地快照`
                      : "修改选项后可应用"}
                  </Text>
                </Group>
              </Box>
            )}
          {generic.tool.id === "winopt_assistant" &&
            winoptSettings.length > 0 && (
              <Box mt="md">
                <Text size="sm" fw={700}>
                  可应用的用户级设置
                </Text>
                <Text size="xs" c="dimmed" mt={4}>
                  仅写入 HKCU；写入后立即回读，可单项应用或一键撤销
                </Text>
                <ScrollArea type="auto" mt="xs">
                  <Table miw={760} verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>设置项</Table.Th>
                        <Table.Th>当前</Table.Th>
                        <Table.Th>修改为</Table.Th>
                        <Table.Th>范围</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {winoptSettings.map((setting) => {
                        const currentLabel =
                          setting.current == null
                            ? "未设置"
                            : setting.options.find(
                                (option) => option.value === setting.current,
                              )?.label ?? String(setting.current);
                        const value =
                          winoptDraft[setting.id] ??
                          setting.current ??
                          setting.min;
                        return (
                          <Table.Tr key={setting.id}>
                            <Table.Td>
                              <Text size="sm" fw={600}>
                                {setting.label}
                              </Text>
                            </Table.Td>
                            <Table.Td>
                              <Badge size="xs" variant="light">
                                {currentLabel}
                              </Badge>
                            </Table.Td>
                            <Table.Td>
                              <Select
                                size="xs"
                                w={170}
                                data={setting.options.map((option) => ({
                                  value: String(option.value),
                                  label: option.label,
                                }))}
                                value={String(value)}
                                allowDeselect={false}
                                onChange={(next) =>
                                  next != null &&
                                  updateWinoptValue(setting.id, next)
                                }
                              />
                            </Table.Td>
                            <Table.Td>
                              <Text size="xs" c="dimmed">
                                HKCU · 可还原
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
                    disabled={winoptChanges.length === 0}
                    loading={applying === `${generic.tool.id}:apply_settings`}
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply_settings",
                          label: "应用所选设置",
                          write: true,
                        },
                        { changes: winoptChanges },
                      )
                    }
                  >
                    应用所选设置
                  </Button>
                  {winoptUndo.length > 0 && (
                    <Button
                      size="xs"
                      color="red"
                      variant="light"
                      loading={applying === `${generic.tool.id}:apply_settings`}
                      onClick={() =>
                        void runApply(
                          generic.tool,
                          {
                            id: "apply_settings",
                            label: "撤销本次修改",
                            write: true,
                          },
                          { changes: winoptUndo },
                        )
                      }
                    >
                      撤销本次修改
                    </Button>
                  )}
                  <Text size="xs" c="dimmed">
                    {winoptChanges.length > 0
                      ? `将修改 ${winoptChanges.length} 项；原值写入本地快照`
                      : "修改选项后可应用"}
                  </Text>
                </Group>
              </Box>
            )}
          {generic.tool.id === "winopt_assistant" &&
            winoptAdminSettings.length > 0 && (
              <Box mt="md">
                <Group gap="xs" align="center">
                  <Text size="sm" fw={700}>
                    管理员级设置（HKLM）
                  </Text>
                  <Badge size="xs" color="orange" variant="light">
                    一次 UAC 整批写入
                  </Badge>
                </Group>
                <Text size="xs" c="dimmed" mt={4}>
                  这几项写在 HKLM，整批只弹一次 UAC；单项失败不影响其他项，写完立即回读复核。
                </Text>
                <ScrollArea type="auto" mt="xs">
                  <Table miw={760} verticalSpacing="xs">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>设置项</Table.Th>
                        <Table.Th>当前</Table.Th>
                        <Table.Th>修改为</Table.Th>
                        <Table.Th>范围</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {winoptAdminSettings.map((setting) => {
                        const currentLabel =
                          setting.current == null
                            ? "未设置"
                            : setting.options.find(
                                (option) => option.value === setting.current,
                              )?.label ?? String(setting.current);
                        const value =
                          winoptAdminDraft[setting.id] ??
                          setting.current ??
                          setting.options[0]?.value;
                        return (
                          <Table.Tr key={setting.id}>
                            <Table.Td>
                              <Text size="sm" fw={600}>
                                {setting.label}
                              </Text>
                            </Table.Td>
                            <Table.Td>
                              <Badge size="xs" variant="light">
                                {currentLabel}
                              </Badge>
                            </Table.Td>
                            <Table.Td>
                              <Select
                                size="xs"
                                w={200}
                                data={setting.options.map((option) => ({
                                  value: String(option.value),
                                  label: option.label,
                                }))}
                                value={value == null ? null : String(value)}
                                allowDeselect={false}
                                onChange={(next) =>
                                  next != null &&
                                  updateWinoptAdminValue(setting.id, next)
                                }
                              />
                            </Table.Td>
                            <Table.Td>
                              <Text size="xs" c="dimmed">
                                HKLM · 需 UAC
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
                    disabled={
                      winoptAdminChanges.length === 0 || !winoptAdminToken
                    }
                    loading={
                      applying === `${generic.tool.id}:apply_admin_settings`
                    }
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply_admin_settings",
                          label: "整批应用管理员设置",
                          write: true,
                        },
                        { changes: winoptAdminChanges },
                        winoptAdminToken,
                        "Windows 会弹出一次管理员授权（UAC），确认后整批写入。",
                      )
                    }
                  >
                    整批应用（一次 UAC）
                  </Button>
                  {winoptAdminUndo.length > 0 && (
                    <Button
                      size="xs"
                      color="red"
                      variant="light"
                      disabled={!winoptAdminToken}
                      loading={
                        applying === `${generic.tool.id}:apply_admin_settings`
                      }
                      onClick={() =>
                        void runApply(
                          generic.tool,
                          {
                            id: "apply_admin_settings",
                            label: "撤销本次管理员修改",
                            write: true,
                          },
                          { changes: winoptAdminUndo },
                          winoptAdminToken,
                          "Windows 会弹出一次管理员授权（UAC），确认后恢复原值。",
                        )
                      }
                    >
                      撤销本次修改
                    </Button>
                  )}
                  <Text size="xs" c="dimmed">
                    {winoptAdminChanges.length > 0
                      ? `将修改 ${winoptAdminChanges.length} 项；原值写入本地快照，可一键还原`
                      : "修改选项后可整批应用"}
                  </Text>
                </Group>
              </Box>
            )}
          {generic.tool.id === "pc_optimizer" && powerPresets.length > 0 && (
            <Box mt="md">
              <Text size="sm" fw={700}>
                一键预设（一次授权写入整套设置）
              </Text>
              <Group gap="xs" mt="xs">
                {powerPresets.map((preset) => (
                  <Button
                    key={preset.id}
                    size="xs"
                    variant="light"
                    title={preset.note}
                    loading={
                      applying === `${generic.tool.id}:apply_preset`
                    }
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply_preset",
                          label: `应用预设「${preset.label}」`,
                          write: true,
                        },
                        { preset: preset.id },
                      )
                    }
                  >
                    {preset.label}
                  </Button>
                ))}
              </Group>
              <Text size="xs" c="dimmed" mt={4}>
                预设只改当前电源计划的这几项取值，不会切换计划；应用后可用“撤销本次修改”还原。
              </Text>
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
                {(generic.data.undoChanges?.length ?? 0) > 0 && (
                  <Button
                    size="xs"
                    color="red"
                    variant="light"
                    loading={applying === `${generic.tool.id}:apply_settings`}
                    onClick={() =>
                      void runApply(
                        generic.tool,
                        {
                          id: "apply_settings",
                          label: "撤销本次修改",
                          write: true,
                        },
                        { changes: generic.data.undoChanges },
                      )
                    }
                  >
                    撤销本次修改
                  </Button>
                )}
                <Text size="xs" c="dimmed">
                  {powerChanges.length > 0
                    ? `将提交 ${powerChanges.length} 项修改；原值写入本地操作日志`
                    : "修改数值后可应用；0 表示永不"}
                </Text>
              </Group>
            </Box>
          )}
          {generic.tool.id === "omni_zip" && (
            <Box mt="md">
              <SegmentedControl
                size="xs"
                value={zipMode}
                onChange={(value) =>
                  setZipMode(value as "compress" | "extract" | "list")
                }
                data={[
                  { label: "压缩", value: "compress" },
                  { label: "解压", value: "extract" },
                  { label: "查看列表", value: "list" },
                ]}
              />
              {zipMode === "compress" && (
                <Textarea
                  mt="sm"
                  size="xs"
                  minRows={3}
                  label="源文件或目录（每行一个绝对路径）"
                  placeholder="C:\\Users\\me\\Documents\\report.docx"
                  value={zipSources}
                  onChange={(event) => setZipSources(event.currentTarget.value)}
                />
              )}
              <TextInput
                mt="sm"
                size="xs"
                label="压缩包路径（.zip）"
                placeholder="C:\\Users\\me\\Desktop\\archive.zip"
                value={zipArchive}
                onChange={(event) => setZipArchive(event.currentTarget.value)}
              />
              {zipMode === "extract" && (
                <TextInput
                  mt="sm"
                  size="xs"
                  label="解压到目录"
                  placeholder="C:\\Users\\me\\Desktop\\archive"
                  value={zipDestination}
                  onChange={(event) =>
                    setZipDestination(event.currentTarget.value)
                  }
                />
              )}
              {zipMode !== "list" && (
                <Switch
                  mt="sm"
                  size="xs"
                  label="覆盖已存在的文件"
                  checked={zipOverwrite}
                  onChange={(event) =>
                    setZipOverwrite(event.currentTarget.checked)
                  }
                />
              )}
              <Group mt="sm" align="center">
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  loading={applying === `${generic.tool.id}:${zipMode}`}
                  onClick={() => void runZip(generic.tool, zipMode)}
                >
                  {zipMode === "compress"
                    ? "预览并压缩"
                    : zipMode === "extract"
                      ? "预览并解压"
                      : "查看列表"}
                </Button>
                <Text size="xs" c="dimmed">
                  绝对路径；不跟随符号链接；系统目录受限
                </Text>
              </Group>
              {zipProgress && (
                <Box mt="sm">
                  <Group justify="space-between" align="center" gap="xs">
                    <Text size="xs" c="dimmed">
                      {zipProgress.message || "处理中…"}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {zipProgress.total > 0
                        ? `${zipProgress.done}/${zipProgress.total}`
                        : ""}
                    </Text>
                  </Group>
                  <Progress
                    value={
                      zipProgress.total > 0
                        ? (zipProgress.done / zipProgress.total) * 100
                        : 0
                    }
                    mt={4}
                    size="sm"
                    animated
                  />
                  <Button
                    size="xs"
                    color="orange"
                    variant="subtle"
                    mt="xs"
                    loading={zipCancelling}
                    onClick={() => void cancelZip()}
                  >
                    取消本次{zipMode === "compress" ? "压缩" : "解压"}
                  </Button>
                </Box>
              )}
              {zipNotice && (
                <Alert color="orange" title="已取消" mt="sm">
                  {zipNotice}
                </Alert>
              )}
            </Box>
          )}
          {generic.tool.id === "master_pdf" && (
            <Box mt="md">
              <SegmentedControl
                size="xs"
                value={pdfMode}
                onChange={(value) =>
                  setPdfMode(
                    value as "info" | "pages" | "merge" | "split" | "rotate",
                  )
                }
                data={[
                  { label: "信息", value: "info" },
                  { label: "页面预览", value: "pages" },
                  { label: "合并", value: "merge" },
                  { label: "拆分", value: "split" },
                  { label: "旋转", value: "rotate" },
                ]}
              />
              <Textarea
                mt="sm"
                size="xs"
                minRows={2}
                label="输入 PDF（每行一个绝对路径）"
                placeholder="C:\\Users\\me\\a.pdf"
                value={pdfInputs}
                onChange={(event) => setPdfInputs(event.currentTarget.value)}
              />
              <TextInput
                mt="sm"
                size="xs"
                type="password"
                label="打开密码（输入 PDF 已加密时填写）"
                placeholder="留空表示未加密"
                value={pdfPassword}
                onChange={(event) => setPdfPassword(event.currentTarget.value)}
              />
              {pdfCanWrite && (
                <TextInput
                  mt="sm"
                  size="xs"
                  label="输出 PDF"
                  placeholder="C:\\Users\\me\\output.pdf"
                  value={pdfOutput}
                  onChange={(event) => setPdfOutput(event.currentTarget.value)}
                />
              )}
              {(pdfMode === "split" || pdfMode === "rotate") && (
                <TextInput
                  mt="sm"
                  size="xs"
                  label={pdfMode === "split" ? "保留页码" : "旋转页码（留空为全部）"}
                  placeholder="1-3,5"
                  value={pdfPages}
                  onChange={(event) => setPdfPages(event.currentTarget.value)}
                />
              )}
              {pdfMode === "rotate" && (
                <Select
                  mt="sm"
                  size="xs"
                  w={160}
                  label="旋转角度"
                  allowDeselect={false}
                  value={pdfAngle}
                  onChange={(value) => setPdfAngle(value ?? "90")}
                  data={[
                    { value: "90", label: "顺时针 90°" },
                    { value: "180", label: "180°" },
                    { value: "270", label: "270°" },
                  ]}
                />
              )}
              {pdfCanWrite && (
                <>
                  <Select
                    mt="sm"
                    size="xs"
                    w={220}
                    label="同名输出处理"
                    allowDeselect={false}
                    value={pdfConflict}
                    onChange={(value) =>
                      setPdfConflict(
                        (value ?? "fail") as "fail" | "overwrite" | "rename",
                      )
                    }
                    data={[
                      { value: "fail", label: "报错并给出改名建议" },
                      { value: "overwrite", label: "覆盖已有文件" },
                      { value: "rename", label: "自动改名（name (1).pdf）" },
                    ]}
                  />
                  <TextInput
                    mt="sm"
                    size="xs"
                    type="password"
                    label="输出密码（留空表示不加密）"
                    placeholder="留空即不加密"
                    value={pdfOutPassword}
                    onChange={(event) => setPdfOutPassword(event.currentTarget.value)}
                  />
                  {pdfOutPassword.trim() !== "" && (
                    <Box mt="sm">
                      <Text size="xs" fw={600}>
                        允许的操作（不勾选即仅允许阅读）
                      </Text>
                      <Group gap="md" mt={4}>
                        {[
                          { key: "print", label: "打印" },
                          { key: "copy", label: "复制文字" },
                          { key: "modify", label: "修改" },
                          { key: "annotate", label: "批注" },
                          { key: "fill", label: "填写表单" },
                          { key: "assemble", label: "组织页面" },
                        ].map((entry) => (
                          <Checkbox
                            key={entry.key}
                            size="xs"
                            label={entry.label}
                            checked={pdfPermissions.includes(entry.key)}
                            onChange={(event) =>
                              setPdfPermissions((current) =>
                                event.currentTarget.checked
                                  ? [...current, entry.key]
                                  : current.filter((value) => value !== entry.key),
                              )
                            }
                          />
                        ))}
                      </Group>
                    </Box>
                  )}
                </>
              )}
              <Group mt="sm" align="center">
                <Button
                  size="xs"
                  color="orange"
                  variant="light"
                  loading={applying === `${generic.tool.id}:${pdfMode}`}
                  onClick={() => void runPdf(generic.tool, pdfMode)}
                >
                  {pdfMode === "info"
                    ? "查看信息"
                    : pdfMode === "pages"
                      ? "预览页面"
                      : pdfMode === "merge"
                        ? "预览并合并"
                        : pdfMode === "split"
                          ? "预览并拆分"
                          : "预览并旋转"}
                </Button>
                <Text size="xs" c="dimmed">
                  输出新文件；同名按上方策略处理；加密输入需填打开密码
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
                <Table.Th w={44}></Table.Th>
                <Table.Th>位置</Table.Th>
                <Table.Th>占用</Table.Th>
                <Table.Th>类别</Table.Th>
                <Table.Th>权限</Table.Th>
                <Table.Th>说明</Table.Th>
                <Table.Th w={112}>操作</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {slim.items.map((item) => {
                const selectable = item.cleanable !== false && item.fileCount > 0;
                return (
                <Table.Tr key={item.id}>
                  <Table.Td>
                    <Checkbox
                      size="xs"
                      checked={slimSelection.includes(item.id)}
                      disabled={!selectable}
                      onChange={(event) =>
                        setSlimSelection((current) =>
                          event.currentTarget.checked
                            ? [...current, item.id]
                            : current.filter((id) => id !== item.id),
                        )
                      }
                    />
                  </Table.Td>
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
                    {item.cleanable === false && (
                      <Badge size="xs" color="gray" variant="light" mt={4}>
                        仅统计
                      </Badge>
                    )}
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
                );
              })}
            </Table.Tbody>
          </Table>
          </ScrollArea>
          <Group mt="sm" gap="xs" align="center">
            <Button
              size="xs"
              color="orange"
              variant="light"
              disabled={slimSelection.length === 0}
              loading={slimApplying}
              onClick={() => void slimApply()}
            >
              清理所选（移入回收站）
            </Button>
            <Button size="xs" variant="subtle" onClick={() => void scan()}>
              重新扫描
            </Button>
            <Text size="xs" c="dimmed">
              已选 {slimSelection.length} 个位置；清理后可到回收站还原
            </Text>
          </Group>
          {slimResult && (
            <Alert color="teal" icon={<IconShieldCheck />} mt="sm">
              清理完成：
              {(slimResult.summary ?? [])
                .map((entry) => `${entry.label} ${entry.value}`)
                .join(" · ")}
            </Alert>
          )}
          {slimResult && (slimResult.diff ?? []).length > 0 && (
            <CleanupDiffTable rows={slimResult.diff ?? []} />
          )}
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

// 隔离静态重型页面，避免传感器或其他全局状态更新时重建数百行表格。
const MemoHardwarePanel = memo(HardwarePanel);
const MemoDevicePanel = memo(DevicePanel);
const MemoDriversPanel = memo(DriversPanel);
const MemoNetworkPanel = memo(NetworkPanel);
const MemoSoftwarePanel = memo(SoftwarePanel);
const MemoScheduledTasksPanel = memo(ScheduledTasksPanel);
const MemoProcessPanel = memo(ProcessPanel);

function PanelFallback() {
  return (
    <Center h={260}>
      <Loader />
    </Center>
  );
}

function OverviewSkeleton() {
  return (
    <Stack gap="md">
      <SimpleGrid cols={{ base: 2, xl: 4 }}>
        {[0, 1, 2, 3].map((index) => (
          <Card key={index} className="stat-card" padding="lg" radius="lg">
            <Group justify="space-between" align="flex-start">
              <Box style={{ flex: 1 }}>
                <Skeleton height={10} width="42%" radius="xl" />
                <Skeleton height={30} width="58%" mt={12} radius="sm" />
                <Skeleton height={9} width="76%" mt={12} radius="xl" />
              </Box>
              <Skeleton height={42} width={42} circle />
            </Group>
          </Card>
        ))}
      </SimpleGrid>
      <SimpleGrid cols={{ base: 1, lg: 2 }}>
        {[0, 1].map((index) => (
          <Card key={index} className="glass-card" radius="lg">
            <Skeleton height={13} width="28%" radius="xl" />
            <Skeleton height={245} mt="md" radius="md" />
          </Card>
        ))}
      </SimpleGrid>
      <Card className="glass-card" radius="lg">
        <Skeleton height={13} width="22%" radius="xl" />
        <Skeleton height={180} mt="md" radius="md" />
      </Card>
    </Stack>
  );
}

export default function App() {
  const { colorScheme, setColorScheme } = useMantineColorScheme();
  const [tab, setTab] = useState<Tab>("overview");
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const navViewportRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    navViewportRef.current
      ?.querySelector<HTMLElement>("[data-active]")
      ?.scrollIntoView({ block: "nearest" });
  }, [tab]);

  useEffect(() => {
    void getAppSettings()
      .then(setAppSettings)
      .catch(() => undefined);
  }, []);

  const [eyeCare, setEyeCare] = useState<EyeCareSettings | null>(null);
  const [eyeBreak, setEyeBreak] = useState(false);

  useEffect(() => {
    void getEyeCareSettings()
      .then(setEyeCare)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!eyeCare?.enabled) {
      setEyeBreak(false);
      return;
    }
    const hour = new Date().getHours();
    const inSchedule =
      !eyeCare.scheduleEnabled ||
      eyeCare.scheduleStartHour === eyeCare.scheduleEndHour ||
      (eyeCare.scheduleStartHour < eyeCare.scheduleEndHour
        ? hour >= eyeCare.scheduleStartHour && hour < eyeCare.scheduleEndHour
        : hour >= eyeCare.scheduleStartHour || hour < eyeCare.scheduleEndHour);
    if (!inSchedule) return;
    const timer = window.setInterval(() => {
      if (document.hidden || document.fullscreenElement) return;
      setEyeBreak(true);
      void sendBreakReminder().catch(() => undefined);
      window.setTimeout(
        () => setEyeBreak(false),
        Math.max(5, eyeCare.breakDurationSeconds) * 1000,
      );
    }, Math.max(5, eyeCare.breakIntervalMinutes) * 60_000);
    return () => window.clearInterval(timer);
  }, [eyeCare]);

  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
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
    // 只有实时页面需要高频系统快照；静态清单页不应被两秒轮询反复重绘。
    if (tab !== "overview") return;
    let active = true;
    let timer: number | undefined;
    const poll = async () => {
      await refresh();
      if (active) timer = window.setTimeout(() => void poll(), Math.max(1, appSettings?.snapshotIntervalSeconds ?? 2) * 1000);
    };
    void poll();
    return () => { active = false; if (timer) window.clearTimeout(timer); };
  }, [refresh, tab, appSettings?.snapshotIntervalSeconds]);
  useEffect(() => {
    if (tab !== "processes") return;
    let active = true;
    let timer: number | undefined;
    const load = async () => {
      try {
        const next = await getProcessInventory();
        if (active) setProcesses(next);
      } catch (reason) {
        if (active) setError(String(reason));
      } finally {
        if (active) timer = window.setTimeout(() => void load(), 3000);
      }
    };
    void load();
    return () => { active = false; if (timer) window.clearTimeout(timer); };
  }, [tab]);
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

  const diskRead = (snapshot?.disks ?? []).reduce(
    (sum, x) => sum + x.readBytesPerSec,
    0,
  );
  const diskWrite = (snapshot?.disks ?? []).reduce(
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
        <ScrollArea
          className="side-nav"
          type="auto"
          scrollbarSize={6}
          offsetScrollbars
          viewportRef={navViewportRef}
        >
          <Stack gap={5} pr={2}>
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
        </ScrollArea>
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
          v0.57.0 · 25 个工具 · AI 解读
        </Text>
      </aside>
      <main className="content">
        {eyeBreak && (
          <Alert
            color="orange"
            icon={<IconEye />}
            mb="md"
            withCloseButton
            onClose={() => setEyeBreak(false)}
            title="护眼提醒"
          >
            休息一下：看向 6 米外约 20 秒，眨眼放松。
          </Alert>
        )}

        <Group justify="space-between" mb="xl">
          <Box>
            <Text size="sm" c="dimmed">
              {snapshot
                ? `${snapshot.hostName} · ${snapshot.osName}`
                : "正在识别本机硬件…"}
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
        {tab === "overview" &&
          (snapshot ? (
            <Stack gap="md">
            <div className="overview-stat-grid">
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
            </div>
            <div className="overview-chart-grid">
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
            </div>
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
          ) : (
            <OverviewSkeleton />
          ))}
        {tab === "benchmark" && <BenchmarkPanel />}
        {tab === "hardware" && <MemoHardwarePanel details={hardware} />}
        {tab === "devices" && <MemoDevicePanel data={deviceDetails} />}
        {tab === "sensors" && <SensorsPanel data={sensors} />}
        {tab === "ai" && (
          <Suspense fallback={<PanelFallback />}>
            <AiPanel onOpenToolbox={() => setTab("toolbox")} />
          </Suspense>
        )}
        {tab === "eye-care" && <EyeCarePanel settings={eyeCare} onSaved={setEyeCare} />}
        {tab === "settings" && <SettingsPanel settings={appSettings} onSaved={setAppSettings} onNavigate={setTab} />}
        {tab === "desktop" && <DesktopPanel />}
        {tab === "launcher" && <StartMenuPanel />}
        {tab === "drivers" && <MemoDriversPanel data={drivers} />}
        {tab === "diagnostics" && (
          <Suspense fallback={<PanelFallback />}>
            <DiagnosticsPanel data={diagnostics} />
          </Suspense>
        )}
        {tab === "network" && <MemoNetworkPanel data={networkDiagnostics} />}
        {tab === "software" && <MemoSoftwarePanel data={software} />}
        {tab === "storage" && <StoragePanel />}
        {tab === "processes" && <MemoProcessPanel processes={processes} />}
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
        {tab === "tasks" && <MemoScheduledTasksPanel data={scheduledTasks} />}
        {tab === "toolbox" && <ToolboxPanel onOpenEyeCare={() => setTab("eye-care")} onOpenStartMenu={() => setTab("launcher")} onOpenDesktopQuick={() => setTab("desktop")} />}
        {tab === "report" && (
          <Suspense fallback={<PanelFallback />}>
            <ReportPanel />
          </Suspense>
        )}
      </main>
    </div>
  );
}
