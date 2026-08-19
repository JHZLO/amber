// 로컬 달력 날짜('YYYY-MM-DD') 유틸 — 로컬 날짜의 유일한 생성·파싱 지점 (.claude/DESIGN.md §10).
//
// 왜 문자열인가: 캘린더 좌표·마감일은 "사용자가 보는 달력의 그 날"이지 순간(instant)이 아니다.
// UTC ms 를 날짜로 재해석하면 시간대 경계(자정 부근)에서 하루가 밀린다. 그래서 로컬 날짜는
// 'YYYY-MM-DD' 문자열로 다루고, Date 로 변환할 때는 반드시 new Date(y, m-1, d)(로컬 생성자)만 쓴다.
// new Date('YYYY-MM-DD') 는 UTC 자정으로 파싱되므로 절대 쓰지 않는다.

import { getLang } from "./i18n";

export const WEEKDAYS_KO = ["일", "월", "화", "수", "목", "금", "토"] as const;
const WEEKDAYS_EN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

/** 요일 짧은 표기(일~토) — 캘린더 헤더 등. 언어 설정을 따른다 */
export function weekdaysShort(startsOn: 0 | 1 = WEEK_STARTS_ON): readonly string[] {
  const base = getLang() === "ko" ? WEEKDAYS_KO : WEEKDAYS_EN;
  // 월요일 시작이면 일요일을 뒤로 보낸다 — 그리드 컬럼과 어긋나면 요일 색(일=빨강)이 틀어진다
  return startsOn === 0 ? base : [...base.slice(1), base[0]];
}

const pad2 = (n: number) => (n < 10 ? `0${n}` : `${n}`);

/** Date → 로컬 'YYYY-MM-DD' */
export function localDateStr(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** 오늘(로컬) 'YYYY-MM-DD' */
export function todayStr(): string {
  return localDateStr(new Date());
}

/** 'YYYY-MM-DD' → 로컬 자정 Date. (UTC 파싱 함정 회피 위해 항상 이 함수를 거친다) */
export function parseLocalDate(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** 날짜 문자열에 delta 일을 더한 로컬 날짜 */
export function shiftDay(s: string, delta: number): string {
  const d = parseLocalDate(s);
  d.setDate(d.getDate() + delta);
  return localDateStr(d);
}

/** {year, month(1-12)} 에 delta 개월을 더한 달 */
export function addMonths(
  cur: { year: number; month: number },
  delta: number,
): { year: number; month: number } {
  // month 를 0-based 로 환산해 Date 산술 → 연도 넘김 자동 처리
  const d = new Date(cur.year, cur.month - 1 + delta, 1);
  return { year: d.getFullYear(), month: d.getMonth() + 1 };
}

/** 'YYYY-MM-DD' 가 속한 {year, month(1-12)} */
/** 주가 시작하는 요일 — 0=일요일, 1=월요일. **앱 전체에서 '주'의 정의는 이 상수 하나다.**
 *
 *  캘린더 표시·주 단위 할 일·주간 리포트가 전부 여기서 파생된다. 예전에는 캘린더가 일요일,
 *  주간 리포트가 월요일(스프린트 규약)로 갈려 있었는데, 한 화면에 나란히 놓이자 같은 '주'가
 *  두 뜻을 갖게 됐다. 바꾸려면 이 값만 바꾼다(마이그레이션 0013 주석 참고). */
export const WEEK_STARTS_ON: 0 | 1 = 0;

/** 그 날짜가 속한 주의 시작 'YYYY-MM-DD' (WEEK_STARTS_ON 기준).
 *  캘린더 선택·주 할 일·주간 리포트가 모두 이 함수 하나를 쓴다. */
export function weekStartOf(s: string): string {
  const d = parseLocalDate(s);
  d.setDate(d.getDate() - ((d.getDay() - WEEK_STARTS_ON + 7) % 7));
  return localDateStr(d);
}

/** 주 시작일 → 그 주 7일의 'YYYY-MM-DD' 배열 */
export function weekDays(weekStart: string): string[] {
  const d = parseLocalDate(weekStart);
  return Array.from({ length: 7 }, (_, i) => {
    const x = new Date(d.getFullYear(), d.getMonth(), d.getDate() + i);
    return localDateStr(x);
  });
}

export function monthOf(s: string): { year: number; month: number } {
  const d = parseLocalDate(s);
  return { year: d.getFullYear(), month: d.getMonth() + 1 };
}

/** 로컬 날짜의 [자정, 다음날 자정) 을 UTC ms 범위로. learned_at 등 타임스탬프 컬럼 조회용 */
export function dayRangeMs(s: string): [number, number] {
  const start = parseLocalDate(s);
  const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + 1);
  return [start.getTime(), end.getTime()];
}

/**
 * 월(1-12) 캘린더 그리드의 날짜 문자열 배열. 필요한 주(4~6주)만.
 * 앞뒤로 인접 월 날짜가 포함될 수 있다(호출부에서 monthOf 로 판별).
 *
 * `startsOn` 은 한 행의 첫 요일이다. 기본값은 WEEK_STARTS_ON — 한 행이 곧 한 주여야
 * 주 단위 선택에서 선택 띠가 두 행으로 잘리지 않는다.
 */
export function monthGridDates(
  year: number,
  month: number,
  startsOn: 0 | 1 = WEEK_STARTS_ON,
): string[] {
  const first = new Date(year, month - 1, 1);
  // 그 주의 첫 칸까지 거슬러 올라간다 (일 시작이면 일요일, 월 시작이면 월요일)
  const back = (first.getDay() - startsOn + 7) % 7;
  const start = new Date(year, month - 1, 1 - back);
  const last = new Date(year, month, 0); // 이 달 마지막 날
  const fwd = (startsOn + 6 - last.getDay() + 7) % 7; // 마지막 날이 속한 주의 끝 칸까지
  const end = new Date(year, month - 1, last.getDate() + fwd);
  const out: string[] = [];
  const cur = new Date(start);
  while (cur <= end) {
    out.push(localDateStr(cur));
    cur.setDate(cur.getDate() + 1);
  }
  return out;
}

/** ko "7월 16일 목요일" / en "Thursday, July 16" */
export function formatDayLong(s: string): string {
  const d = parseLocalDate(s);
  if (getLang() === "ko")
    return `${d.getMonth() + 1}월 ${d.getDate()}일 ${WEEKDAYS_KO[d.getDay()]}요일`;
  return d.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" });
}

/** "월" / "Mon" — 주간 리포트 프롬프트의 날짜 섹션 라벨 */
export function weekdayShort(s: string): string {
  const d = parseLocalDate(s);
  if (getLang() === "ko") return WEEKDAYS_KO[d.getDay()];
  return d.toLocaleDateString("en-US", { weekday: "short" });
}

/** "7/14" — 밀린 항목의 원래 날짜 등 짧은 표기 */
export function formatDayShort(s: string): string {
  const d = parseLocalDate(s);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** ko "2026년 7월" / en "July 2026" */
export function formatMonthTitle(year: number, month: number): string {
  if (getLang() === "ko") return `${year}년 ${month}월`;
  return new Date(year, month - 1, 1).toLocaleDateString("en-US", {
    month: "long",
    year: "numeric",
  });
}

/** ko "2026년" / en "2026" — 캘린더 월 선택 단계의 제목 */
export function formatYearTitle(year: number): string {
  return getLang() === "ko" ? `${year}년` : `${year}`;
}

const MONTHS_KO = Array.from({ length: 12 }, (_, i) => `${i + 1}월`);
const MONTHS_EN = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/** 월 짧은 표기(1~12월 / Jan~Dec) — 캘린더 월 선택 그리드. 언어 설정을 따른다 */
export function monthsShort(): readonly string[] {
  return getLang() === "ko" ? MONTHS_KO : MONTHS_EN;
}
