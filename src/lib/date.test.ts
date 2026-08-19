// 달력 그리드/월 이동 회귀 테스트. 로컬 날짜 문자열 규약(§10)이 깨지면 하루씩 밀리는데
// 화면에선 눈치채기 어렵다. 그리드 경계(4~6주)와 연/월 넘김을 고정한다.

import { describe, expect, it } from "vitest";
import {
  addMonths,
  weekdaysShort,
  monthGridDates,
  monthOf,
  weekDays,
  weekStartOf,
} from "./date";

describe("monthGridDates", () => {
  it("일요일에 시작해 토요일에 끝나는 온전한 주 단위", () => {
    for (const [y, m] of [
      [2015, 2],
      [2026, 1],
      [2026, 7],
      [2026, 8],
      [2025, 12],
    ] as const) {
      const g = monthGridDates(y, m);
      expect(g.length % 7).toBe(0);
      expect(weekStartOf(g[0])).toBe(g[0]); // 첫 칸 = 일요일
      // 하루씩 연속 (중복·누락 없음)
      const days = g.map((s) => Date.parse(`${s}T00:00:00Z`));
      days.forEach((t, i) => i && expect(t - days[i - 1]).toBe(86_400_000));
    }
  });

  it("해당 월의 모든 날을 포함하고, 앞뒤로만 인접 월이 붙는다", () => {
    const g = monthGridDates(2026, 7);
    const inMonth = g.filter((s) => monthOf(s).month === 7);
    expect(inMonth.length).toBe(31);
    expect(inMonth[0]).toBe("2026-07-01");
    expect(inMonth[30]).toBe("2026-07-31");
    expect(g[0]).toBe("2026-06-28");
    expect(g[g.length - 1]).toBe("2026-08-01");
    expect(g.length).toBe(35);
  });

  it("일요일에 시작해 토요일에 끝나는 달은 4주 — 인접 월 없이 딱 떨어진다", () => {
    const g = monthGridDates(2015, 2);
    expect(g.length).toBe(28);
    expect(g[0]).toBe("2015-02-01");
    expect(g[27]).toBe("2015-02-28");
  });

  it("토요일에 시작하는 31일 달은 6주까지 늘어난다", () => {
    const g = monthGridDates(2026, 8);
    expect(g.length).toBe(42);
    expect(g[0]).toBe("2026-07-26");
    expect(g[41]).toBe("2026-09-05");
  });

  it("연말/연초 그리드가 해를 넘어간다", () => {
    expect(monthGridDates(2025, 12)[0]).toBe("2025-11-30");
    const dec = monthGridDates(2025, 12);
    expect(dec[dec.length - 1]).toBe("2026-01-03");
    expect(monthGridDates(2026, 1)[0]).toBe("2025-12-28");
  });

  it("윤년 2월 29일이 빠지지 않는다", () => {
    expect(monthGridDates(2024, 2)).toContain("2024-02-29");
    expect(monthGridDates(2023, 2)).not.toContain("2023-02-29");
  });
});

describe("addMonths", () => {
  it("연도를 넘겨도 month 는 1-12 를 유지한다", () => {
    expect(addMonths({ year: 2025, month: 12 }, 1)).toEqual({ year: 2026, month: 1 });
    expect(addMonths({ year: 2026, month: 1 }, -1)).toEqual({ year: 2025, month: 12 });
    expect(addMonths({ year: 2026, month: 7 }, 18)).toEqual({ year: 2028, month: 1 });
    expect(addMonths({ year: 2026, month: 7 }, -19)).toEqual({ year: 2024, month: 12 });
  });
});

describe("weekStartOf", () => {
  it("월/연 경계를 넘어 그 주 일요일로 간다", () => {
    expect(weekStartOf("2026-08-01")).toBe("2026-07-26"); // 토요일
    expect(weekStartOf("2026-07-26")).toBe("2026-07-26"); // 이미 일요일
    expect(weekStartOf("2026-01-01")).toBe("2025-12-28");
  });
});

describe("weekStartOf", () => {
  // 주의 정의는 앱 전체에서 하나다(date.ts WEEK_STARTS_ON = 0, 일요일 시작).
  // 캘린더 표시·주 단위 할 일·주간 리포트가 전부 이 함수를 쓴다.
  it("주중 어느 날이든 그 주 시작(일요일)", () => {
    // 2026-08-16 은 일요일
    for (const d of [
      "2026-08-16",
      "2026-08-17",
      "2026-08-18",
      "2026-08-19",
      "2026-08-20",
      "2026-08-21",
      "2026-08-22",
    ]) {
      expect(weekStartOf(d)).toBe("2026-08-16");
    }
  });

  it("토요일 다음날은 새 주로 넘어간다", () => {
    expect(weekStartOf("2026-08-22")).toBe("2026-08-16"); // 토 → 그 주
    expect(weekStartOf("2026-08-23")).toBe("2026-08-23"); // 일 → 자기 자신
  });

  it("달·해 경계를 넘어간다", () => {
    expect(weekStartOf("2026-01-01")).toBe("2025-12-28"); // 목요일 → 지난해 일요일
    expect(weekStartOf("2026-03-03")).toBe("2026-03-01");
  });
});

describe("weekDays", () => {
  it("주 시작에서 7일을 순서대로", () => {
    expect(weekDays("2026-08-16")).toEqual([
      "2026-08-16",
      "2026-08-17",
      "2026-08-18",
      "2026-08-19",
      "2026-08-20",
      "2026-08-21",
      "2026-08-22",
    ]);
  });

  it("달 경계를 넘어도 7일", () => {
    const d = weekDays("2026-08-30");
    expect(d).toHaveLength(7);
    expect(d[0]).toBe("2026-08-30");
    expect(d[6]).toBe("2026-09-05");
  });

  it("모든 날이 같은 주 시작으로 되돌아온다 (weekStartOf 와 왕복)", () => {
    for (const d of weekDays("2026-08-16")) expect(weekStartOf(d)).toBe("2026-08-16");
  });
});

describe("monthGridDates — 한 행 = 한 주", () => {
  // 주 단위 선택에서 한 행 = 한 주여야 선택 띠가 두 행으로 잘리지 않는다.
  // 그리드 기본 시작 요일과 weekStartOf 가 같은 상수를 쓰는지 지키는 테스트다.
  it("모든 행이 주 시작 요일에서 시작한다", () => {
    for (const [y, m] of [
      [2026, 1],
      [2026, 8],
      [2026, 11],
      [2015, 2],
    ] as const) {
      const g = monthGridDates(y, m);
      expect(g.length % 7).toBe(0);
      for (let i = 0; i < g.length; i += 7) {
        expect(weekStartOf(g[i])).toBe(g[i]); // 각 행의 첫 칸이 곧 그 주 시작
      }
    }
  });

  it("한 행의 7칸이 같은 주에 속한다", () => {
    const g = monthGridDates(2026, 8);
    for (let i = 0; i < g.length; i += 7) {
      const start = g[i];
      for (let k = 0; k < 7; k++) expect(weekStartOf(g[i + k])).toBe(start);
    }
  });

  it("그 달의 1일과 말일을 모두 덮는다", () => {
    const g = monthGridDates(2026, 8);
    expect(g).toContain("2026-08-01");
    expect(g).toContain("2026-08-31");
  });

  it("일요일 시작이 기본 — 기존 동작 그대로", () => {
    expect(monthGridDates(2026, 8)).toEqual(monthGridDates(2026, 8, 0));
  });
});

describe("weekdaysShort", () => {
  it("월요일 시작이면 일요일이 맨 뒤로 간다", () => {
    const sun = weekdaysShort(0);
    const mon = weekdaysShort(1);
    expect(mon[0]).toBe(sun[1]);
    expect(mon[6]).toBe(sun[0]);
    expect(mon).toHaveLength(7);
  });
});
