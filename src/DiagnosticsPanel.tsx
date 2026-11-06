import { useState } from "react";
import { Alert, Badge, Box, Button, Card, Center, Group, Loader, ScrollArea, SegmentedControl, Select, SimpleGrid, Stack, Table, Tabs, Text, TextInput, Title } from "@mantine/core";
import { IconAlertCircle, IconBolt, IconHeartbeat, IconSearch, IconShieldCheck, IconTool } from "@tabler/icons-react";
import { queryEventLog } from "./api";
import type { DiagnosticReport, EventLogResult } from "./types";
import { Stat } from "./Stat";

export default function DiagnosticsPanel({ data }: { data: DiagnosticReport | null }) {
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
