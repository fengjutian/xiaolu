import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AiAnswer,
  AiCapabilityReport,
  AiPayloadPreview,
  AiSessionRecord,
  AiSettings,
  AiSettingsView,
  AiStreamEvent,
  AiToolAuditEntry,
  AiTurn,
  DiagnosticSnapshot,
} from "./types";

export const DEFAULT_AI_SETTINGS: AiSettings = {
  enabled: false,
  provider: "openai-compatible",
  baseUrl: "https://api.deepseek.com/v1",
  model: "deepseek-chat",
  maxOutputTokens: 1200,
  timeoutSeconds: 60,
  nativeTools: false,
};

export async function getAiSettings(): Promise<AiSettingsView> {
  if (!isTauri()) {
    return {
      settings: { ...DEFAULT_AI_SETTINGS },
      hasApiKey: false,
      keyStorage: "Windows DPAPI（当前用户）",
    };
  }
  return invoke<AiSettingsView>("get_ai_settings");
}

export async function saveAiSettings(
  settings: AiSettings,
  apiKey?: string,
): Promise<AiSettingsView> {
  if (!isTauri()) {
    throw new Error("AI 设置需要桌面运行时才能保存。");
  }
  return invoke<AiSettingsView>("save_ai_settings", { settings, apiKey });
}

export async function clearAiApiKey(): Promise<AiSettingsView> {
  if (!isTauri()) {
    throw new Error("AI 凭据管理需要桌面运行时。");
  }
  return invoke<AiSettingsView>("clear_ai_api_key");
}

export async function buildAiSnapshot(): Promise<DiagnosticSnapshot> {
  if (!isTauri()) {
    throw new Error("AI 解读需要桌面运行时；浏览器演示模式不采集本机数据。");
  }
  return invoke<DiagnosticSnapshot>("build_ai_snapshot");
}

export async function previewAiPayload(
  snapshotId: string,
  question?: string,
): Promise<AiPayloadPreview> {
  if (!isTauri()) {
    throw new Error("发送预览需要桌面运行时。");
  }
  return invoke<AiPayloadPreview>("preview_ai_payload", {
    snapshotId,
    question: question ?? null,
  });
}

export async function aiAnalyze(
  requestId: string,
  snapshotId: string,
  question: string | null,
  history: AiTurn[],
): Promise<AiAnswer> {
  if (!isTauri()) {
    throw new Error("AI 解读需要桌面运行时。");
  }
  return invoke<AiAnswer>("ai_analyze", {
    requestId,
    snapshotId,
    question,
    history,
  });
}

export async function cancelAiRequest(requestId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_ai_request", { requestId });
}

export async function getAiSessions(): Promise<AiSessionRecord[]> {
  if (!isTauri()) return [];
  return invoke<AiSessionRecord[]>("get_ai_sessions");
}

export async function deleteAiSession(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("delete_ai_session", { sessionId });
}

export async function clearAiSessions(): Promise<void> {
  if (!isTauri()) return;
  await invoke("clear_ai_sessions");
}

export async function getAiToolAudit(limit = 50): Promise<AiToolAuditEntry[]> {
  if (!isTauri()) return [];
  return invoke<AiToolAuditEntry[]>("get_ai_tool_audit", { limit });
}

export async function clearAiToolAudit(): Promise<void> {
  if (!isTauri()) return;
  await invoke("clear_ai_tool_audit");
}

export async function probeAiCapabilities(): Promise<AiCapabilityReport> {
  if (!isTauri()) {
    throw new Error("模型能力探测需要桌面运行时。");
  }
  return invoke<AiCapabilityReport>("probe_ai_capabilities");
}

export async function listenAiStream(
  handler: (event: AiStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<AiStreamEvent>("ai-stream", (event) => handler(event.payload));
}

export function newAiRequestId(prefix = "req"): string {
  const random = Math.random().toString(36).slice(2, 8);
  return `${prefix}-${Date.now().toString(36)}-${random}`;
}
