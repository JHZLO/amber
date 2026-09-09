// 필기노트 '전문 AI 작성' 실행을 컴포넌트 밖 모듈 스토어로 둔다 — 데일리 리포트(lib/reportRun)와 같은 구조.
// → 모달을 닫거나 다른 노트·탭으로 옮겨도 CLI 는 끝까지 돌고, 결과는 노트 경로에 매달려 검토를 기다린다.
//   (예전엔 상태가 NoteAiModal 안에 있어 닫기 = 중단이었고, 10분짜리 생성 동안 다른 일을 할 수 없었다.)
// 컴포넌트(NoteAiModal · NotesView 의 배너/트리 점 · 레일 점)는 useNoteAiRun(path) 등으로 구독만 한다.
// 노트 하나에 실행 하나(키 = vault 상대 경로). 부분 수정(NoteSpanAiModal)은 초 단위라 여기 오지 않는다.

import { useSyncExternalStore } from "react";
import type { AppConfig } from "./config";
import {
  aiCancel,
  aiNoteComposeStream,
  aiNoteEditSpanStream,
  friendlyError,
  newCancelKey,
  type AiActivity,
  type InvocationMeta,
} from "./ai";
import { CONTINUE_INSTRUCTION, tailSpan } from "./aiInstruction";

export type NoteAiPhase = "running" | "done" | "error";

export interface NoteAiRun {
  /** vault 상대 경로 — 스토어 키. 이름 변경·이동은 remapNoteAiPaths 로 따라간다 */
  path: string;
  title: string;
  phase: NoteAiPhase;
  /** 내가 친 지시와 체크한 칩·참고 폴더 — "다시 지시" 가 이걸 되살린다 */
  typed: string;
  chosen: string[];
  refDirs: string[];
  /** 실행을 시작한 시점의 본문 — 검토 때 노트가 그 사이 바뀌었는지 가른다 */
  baseMarkdown: string;
  /** 생성 중 실시간 텍스트(파일 모드에선 초안 폴더 스냅샷) */
  stream: string;
  activity: AiActivity | null;
  activityAt: number;
  startedAt: number;
  finishedAt: number | null;
  /** 마지막으로 성공한 결과. 다시 실행해도 새 결과가 올 때까지 남는다 — 실패·중단이 이미 받은 것을 지우지 않게 */
  result: string | null;
  meta: InvocationMeta | null;
  truncated: boolean;
  continued: boolean;
  /** 잘린 결과를 이어 쓰는 중(phase 는 running, result 는 그대로) */
  continuing: boolean;
  error: string | null;
  /** 진행 중인 실행의 취소 키 — 중단 버튼이 이걸로 ai_cancel 을 부른다. 끝나면 null */
  cancelKey: string | null;
}

const runs = new Map<string, NoteAiRun>();
const listeners = new Set<() => void>();
// 실행 세대(경로별). 중단·재실행 뒤 늦게 오는 델타·결과·에러는 세대가 다르면 버린다 — 취소는 비동기라
// 프로세스가 죽기 전 조각이 더 오고, 그게 새 실행 텍스트와 섞이면 한 글자씩 뒤엉킨 결과가 나온다.
const seqs = new Map<string, number>();
function bump(path: string): number {
  const n = (seqs.get(path) ?? 0) + 1;
  seqs.set(path, n);
  return n;
}

// 트리용 스냅샷: 경로 → 단계. useSyncExternalStore 의 getSnapshot 은 값이 같으면 **같은 참조**를 돌려줘야 해서
// (매번 새 Map 이면 무한 렌더) 구성이 바뀔 때만 새로 만든다 — 델타마다 트리 전체가 다시 그려지지 않게.
let phases: ReadonlyMap<string, NoteAiPhase> = new Map();
let runningCount = 0;
function refreshPhases() {
  let running = 0;
  let same = phases.size === runs.size;
  for (const [p, r] of runs) {
    if (r.phase === "running") running += 1;
    if (same && phases.get(p) !== r.phase) same = false;
  }
  runningCount = running;
  if (same) return;
  const next = new Map<string, NoteAiPhase>();
  for (const [p, r] of runs) next.set(p, r.phase);
  phases = next;
}

const emit = () => {
  refreshPhases();
  listeners.forEach((l) => l());
};

function patch(path: string, p: Partial<NoteAiRun>) {
  const cur = runs.get(path);
  if (!cur) return;
  runs.set(path, { ...cur, ...p });
  emit();
}

function remove(path: string) {
  if (runs.delete(path)) emit();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1).replace(/\.md$/i, "");
}

export function getNoteAiRun(path: string): NoteAiRun | undefined {
  return runs.get(path);
}
export function isNoteAiRunning(path: string): boolean {
  return runs.get(path)?.phase === "running";
}
export function getNoteAiPhases(): ReadonlyMap<string, NoteAiPhase> {
  return phases;
}

/** 한 노트의 실행 상태 구독 (없으면 undefined) */
export function useNoteAiRun(path: string | null): NoteAiRun | undefined {
  return useSyncExternalStore(subscribe, () => (path ? runs.get(path) : undefined));
}
/** 경로 → 단계 — 트리처럼 여러 노트를 한 번에 그리는 곳에서 쓴다 */
export function useNoteAiPhases(): ReadonlyMap<string, NoteAiPhase> {
  return useSyncExternalStore(subscribe, () => phases);
}
/** 어느 노트라도 쓰는 중인지 — 레일의 전역 표시용 */
export function useAnyNoteAiRunning(): boolean {
  return useSyncExternalStore(subscribe, () => runningCount > 0);
}

export interface StartNoteAiParams {
  path: string;
  title: string;
  /** 지금 본문(편집 중이면 초안) — 프롬프트의 [현재 본문] 이자 baseMarkdown */
  markdown: string;
  typed: string;
  chosen: string[];
  refDirs: string[];
  /** 보낼 최종 지시(내가 친 말 + 체크한 지시) */
  instruction: string;
  config: AppConfig;
}

/** 전문 작성 시작 — 컴포넌트와 무관하게 끝까지 돌아 결과를 스토어에 남긴다.
 *  게이트(provider 연결·지시 길이)는 호출부가 통과시킨 뒤 부른다. 같은 노트가 이미 쓰는 중이면 무시. */
export async function startNoteAi(p: StartNoteAiParams): Promise<void> {
  if (isNoteAiRunning(p.path)) return;
  const seq = bump(p.path);
  const live = () => seqs.get(p.path) === seq;
  const key = newCancelKey();
  const prev = runs.get(p.path);
  runs.set(p.path, {
    path: p.path,
    title: p.title,
    phase: "running",
    typed: p.typed,
    chosen: p.chosen,
    refDirs: p.refDirs,
    baseMarkdown: p.markdown,
    stream: "",
    activity: null,
    activityAt: 0,
    startedAt: Date.now(),
    finishedAt: null,
    // 이전 결과는 새 결과가 올 때까지 남긴다 — 다시 실행이 실패해도 받아 둔 것을 잃지 않게
    result: prev?.result ?? null,
    meta: prev?.meta ?? null,
    truncated: prev?.truncated ?? false,
    continued: prev?.continued ?? false,
    continuing: false,
    error: null,
    cancelKey: key,
  });
  emit();
  // 초안 스냅샷이 한 번 오면 텍스트 델타는 무시한다 — 파일 모드의 델타는 "DONE 5" 같은 마무리 한 줄뿐이다
  let draftSeen = false;
  try {
    const { markdown, meta } = await aiNoteComposeStream(
      {
        title: p.title,
        markdown: p.markdown,
        instruction: p.instruction,
        model: p.config.model,
        cliPath: p.config.cliPath,
        provider: p.config.provider,
        cancelKey: key,
        refDirs: p.refDirs,
      },
      (delta) => {
        if (!live() || draftSeen) return; // 버려진 실행·파일 모드의 잔여 델타
        const cur = runs.get(p.path);
        if (cur) patch(p.path, { stream: cur.stream + delta });
      },
      (a) => {
        if (live()) patch(p.path, { activity: a, activityAt: Date.now() });
      },
      (full) => {
        if (!live()) return;
        draftSeen = true;
        patch(p.path, { stream: full });
      },
    );
    if (!live()) return; // 중단·재실행됨 — 이 결과로 덮지 않는다
    patch(p.path, {
      phase: "done",
      result: markdown,
      meta,
      truncated: meta.truncated,
      continued: meta.continued,
      finishedAt: Date.now(),
      stream: "",
      cancelKey: null,
    });
  } catch (e) {
    if (!live()) return; // 사용자가 직접 끊은 실행은 에러가 아니다
    patch(p.path, {
      phase: "error",
      error: friendlyError(e),
      finishedAt: Date.now(),
      stream: "",
      cancelKey: null,
    });
  }
}

/** 잘린 결과 이어 쓰기 — 끝 조각(~700자)을 span 으로 넘겨 "조각 + 이어질 내용"을 받아 그 자리에 되끼운다.
 *  전문을 다시 받지 않으니 상한에 다시 걸릴 일이 없고, 앞부분은 한 글자도 바뀌지 않는다. */
export async function continueNoteAi(path: string, config: AppConfig): Promise<void> {
  const run = runs.get(path);
  if (!run || run.phase === "running" || !run.result) return;
  const base = run.result;
  const tail = tailSpan(base);
  const seq = bump(path);
  const live = () => seqs.get(path) === seq;
  const key = newCancelKey();
  patch(path, {
    phase: "running",
    continuing: true,
    stream: "",
    activity: null,
    activityAt: 0,
    startedAt: Date.now(),
    error: null,
    cancelKey: key,
  });
  try {
    const { text, meta } = await aiNoteEditSpanStream(
      {
        title: run.title,
        markdown: base,
        span: tail,
        instruction: CONTINUE_INSTRUCTION,
        spanKind: "selection",
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
        cancelKey: key,
        refDirs: run.refDirs,
      },
      (delta) => {
        if (!live()) return;
        const cur = runs.get(path);
        if (cur) patch(path, { stream: cur.stream + delta });
      },
      (a) => {
        if (live()) patch(path, { activity: a, activityAt: Date.now() });
      },
    );
    if (!live()) return;
    patch(path, {
      phase: "done",
      continuing: false,
      result: base.slice(0, base.length - tail.length) + text,
      truncated: meta.truncated,
      finishedAt: Date.now(),
      stream: "",
      cancelKey: null,
    });
  } catch (e) {
    if (!live()) return;
    // 잘린 결과라도 남겨 둔다 — 실패했다고 이미 받은 것을 버리지 않는다
    patch(path, {
      phase: "error",
      continuing: false,
      error: friendlyError(e),
      finishedAt: Date.now(),
      stream: "",
      cancelKey: null,
    });
  }
}

/** 중단 — CLI 를 죽이고 **기다리지 않고** 바로 정리한다(프로세스가 실제로 끝나길 기다리면 누른 뒤에도
 *  몇 초간 글자가 계속 흘러 안 먹은 듯 보인다). 이어 쓰기·다시 실행이었으면 받아 둔 결과로 돌아가고,
 *  첫 실행이면 실행 자체를 지운다(지시 화면으로). */
export function stopNoteAi(path: string): void {
  const run = runs.get(path);
  if (!run || run.phase !== "running") return;
  bump(path); // 이 시점 이후 도착하는 델타·결과·에러는 전부 무효
  if (run.cancelKey) void aiCancel(run.cancelKey);
  if (run.result) {
    patch(path, { phase: "done", continuing: false, stream: "", error: null, cancelKey: null });
  } else {
    remove(path);
  }
}

/** 실행을 통째로 버린다(결과 포함) — 검토 뒤 적용했거나 사용자가 버리기를 확인했을 때. 진행 중이면 먼저 끊는다 */
export function dismissNoteAi(path: string): void {
  const run = runs.get(path);
  if (!run) return;
  if (run.phase === "running") {
    bump(path);
    if (run.cancelKey) void aiCancel(run.cancelKey);
  }
  remove(path);
}

/** 실패 알림만 치운다 — 이전 결과가 있으면 그 결과로 돌아가고, 없으면 실행을 지운다 */
export function dismissNoteAiError(path: string): void {
  const run = runs.get(path);
  if (!run || run.phase !== "error") return;
  if (run.result) patch(path, { phase: "done", error: null });
  else remove(path);
}

/** 소스 탭에서 결과를 손으로 고쳤을 때 */
export function setNoteAiResult(path: string, markdown: string): void {
  const run = runs.get(path);
  if (!run || run.phase === "running") return;
  patch(path, { result: markdown });
}

/** 노트·폴더 이름 변경/이동을 따라간다 — 키가 경로라서 안 옮기면 결과가 고아가 된다 */
export function remapNoteAiPaths(oldP: string, newP: string, isDir: boolean): void {
  const moves: [string, string][] = [];
  for (const p of runs.keys()) {
    if (p === oldP) moves.push([p, newP]);
    else if (isDir && p.startsWith(`${oldP}/`)) moves.push([p, newP + p.slice(oldP.length)]);
  }
  if (!moves.length) return;
  for (const [from, to] of moves) {
    const r = runs.get(from);
    if (!r) continue;
    runs.delete(from);
    runs.set(to, { ...r, path: to, title: baseName(to) });
    const s = seqs.get(from);
    seqs.delete(from);
    if (s !== undefined) seqs.set(to, s);
  }
  emit();
}

/** 노트·폴더 삭제 — 그 아래 실행은 끊고 지운다 */
export function dropNoteAiUnder(path: string): void {
  let changed = false;
  for (const [p, r] of [...runs]) {
    if (p !== path && !p.startsWith(`${path}/`)) continue;
    bump(p);
    if (r.phase === "running" && r.cancelKey) void aiCancel(r.cancelKey);
    runs.delete(p);
    changed = true;
  }
  if (changed) emit();
}

/** 테스트 전용 — 스토어를 비운다 */
export function resetNoteAiRunsForTest(): void {
  runs.clear();
  seqs.clear();
  emit();
}
