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
  IconApps,
  IconBolt,
  IconCpu,
  IconDeviceDesktop,
  IconDownload,
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
  getDiagnosticReport,
  getHardwareDetails,
  getNetworkDiagnostics,
  getSensorSnapshot,
  getSoftwareInventory,
  getStartupItems,
  getSystemSnapshot,
  isDesktopRuntime,
  runBenchmark,
  scanDriverUpdates,
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
  SensorReading,
  SensorSnapshot,
  SoftwareInventory,
  StartupItem,
  SystemSnapshot,
} from "./types";
import "./styles.css";

type Tab =
  | "overview"
  | "benchmark"
  | "sensors"
  | "drivers"
  | "diagnostics"
  | "network"
  | "software"
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
          v0.10.0 · 软件清单
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
        {tab === "diagnostics" && <DiagnosticsPanel data={diagnostics} />}
        {tab === "network" && <NetworkPanel data={networkDiagnostics} />}
        {tab === "software" && <SoftwarePanel data={software} />}
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
