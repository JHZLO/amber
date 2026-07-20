// Rust claude 브리지의 프론트 wrapper.
// Tauri v2 는 JS camelCase 인자를 Rust snake_case 로 자동 변환한다 (claudePath → claude_path).

import { invoke, Channel } from "@tauri-apps/api/core";
import type { Confidence } from "../types";

export interface GeneratedNote {
  title: string;
  summary: string;
  detail_markdown: string;
  tags: string[];
  confidence_suggestion: Confidence;
  source_excerpt: string | null;
}

export interface InvocationMeta {
  model: string;
  session_id: string | null;
  cost_usd: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  duration_ms: number | null;
}

export interface GenerateResult {
  note: GeneratedNote;
  meta: InvocationMeta;
}

/** Rust AiError 와 동일 구조. invoke rejection 으로 전달됨 */
export interface AiError {
  code:
    | "EMPTY_INPUT"
    | "AI_NOT_FOUND"
    | "AI_AUTH"
    | "AI_RATE_LIMIT"
    | "AI_TIMEOUT"
    | "AI_BAD_ENVELOPE"
    | "AI_BAD_CONTRACT"
    | "AI_ERROR"
    | "SPAWN_ERROR"
    | "STDIN_ERROR"
    | "WAIT_ERROR";
  message: string;
}

export function isAiError(e: unknown): e is AiError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

/** invoke rejection 을 사용자 안내 문구로 변환 (UI 공용) */
export function friendlyError(e: unknown): string {
  if (!isAiError(e)) return String(e);
  switch (e.code) {
    case "AI_NOT_FOUND":
      return "claude CLI를 찾을 수 없어요. 설정(⚙)에서 경로를 확인하세요.";
    case "AI_AUTH":
      return "claude 인증이 필요해요. 터미널에서 `claude` 로그인 후 다시 시도하세요.";
    case "AI_RATE_LIMIT":
      return "사용량 한도에 도달했어요. 잠시 후 다시 시도하세요.";
    case "AI_TIMEOUT":
      return "응답이 너무 오래 걸렸어요. 다시 시도해 주세요.";
    case "AI_BAD_ENVELOPE":
    case "AI_BAD_CONTRACT":
      return "정리 결과를 해석하지 못했어요. 다시 생성하거나 수동으로 작성하세요.";
    default:
      return e.message;
  }
}

export async function aiGenerate(params: {
  transcript: string;
  instruction?: string | null;
  model?: string | null;
  cliPath?: string | null;
  provider?: string | null;
  timeoutSecs?: number | null;
}): Promise<GenerateResult> {
  return invoke<GenerateResult>("ai_generate", {
    transcript: params.transcript,
    instruction: params.instruction ?? null,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
  });
}

/** 이미 정리된 노트 + 사용자 프롬프트로 노트를 보강 → 보강된 노트 반환 */
export async function aiAugment(params: {
  title: string;
  summary: string;
  tags: string[];
  markdown: string;
  instruction: string;
  model?: string | null;
  cliPath?: string | null;
  provider?: string | null;
  timeoutSecs?: number | null;
}): Promise<GenerateResult> {
  return invoke<GenerateResult>("ai_augment", {
    title: params.title,
    summary: params.summary,
    tags: params.tags,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
  });
}

export interface NoteComposeResult {
  markdown: string;
  meta: InvocationMeta;
}

/** 필기노트 작성/보강: 자유 형식 마크다운 + 지시 → 완성본(raw 마크다운).
 *  빈 본문이면 요청 주제로 처음부터 작성. 개념 정리와 달리 JSON 계약이 아니라 마크다운을 바로 받는다
 *  (큰 마크다운을 JSON 문자열에 담는 이중 파싱의 간헐적 실패를 원천 차단). */
export async function aiNoteCompose(params: {
  title: string;
  markdown: string;
  instruction: string;
  model?: string | null;
  cliPath?: string | null;
  provider?: string | null;
  timeoutSecs?: number | null;
}): Promise<NoteComposeResult> {
  return invoke<NoteComposeResult>("ai_note_compose", {
    title: params.title,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
  });
}

/** 필기노트 작성/보강 (스트리밍). 생성 텍스트가 onDelta 로 조각조각 들어오고,
 *  최종 완성본은 반환값 markdown 으로 확정된다(스트림 마지막 봉투 기준). */
export async function aiNoteComposeStream(
  params: {
    title: string;
    markdown: string;
    instruction: string;
    model?: string | null;
    cliPath?: string | null;
    provider?: string | null;
    timeoutSecs?: number | null;
  },
  onDelta: (text: string) => void,
): Promise<NoteComposeResult> {
  const channel = new Channel<string>();
  channel.onmessage = onDelta;
  return invoke<NoteComposeResult>("ai_note_compose_stream", {
    title: params.title,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    onDelta: channel,
  });
}

export interface NoteAskResult {
  answer: string;
  meta: InvocationMeta;
}

/** 필기노트 인라인 질문: 드래그한 문장 + 질문 + 노트 문맥 → 짧은 답변.
 *  history 를 주면 같은 선택 부분에 대한 이전 문답을 이어받아 후속 질문으로 답한다. */
export async function aiNoteAsk(params: {
  selection: string;
  question: string;
  noteMarkdown: string;
  history?: { question: string; answer: string }[];
  model?: string | null;
  cliPath?: string | null;
  provider?: string | null;
  timeoutSecs?: number | null;
}): Promise<NoteAskResult> {
  return invoke<NoteAskResult>("ai_note_ask", {
    selection: params.selection,
    question: params.question,
    noteMarkdown: params.noteMarkdown,
    history: params.history?.length
      ? params.history.map((t) => ({ question: t.question, answer: t.answer }))
      : null,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
  });
}

export async function aiHealth(cliPath?: string | null): Promise<string> {
  return invoke<string>("ai_health", { cliPath: cliPath ?? null });
}

// ---- AI CLI 자동 감지 (온보딩/설정) ----

export interface DetectedCli {
  id: "claude" | "codex" | "gemini";
  name: string;
  path: string;
  version: string;
}

/** 로그인 셸 PATH 에서 설치된 AI CLI 를 감지 (경로 + 버전) */
export async function detectAiClis(): Promise<DetectedCli[]> {
  return invoke<DetectedCli[]>("detect_ai_clis");
}
