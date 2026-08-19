// 데일리 리포트 '생성 실행'을 컴포넌트 밖 모듈 스토어로 둔다.
// → 탭/날짜 전환·컴포넌트 리마운트와 무관하게 백그라운드로 완주하고 파일/DB 에 저장한다.
//   (예전엔 상태가 DailyReportPanel 안에 있어, 뷰가 바뀌면 진행이 끊긴 것처럼 보였다.)
// 컴포넌트는 useReportRun(date) 로 구독만 한다. 날짜별로 한 번에 하나의 실행.

import { useSyncExternalStore } from "react";
import type { AppConfig } from "./config";
import type {
  CollectProgress,
  DailyReport,
  SourceDigest,
  WeeklyReport,
} from "../types";
import { aiCancel, friendlyError, newCancelKey } from "./ai";
import { dayRangeMs } from "./date";
import {
  buildTodosDigest,
  getReport,
  loadReportConfig,
  mcpSourcesFrom,
  rankedSources,
  reportCollect,
  reportGenerate,
  upsertReport,
  writeReportFile,
  loadWeekSources,
  reportGenerateWeekly,
  writeWeeklyReportFile,
  upsertWeeklyReport,
  getWeeklyReport,
} from "./report";

export type RunPhase = "collecting" | "streaming" | "done" | "empty" | "error";
export interface RunChip {
  id: string;
  status: "ok" | "pending" | "error" | "mcp";
  items: number;
  /** 실패 사유 — 백엔드가 유일하게 실행 가능한 안내("gh auth login 하세요")를 여기 담는다.
   *  버리면 칩이 빨개지기만 하고 왜 실패했는지 알 방법이 없다. */
  error?: string | null;
}
/** 주간 실행 키 — 같은 스토어를 쓰되 접두어로 격리한다.
 *  캘린더 점은 일간만 봐야 하고(주간은 특정 날짜의 상태가 아니다), 레일의 생성 중 표시는 둘 다 본다. */
export const weeklyKey = (monday: string) => `w:${monday}`;

export interface RunState {
  phase: RunPhase;
  /** 진행 중인 실행의 취소 키 — 중단 버튼이 이걸로 ai_cancel 을 부른다. 끝나면 null */
  cancelKey: string | null;
  stream: string;
  chips: RunChip[];
  error: string | null;
  report: DailyReport | WeeklyReport | null; // phase==='done' 일 때 채워짐
  body: string;
}

const runs = new Map<string, RunState>();
const listeners = new Set<() => void>();

// 생성 중인 날짜 집합의 캐시 스냅샷. useSyncExternalStore 의 getSnapshot 은 값이 같으면
// **같은 참조**를 돌려줘야 해서(새 Set 을 매번 만들면 무한 렌더) 구성이 바뀔 때만 새로 만든다.
let runningDates: ReadonlySet<string> = new Set();
let runningCount = 0;
function refreshRunningDates() {
  const next: string[] = [];
  let total = 0;
  for (const [key, r] of runs) {
    if (r.phase !== "collecting" && r.phase !== "streaming") continue;
    total += 1;
    if (!key.startsWith("w:")) next.push(key); // 주간은 캘린더 점 대상이 아니다
  }
  runningCount = total;
  if (next.length === runningDates.size && next.every((d) => runningDates.has(d)))
    return;
  runningDates = new Set(next);
}

const emit = () => {
  refreshRunningDates();
  listeners.forEach((l) => l());
};

function patch(date: string, p: Partial<RunState>) {
  const cur =
    runs.get(date) ??
    ({
      phase: "collecting",
      cancelKey: null,
      stream: "",
      chips: [],
      error: null,
      report: null,
      body: "",
    } as RunState);
  runs.set(date, { ...cur, ...p });
  emit();
}

export function isRunning(date: string): boolean {
  const r = runs.get(date);
  return r?.phase === "collecting" || r?.phase === "streaming";
}

/** 완료/에러 실행을 스토어에서 제거 (삭제 시). */
export function clearRun(date: string): void {
  if (runs.delete(date)) emit();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function anyRunning(): boolean {
  return runningCount > 0; // 주간 생성도 레일에 표시한다
}

/** 특정 날짜의 실행 상태 구독 (없으면 undefined) */
export function useReportRun(date: string): RunState | undefined {
  return useSyncExternalStore(
    subscribe,
    () => runs.get(date),
  );
}

/** 아무 날짜라도 생성 중인지 — 전역 로딩 표시(레일 등)용 */
export function useAnyReportGenerating(): boolean {
  return useSyncExternalStore(subscribe, anyRunning);
}

/** 지금 생성 중인 날짜들 — 캘린더처럼 여러 날을 한 번에 그리는 곳에서 쓴다 */
export function useReportGeneratingDates(): ReadonlySet<string> {
  return useSyncExternalStore(subscribe, () => runningDates);
}

/** 리포트 생성 시작 — 컴포넌트와 무관하게 끝까지 돌아 파일/DB 에 저장.
 *  게이트(provider 연결·onboarded)는 호출부가 통과시킨 뒤 부른다. */
export async function startReport(date: string, config: AppConfig): Promise<void> {
  if (isRunning(date)) return;
  patch(date, {
    phase: "collecting",
    cancelKey: null,
    stream: "",
    chips: [],
    error: null,
    report: null,
    body: "",
  });
  try {
    const rc = await loadReportConfig();
    const { md: todosDigest, count } = await buildTodosDigest(date);
    const ranked = rankedSources(rc);
    const githubR = ranked.find((s) => s.id === "github");
    const sessR = ranked.find((s) => s.id === "ai_sessions");
    const mcpSources = config.provider === "claude" ? mcpSourcesFrom(rc) : [];

    const initChips: RunChip[] = [{ id: "todos", status: "ok", items: count }];
    for (const s of ranked)
      if (s.id === "github" || s.id === "ai_sessions")
        initChips.push({ id: s.id, status: "pending", items: 0 });
    for (const m of mcpSources) initChips.push({ id: m.id, status: "mcp", items: 0 });
    patch(date, { chips: initChips });

    const [startMs, endMs] = dayRangeMs(date);
    const digests = await reportCollect(
      {
        date,
        startMs,
        endMs,
        tzOffsetMin: new Date().getTimezoneOffset(),
        github: githubR
          ? {
              rank: githubR.rank,
              path: rc.githubPath || null,
              repos: rc.githubRepos,
              account: rc.githubAccount || null,
            }
          : null,
        aiSessions: sessR
          ? { rank: sessR.rank, claude: rc.sessionsClaude, codex: rc.sessionsCodex }
          : null,
      },
      (p: CollectProgress) => {
        const cur = runs.get(date);
        if (!cur) return;
        patch(date, {
          chips: cur.chips.map((c) =>
            c.id === p.id
              ? {
                  id: c.id,
                  status: p.ok ? "ok" : "error",
                  items: p.items,
                  error: p.error,
                }
              : c,
          ),
        });
      },
    );

    const hasActivity =
      count > 0 || digests.some((d) => d.ok && d.items > 0) || mcpSources.length > 0;
    if (!hasActivity) {
      patch(date, { phase: "empty" });
      return;
    }

    const cancelKey = newCancelKey();
    patch(date, { phase: "streaming", cancelKey });
    const { markdown, meta } = await reportGenerate(
      {
        date,
        todosDigest,
        digests,
        mcpSources,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
        cancelKey,
      },
      (t: string) => {
        const cur = runs.get(date);
        patch(date, { stream: (cur?.stream ?? "") + t });
      },
    );

    const filePath = await writeReportFile(date, markdown);
    const sourcesJson = JSON.stringify([
      ...digests.map((d: SourceDigest) => ({
        id: d.id,
        rank: d.rank,
        ok: d.ok,
        items: d.items,
        error: d.error,
      })),
      ...mcpSources.map((m) => ({
        id: m.id,
        rank: m.rank,
        ok: true,
        items: 0,
        mcp: true,
        error: null,
      })),
    ]);
    await upsertReport({
      date,
      filePath,
      sourcesJson,
      provider: config.provider,
      model: meta.model,
      durationMs: meta.duration_ms,
    });
    const report = await getReport(date);
    patch(date, { phase: "done", body: markdown, report, cancelKey: null });
  } catch (e) {
    patch(date, { phase: "error", error: friendlyError(e), cancelKey: null });
  }
}

/** 진행 중인 리포트 생성을 중단한다. CLI 를 죽이면 reportGenerate 가 에러로 빠져
 *  위 catch 가 phase='error' 로 마감한다 — 별도 상태 전환이 필요 없다. */
export async function cancelReport(date: string): Promise<void> {
  const key = runs.get(date)?.cancelKey;
  if (key) await aiCancel(key);
}

/** 주간 리포트 생성 시작. 재료는 그 주의 일간 리포트 본문 — 수집 단계가 없으므로
 *  바로 streaming 으로 간다. 리포트가 하나도 없는 주는 phase='empty'. */
export async function startWeeklyReport(
  weekStart: string,
  weekEnd: string,
  config: AppConfig,
): Promise<void> {
  const key = weeklyKey(weekStart);
  if (isRunning(key)) return;
  patch(key, {
    phase: "streaming",
    cancelKey: null,
    stream: "",
    chips: [],
    error: null,
    report: null,
    body: "",
  });
  try {
    const sources = await loadWeekSources(weekStart);
    const days = sources
      .filter((d): d is typeof d & { body: string } => !!d.body?.trim())
      .map((d) => ({ date: d.date, weekday: d.weekday, body: d.body }));
    if (!days.length) {
      patch(key, { phase: "empty" });
      return;
    }

    const rc = await loadReportConfig();
    const cancelKey = newCancelKey();
    patch(key, { cancelKey });
    const { markdown, meta } = await reportGenerateWeekly(
      {
        weekStart,
        weekEnd,
        days,
        displayName: rc.displayName,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
        cancelKey,
      },
      (t: string) => {
        const cur = runs.get(key);
        patch(key, { stream: (cur?.stream ?? "") + t });
      },
    );

    const filePath = await writeWeeklyReportFile(weekStart, markdown);
    await upsertWeeklyReport({
      weekStart,
      filePath,
      // 어느 날짜를 묶었는지 — 나중에 "그때는 5일치였다"를 설명할 수 있게 남긴다
      sourcesJson: JSON.stringify(days.map((d) => d.date)),
      provider: config.provider,
      model: meta.model,
      durationMs: meta.duration_ms,
    });
    const report = await getWeeklyReport(weekStart);
    patch(key, { phase: "done", body: markdown, report, cancelKey: null });
  } catch (e) {
    patch(key, { phase: "error", error: friendlyError(e), cancelKey: null });
  }
}

/** 진행 중인 주간 생성 중단 */
export async function cancelWeeklyReport(weekStart: string): Promise<void> {
  const k = runs.get(weeklyKey(weekStart))?.cancelKey;
  if (k) await aiCancel(k);
}
