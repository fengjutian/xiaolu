import type { AiFinding, AiSessionRecord, DiagnosticFact, DiagnosticSnapshot } from "./types";

const APP_VERSION = "0.55.0";

const factMap = (snapshot?: DiagnosticSnapshot | null) => {
  const map = new Map<string, DiagnosticFact>();
  for (const fact of snapshot?.facts ?? []) map.set(fact.id, fact);
  return map;
};

const compact = (value: unknown, max = 300) => {
  try {
    const text = JSON.stringify(value);
    return text.length > max ? `${text.slice(0, max)}…` : text;
  } catch {
    return String(value);
  }
};

const findingLines = (finding: AiFinding, facts: Map<string, DiagnosticFact>) => {
  const lines: string[] = [];
  lines.push(`#### [${finding.severity}] ${finding.title}`);
  lines.push("");
  lines.push(finding.summary);
  lines.push("");
  lines.push(`- 置信度：${finding.confidence}`);
  for (const reference of finding.evidenceRefs) {
    const fact = facts.get(reference);
    lines.push(
      fact
        ? `- 证据：${fact.title}（${fact.source}，${new Date(fact.capturedAtUnixMs).toLocaleString()}）：${compact(fact.value)}`
        : `- 证据：${reference}（历史快照，未包含当前事实）`,
    );
  }
  for (const recommendation of finding.recommendations) {
    lines.push(
      `- 建议：${recommendation.label}（${recommendation.kind}，风险 ${recommendation.risk}${recommendation.requiresConfirmation ? "，需确认" : ""}）`,
    );
  }
  for (const limitation of finding.limitations) {
    lines.push(`- 限制：${limitation}`);
  }
  lines.push("");
  return lines;
};

export function buildAiSectionMarkdown(records: AiSessionRecord[]): string[] {
  const lines: string[] = [
    "## AI 解读",
    "",
    "> AI 生成的解释性结论。每条结论必须引用本地诊断事实；数据缺失、模型降级和只读检查失败会明确标注。AI 结论不替代本地规则、原始指标或人工判断。",
    "",
  ];
  if (records.length === 0) {
    lines.push("没有可包含的 AI 解读记录。", "");
    return lines;
  }
  const noFacts = new Map<string, DiagnosticFact>();
  for (const record of records) {
    const answer = record.answer;
    lines.push(`### ${record.question ?? "本机健康解读"}`, "");
    lines.push(
      `- 生成时间：${new Date(answer.generatedAtUnixMs).toLocaleString()}`,
      `- 模型：${answer.provider} / ${answer.model}`,
      `- 提示词版本：${answer.promptVersion}`,
      `- 快照：${answer.snapshotId}`,
      `- 状态：${answer.degraded ? "已降级（使用本地规则）" : "AI 正常输出"}`,
      "",
    );
    if (answer.degraded) {
      lines.push(`> ${answer.degradedReason ?? "模型不可用"}`, "");
    }
    if (answer.summary) lines.push(answer.summary, "");
    for (const finding of answer.findings) {
      lines.push(...findingLines(finding, noFacts));
    }
    if (answer.checks && answer.checks.length > 0) {
      lines.push("#### 只读检查", "");
      for (const check of answer.checks) {
        lines.push(
          `- ${check.label}（${check.toolId}）：${check.ok ? "完成" : "失败"}，${check.durationMs} ms${check.error ? `，${check.error}` : ""}`,
        );
      }
      lines.push("");
    }
    if (answer.limitations.length > 0) {
      lines.push("#### 结论限制", "");
      for (const limitation of answer.limitations) lines.push(`- ${limitation}`);
      lines.push("");
    }
  }
  return lines;
}

export function buildAiReportMarkdown(
  records: AiSessionRecord[],
  snapshot?: DiagnosticSnapshot | null,
): string {
  const facts = factMap(snapshot);
  const lines: string[] = [
    "# 小鲁 · System Lens AI 解读报告",
    "",
    `- 生成时间：${new Date().toLocaleString()}`,
    `- 应用版本：${APP_VERSION}`,
    `- 快照：${snapshot?.snapshotId ?? records[0]?.answer.snapshotId ?? "未提供"}`,
    "- 说明：本报告包含 AI 生成的解释。每条结论必须引用本地诊断事实；数据缺失、模型降级和工具失败会明确标注。AI 结论不替代本地规则、原始指标或人工判断。",
    "",
  ];

  if (snapshot) {
    lines.push("## 本地规则诊断", "");
    if (snapshot.localFindings.length === 0) {
      lines.push("本地规则未发现需要关注的项。", "");
    } else {
      for (const finding of snapshot.localFindings) {
        lines.push(...findingLines(finding, facts));
      }
    }
    if (snapshot.limitations.length > 0) {
      lines.push("### 数据限制", "");
      for (const limitation of snapshot.limitations) lines.push(`- ${limitation}`);
      lines.push("");
    }
  }

  if (records.length === 0) {
    lines.push("## AI 解读", "", "没有可导出的 AI 解读记录。", "");
  }

  for (const record of records) {
    const answer = record.answer;
    const recordFacts =
      snapshot && answer.snapshotId === snapshot.snapshotId
        ? facts
        : new Map<string, DiagnosticFact>();
    lines.push(`## ${record.question ?? "本机健康解读"}`, "");
    lines.push(
      `- 生成时间：${new Date(answer.generatedAtUnixMs).toLocaleString()}`,
      `- 模型：${answer.provider} / ${answer.model}`,
      `- 提示词版本：${answer.promptVersion}`,
      `- 快照：${answer.snapshotId}`,
      `- 状态：${answer.degraded ? "已降级（使用本地规则）" : "AI 正常输出"}`,
      "",
    );
    if (answer.degraded) {
      lines.push(`> ${answer.degradedReason ?? "模型不可用"}`, "");
    }
    if (answer.summary) {
      lines.push(answer.summary, "");
    }
    for (const finding of answer.findings) {
      lines.push(...findingLines(finding, recordFacts));
    }
    if (answer.checks && answer.checks.length > 0) {
      lines.push("### 只读检查", "");
      for (const check of answer.checks) {
        lines.push(
          `- ${check.label}（${check.toolId}）：${check.ok ? "完成" : "失败"}，${check.durationMs} ms${check.error ? `，${check.error}` : ""}`,
        );
      }
      lines.push("");
    }
    if (answer.limitations.length > 0) {
      lines.push("### 结论限制", "");
      for (const limitation of answer.limitations) lines.push(`- ${limitation}`);
      lines.push("");
    }
    if (answer.usage) {
      lines.push(
        `- Token 用量：总 ${answer.usage.totalTokens ?? "—"}（输入 ${answer.usage.promptTokens ?? "—"} / 输出 ${answer.usage.completionTokens ?? "—"}）`,
        "",
      );
    }
  }

  lines.push("---", "", "报告结束。所有数据均在本机生成；AI 载荷在发送前已脱敏。");
  return lines.join("\n");
}

export function buildAiReportJson(
  records: AiSessionRecord[],
  snapshot?: DiagnosticSnapshot | null,
): string {
  return JSON.stringify(
    {
      schemaVersion: 1,
      generatedAt: new Date().toISOString(),
      application: { name: "小鲁 · 系统透镜", version: APP_VERSION },
      note:
        "AI generated interpretation. Findings must reference local evidence; AI output is not a substitute for local rules or human judgement.",
      snapshotId: snapshot?.snapshotId ?? null,
      localFindings: snapshot?.localFindings ?? [],
      limitations: snapshot?.limitations ?? [],
      aiSessions: records,
    },
    null,
    2,
  );
}

export function downloadAiReport(filename: string, content: string, type: string) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
