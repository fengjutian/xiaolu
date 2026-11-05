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
  const [settings, setSettings] = useState<SensorSettings | null>(null);
  const [draft, setDraft] = useState<SensorSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [settingsStatus, setSettingsStatus] = useState<string | null>(null);
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
        setSettings(value);
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
      setSettings(saved);
      setDraft(saved);
      setSettingsStatus("阈值与保留策略已保存");
    } catch (error) {
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
              <Text size="sm" c={settings ? "teal" : "red"}>
                {settingsStatus}
              </Text>
            )}
          </Group>
        </Card>
      )}
    </Stack>
  );
}
