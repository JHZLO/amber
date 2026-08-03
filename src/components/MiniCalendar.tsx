// 미니 월 캘린더 (할 일 탭 좌측). 순수 프레젠테이션 — 커서(year/month)·selected 는 부모가 소유.
// 상태 표현은 모노톤 문법(.claude/DESIGN.md §3): 오늘=primary 필, 선택=surface-3, 점=채움/아웃라인.
//
// 제목을 누르면 흔한 달력 컴포넌트(Chrome·Windows 날짜 선택기)처럼 **위로 한 단계씩** 열린다:
//   일 ── 제목("2026년 8월") ─▶ 월 ── 제목("2026년") ─▶ 연
// 되돌아오는 길은 선택이다 — 월을 고르면 그 달의 일 그리드로, 연을 고르면 그 해의 월 그리드로.
// 좌우 화살표는 보고 있는 단계의 한 판씩 움직인다(일=±1달, 월=±1년, 연=±10년).

import { useEffect, useState } from "react";
import type { DayTodoCount } from "../types";
import {
  formatMonthTitle,
  formatYearTitle,
  monthGridDates,
  monthsShort,
  parseLocalDate,
  weekdaysShort,
} from "../lib/date";
import { t } from "../lib/i18n";
import { Icon } from "../icons";

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
  onSelect,
  onCursor,
}: {
  year: number;
  month: number;
  selected: string;
  today: string;
  counts: Record<string, DayTodoCount>;
  onSelect: (date: string) => void;
  /** 보고 있는 달을 옮긴다 (화살표·월/연 선택) */
  onCursor: (year: number, month: number) => void;
}) {
  const [level, setLevel] = useState<Level>("day");

  // 바깥에서 날짜가 바뀌면(상단 '오늘' 버튼·날짜 이동) 일 그리드로 돌아온다 — 월/연 판을
  // 띄워둔 채로 뒤에서 날짜만 바뀌면 뭘 보고 있는지 어긋난다. 판 안에서의 이동은 selected 를
  // 건드리지 않으므로(커서만 바뀜) 이 effect 가 드릴다운을 되감지 않는다.
  useEffect(() => setLevel("day"), [selected]);

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

      {level === "day" && (
        <>
          <div className="cal-dow">
            {weekdaysShort().map((d) => (
              <span key={d}>{d}</span>
            ))}
          </div>

          <div className="cal-grid">
            {monthGridDates(year, month).map((date) => {
              const d = parseLocalDate(date);
              const inMonth =
                d.getMonth() + 1 === month && d.getFullYear() === year;
              const c = counts[date];
              const hasOpen = c ? c.done < c.total : false;
              const cls =
                date === today
                  ? "today"
                  : date === selected
                    ? "selected"
                    : inMonth
                      ? ""
                      : "adjacent";
              return (
                <button
                  key={date}
                  className={`cal-cell ${cls}`}
                  onClick={() => onSelect(date)}
                >
                  <span className="cal-num">{d.getDate()}</span>
                  <span
                    className={`cal-dot ${!c ? "none" : hasOpen ? "" : "on"}`}
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
