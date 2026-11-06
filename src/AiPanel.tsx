import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Card,
  Code,
  Divider,
  Group,
  Loader,
  Modal,
  NumberInput,
  PasswordInput,
  ScrollArea,
  Select,
  SimpleGrid,
  Stack,
  Switch,
  Tabs,
  Text,
  Textarea,
  ThemeIcon,
  Title,
  Tooltip,
} from "@mantine/core";
import {
  IconAlertCircle,
  IconKey,
  IconLock,
  IconRefresh,
  IconRobot,
  IconSend,
  IconShieldCheck,
  IconSparkles,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { isDesktopRuntime } from "./api";
import {
  buildAiReportJson,
  buildAiReportMarkdown,
  downloadAiReport,
} from "./aiReport";
import {
  DEFAULT_AI_SETTINGS,
  aiAnalyze,
  buildAiSnapshot,
  cancelAiRequest,
  clearAiApiKey,
  clearAiSessions,
  clearAiToolAudit,
  deleteAiSession,
  getAiSessions,
  getAiSettings,
  getAiToolAudit,
  listenAiStream,
  newAiRequestId,
  previewAiPayload,
  probeAiCapabilities,
  saveAiSettings,
} from "./aiApi";
import type {
  AiAnswer,
  AiCapabilityReport,
  AiConfidence,
  AiFinding,
  AiPayloadPreview,
  AiSessionRecord,
  AiSettings,
  AiSettingsView,
  AiToolAuditEntry,
  AiTurn,
  DiagnosticFact,
  DiagnosticSnapshot,
} from "./types";

type ConversationEntry = {
  question: string | null;
  answer: AiAnswer;
  snapshot: DiagnosticSnapshot;
};

const severityColor = (severity: string) =>
  severity === "critical" ? "red" : severity === "warning" ? "orange" : "blue";

const severityLabel = (severity: string) =>
  severity === "critical" ? "严重" : severity === "warning" ? "警告" : "提示";

const confidenceLabel = (confidence: AiConfidence | string) =>
  confidence === "high" ? "高" : confidence === "medium" ? "中" : "低";

const kindLabel = (kind: string) =>
  kind === "manual_action"
    ? "人工操作"
    : kind === "controlled_write_preview"
      ? "写操作预览"
      : "只读检查";

const riskColor = (risk: string) =>
  risk === "high" ? "red" : risk === "medium" ? "orange" : risk === "low" ? "yellow" : "gray";

const compactValue = (value: unknown) => {
  try {
    const text = JSON.stringify(value);
    return text.length > 400 ? `${text.slice(0, 400)}…` : text;
  } catch {
    return String(value);
  }
};

const formatTime = (unixMs: number) =>
  new Date(unixMs).toLocaleString([], {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });

function FindingCard({
  finding,
  facts,
  historical,
}: {
  finding: AiFinding;
  facts: Map<string, DiagnosticFact>;
  historical: boolean;
}) {
  return (
    <Card withBorder radius="md" padding="md">
      <Group justify="space-between" align="flex-start" wrap="nowrap">
        <Box>
          <Group gap="xs">
            <Text fw={700}>{finding.title}</Text>
            <Badge color={severityColor(finding.severity)} variant="light" size="sm">
              {severityLabel(finding.severity)}
            </Badge>
            <Badge color="gray" variant="outline" size="sm">
              置信度 {confidenceLabel(finding.confidence)}
            </Badge>
            {historical && (
              <Badge color="gray" variant="light" size="sm">
                历史结果
              </Badge>
            )}
          </Group>
          <Text size="sm" mt={6}>
            {finding.summary}
          </Text>
        </Box>
      </Group>
      {finding.evidenceRefs.length > 0 && (
        <Box mt="sm">
          <Text size="xs" c="dimmed" fw={700}>
            证据
          </Text>
          <Stack gap={4} mt={4}>
            {finding.evidenceRefs.map((reference) => {
              const fact = facts.get(reference);
              return (
                <Group key={reference} gap="xs" wrap="nowrap" align="flex-start">
                  <Badge variant="light" color="teal" size="sm" style={{ flexShrink: 0 }}>
                    {fact?.title ?? reference}
                  </Badge>
                  {fact && (
                    <Tooltip label={compactValue(fact.value)} multiline maw={420} position="top-start">
                      <Text size="xs" c="dimmed" truncate>
                        {fact.source} · {formatTime(fact.capturedAtUnixMs)} · 置信度{" "}
                        {Math.round(fact.confidence * 100)}%
                      </Text>
                    </Tooltip>
                  )}
                </Group>
              );
            })}
          </Stack>
        </Box>
      )}
      {finding.recommendations.length > 0 && (
        <Box mt="sm">
          <Text size="xs" c="dimmed" fw={700}>
            建议
          </Text>
          <Group gap="xs" mt={4}>
            {finding.recommendations.map((recommendation, index) => (
              <Badge
                key={`${recommendation.label}-${index}`}
                variant="light"
                color={riskColor(recommendation.risk)}
                size="sm"
              >
                {kindLabel(recommendation.kind)} · {recommendation.label}
                {recommendation.requiresConfirmation ? " · 需确认" : ""}
              </Badge>
            ))}
          </Group>
        </Box>
      )}
      {finding.limitations.length > 0 && (
        <Alert color="gray" variant="light" mt="sm" icon={<IconAlertCircle size={16} />}>
          <Stack gap={2}>
            {finding.limitations.map((item, index) => (
              <Text key={`${item}-${index}`} size="xs">
                {item}
              </Text>
            ))}
          </Stack>
        </Alert>
      )}
    </Card>
  );
}

function FindingsList({
  findings,
  facts,
  historical,
}: {
  findings: AiFinding[];
  facts: Map<string, DiagnosticFact>;
  historical: boolean;
}) {
  if (findings.length === 0) return null;
  return (
    <Stack gap="sm">
      {findings.map((finding) => (
        <FindingCard
          key={finding.id}
          finding={finding}
          facts={facts}
          historical={historical}
        />
      ))}
    </Stack>
  );
}

export default function AiPanel() {
  const desktop = isDesktopRuntime();
  const [view, setView] = useState<AiSettingsView | null>(null);
  const [form, setForm] = useState<AiSettings>({ ...DEFAULT_AI_SETTINGS });
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [clearingKey, setClearingKey] = useState(false);
  const [settingsMessage, setSettingsMessage] = useState<{
    text: string;
    error: boolean;
  } | null>(null);

  const [tab, setTab] = useState<string | null>("analysis");
  const [snapshot, setSnapshot] = useState<DiagnosticSnapshot | null>(null);
  const [building, setBuilding] = useState(false);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);

  const [entries, setEntries] = useState<ConversationEntry[]>([]);
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [streamChars, setStreamChars] = useState(0);
  const [streamMessage, setStreamMessage] = useState("");
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const activeRequest = useRef<string | null>(null);

  const [previewOpen, setPreviewOpen] = useState(false);
  const [preview, setPreview] = useState<AiPayloadPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  const [sessions, setSessions] = useState<AiSessionRecord[]>([]);
  const [sessionsBusy, setSessionsBusy] = useState(false);
  const [audits, setAudits] = useState<AiToolAuditEntry[]>([]);
  const [auditBusy, setAuditBusy] = useState(false);
  const [capability, setCapability] = useState<AiCapabilityReport | null>(null);
  const [probing, setProbing] = useState(false);

  const refreshAudits = useCallback(async () => {
    try {
      setAudits(await getAiToolAudit(30));
    } catch {
      // 审计读取失败不应影响 AI 解读主流程。
    }
  }, []);

  useEffect(() => {
    void refreshAudits();
  }, [refreshAudits]);

  const refreshSessions = useCallback(async () => {
    try {
      setSessions(await getAiSessions());
    } catch {
      // 会话记录读取失败不应影响 AI 解读主流程。
    }
  }, []);

  useEffect(() => {
    void refreshSessions();
  }, [refreshSessions]);

  useEffect(() => {
    let alive = true;
    void getAiSettings()
      .then((next) => {
        if (!alive) return;
        setView(next);
        setForm(next.settings);
      })
      .catch((reason) => {
        if (!alive) return;
        setSettingsMessage({
          text: reason instanceof Error ? reason.message : String(reason),
          error: true,
        });
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listenAiStream((event) => {
      if (event.requestId !== activeRequest.current) return;
      if (event.stage === "delta") {
        setStreamChars(event.charCount);
      } else if (event.message) {
        setStreamMessage(event.message);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const factsById = useMemo(() => {
    const map = new Map<string, DiagnosticFact>();
    for (const fact of snapshot?.facts ?? []) map.set(fact.id, fact);
    return map;
  }, [snapshot]);

  const currentSnapshotEntries = useMemo(
    () =>
      entries.filter(
        (entry) => snapshot && entry.answer.snapshotId === snapshot.snapshotId,
      ),
    [entries, snapshot],
  );

  const buildHistory = useCallback((): AiTurn[] => {
    if (!snapshot) return [];
    const history: AiTurn[] = [];
    for (const entry of currentSnapshotEntries.slice(-4)) {
      if (entry.question) {
        history.push({
          snapshotId: snapshot.snapshotId,
          role: "user",
          content: entry.question,
        });
      }
      history.push({
        snapshotId: snapshot.snapshotId,
        role: "assistant",
        content: entry.answer.summary.slice(0, 1200),
      });
    }
    return history;
  }, [currentSnapshotEntries, snapshot]);

  const refreshSnapshot = async () => {
    setBuilding(true);
    setSnapshotError(null);
    try {
      const next = await buildAiSnapshot();
      setSnapshot(next);
    } catch (reason) {
      setSnapshotError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBuilding(false);
    }
  };

  const analyze = async (nextQuestion: string | null) => {
    if (!snapshot || asking) return;
    const requestId = newAiRequestId("ai");
    activeRequest.current = requestId;
    setAsking(true);
    setCancelling(false);
    setAnalysisError(null);
    setStreamChars(0);
    setStreamMessage("正在整理本地诊断事实…");
    try {
      const answer = await aiAnalyze(
        requestId,
        snapshot.snapshotId,
        nextQuestion,
        buildHistory(),
      );
      setEntries((previous) => [...previous, { question: nextQuestion, answer, snapshot }]);
      void refreshSessions();
      void refreshAudits();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      if (message.includes("已取消")) {
        setStreamMessage("已取消本次 AI 请求。");
      } else {
        setAnalysisError(message);
      }
    } finally {
      setAsking(false);
      activeRequest.current = null;
    }
  };

  const cancel = async () => {
    const requestId = activeRequest.current;
    if (!requestId) return;
    setCancelling(true);
    setStreamMessage("正在取消…");
    try {
      await cancelAiRequest(requestId);
    } catch (reason) {
      setAnalysisError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCancelling(false);
    }
  };

  const save = async () => {
    setSaving(true);
    setSettingsMessage(null);
    try {
      const next = await saveAiSettings(form, apiKey.trim() ? apiKey.trim() : undefined);
      setView(next);
      setForm(next.settings);
      setApiKey("");
      setSettingsMessage({ text: "AI 设置已保存。", error: false });
    } catch (reason) {
      setSettingsMessage({
        text: reason instanceof Error ? reason.message : String(reason),
        error: true,
      });
    } finally {
      setSaving(false);
    }
  };

  const probe = async () => {
    setProbing(true);
    setSettingsMessage(null);
    try {
      setCapability(await probeAiCapabilities());
    } catch (reason) {
      setSettingsMessage({
        text: reason instanceof Error ? reason.message : String(reason),
        error: true,
      });
    } finally {
      setProbing(false);
    }
  };

  const clearKey = async () => {    if (!window.confirm("确定要清除已保存的 API 密钥吗？")) return;
    setClearingKey(true);
    setSettingsMessage(null);
    try {
      const next = await clearAiApiKey();
      setView(next);
      setSettingsMessage({ text: "API 密钥已清除。", error: false });
    } catch (reason) {
      setSettingsMessage({
        text: reason instanceof Error ? reason.message : String(reason),
        error: true,
      });
    } finally {
      setClearingKey(false);
    }
  };

  const exportAiReport = (format: "md" | "json") => {
    const filtered = sessions.filter(
      (record) => !snapshot || record.answer.snapshotId === snapshot.snapshotId,
    );
    const records =
      filtered.length > 0
        ? filtered
        : entries.map((entry, index) => ({
            id: `local-${index}`,
            savedAtUnixMs: entry.answer.generatedAtUnixMs,
            question: entry.question,
            answer: entry.answer,
          }));
    const source = records.length > 0 ? records : sessions;
    if (source.length === 0) return;
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    if (format === "md") {
      downloadAiReport(
        `system-lens-ai-${stamp}.md`,
        buildAiReportMarkdown(source, snapshot),
        "text/markdown",
      );
    } else {
      downloadAiReport(
        `system-lens-ai-${stamp}.json`,
        buildAiReportJson(source, snapshot),
        "application/json",
      );
    }
  };

  const openPreview = async () => {    if (!snapshot) return;
    setPreviewOpen(true);
    setPreviewLoading(true);
    setPreviewError(null);
    setPreview(null);
    try {
      setPreview(await previewAiPayload(snapshot.snapshotId, question.trim() || undefined));
    } catch (reason) {
      setPreviewError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setPreviewLoading(false);
    }
  };

  const aiEnabled = Boolean(view?.settings.enabled);

  return (
    <Stack gap="lg">
      {!desktop && (
        <Alert color="orange" icon={<IconAlertCircle />} title="浏览器演示模式">
          AI 解读需要 Tauri 桌面运行时。浏览器中仅展示界面，不会采集系统数据，也不会发起模型请求。
        </Alert>
      )}

      <Tabs value={tab} onChange={setTab} keepMounted={false}>
        <Tabs.List>
          <Tabs.Tab value="analysis" leftSection={<IconSparkles size={16} />}>
            AI 解读
          </Tabs.Tab>
          <Tabs.Tab value="settings" leftSection={<IconShieldCheck size={16} />}>
            模型与隐私
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="analysis" pt="md">
          <Stack gap="lg">
            <Card className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Title order={3}>本机健康快照</Title>
                  <Text size="sm" c="dimmed" mt={4}>
                    快照包含系统、温度、SMART、驱动、诊断和 24 小时趋势事实；生成过程只读且在本机完成。
                  </Text>
                </Box>
                <Badge color={aiEnabled ? "teal" : "gray"} variant="light">
                  {aiEnabled ? "AI 已启用" : "AI 已关闭"}
                </Badge>
              </Group>

              <Group mt="md">
                <Button
                  leftSection={<IconRefresh size={16} />}
                  loading={building}
                  onClick={() => void refreshSnapshot()}
                >
                  生成本机健康快照
                </Button>
                <Button
                  variant="light"
                  leftSection={<IconLock size={16} />}
                  disabled={!snapshot}
                  onClick={() => void openPreview()}
                >
                  预览将发送的数据
                </Button>
              </Group>

              {snapshotError && (
                <Alert color="red" mt="md" icon={<IconAlertCircle />}>
                  {snapshotError}
                </Alert>
              )}

              {building && (
                <Alert color="blue" mt="md" icon={<Loader size="sm" />}>
                  正在采集系统、传感器、驱动与诊断数据，通常需要数秒…
                </Alert>
              )}

              {snapshot && (
                <Box mt="md">
                  <Group gap="xs">
                    <Badge variant="light" color="indigo">
                      快照 {snapshot.snapshotId.slice(0, 18)}
                    </Badge>
                    <Badge variant="outline">{formatTime(snapshot.capturedAtUnixMs)}</Badge>
                    <Badge variant="outline">{snapshot.facts.length} 条事实</Badge>
                    {snapshot.excludedFactCount > 0 && (
                      <Badge color="orange" variant="light">
                        已排除 {snapshot.excludedFactCount} 条敏感事实
                      </Badge>
                    )}
                  </Group>
                  <Group gap="xs" mt="sm">
                    {snapshot.sources.map((source) => (
                      <Tooltip key={source.id} label={source.detail} multiline maw={360}>
                        <Badge color={source.ok ? "teal" : "red"} variant="dot">
                          {source.label}
                        </Badge>
                      </Tooltip>
                    ))}
                  </Group>
                  {snapshot.limitations.length > 0 && (
                    <Alert
                      color="yellow"
                      variant="light"
                      mt="sm"
                      icon={<IconAlertCircle />}
                      title="数据限制"
                    >
                      <Stack gap={2}>
                        {snapshot.limitations.slice(0, 6).map((limitation, index) => (
                          <Text key={`${limitation}-${index}`} size="xs">
                            {limitation}
                          </Text>
                        ))}
                      </Stack>
                    </Alert>
                  )}
                </Box>
              )}
            </Card>

            {snapshot && snapshot.localFindings.length > 0 && (
              <Card className="glass-card" radius="lg">
                <Group justify="space-between">
                  <Box>
                    <Title order={4}>本地规则诊断</Title>
                    <Text size="sm" c="dimmed" mt={4}>
                      由本地阈值和 SMART/告警规则判定，不依赖 AI；模型不可用时仍然有效。
                    </Text>
                  </Box>
                  <Badge variant="light" color="gray">
                    {snapshot.localFindings.length} 项
                  </Badge>
                </Group>
                <Box mt="md">
                  <FindingsList findings={snapshot.localFindings} facts={factsById} historical={false} />
                </Box>
              </Card>
            )}

            <Card className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Title order={4}>AI 健康解读</Title>
                  <Text size="sm" c="dimmed" mt={4}>
                    每次解读最多输出 3 个重点问题；结论必须引用本地事实，数据不足时会明确说明。
                  </Text>
                </Box>
                <ThemeIcon size={40} radius="xl" variant="light" color="grape">
                  <IconRobot size={22} />
                </ThemeIcon>
              </Group>

              {!aiEnabled && (
                <Alert color="gray" mt="md" icon={<IconLock />} title="AI 默认关闭">
                  启用后才会向所配置的模型服务发送脱敏事实。关闭状态下，快照、本地规则诊断、原始指标和报告完全可用。
                  <Button
                    size="xs"
                    variant="light"
                    mt="sm"
                    onClick={() => setTab("settings")}
                  >
                    前往模型与隐私设置
                  </Button>
                </Alert>
              )}

              <Group mt="md">
                <Button
                  leftSection={<IconSparkles size={16} />}
                  loading={asking}
                  disabled={!snapshot || !aiEnabled}
                  onClick={() => void analyze(null)}
                >
                  让 AI 解读本机健康
                </Button>
                <Button
                  variant="light"
                  color="red"
                  leftSection={<IconX size={16} />}
                  disabled={!asking}
                  loading={cancelling}
                  onClick={() => void cancel()}
                >
                  取消
                </Button>
              </Group>

              {asking && (
                <Alert color="blue" mt="md" icon={<Loader size="sm" />} title="AI 正在生成">
                  <Text size="sm">{streamMessage || "正在等待模型响应…"}</Text>
                  <Text size="xs" c="dimmed" mt={4}>
                    已接收 {streamChars.toLocaleString()} 个字符。可以随时取消。
                  </Text>
                </Alert>
              )}

              {analysisError && (
                <Alert color="red" mt="md" icon={<IconAlertCircle />} withCloseButton onClose={() => setAnalysisError(null)}>
                  {analysisError}
                </Alert>
              )}
            </Card>

            {entries.length > 0 && (
              <Stack gap="md">
                {entries.map((entry, index) => {
                  const historical =
                    !snapshot || entry.answer.snapshotId !== snapshot.snapshotId;
                  const entryFacts = new Map(
                    entry.snapshot.facts.map((fact) => [fact.id, fact]),
                  );
                  return (
                    <Card
                      key={`${entry.answer.requestId}-${index}`}
                      className="glass-card"
                      radius="lg"
                    >
                      <Group justify="space-between" align="flex-start">
                        <Box>
                          <Text size="xs" c="dimmed">
                            {entry.question ? "追问" : "健康解读"} ·{" "}
                            {formatTime(entry.answer.generatedAtUnixMs)}
                          </Text>
                          <Text fw={700} mt={2}>
                            {entry.question ?? "本机健康解读"}
                          </Text>
                        </Box>
                        <Group gap="xs">
                          {historical && (
                            <Badge color="gray" variant="light">
                              历史快照
                            </Badge>
                          )}
                          <Badge variant="outline" color={entry.answer.degraded ? "orange" : "teal"}>
                            {entry.answer.degraded ? "已降级" : entry.answer.model || "模型"}
                          </Badge>
                        </Group>
                      </Group>

                      {(entry.answer.checks?.length ?? 0) > 0 && (
                        <Box mt="md">
                          <Text size="xs" c="dimmed" fw={700}>
                            AI 请求的只读检查
                          </Text>
                          <Stack gap={6} mt={6}>
                            {(entry.answer.checks ?? []).map((check, checkIndex) => (
                              <Group
                                key={`${check.toolId}-${checkIndex}`}
                                gap="xs"
                                wrap="nowrap"
                                align="flex-start"
                              >
                                <Badge
                                  color={check.ok ? "teal" : "red"}
                                  variant="light"
                                  size="sm"
                                  style={{ flexShrink: 0 }}
                                >
                                  {check.label}
                                </Badge>
                                <Tooltip
                                  label={compactValue(check.summary)}
                                  multiline
                                  maw={420}
                                  position="top-start"
                                >
                                  <Text size="xs" c="dimmed" truncate>
                                    {check.ok ? "完成" : "失败"} · {check.durationMs} ms
                                    {check.error ? ` · ${check.error}` : ""}
                                    {check.reason ? ` · ${check.reason}` : ""}
                                  </Text>
                                </Tooltip>
                              </Group>
                            ))}
                          </Stack>
                        </Box>
                      )}

                      {entry.answer.degraded ? (
                        <Alert color="orange" mt="md" icon={<IconAlertCircle />} title="AI 解读不可用">
                          {entry.answer.degradedReason ?? "模型服务不可用。"}
                          <Text size="sm" mt="xs">
                            已保留本地规则诊断；原始指标、规则结论和报告仍可使用。
                          </Text>
                          {entry.answer.localFindings.length > 0 && (
                            <Box mt="sm">
                              <FindingsList
                                findings={entry.answer.localFindings}
                                facts={entryFacts}
                                historical={historical}
                              />
                            </Box>
                          )}
                        </Alert>
                      ) : (
                        <>
                          {entry.answer.summary && (
                            <Text size="sm" mt="md">
                              {entry.answer.summary}
                            </Text>
                          )}
                          <Box mt="md">
                            <FindingsList
                              findings={entry.answer.findings}
                              facts={entryFacts}
                              historical={historical}
                            />
                          </Box>
                          {entry.answer.limitations.length > 0 && (
                            <Alert color="gray" variant="light" mt="md" icon={<IconAlertCircle />} title="结论限制">
                              <Stack gap={2}>
                                {entry.answer.limitations.map((limitation, limitationIndex) => (
                                  <Text key={`${limitation}-${limitationIndex}`} size="xs">
                                    {limitation}
                                  </Text>
                                ))}
                              </Stack>
                            </Alert>
                          )}
                        </>
                      )}

                      <Divider my="sm" />
                      <Group justify="space-between">
                        <Text size="xs" c="dimmed">
                          {entry.answer.provider} · 提示词 {entry.answer.promptVersion} · 快照{" "}
                          {entry.answer.snapshotId.slice(0, 14)}
                        </Text>
                        {entry.answer.usage && (
                          <Text size="xs" c="dimmed">
                            tokens：{entry.answer.usage.totalTokens ?? "—"}（输入{" "}
                            {entry.answer.usage.promptTokens ?? "—"} / 输出{" "}
                            {entry.answer.usage.completionTokens ?? "—"}）
                          </Text>
                        )}
                      </Group>
                    </Card>
                  );
                })}
              </Stack>
            )}

            <Card className="glass-card" radius="lg">
              <Text fw={700}>围绕当前快照追问</Text>
              <Text size="xs" c="dimmed" mt={4}>
                追问只绑定当前快照 ID；切换或重新生成快照后，旧回答会标记为历史结果。
              </Text>
              <Group mt="sm" align="flex-end">
                <Textarea
                  style={{ flex: 1 }}
                  minRows={2}
                  maxRows={5}
                  autosize
                  placeholder="例如：磁盘是否应该更换？游戏闪退更可能是温度、驱动还是内存？"
                  value={question}
                  onChange={(event) => setQuestion(event.currentTarget.value)}
                  disabled={!snapshot}
                />
                <Button
                  leftSection={<IconSend size={16} />}
                  disabled={!snapshot || !aiEnabled || asking || !question.trim()}
                  onClick={() => {
                    const value = question.trim();
                    if (!value) return;
                    setQuestion("");
                    void analyze(value);
                  }}
                >
                  提问
                </Button>
              </Group>
            </Card>

            <Group justify="flex-end" gap="xs">
              <Button
                size="xs"
                variant="light"
                disabled={entries.length === 0 && sessions.length === 0}
                onClick={() => exportAiReport("md")}
              >
                导出 AI 报告（Markdown）
              </Button>
              <Button
                size="xs"
                variant="light"
                disabled={entries.length === 0 && sessions.length === 0}
                onClick={() => exportAiReport("json")}
              >
                导出 AI 报告（JSON）
              </Button>
            </Group>

            <Card className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Text fw={700}>本地会话记录</Text>
                  <Text size="xs" c="dimmed" mt={4}>
                    AI 回答默认保存在本机数据目录（最多 50 条，不包含原始事实和 API 密钥），可逐条删除或全部清除。
                  </Text>
                </Box>
                {sessions.length > 0 && (
                  <Button
                    size="xs"
                    variant="light"
                    color="red"
                    loading={sessionsBusy}
                    onClick={async () => {
                      if (!window.confirm("确定要清除全部本地 AI 会话记录吗？此操作不可撤销。")) {
                        return;
                      }
                      setSessionsBusy(true);
                      try {
                        await clearAiSessions();
                        setSessions([]);
                      } catch (reason) {
                        setAnalysisError(
                          reason instanceof Error ? reason.message : String(reason),
                        );
                      } finally {
                        setSessionsBusy(false);
                      }
                    }}
                  >
                    清除全部
                  </Button>
                )}
              </Group>
              {sessions.length === 0 ? (
                <Text size="sm" c="dimmed" mt="sm">
                  还没有本地 AI 会话记录。完成一次解读后会自动保存。
                </Text>
              ) : (
                <ScrollArea.Autosize mah={280} mt="sm">
                  <Stack gap="xs">
                    {sessions.map((session) => (
                      <Group
                        key={session.id}
                        justify="space-between"
                        wrap="nowrap"
                        align="flex-start"
                      >
                        <Box style={{ minWidth: 0 }}>
                          <Text size="sm" fw={600} truncate>
                            {session.question ?? "本机健康解读"}
                          </Text>
                          <Text size="xs" c="dimmed" truncate>
                            {formatTime(session.savedAtUnixMs)} · {session.answer.model} ·{" "}
                            {session.answer.degraded ? "已降级" : "AI 结论"} · 快照{" "}
                            {session.answer.snapshotId.slice(0, 14)}
                          </Text>
                        </Box>
                        <Tooltip label="删除这条记录">
                          <ActionIcon
                            variant="subtle"
                            color="red"
                            disabled={sessionsBusy}
                            onClick={async () => {
                              setSessionsBusy(true);
                              try {
                                await deleteAiSession(session.id);
                                setSessions((previous) =>
                                  previous.filter((item) => item.id !== session.id),
                                );
                              } catch (reason) {
                                setAnalysisError(
                                  reason instanceof Error ? reason.message : String(reason),
                                );
                              } finally {
                                setSessionsBusy(false);
                              }
                            }}
                          >
                            <IconTrash size={16} />
                          </ActionIcon>
                        </Tooltip>
                      </Group>
                    ))}
                  </Stack>
                </ScrollArea.Autosize>
              )}
            </Card>
          </Stack>
        </Tabs.Panel>

        <Tabs.Panel value="settings" pt="md">
          <Stack gap="lg">
            <Card className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Title order={4}>模型服务</Title>
                  <Text size="sm" c="dimmed" mt={4}>
                    通过厂商无关的 Provider 接口接入；当前支持 OpenAI 兼容的 /chat/completions
                    流式接口。
                  </Text>
                </Box>
                <Switch
                  checked={form.enabled}
                  onChange={(event) => {
                    const checked = event.currentTarget.checked;
                    if (
                      checked &&
                      !form.enabled &&
                      !window.confirm(
                        "启用 AI 后将向所配置的模型服务发送脱敏后的系统诊断事实（系统指标、温度/SMART 摘要、驱动统计、诊断计数和 24 小时趋势），用于生成健康解读。不会发送主机名、用户名、序列号、MAC/IP、命令行或完整路径。是否启用？",
                      )
                    ) {
                      return;
                    }
                    setForm({ ...form, enabled: checked });
                  }}
                  label={form.enabled ? "已启用" : "默认关闭"}
                  disabled={!desktop}
                />
              </Group>

              <SimpleGrid cols={{ base: 1, md: 2 }} mt="md">
                <Select
                  label="服务类型"
                  data={[
                    {
                      value: "openai-compatible",
                      label: "OpenAI 兼容接口（DeepSeek / OpenAI / Ollama 等）",
                    },
                  ]}
                  value={form.provider}
                  onChange={(value) =>
                    setForm({ ...form, provider: value ?? "openai-compatible" })
                  }
                  allowDeselect={false}
                  disabled={!desktop}
                />
                <Textarea
                  label="服务地址"
                  description="必须为 HTTPS；仅本机回环地址允许 HTTP"
                  autosize
                  minRows={1}
                  maxRows={3}
                  value={form.baseUrl}
                  onChange={(event) => setForm({ ...form, baseUrl: event.currentTarget.value })}
                  disabled={!desktop}
                />
                <Textarea
                  label="模型名称"
                  autosize
                  minRows={1}
                  maxRows={2}
                  value={form.model}
                  onChange={(event) => setForm({ ...form, model: event.currentTarget.value })}
                  disabled={!desktop}
                />
                <PasswordInput
                  label="API 密钥"
                  description={
                    view?.hasApiKey
                      ? `已保存（${view.keyStorage}）；留空保持不变`
                      : `使用 ${view?.keyStorage ?? "Windows DPAPI"} 加密保存`
                  }
                  placeholder={view?.hasApiKey ? "••••••••（留空保持不变）" : "输入 API 密钥"}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.currentTarget.value)}
                  leftSection={<IconKey size={16} />}
                  disabled={!desktop}
                />
                <NumberInput
                  label="最大输出 tokens"
                  min={128}
                  max={4096}
                  step={128}
                  value={form.maxOutputTokens}
                  onChange={(value) =>
                    setForm({
                      ...form,
                      maxOutputTokens:
                        typeof value === "number" ? value : Number(value) || 1200,
                    })
                  }
                  disabled={!desktop}
                />
                <NumberInput
                  label="超时时间（秒）"
                  min={10}
                  max={180}
                  step={10}
                  value={form.timeoutSeconds}
                  onChange={(value) =>
                    setForm({
                      ...form,
                      timeoutSeconds:
                        typeof value === "number" ? value : Number(value) || 60,
                    })
                  }
                  disabled={!desktop}
                />
              </SimpleGrid>

              <Switch
                mt="md"
                checked={form.nativeTools}
                onChange={(event) =>
                  setForm({ ...form, nativeTools: event.currentTarget.checked })
                }
                label="原生工具调用（tool_calls）"
                description="模型支持时最多执行 2 轮、6 项只读检查；关闭时使用受控 requestedChecks JSON 计划协议。建议先检测模型能力。"
                disabled={!desktop}
              />

              <Group mt="md">
                <Button
                  variant="light"
                  loading={probing}
                  onClick={() => void probe()}
                  disabled={!desktop}
                >
                  检测模型能力
                </Button>
                {capability && (
                  <Group gap="xs">
                    <Badge color={capability.reachable ? "teal" : "red"} variant="light">
                      {capability.reachable ? "服务可达" : "服务不可达"}
                    </Badge>
                    <Badge color={capability.structuredOutput ? "teal" : "orange"} variant="light">
                      结构化输出 {capability.structuredOutput ? "可用" : "未验证"}
                    </Badge>
                    <Badge color={capability.nativeTools ? "teal" : "gray"} variant="light">
                      原生工具 {capability.nativeTools ? "可用" : "不可用"}
                    </Badge>
                    <Badge variant="outline">{capability.latencyMs} ms</Badge>
                  </Group>
                )}
              </Group>

              {capability && (
                <Alert
                  color={capability.nativeTools ? "teal" : "gray"}
                  variant="light"
                  mt="sm"
                >
                  <Text size="xs">
                    模型：{capability.model || form.model}
                    {capability.note ? ` · ${capability.note}` : ""}
                  </Text>
                  <Text size="xs" c="dimmed" mt={4}>
                    检测只发送最小能力探测请求，不包含本机诊断数据。若原生工具不可用，请保持关闭并使用
                    requestedChecks 协议。
                  </Text>
                </Alert>
              )}

              <Group mt="md">
                <Button loading={saving} onClick={() => void save()} disabled={!desktop}>
                  保存设置
                </Button>
                {view?.hasApiKey && (
                  <Button
                    variant="light"
                    color="red"
                    loading={clearingKey}
                    onClick={() => void clearKey()}
                    disabled={!desktop}
                  >
                    清除 API 密钥
                  </Button>
                )}
              </Group>

              {settingsMessage && (
                <Alert
                  color={settingsMessage.error ? "red" : "teal"}
                  mt="md"
                  icon={settingsMessage.error ? <IconAlertCircle /> : <IconShieldCheck />}
                >
                  {settingsMessage.text}
                </Alert>
              )}
            </Card>

            <Card className="glass-card" radius="lg">
              <Title order={4}>默认发送与默认排除</Title>
              <SimpleGrid cols={{ base: 1, md: 2 }} mt="md">
                <Box>
                  <Text size="sm" fw={700} c="teal">
                    发送（脱敏后）
                  </Text>
                  <Stack gap={4} mt="xs">
                    {[
                      "系统版本、CPU/内存/磁盘/网络指标",
                      "温度、SMART 摘要与本地告警",
                      "驱动数量与问题设备名称",
                      "崩溃/待重启/电池等诊断计数",
                      "最近 24 小时趋势统计",
                      "本地规则已生成的结论与证据 ID",
                    ].map((item) => (
                      <Text key={item} size="xs" c="dimmed">
                        · {item}
                      </Text>
                    ))}
                  </Stack>
                </Box>
                <Box>
                  <Text size="sm" fw={700} c="red">
                    默认排除
                  </Text>
                  <Stack gap={4} mt="xs">
                    {[
                      "主机名、用户名、SID",
                      "硬件序列号、设备 ID、MAC/IP",
                      "完整本地路径与进程命令行",
                      "事件日志全文与原始转储",
                      "API 密钥（仅经 DPAPI 保存）",
                      "任何写操作或可执行命令",
                    ].map((item) => (
                      <Text key={item} size="xs" c="dimmed">
                        · {item}
                      </Text>
                    ))}
                  </Stack>
                </Box>
              </SimpleGrid>
              <Alert color="blue" variant="light" mt="md" icon={<IconLock />}>
                模型只能请求白名单内的只读检查（最近事件元数据、驱动问题、网络摘要、启动项、计划任务、温度与存储健康、系统诊断），由宿主校验参数、限制超时并记录审计；没有 shell、脚本、路径或写操作通道。写操作仍需本地 UI
                预览、二次确认和确认令牌。发送前可在“AI 解读”页预览完整载荷。
              </Alert>
            </Card>

            <Card className="glass-card" radius="lg">
              <Group justify="space-between" align="flex-start">
                <Box>
                  <Title order={4}>只读检查审计</Title>
                  <Text size="sm" c="dimmed" mt={4}>
                    本机记录工具 ID、耗时、状态和脱敏参数，不记录结果正文；最多读取最近 30 条。
                  </Text>
                </Box>
                <Group gap="xs">
                  <Button
                    size="xs"
                    variant="light"
                    loading={auditBusy}
                    onClick={async () => {
                      setAuditBusy(true);
                      try {
                        await refreshAudits();
                      } finally {
                        setAuditBusy(false);
                      }
                    }}
                  >
                    刷新
                  </Button>
                  {audits.length > 0 && (
                    <Button
                      size="xs"
                      variant="light"
                      color="red"
                      loading={auditBusy}
                      onClick={async () => {
                        if (!window.confirm("确定要清除全部只读检查审计记录吗？")) return;
                        setAuditBusy(true);
                        try {
                          await clearAiToolAudit();
                          setAudits([]);
                        } catch (reason) {
                          setSettingsMessage({
                            text: reason instanceof Error ? reason.message : String(reason),
                            error: true,
                          });
                        } finally {
                          setAuditBusy(false);
                        }
                      }}
                    >
                      清除
                    </Button>
                  )}
                </Group>
              </Group>
              {audits.length === 0 ? (
                <Text size="sm" c="dimmed" mt="sm">
                  还没有只读检查审计记录。AI 请求只读检查后会写入本机。
                </Text>
              ) : (
                <ScrollArea.Autosize mah={280} mt="sm">
                  <Stack gap="xs">
                    {audits.map((entry, index) => (
                      <Group
                        key={`${entry.requestId}-${entry.toolId}-${index}`}
                        justify="space-between"
                        wrap="nowrap"
                        align="flex-start"
                      >
                        <Box style={{ minWidth: 0 }}>
                          <Group gap="xs">
                            <Badge
                              size="sm"
                              color={entry.ok ? "teal" : "red"}
                              variant="light"
                            >
                              {entry.ok ? "完成" : "失败"}
                            </Badge>
                            <Text size="sm" fw={600} truncate>
                              {entry.toolId}
                            </Text>
                          </Group>
                          <Text size="xs" c="dimmed" truncate>
                            {formatTime(entry.savedAtUnixMs)} · {entry.durationMs} ms
                            {entry.reason ? ` · ${entry.reason}` : ""}
                            {entry.error ? ` · ${entry.error}` : ""}
                          </Text>
                        </Box>
                        <Tooltip
                          label={compactValue(entry.parameters)}
                          multiline
                          maw={420}
                          position="top-start"
                        >
                          <Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
                            {entry.requestId.slice(0, 16)}
                          </Text>
                        </Tooltip>
                      </Group>
                    ))}
                  </Stack>
                </ScrollArea.Autosize>
              )}
            </Card>
          </Stack>
        </Tabs.Panel>
      </Tabs>

      <Modal
        opened={previewOpen}
        onClose={() => setPreviewOpen(false)}
        title="将发送给模型的数据（脱敏后）"
        size="xl"
        scrollAreaComponent={ScrollArea}
      >
        {previewLoading && (
          <Group justify="center" py="xl">
            <Loader />
          </Group>
        )}
        {previewError && (
          <Alert color="red" icon={<IconAlertCircle />}>
            {previewError}
          </Alert>
        )}
        {preview && !previewLoading && (
          <Stack gap="sm">
            <Group gap="xs">
              <Badge variant="light" color="indigo">
                {preview.model}
              </Badge>
              <Badge variant="outline">估算输入 {preview.estimatedInputTokens} tokens</Badge>
              <Badge color="teal" variant="light">
                已移除 {preview.redaction.removedFields} 个敏感字段
              </Badge>
              {preview.redaction.excludedFacts > 0 && (
                <Badge color="orange" variant="light">
                  排除 {preview.redaction.excludedFacts} 条事实
                </Badge>
              )}
            </Group>
            <Text size="xs" c="dimmed">
              目标服务：{preview.baseUrl}
            </Text>
            <Code block style={{ maxHeight: 420, overflow: "auto", whiteSpace: "pre-wrap" }}>
              {JSON.stringify(preview.payload, null, 2)}
            </Code>
            <Group gap="xs">
              {preview.redaction.appliedRules.map((rule) => (
                <Badge key={rule} size="xs" variant="light" color="gray">
                  {rule}
                </Badge>
              ))}
            </Group>
          </Stack>
        )}
      </Modal>
    </Stack>
  );
}
