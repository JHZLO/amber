import { describe, expect, it } from "vitest";
import {
  daysBetween,
  formatAnytime,
  formatOverdue,
  formatToday,
  getSuggestState,
  resetSuggestForTest,
  runSuggest,
} from "./todoSuggest";
import type { AppConfig } from "./config";
import type { Todo } from "../types";

const t = (over: Partial<Todo>): Todo =>
  ({
    id: 1,
    content: "할 일",
    due_date: "2026-09-21",
    scope: "day",
    done: 0,
    completed_at: null,
    parent_id: null,
    sort_order: 0,
    created_at: 0,
    updated_at: 0,
    parked_at: null,
    ...over,
  }) as Todo;

describe("formatToday", () => {
  it("완료 여부를 붙인다 — 끝낸 걸 다시 제안하지 않게", () => {
    const out = formatToday([t({ content: "A" }), t({ content: "B", done: 1 })]);
    expect(out).toBe("- A\n- B (완료)");
  });

  it("이월 고스트는 보내지 않는다 — 그 할 일이 사는 곳은 다른 날짜다", () => {
    expect(formatToday([t({ content: "유령", carried: 1 } as Partial<Todo>)])).toBe("");
  });
});

describe("formatOverdue", () => {
  it("며칠 밀렸는지를 붙인다 — 그게 곧 이유의 재료다", () => {
    const out = formatOverdue([t({ content: "밀린 일", due_date: "2026-09-14" })], "2026-09-21");
    expect(out).toBe("- 밀린 일 (2026-09-14 이후, 7일 밀림)");
  });
});

describe("formatAnytime", () => {
  const DAY = 86_400_000;
  it("내려놓은 지 며칠인지를 붙인다", () => {
    const now = Date.parse("2026-09-21T00:00:00Z");
    const out = formatAnytime([t({ content: "벼르던 일", parked_at: now - DAY * 17 })], now);
    expect(out).toBe("- 벼르던 일 (17일째)");
  });

  it("parked_at 이 없으면 0일로 — 줄이 깨지지 않게", () => {
    expect(formatAnytime([t({ content: "X" })], 0)).toBe("- X (0일째)");
  });
});

describe("daysBetween", () => {
  it("달력 날짜 사이의 일수", () => {
    expect(daysBetween("2026-09-14", "2026-09-21")).toBe(7);
    expect(daysBetween("2026-09-21", "2026-09-21")).toBe(0);
  });

  it("서머타임이 낀 구간도 정확하다 — UTC 자정 기준이라 23시간짜리 날이 없다", () => {
    expect(daysBetween("2026-03-07", "2026-03-09")).toBe(2);
  });

  it("미래 날짜는 음수가 아니라 0", () => {
    expect(daysBetween("2026-09-25", "2026-09-21")).toBe(0);
  });

  it("형식이 깨진 값에 NaN 을 내지 않는다", () => {
    expect(daysBetween("", "2026-09-21")).toBe(0);
  });
});

describe("runSuggest", () => {
  const config = { provider: "claude", model: "", cliPath: null } as unknown as AppConfig;

  it("볼 거리가 하나도 없으면 CLI 를 부르지 않고 'empty' 로 끝난다", async () => {
    // 기록이 쌓이기 전에는 당연한 상태다. 에러로 올리면 빨간 판이 떠서 고장으로 읽힌다
    resetSuggestForTest();
    await runSuggest({
      today: [t({ content: "오늘 것" })],
      overdue: [],
      anytime: [],
      activity: "",
      todayDate: "2026-09-21",
      config,
    });
    const st = getSuggestState();
    expect(st.phase).toBe("empty");
    expect(st.error).toBeNull(); // 빨간 판이 뜨면 안 된다
    expect(st.items).toEqual([]);
  });

  it("볼 거리가 하나라도 있으면 실행으로 넘어간다", async () => {
    resetSuggestForTest();
    // CLI 는 이 환경에 없으므로 실패로 끝나지만, 'empty' 가 아니라는 게 요점이다
    await runSuggest({
      today: [],
      overdue: [t({ content: "밀린 것", due_date: "2026-09-14" })],
      anytime: [],
      activity: "",
      todayDate: "2026-09-21",
      config,
    });
    expect(getSuggestState().phase).not.toBe("empty");
  });
});
