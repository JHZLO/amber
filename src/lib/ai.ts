// Rust claude 브리지의 프론트 wrapper.
// Tauri v2 는 JS camelCase 인자를 Rust snake_case 로 자동 변환한다 (claudePath → claude_path).

import { invoke, Channel } from "@tauri-apps/api/core";
import { ulid } from "ulid";
import type { Confidence } from "../types";
import { getLang } from "./i18n";
import { errText, isCodedError } from "./errors";

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

/** Rust 에러 봉투 (앱 공용) — 구조·코드 목록은 lib/errors.ts 가 정본 */
export type { CodedError as AiError } from "./errors";
export { isCodedError as isAiError } from "./errors";

/** invoke rejection 을 사용자 안내 문구로 — 문구 매핑은 lib/errors.ts 한 곳에만 둔다 */
export const friendlyError = errText;

/** 인증 만료(AI_AUTH)를 알림받을 핸들러 — App 이 로그인 모달을 연다.
 *  만료는 특정 화면의 사고가 아니라 앱 전체가 같이 멈추는 상태라, 기능마다 안내를 따로 두지 않고
 *  호출 지점 한 곳에서 잡아 같은 창을 띄운다. */
let onAuthRequired: (() => void) | null = null;

export function setAuthRequiredHandler(fn: (() => void) | null): void {
  onAuthRequired = fn;
}

/** AI 를 쓰는 커맨드 공용 invoke. 인증 만료만 가로채 알리고, 에러는 그대로 다시 던진다 —
 *  각 화면의 기존 에러 표시를 뺏지 않는다(모달이 뜬 뒤에도 무엇이 실패했는지 남아야 한다). */
export async function aiInvoke<T>(
  cmd: string,
  args: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isCodedError(e) && e.code === "AI_AUTH") onAuthRequired?.();
    throw e;
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
  return aiInvoke<GenerateResult>("ai_generate", {
    transcript: params.transcript,
    instruction: params.instruction ?? null,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    lang: getLang(),
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
  return aiInvoke<GenerateResult>("ai_augment", {
    title: params.title,
    summary: params.summary,
    tags: params.tags,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    lang: getLang(),
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
  return aiInvoke<NoteComposeResult>("ai_note_compose", {
    title: params.title,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    lang: getLang(),
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
    /** 중단 버튼이 aiCancel 에 넘길 키. 생략하면 취소 불가 */
    cancelKey?: string | null;
  },
  onDelta: (text: string) => void,
): Promise<NoteComposeResult> {
  const channel = new Channel<string>();
  channel.onmessage = onDelta;
  return aiInvoke<NoteComposeResult>("ai_note_compose_stream", {
    title: params.title,
    markdown: params.markdown,
    instruction: params.instruction,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    lang: getLang(),
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
  return aiInvoke<NoteAskResult>("ai_note_ask", {
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
    lang: getLang(),
  });
}

export interface ErdResult {
  mermaid: string;
  meta: InvocationMeta;
}

/** 다이어그램 탭: 스키마 DDL → ERD mermaid 소스 (스트리밍).
 *  current 를 주면 에디터에 열려 있는 기존 소스의 문법·구성을 이어받아 확장한다. */
export async function aiErdGenerateStream(
  params: {
    ddl: string;
    instruction?: string | null;
    current?: string | null;
    model?: string | null;
    cliPath?: string | null;
    provider?: string | null;
    timeoutSecs?: number | null;
    cancelKey?: string | null;
  },
  onDelta: (text: string) => void,
): Promise<ErdResult> {
  const channel = new Channel<string>();
  channel.onmessage = onDelta;
  return aiInvoke<ErdResult>("ai_erd_generate_stream", {
    ddl: params.ddl,
    instruction: params.instruction ?? null,
    current: params.current ?? null,
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    lang: getLang(),
    onDelta: channel,
    cancelKey: params.cancelKey ?? null,
  });
}

/** 실행마다 새로 만드는 취소 키 — 다른 창에서 도는 리포트 생성까지 같이 죽이지 않게 실행 단위로 격리한다 */
export function newCancelKey(): string {
  return ulid();
}

/** 진행 중인 AI 실행 중단. 이미 끝난 키는 무시되므로 완료와 겹쳐도 안전하다 */
export async function aiCancel(cancelKey: string): Promise<void> {
  await invoke("ai_cancel", { cancelKey });
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
