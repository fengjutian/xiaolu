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
  Paper,
  Progress,
  ScrollArea,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Text,
  TextInput,
  ThemeIcon,
  Title,
  Tooltip,
} from "@mantine/core";
import {
  IconActivity,
  IconAlertCircle,
  IconArrowDown,
  IconArrowUp,
  IconBolt,
  IconCpu,
  IconDeviceDesktop,
  IconDownload,
  IconGauge,
  IconNetwork,
  IconPlayerPause,
  IconRefresh,
  IconRocket,
  IconSearch,
  IconServer,
  IconShieldCheck,
  IconTemperature,
  IconTool,
  IconWind,
} from "@tabler/icons-react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  getBenchmarkHistory,
  getDriverInventory,
  getHardwareDetails,
  getSensorSnapshot,
  getStartupItems,
  getSystemSnapshot,
  isDesktopRuntime,
  runBenchmark,
  setStartupEnabled,
} from "./api";
import type {
  BenchmarkRecord,
  BenchmarkResult,
  DriverInventory,
  HardwareDetails,
  SensorReading,
  SensorSnapshot,
  StartupItem,
  SystemSnapshot,
} from "./types";
import "./styles.css";

type Tab =
  | "overview"
  | "benchmark"
  | "sensors"
  | "drivers"
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

function SensorCard({ sensor }: { sensor: SensorReading }) {
  const Icon =
    sensor.category === "temperature"
      ? IconTemperature
      : sensor.category === "fan"
        ? IconWind
        : sensor.category === "power"
          ? IconBolt
          : IconGauge;
  const color =
    sensor.category === "temperature" && sensor.value >= 85
      ? "red"
      : sensor.category === "temperature" && sensor.value >= 70
        ? "orange"
        : "cyan";
  return (
    <Card className="stat-card" radius="lg">
      <Group justify="space-between">
        <Box>
          <Text size="xs" c="dimmed" fw={700}>
            {sensor.name}
          </Text>
          <Title order={2} mt={7}>
            {sensor.value.toFixed(sensor.category === "temperature" ? 0 : 1)}{" "}
            {sensor.unit}
          </Title>
          <Text size="xs" c="dimmed" mt={7}>
            {sensor.source}
          </Text>
        </Box>
        <ThemeIcon size={44} radius="xl" color={color} variant="light">
          <Icon size={23} />
        </ThemeIcon>
      </Group>
    </Card>
  );
}

function SensorsPanel({ data }: { data: SensorSnapshot | null }) {
  if (!data)
    return (
      <Center h={260}>
        <Loader />
      </Center>
    );
  return (
    <Stack gap="md">
      {data.limitations.map((message) => (
        <Alert key={message} color="yellow" icon={<IconAlertCircle />}>
          {message}
        </Alert>
      ))}
      {data.readings.length > 0 && (
        <SimpleGrid cols={{ base: 2, xl: 4 }}>
          {data.readings.map((sensor) => (
            <SensorCard key={sensor.id} sensor={sensor} />
          ))}
        </SimpleGrid>
      )}
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
    </Stack>
  );
}

function DriversPanel({ data }: { data: DriverInventory | null }) {
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
        跑分会在短时间内提高 CPU、内存、磁盘或 GPU 负载。综合分采用加权几何平均：单核
        20%、多核 30%、内存 15%、磁盘 15%、GPU 20%。
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
  const [tab, setTab] = useState<Tab>("overview");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [hardware, setHardware] = useState<HardwareDetails | null>(null);
  const [sensors, setSensors] = useState<SensorSnapshot | null>(null);
  const [drivers, setDrivers] = useState<DriverInventory | null>(null);
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
    if (tab !== "sensors") return;
    const load = () =>
      void getSensorSnapshot()
        .then(setSensors)
        .catch((e) => setError(String(e)));
    load();
    const timer = window.setInterval(load, 10_000);
    return () => window.clearInterval(timer);
  }, [tab]);
  useEffect(() => {
    if (tab === "drivers" && !drivers)
      void getDriverInventory()
        .then(setDrivers)
        .catch((e) => setError(String(e)));
  }, [tab, drivers]);
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
          <ThemeIcon
            size={42}
            radius={14}
            variant="gradient"
            gradient={{ from: "indigo", to: "cyan" }}
          >
            <IconDeviceDesktop />
          </ThemeIcon>
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
          v0.6.0 · GPU 跑分
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
