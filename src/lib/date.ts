// 로컬 달력 날짜('YYYY-MM-DD') 유틸 — 로컬 날짜의 유일한 생성·파싱 지점 (.claude/DESIGN.md §10).
//
// 왜 문자열인가: 캘린더 좌표·마감일은 "사용자가 보는 달력의 그 날"이지 순간(instant)이 아니다.
// UTC ms 를 날짜로 재해석하면 시간대 경계(자정 부근)에서 하루가 밀린다. 그래서 로컬 날짜는
// 'YYYY-MM-DD' 문자열로 다루고, Date 로 변환할 때는 반드시 new Date(y, m-1, d)(로컬 생성자)만 쓴다.
// new Date('YYYY-MM-DD') 는 UTC 자정으로 파싱되므로 절대 쓰지 않는다.

export const WEEKDAYS_KO = ["일", "월", "화", "수", "목", "금", "토"] as const;

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

/** "7월 16일 목요일" */
export function formatDayLong(s: string): string {
  const d = parseLocalDate(s);
  return `${d.getMonth() + 1}월 ${d.getDate()}일 ${WEEKDAYS_KO[d.getDay()]}요일`;
}

/** "7/14" — 밀린 항목의 원래 날짜 등 짧은 표기 */
export function formatDayShort(s: string): string {
  const d = parseLocalDate(s);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** "2026년 7월" */
export function formatMonthTitle(year: number, month: number): string {
  return `${year}년 ${month}월`;
}
