// 할 일 탭 '오늘 후보' — **오늘 해야 하는데 목록에 없는 것**을 찾는다.
//
// 재료는 일간 리포트가 쓰는 수집기와 같다(report_collect): 저장소 이벤트와 AI 세션.
// 이미 투두에 있는 것(밀린 것, 언젠가)은 정의상 '추가 안 한 것'이 아니라서 후보의 본류가
// 아니다 — 오늘의 움직임이 그것을 건드렸을 때만 딸려 올라온다.
//
// github·ai_sessions 수집기는 `gh` CLI 와 로컬 세션 파일을 읽는 Rust 쪽이라
// **MCP 커넥터 없이 동작한다**. Slack·Notion 은 MCP 가 필요해서 아직 여기 오지 않는다.
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
  /** 오늘 실제로 움직인 것 (report_collect 의 digest 를 이어 붙인 것) */
  activity: string;
  /** 설정 › 리포트에서 켜 둔 MCP 소스 — 같은 스위치가 리포트와 후보를 함께 다스린다 */
  mcpSources?: { id: string; rank: number; server: string }[];
  todayDate: string;
  config: AppConfig;
}

/** 훑기 시작. 이미 돌고 있으면 무시한다 */
export async function runSuggest(p: RunSuggestParams): Promise<void> {
  if (state.phase === "running") return;
  const overdue = formatOverdue(p.overdue, p.todayDate);
  const anytime = formatAnytime(p.anytime, Date.now());
  const activity = p.activity.slice(0, 12000);
  // 볼 거리가 하나도 없으면 CLI 를 깨우지 않는다. 그리고 이건 **에러가 아니다** —
  // 기록이 쌓이기 전에는 당연한 상태고, 빨간 판으로 알리면 고장으로 읽힌다.
  // MCP 소스가 켜져 있으면 직접 긁으러 가므로 로컬 재료가 비어도 부른다.
  const hasMcp = (p.mcpSources?.length ?? 0) > 0;
  if (!hasMcp && !overdue.trim() && !anytime.trim() && !activity.trim()) {
    set({ phase: "empty", items: [], error: null, ranAt: Date.now() });
    return;
  }
  set({ phase: "running", error: null });
  try {
    const { items } = await aiTodoSuggest({
      date: p.todayDate,
      today: formatToday(p.today),
      overdue,
      anytime,
      activity,
      mcpSources: p.mcpSources,
      model: p.config.model,
      cliPath: p.config.cliPath,
      provider: p.config.provider,
    });
    set({ phase: "done", items, ranAt: Date.now() });
  } catch (e) {
    set({ phase: "error", error: friendlyError(e) });
  }
}
