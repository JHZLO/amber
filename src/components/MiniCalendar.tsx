// 미니 월 캘린더 (할 일 탭 좌측). 순수 프레젠테이션 — 커서(year/month)·selected 는 부모가 소유.
// 상태 표현은 모노톤 문법(.claude/DESIGN.md §3): 오늘=primary 필, 선택=surface-3, 점=채움/아웃라인.
//
// 제목을 누르면 흔한 달력 컴포넌트(Chrome·Windows 날짜 선택기)처럼 **위로 한 단계씩** 열린다:
//   일 ── 제목("2026년 8월") ─▶ 월 ── 제목("2026년") ─▶ 연
// 되돌아오는 길은 선택이다 — 월을 고르면 그 달의 일 그리드로, 연을 고르면 그 해의 월 그리드로.
// 좌우 화살표는 보고 있는 단계의 한 판씩 움직인다(일=±1달, 월=±1년, 연=±10년).

import { useEffect, useState } from "react";
import type { DayTodoCount, TodoUnit } from "../types";
import {
  formatMonthTitle,
  formatYearTitle,
  mondayOf,
  monthGridDates,
  monthsShort,
  parseLocalDate,
  weekdaysShort,
} from "../lib/date";
import { holidayOf } from "../lib/holidays";
import { vacationLabel, type VacationKind } from "../lib/vacations";
import { t } from "../lib/i18n";
import { Icon } from "../icons";
import { Tooltip } from "../ui";

/** 제목 클릭으로 오르내리는 단계 */
type Level = "day" | "month" | "year";

// 연 그리드는 10년 한 판 + 앞뒤 한 칸(흐리게) = 12칸 3×4 — 월 그리드와 같은 판형이 된다.
const DECADE = 10;
const decadeStart = (y: number) => Math.floor(y / DECADE) * DECADE;

export function MiniCalendar({
  year,
  month,
  selected,
  today,
  counts,
  vacations,
  generating,
  unit,
  onUnitChange,
  weekCounts,
  onSelect,
  onCursor,
}: {
  year: number;
  month: number;
  selected: string;
  today: string;
  counts: Record<string, DayTodoCount>;
  /** 휴가로 표시한 날짜 → 종류. 공휴일과 겹치면 이쪽(사용자가 직접 정한 것)을 그린다 */
  vacations: Record<string, VacationKind>;
  /** 데일리 리포트를 생성 중인 날짜들 — 그날의 점이 깜빡인다 */
  generating: ReadonlySet<string>;
  /** 선택 단위. 'week' 면 날짜가 아니라 그 날이 속한 주(월~일)를 고르는 판이 된다 */
  unit: TodoUnit;
  onUnitChange: (u: TodoUnit) => void;
  /** 월요일 → 그 주의 '주 할 일' 개수. 주 모드에서 행 끝 표식으로만 쓴다 */
  weekCounts: Record<string, DayTodoCount>;
  onSelect: (date: string) => void;
  /** 보고 있는 달을 옮긴다 (화살표·월/연 선택) */
  onCursor: (year: number, month: number) => void;
}) {
  const [level, setLevel] = useState<Level>("day");

  // 바깥에서 날짜가 바뀌면(상단 '오늘' 버튼·날짜 이동) 일 그리드로 돌아온다 — 월/연 판을
  // 띄워둔 채로 뒤에서 날짜만 바뀌면 뭘 보고 있는지 어긋난다. 판 안에서의 이동은 selected 를
  // 건드리지 않으므로(커서만 바뀜) 이 effect 가 드릴다운을 되감지 않는다.
  useEffect(() => setLevel("day"), [selected]);

  // 주 모드면 한 행이 곧 한 주가 되도록 월요일 시작 그리드를 쓴다 (주는 월~일)
  const gridStart: 0 | 1 = unit === "week" ? 1 : 0;
  const selMonday = mondayOf(selected);
  const dowAll = weekdaysShort(0);
  const [sunLabel, satLabel] = [dowAll[0], dowAll[6]];

  const td = parseLocalDate(today);
  const todayY = td.getFullYear();
  const todayM = td.getMonth() + 1;
  const start = decadeStart(year);

  // 화살표 한 번 = 지금 보고 있는 판 하나
  const step = (dir: -1 | 1) => {
    if (level === "day") {
      const m = month + dir;
      if (m < 1) onCursor(year - 1, 12);
      else if (m > 12) onCursor(year + 1, 1);
      else onCursor(year, m);
    } else if (level === "month") onCursor(year + dir, month);
    else onCursor(year + dir * DECADE, month);
  };

  const title =
    level === "day"
      ? formatMonthTitle(year, month)
      : level === "month"
        ? formatYearTitle(year)
        : `${start} – ${start + DECADE - 1}`;

  const arrowLabels = {
    day: [t("todos.cal.prevMonth"), t("todos.cal.nextMonth")],
    month: [t("todos.cal.prevYear"), t("todos.cal.nextYear")],
    year: [t("todos.cal.prevDecade"), t("todos.cal.nextDecade")],
  }[level];

  return (
    <div className="todo-cal">
      <div className="cal-head">
        <button
          className="icon-btn ghost sm"
          onClick={() => step(-1)}
          aria-label={arrowLabels[0]}
        >
          <Icon name="chevron-left" size={15} />
        </button>
        {/* 연 단계가 천장이라 더 올라갈 곳이 없다 — 누를 수 없게 두어 헛클릭을 막는다 */}
        <button
          className="cal-title"
          onClick={() => setLevel(level === "day" ? "month" : "year")}
          disabled={level === "year"}
          title={
            level === "day"
              ? t("todos.cal.pickMonth")
              : level === "month"
                ? t("todos.cal.pickYear")
                : undefined
          }
        >
          {title}
        </button>
        <button
          className="icon-btn ghost sm"
          onClick={() => step(1)}
          aria-label={arrowLabels[1]}
        >
          <Icon name="chevron-right" size={15} />
        </button>
      </div>

      {/* 선택 단위 — 일이면 하루, 주면 그 날이 속한 주(월~일) 전체가 대상이 된다.
          월/연 드릴다운 판에서는 고를 날짜 자체가 없으므로 숨긴다. */}
      {level === "day" && (
        <div className="cal-unit segmented">
          {(["day", "week"] as TodoUnit[]).map((u) => (
            <button
              key={u}
              className={`tab ${unit === u ? "active" : ""}`}
              onClick={() => onUnitChange(u)}
            >
              {u === "day" ? t("todos.unit.day") : t("todos.unit.week")}
            </button>
          ))}
        </div>
      )}

      {level === "day" && (
        <>
          {/* 일=빨강, 토=파랑 (한국 달력 관례) — 헤더도 같은 색을 쓴다 */}
          <div className="cal-dow">
            {weekdaysShort(gridStart).map((d) => (
              <span
                key={d}
                className={d === sunLabel ? "sun" : d === satLabel ? "sat" : ""}
              >
                {d}
              </span>
            ))}
          </div>

          <div className={`cal-grid ${unit === "week" ? "by-week" : ""}`}>
            {monthGridDates(year, month, gridStart).map((date) => {
              const d = parseLocalDate(date);
              const inMonth =
                d.getMonth() + 1 === month && d.getFullYear() === year;
              // 주 모드의 점은 '주 할 일' 기준이고 그 주 월요일 칸에만 찍는다 —
              // 일별 점을 그대로 두면 주를 고르는 판에서 하루 단위 정보가 섞여 읽힌다.
              const c =
                unit === "week"
                  ? mondayOf(date) === date
                    ? weekCounts[date]
                    : undefined
                  : counts[date];
              const hasOpen = c ? c.done < c.total : false;
              // AI 가 이날의 리포트를 만드는 중이면 점이 깜빡인다. 할 일이 없어 점이 숨겨진
              // 날도 이때만은 보여준다 — 어느 날이 도는지가 안 보이면 표시의 뜻이 없다.
              const gen = generating.has(date);
              // 주 모드에서는 고른 대상이 하루가 아니라 주다 — 그 주 7칸을 함께 칠한다.
              // today 는 그대로 하루 표식으로 남긴다(오늘이 어디인지는 주 모드에서도 필요하다).
              const inSelWeek = unit === "week" && mondayOf(date) === selMonday;
              const cls =
                date === today
                  ? "today"
                  : unit === "week"
                    ? inSelWeek
                      ? "selected"
                      : inMonth
                        ? ""
                        : "adjacent"
                    : date === selected
                      ? "selected"
                      : inMonth
                        ? ""
                        : "adjacent";
              // 쉬는 날은 일요일과 같은 빨강 — 공휴일이 곧 '일요일 취급'이라는 관례를 따른다.
              // 이름은 칸에 그리지 않는다(그리면 판이 복잡해 보인다) — hover tooltip 으로만.
              //
              // 휴가는 노랑 필로 따로 센다: 공휴일은 모두에게 같은 사실이고 휴가는 내가 정한
              // 것이라 뜻이 다르다. 겹치면 휴가가 이긴다 — 내가 표시한 것이 안 보이면 표시의
              // 뜻이 없다(공휴일에 굳이 연차를 걸었다면 그건 의도한 기록이다).
              const vac = vacations[date];
              const hol = holidayOf(date);
              const dow = d.getDay();
              const tone = vac
                ? " vac"
                : dow === 0 || hol
                  ? " sun"
                  : dow === 6
                    ? " sat"
                    : "";
              const label = vac ? vacationLabel(vac) : (hol?.name ?? "");
              const tip = [label, gen ? t("todos.cal.generating") : null]
                .filter(Boolean)
                .join(" · ");
              // 공휴일/휴가 이름은 hover 로만 — 네이티브 title 은 WKWebView 에서 엉뚱한
              // 자리에 뜨거나 안 떠서(ui.tsx Tooltip 도입 사유) 공용 Tooltip 으로 감싼다.
              // 감싸는 건 칸이 아니라 **날짜 원** — 칸(56px) 기준으로 띄우면 원에서 한참
              // 떨어져 다음 행 높이에 떠서 아랫줄 날짜의 라벨처럼 읽힌다.
              const num = <span className="cal-num">{d.getDate()}</span>;
              return (
                <button
                  key={date}
                  className={`cal-cell ${cls}${tone}`}
                  onClick={() => onSelect(date)}
                >
                  {tip ? <Tooltip label={tip}>{num}</Tooltip> : num}
                  <span
                    className={`cal-dot ${!c ? "none" : hasOpen ? "" : "on"}${gen ? " gen" : ""}`}
                    aria-hidden="true"
                  />
                </button>
              );
            })}
          </div>
        </>
      )}

      {level === "month" && (
        <div className="cal-grid cal-grid-pick">
          {monthsShort().map((label, i) => {
            const m = i + 1;
            const cls =
              year === todayY && m === todayM
                ? "today"
                : m === month
                  ? "selected"
                  : "";
            return (
              <button
                key={label}
                className={`cal-cell cal-pick ${cls}`}
                onClick={() => {
                  onCursor(year, m);
                  setLevel("day");
                }}
              >
                {label}
              </button>
            );
          })}
        </div>
      )}

      {level === "year" && (
        <div className="cal-grid cal-grid-pick">
          {Array.from({ length: DECADE + 2 }, (_, i) => start - 1 + i).map(
            (y) => {
              // 앞뒤 한 칸은 옆 판의 해 — 일 그리드의 '이웃 달' 칸과 같은 흐림 처리
              const outside = y < start || y >= start + DECADE;
              const cls =
                y === todayY
                  ? "today"
                  : y === year
                    ? "selected"
                    : outside
                      ? "adjacent"
                      : "";
              return (
                <button
                  key={y}
                  className={`cal-cell cal-pick ${cls}`}
                  onClick={() => {
                    onCursor(y, month);
                    setLevel("month");
                  }}
                >
                  {y}
                </button>
              );
            },
          )}
        </div>
      )}
    </div>
  );
}
