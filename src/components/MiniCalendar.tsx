// 미니 월 캘린더 (할 일 탭 좌측). 순수 프레젠테이션 — 상태(cursor/selected)는 부모가 소유.
// 상태 표현은 모노톤 문법(.claude/DESIGN.md §3): 오늘=primary 필, 선택=surface-3, 점=채움/아웃라인.

import type { DayTodoCount } from "../types";
import {
  WEEKDAYS_KO,
  formatMonthTitle,
  monthGridDates,
  monthOf,
  parseLocalDate,
} from "../lib/date";
import { Icon } from "../icons";

export function MiniCalendar({
  year,
  month,
  selected,
  today,
  counts,
  onSelect,
  onPrevMonth,
  onNextMonth,
  onToday,
}: {
  year: number;
  month: number;
  selected: string;
  today: string;
  counts: Record<string, DayTodoCount>;
  onSelect: (date: string) => void;
  onPrevMonth: () => void;
  onNextMonth: () => void;
  onToday: () => void;
}) {
  const cells = monthGridDates(year, month);

  // 월 요약: 이 달(인접 월 제외) 항목 합계
  let monthTotal = 0;
  let monthDone = 0;
  for (const [date, c] of Object.entries(counts)) {
    const m = monthOf(date);
    if (m.year === year && m.month === month) {
      monthTotal += c.total;
      monthDone += c.done;
    }
  }

  return (
    <div className="todo-cal">
      <div className="cal-head">
        <button className="icon-btn ghost sm" onClick={onPrevMonth} aria-label="이전 달">
          <Icon name="chevron-left" size={15} />
        </button>
        <button className="cal-title" onClick={onToday} title="오늘로">
          {formatMonthTitle(year, month)}
        </button>
        <button className="icon-btn ghost sm" onClick={onNextMonth} aria-label="다음 달">
          <Icon name="chevron-right" size={15} />
        </button>
      </div>

      <div className="cal-dow">
        {WEEKDAYS_KO.map((d) => (
          <span key={d}>{d}</span>
        ))}
      </div>

      <div className="cal-grid">
        {cells.map((date) => {
          const d = parseLocalDate(date);
          const inMonth = d.getMonth() + 1 === month && d.getFullYear() === year;
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

      <div className="cal-month-sum">
        <span>이번 달</span>
        <span>
          {monthDone} / {monthTotal}
        </span>
      </div>
    </div>
  );
}
