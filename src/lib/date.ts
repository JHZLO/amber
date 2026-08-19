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
export function weekdaysShort(): readonly string[] {
  return getLang() === "ko" ? WEEKDAYS_KO : WEEKDAYS_EN;
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
/** 그 날짜가 속한 주의 시작(일요일) 'YYYY-MM-DD' — 미니 캘린더(일~토)와 같은 기준 */
export function weekStartOf(s: string): string {
  const d = parseLocalDate(s);
  d.setDate(d.getDate() - d.getDay());
  return localDateStr(d);
}

/** 그 날짜가 속한 주의 월요일 'YYYY-MM-DD'.
 *  weekStartOf 는 **일요일** 기준(미니 캘린더·타임테이블 주간 뷰가 일~토)이라 따로 둔다.
 *  둘이 다른 건 의도다: 주간 리포트는 노션 팀 공유용이라 월~일 스프린트 규약을 따라야 하고,
 *  캘린더/타임테이블은 기존 표시 기준을 그대로 유지한다. 합치지 말 것. */
export function mondayOf(s: string): string {
  const d = parseLocalDate(s);
  // getDay(): 0=일 … 6=토. 일요일은 '지난 월요일'(-6)로 붙인다
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return localDateStr(d);
}

/** 월요일 → 그 주 7일의 'YYYY-MM-DD' 배열 (월~일) */
export function weekDays(monday: string): string[] {
  const d = parseLocalDate(monday);
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
 * 월(1-12) 캘린더 그리드의 날짜 문자열 배열. 일요일 시작, 필요한 주(4~6주)만.
 * 앞뒤로 인접 월 날짜가 포함될 수 있다(호출부에서 monthOf 로 판별).
 */
export function monthGridDates(year: number, month: number): string[] {
  const first = new Date(year, month - 1, 1);
  const start = new Date(year, month - 1, 1 - first.getDay()); // 그 주 일요일
  const last = new Date(year, month, 0); // 이 달 마지막 날
  const end = new Date(year, month - 1, last.getDate() + (6 - last.getDay())); // 마지막 날이 속한 주 토요일
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
