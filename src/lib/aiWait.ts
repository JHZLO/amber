// AI 대기 상태 한 줄 — 첫 글자가 오기 전까지 로딩 바 아래에서 4초마다 바뀐다(멈춘 것처럼 보이지 않게).
// 단계는 CLI 스트림의 **실제 신호**에서 온다: thinking 블록 시작/끝, 텍스트 블록 시작, 도구 호출(파일 읽기·검색).
// 같은 단계 안에서만 문구를 돌리고, "거의 다 생각했어요"는 thinking 블록이 실제로 끝났을 때(thought)만 나온다 —
// 진행을 추측으로 말하면 한 번 틀린 뒤로는 어떤 문구도 믿지 않게 된다.

import { useEffect, useState } from "react";
import type { AiActivity } from "./ai";
import { describeActivity } from "./aiActivity";
import { t, type MsgKey } from "./i18n";

const ROTATE_MS = 4_000;
const LONG_MS = 60_000;
const LONGER_MS = 180_000;

/** 도구가 아닌 단계 표식 — Rust 가 스트림 이벤트에서 만들어 보낸다 */
export const PHASE_TOOLS = new Set(["thinking", "thought", "writing"]);

const POOLS: Record<string, MsgKey[]> = {
  connecting: ["common.ai.wait.connect1", "common.ai.wait.connect2"],
  thinking: [
    "common.ai.wait.think1",
    "common.ai.wait.think2",
    "common.ai.wait.think3",
    "common.ai.wait.think4",
  ],
  thought: ["common.ai.wait.thought"],
  writing: ["common.ai.wait.write"],
};

export interface WaitInput {
  /** 실행을 시작한 시각(ms) */
  startedAt: number;
  /** 마지막 진행 신호와 그 시각 — 없으면 아직 첫 스트림 이벤트 전 */
  activity: AiActivity | null;
  activityAt: number;
  hasRefDirs: boolean;
}

/** 지금 보일 한 줄 — 순수 함수. 도구 호출은 그대로(무엇을 읽는지가 가장 구체적인 진행이다), 단계는 풀에서 돌린다 */
export function waitLine(input: WaitInput, now: number): string {
  const { startedAt, activity, activityAt, hasRefDirs } = input;
  let line: string;
  if (activity && !PHASE_TOOLS.has(activity.tool)) {
    line = describeActivity(activity);
  } else {
    const phase = activity?.tool ?? "connecting";
    const pool = POOLS[phase] ?? POOLS.connecting;
    const since = activity ? activityAt : startedAt;
    const idx = Math.floor(Math.max(0, now - since) / ROTATE_MS) % pool.length;
    line = t(pool[idx]);
  }
  const elapsed = now - startedAt;
  if (elapsed >= LONGER_MS) {
    return `${line} · ${t(hasRefDirs ? "common.ai.wait.longerRef" : "common.ai.wait.longer")}`;
  }
  if (elapsed >= LONG_MS) return `${line} · ${t("common.ai.wait.long")}`;
  return line;
}

/** 실행 중이면 1초마다 다시 계산한 대기 문구, 아니면 null */
export function useAiWaitLine(args: {
  running: boolean;
  startedAt: number | null;
  activity: AiActivity | null;
  activityAt: number;
  hasRefDirs: boolean;
}): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!args.running) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [args.running]);
  if (!args.running || args.startedAt === null) return null;
  return waitLine(
    {
      startedAt: args.startedAt,
      activity: args.activity,
      activityAt: args.activityAt,
      hasRefDirs: args.hasRefDirs,
    },
    now,
  );
}
