// 할 일 탭 '오늘 후보' — AI 가 고른 것들을 담는 모듈 스토어 + 프롬프트 입력 만들기.
//
// **DB 에 넣지 않는다.** 후보는 아직 내 할 일이 아니다 — 받아들이면 그때 todos 행이 되고,
// 안 받아들이면 그냥 사라지면 된다. 보관하면 "무시한 것"을 또 관리해야 하고, 그 순간
// 비우는 게 목표인 서랍이 세 번째 목록이 된다.
//
// 실행은 노트 AI(lib/noteAiRun)와 같은 자리 — 모듈 스토어라 탭을 옮겨도 계속 돈다.

import { useSyncExternalStore } from "react";
import { aiTodoSuggest, friendlyError, type TodoSuggestion } from "./ai";
import type { AppConfig } from "./config";
import type { Todo } from "../types";

const DAY_MS = 86_400_000;
/** 한 번에 보내는 줄 수 상한 — 목록이 수백 줄이면 프롬프트만 길어지고 고르는 질은 안 오른다 */
const MAX_LINES = 40;

export interface SuggestState {
  /** empty = 볼 거리 자체가 없었다. 고장이 아니라 상태라 error 와 나눈다 */
  phase: "idle" | "running" | "done" | "empty" | "error";
  items: TodoSuggestion[];
  error: string | null;
  /** 마지막으로 성공한 시각 — "방금 훑었다"를 화면이 말할 수 있게 */
  ranAt: number | null;
}

let state: SuggestState = { phase: "idle", items: [], error: null, ranAt: null };
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
function set(next: Partial<SuggestState>) {
  state = { ...state, ...next };
  emit();
}

/** 훅 밖에서 지금 상태를 본다 (테스트, 그리고 실행 중복 방지) */
export function getSuggestState(): SuggestState {
  return state;
}

export function useSuggest(): SuggestState {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

/** 받아들였거나 화면을 떠날 때 — 하나를 집어 가면 목록에서 뺀다 */
export function dropSuggestion(index: number): void {
  set({ items: state.items.filter((_, i) => i !== index) });
}
export function clearSuggestions(): void {
  state = { phase: "idle", items: [], error: null, ranAt: null };
  emit();
}
export function resetSuggestForTest(): void {
  clearSuggestions();
}

/** 오늘 목록 — 이미 있는 것을 다시 제안하지 않게 그대로 넘긴다 */
export function formatToday(rows: Todo[]): string {
  return rows
    .filter((r) => r.carried !== 1)
    .slice(0, MAX_LINES)
    .map((r) => `- ${r.content}${r.done === 1 ? " (완료)" : ""}`)
    .join("\n");
}

/** 밀린 것 — 며칠 밀렸는지가 곧 이유가 된다 */
export function formatOverdue(rows: Todo[], today: string): string {
  return rows
    .slice(0, MAX_LINES)
    .map((r) => `- ${r.content} (${r.due_date} 이후, ${daysBetween(r.due_date, today)}일 밀림)`)
    .join("\n");
}

/** 언젠가 — 나이를 붙인다. 오래됐다는 사실 자체는 이유가 아니지만 재료는 된다 */
export function formatAnytime(rows: Todo[], now: number): string {
  return rows
    .slice(0, MAX_LINES)
    .map((r) => {
      const age = r.parked_at == null ? 0 : Math.max(0, Math.floor((now - r.parked_at) / DAY_MS));
      return `- ${r.content} (${age}일째)`;
    })
    .join("\n");
}

/** 'YYYY-MM-DD' 두 날짜 사이의 일수 — 달력 좌표라 UTC 자정 기준으로 센다 */
export function daysBetween(from: string, to: string): number {
  const a = Date.parse(`${from}T00:00:00Z`);
  const b = Date.parse(`${to}T00:00:00Z`);
  if (Number.isNaN(a) || Number.isNaN(b)) return 0;
  return Math.max(0, Math.round((b - a) / DAY_MS));
}

export interface RunSuggestParams {
  today: Todo[];
  overdue: Todo[];
  anytime: Todo[];
  /** 최근 일간 리포트 본문 — 적어 두기만 하고 할 일로 안 옮긴 것이 여기 남는다 */
  notes: string;
  todayDate: string;
  config: AppConfig;
}

/** 훑기 시작. 이미 돌고 있으면 무시한다 */
export async function runSuggest(p: RunSuggestParams): Promise<void> {
  if (state.phase === "running") return;
  const overdue = formatOverdue(p.overdue, p.todayDate);
  const anytime = formatAnytime(p.anytime, Date.now());
  const notes = p.notes.slice(0, 6000);
  // 볼 거리가 하나도 없으면 CLI 를 깨우지 않는다. 그리고 이건 **에러가 아니다** —
  // 기록이 쌓이기 전에는 당연한 상태고, 빨간 판으로 알리면 고장으로 읽힌다.
  if (!overdue.trim() && !anytime.trim() && !notes.trim()) {
    set({ phase: "empty", items: [], error: null, ranAt: Date.now() });
    return;
  }
  set({ phase: "running", error: null });
  try {
    const { items } = await aiTodoSuggest({
      today: formatToday(p.today),
      overdue,
      anytime,
      notes,
      model: p.config.model,
      cliPath: p.config.cliPath,
      provider: p.config.provider,
    });
    set({ phase: "done", items, ranAt: Date.now() });
  } catch (e) {
    set({ phase: "error", error: friendlyError(e) });
  }
}
