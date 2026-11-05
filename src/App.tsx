import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Card,
  Center,
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
  IconCpu,
  IconDownload,
  IconFolder,
  IconGauge,
  IconHeartbeat,
  IconNetwork,
  IconPlayerPause,
  IconRefresh,
  IconRocket,
  IconSearch,
  IconServer,
  IconShieldCheck,
  IconTemperature,
  IconTool,
  IconMoon,
  IconSun,
  IconWind,
} from "@tabler/icons-react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  getBenchmarkHistory,
  getDriverInventory,
  getDiagnosticReport,
  getHardwareDetails,
  getNetworkDiagnostics,
  getSensorHistory,
  getSensorSettings,
  getSensorSnapshot,
  saveSensorSettings,
  getSoftwareInventory,
  getStartupItems,
  getSystemSnapshot,
  isDesktopRuntime,
  runBenchmark,
  scanDriverUpdates,
  scanStorage,
  setStartupEnabled,
} from "./api";
import type {
  BenchmarkRecord,
  BenchmarkResult,
  DriverInventory,
  DriverUpdate,
  DiagnosticReport,
  HardwareDetails,
  NetworkDiagnosticReport,
  SensorHistorySeries,
  SensorSettings,
  SensorSnapshot,
  SensorValue,
  SoftwareInventory,
  StorageScan,
  StartupItem,
  SystemSnapshot,
} from "./types";
import "./styles.css";
import logoUrl from "../src-tauri/app-icon.png";

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
  | "processes"
  | "startup"
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
  ["processes", "运行进程", IconActivity],
  ["startup", "启动项管理", IconRocket],
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

      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-end" wrap="wrap">
          <Box>
            <Text fw={800}>温度历史</Text>
            <Text size="xs" c="dimmed">
              样本保存在本机 SQLite，按范围自动降采样
            </Text>
          </Box>
          <Group gap="sm">
            <Select
              size="xs"
              w={280}
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
            <Group gap="xl" mt="md">
              <Box>
                <Text size="xs" c="dimmed">
                  最低
                </Text>
                <Text fw={700}>
                  {history?.min == null ? "—" : `${history.min.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  平均
                </Text>
                <Text fw={700}>
                  {history?.avg == null ? "—" : `${history.avg.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  最高
                </Text>
                <Text fw={700}>
                  {history?.max == null ? "—" : `${history.max.toFixed(1)} °C`}
                </Text>
              </Box>
              <Box>
                <Text size="xs" c="dimmed">
                  样本
                </Text>
                <Text fw={700}>{history?.sampleCount ?? 0}</Text>
              </Box>
              {loadingHistory && <Loader size="xs" />}
            </Group>
            {chartData.length === 0 ? (
              <Alert color="gray" mt="md">
                这个传感器还没有历史样本；桌面应用运行一段时间后会自动累积。
              </Alert>
            ) : (
              <Box mt="md">
                <ResponsiveContainer width="100%" height={230}>
                  <LineChart data={chartData} margin={{ left: -14, right: 8, top: 8 }}>
                    <CartesianGrid stroke="#293042" strokeDasharray="4 4" />
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
                        background: "#171b25",
                        border: "1px solid #30384b",
                        borderRadius: 12,
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
              <Badge
                color={
                  disk.healthStatus.toLowerCase() === "healthy"
                    ? "teal"
                    : "yellow"
                }
              >
                {disk.healthStatus} · {disk.operationalStatus}
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
                  <Badge
                    color={
                      device.predictFailure === true
                        ? "red"
                        : device.predictFailure === false
                          ? "teal"
                          : "gray"
                    }
                  >
                    {device.predictFailure === true
                      ? "预测故障"
                      : device.predictFailure === false
                        ? "未预测故障"
                        : "预测状态未知"}
                  </Badge>
                </Group>
              </Box>
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
    </Stack>
  );
}


function DriversPanel({ data }: { data: DriverInventory | null }) {
  const [query, setQuery] = useState("");
  const [updates, setUpdates] = useState<DriverUpdate[] | null>(null);
  const [scanningUpdates, setScanningUpdates] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const scanUpdates = async () => {
    setScanningUpdates(true);
    setUpdateError(null);
    try {
      setUpdates(await scanDriverUpdates());
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
      <Card className="glass-card" radius="lg">
        <Group justify="space-between" align="flex-start">
          <Box>
            <Text fw={800}>Windows Update 驱动候选</Text>
            <Text size="xs" c="dimmed">
              只读查询 Microsoft Update，不接受许可、不下载、不安装。
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
            {updates.map((update) => (
              <Paper
                key={`${update.updateId}-${update.revisionNumber}`}
                p="md"
                radius="md"
                withBorder
              >
                <Group justify="space-between" align="flex-start">
                  <Box maw="75%">
                    <Text fw={700}>{update.title}</Text>
                    <Text size="xs" c="dimmed">
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
                  </Box>
                  <Box ta="right">
                    <Badge color={update.downloaded ? "teal" : "blue"}>
                      {update.downloaded ? "已下载" : "可用"}
                    </Badge>
                    <Text size="xs" c="dimmed" mt={5}>
                      {update.driverVersion || "版本未知"}
                    </Text>
                  </Box>
                </Group>
              </Paper>
            ))}
          </Stack>
        )}
      </Card>
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
  );
}

function StoragePanel() {
  const [running, setRunning] = useState<StorageScan["kind"] | null>(null);
  const [results, setResults] = useState<Record<string, StorageScan>>({});
  const [failure, setFailure] = useState<string | null>(null);
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
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconShieldCheck />}>
        只扫描下载、文档和桌面三个标准目录；结果按扩展名聚合，不显示文件名，也不会删除文件。每次最多统计
        25 万个文件。
      </Alert>
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
          <Stack gap="sm">
            {result.categories.map((category) => {
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
          </Stack>
        </Card>
      ))}
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
  );
}

function DiagnosticsPanel({ data }: { data: DiagnosticReport | null }) {
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
    description: "临时创建并删除 128 MiB 文件",
  },
  { kind: "gpu", label: "GPU 着色器", description: "WebGL2 离屏像素着色器" },
];

function BenchmarkPanel() {
  const [running, setRunning] = useState<BenchmarkResult["kind"] | null>(null);
  const [results, setResults] = useState<Record<string, BenchmarkResult>>({});
  const [benchmarkHistory, setBenchmarkHistory] = useState<BenchmarkRecord[]>(
    [],
  );
  const [failure, setFailure] = useState<string | null>(null);
  useEffect(() => {
    void getBenchmarkHistory()
      .then(setBenchmarkHistory)
      .catch((error) => setFailure(String(error)));
  }, []);
  const run = async (kind: BenchmarkResult["kind"]) => {
    setRunning(kind);
    setFailure(null);
    try {
      const result = await runBenchmark(kind);
      setResults((old) => ({ ...old, [kind]: result }));
      setBenchmarkHistory(await getBenchmarkHistory());
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
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
  return (
    <Stack gap="md">
      <Alert color="blue" icon={<IconGauge />}>
        跑分会在短时间内提高 CPU、内存、磁盘或 GPU
        负载。综合分采用加权几何平均：单核 20%、多核 30%、内存 15%、磁盘
        15%、GPU 20%。
      </Alert>
      {failure && (
        <Alert color="red" icon={<IconAlertCircle />}>
          {failure}
        </Alert>
      )}
      {composite != null && (
        <Card className="glass-card" radius="lg">
          <Text size="xs" c="dimmed" fw={800}>
            SYSTEM LENS 综合分 · SCHEMA 2
          </Text>
          <Title order={1} mt="xs">
            {composite.toFixed(0)} 分
          </Title>
          <Text size="sm" c="dimmed">
            仅用于相同 Schema 和应用版本之间比较。
          </Text>
        </Card>
      )}
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        {benchmarkKinds.map((item) => {
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
                  <Table.Td>{record.result.label}</Table.Td>
                  <Table.Td>{record.result.score.toFixed(0)} 分</Table.Td>
                  <Table.Td>{record.result.durationMs} ms</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>
      )}
    </Stack>
  );
}

export default function App() {
  const { colorScheme, setColorScheme } = useMantineColorScheme();
  const [tab, setTab] = useState<Tab>("overview");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [hardware, setHardware] = useState<HardwareDetails | null>(null);
  const [sensors, setSensors] = useState<SensorSnapshot | null>(null);
  const [drivers, setDrivers] = useState<DriverInventory | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [networkDiagnostics, setNetworkDiagnostics] =
    useState<NetworkDiagnosticReport | null>(null);
  const [software, setSoftware] = useState<SoftwareInventory | null>(null);
  const [startup, setStartup] = useState<StartupItem[]>([]);
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
    if (tab === "hardware" && !hardware)
      void getHardwareDetails()
        .then(setHardware)
        .catch((e) => setError(String(e)));
  }, [tab, hardware]);
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

  const report = useMemo(
    () =>
      snapshot
        ? JSON.stringify(
            {
              schemaVersion: 2,
              generatedAt: new Date(snapshot.capturedAtUnixMs).toISOString(),
              privacy: "主机名和 PID 已脱敏",
              system: {
                ...snapshot,
                hostName: "[已脱敏]",
                processes: snapshot.processes.map(({ pid: _, ...p }) => p),
              },
            },
            null,
            2,
          )
        : "",
    [snapshot],
  );
  const downloadReport = () => {
    const url = URL.createObjectURL(
      new Blob([report], { type: "application/json" }),
    );
    const a = document.createElement("a");
    a.href = url;
    a.download = `system-lens-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };
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
      <Center h="100vh">
        <Stack align="center">
          <Loader size="lg" />
          <Text c="dimmed">正在建立本地系统视图…</Text>
          {error && <Alert color="red">{error}</Alert>}
        </Stack>
      </Center>
    );
  const diskRead = snapshot.disks.reduce(
    (sum, x) => sum + x.readBytesPerSec,
    0,
  );
  const diskWrite = snapshot.disks.reduce(
    (sum, x) => sum + x.writtenBytesPerSec,
    0,
  );

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
          v0.13.0 · 温度历史与告警
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
                  <Text fw={700}>磁盘活动</Text>
                  <Text size="xs" c="dimmed">
                    读取 {speed(diskRead)} · 写入 {speed(diskWrite)}
                  </Text>
                </Box>
                <Badge color="gray">
                  运行 {duration(snapshot.uptimeSeconds)}
                </Badge>
              </Group>
              {snapshot.disks.map((d) => {
                const used = d.totalBytes - d.availableBytes;
                return (
                  <Box key={d.mountPoint} mb="md">
                    <Group justify="space-between">
                      <Text size="sm" fw={600}>
                        {d.name || d.mountPoint} · {d.fileSystem}
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
        {tab === "sensors" && <SensorsPanel data={sensors} />}
        {tab === "drivers" && <DriversPanel data={drivers} />}
        {tab === "diagnostics" && <DiagnosticsPanel data={diagnostics} />}
        {tab === "network" && <NetworkPanel data={networkDiagnostics} />}
        {tab === "software" && <SoftwarePanel data={software} />}
        {tab === "storage" && <StoragePanel />}
        {tab === "processes" && (
          <Card className="glass-card" radius="lg" p={0}>
            <ScrollArea h="calc(100vh - 180px)">
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
                  {snapshot.processes.map((p) => (
                    <Table.Tr key={p.pid}>
                      <Table.Td>
                        <Text fw={600}>{p.name}</Text>
                      </Table.Td>
                      <Table.Td>{p.pid}</Table.Td>
                      <Table.Td>{p.cpuPercent.toFixed(1)}%</Table.Td>
                      <Table.Td>{bytes(p.memoryBytes)}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </ScrollArea>
          </Card>
        )}
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
        {tab === "report" && (
          <SimpleGrid cols={{ base: 1, lg: 2 }}>
            <Card className="glass-card" radius="lg">
              <IconShieldCheck size={40} color="#748ffc" />
              <Title order={2} mt="md">
                隐私优先诊断报告
              </Title>
              <Text c="dimmed" mt="sm">
                包含系统、硬件、存储、网络和进程摘要。导出时自动移除主机名和
                PID。
              </Text>
              <Button
                mt="xl"
                leftSection={<IconDownload size={18} />}
                onClick={downloadReport}
              >
                导出 JSON 报告
              </Button>
            </Card>
            <pre className="report">{report}</pre>
          </SimpleGrid>
        )}
      </main>
    </div>
  );
}
