// 한국 공휴일 — 전부 로컬 계산이다(네트워크 없음, .claude/DESIGN.md 의 로컬 우선 원칙).
//
// 왜 API 가 아니라 표인가: 공공데이터포털의 KASI 특일 정보 API 는 인증키가 필요한데,
// public 레포에 키를 넣을 수도 없고 사용자마다 발급받게 하는 것도 과하다. 무엇보다 오프라인
// 에서 달력이 비어버린다. 반면 음력 날짜는 천문학적으로 확정돼 있고 대체공휴일은 법령대로
// 계산되므로, 표 + 규칙이면 API 와 같은 답이 나온다.
//
// 구성 세 조각:
//   1. LUNAR      — 음력 기반 공휴일의 양력 날짜 (KASI 자료 기반, scripts/gen-lunar-holidays.mjs 생성)
//   2. 양력 고정일 — 신정·삼일절·어린이날·현충일·제헌절·광복절·개천절·한글날·성탄절
//   3. ONE_OFF    — 미리 알 수 없는 임시공휴일·선거일 (확정된 것만 수기로)
// 위 셋을 모은 뒤 대체공휴일 규칙(아래 substituteFor)을 적용한다.
//
// 표 범위(LUNAR_FROM~LUNAR_TO) 밖의 연도는 양력 공휴일만 조용히 내놓는다 — 없는 날을
// 지어내느니 덜 보여주는 편이 낫다. 범위를 늘리려면 생성기를 돌려 LUNAR 를 갱신한다.

import { localDateStr, parseLocalDate, shiftDay } from "./date";
import { t, type MsgKey } from "./i18n";

export type Holiday = {
  /** 표시용 이름 — 한 날에 둘이 겹치면 '·' 로 잇는다 (예: 어린이날·부처님오신날) */
  name: string;
  /** 대체공휴일인가 (본래 공휴일이 주말·다른 공휴일과 겹쳐 밀려난 날) */
  substitute: boolean;
};

/**
 * 음력 기반 공휴일의 양력 날짜 'MM-DD' — [설날(음 1/1), 부처님오신날(음 4/8), 추석(음 8/15)].
 * 한국천문연구원 음양력 자료 기반. 갱신: `node scripts/gen-lunar-holidays.mjs 2020 2050`
 */
const LUNAR: Record<number, readonly [string, string, string]> = {
  2020: ["01-25", "04-30", "10-01"],
  2021: ["02-12", "05-19", "09-21"],
  2022: ["02-01", "05-08", "09-10"],
  2023: ["01-22", "05-27", "09-29"],
  2024: ["02-10", "05-15", "09-17"],
  2025: ["01-29", "05-05", "10-06"],
  2026: ["02-17", "05-24", "09-25"],
  2027: ["02-07", "05-13", "09-15"],
  2028: ["01-27", "05-02", "10-03"],
  2029: ["02-13", "05-20", "09-22"],
  2030: ["02-03", "05-09", "09-12"],
  2031: ["01-23", "05-28", "10-01"],
  2032: ["02-11", "05-16", "09-19"],
  2033: ["01-31", "05-06", "09-08"],
  2034: ["02-19", "05-25", "09-27"],
  2035: ["02-08", "05-15", "09-16"],
  2036: ["01-28", "05-03", "10-04"],
  2037: ["02-15", "05-22", "09-24"],
  2038: ["02-04", "05-11", "09-13"],
  2039: ["01-24", "04-30", "10-02"],
  2040: ["02-12", "05-18", "09-21"],
  2041: ["02-01", "05-07", "09-10"],
  2042: ["01-22", "05-26", "09-28"],
  2043: ["02-10", "05-16", "09-17"],
  2044: ["01-30", "05-05", "10-05"],
  2045: ["02-17", "05-24", "09-25"],
  2046: ["02-06", "05-13", "09-15"],
  2047: ["01-26", "05-02", "10-04"],
  2048: ["02-14", "05-20", "09-22"],
  2049: ["02-02", "05-09", "09-11"],
  2050: ["01-23", "05-28", "09-30"],
};

const LUNAR_FROM = 2020;
const LUNAR_TO = 2050;

/**
 * 규칙으로 못 얻는 공휴일 — 임시공휴일(그때그때 국무회의가 정한다)과 선거일.
 * 확정·시행된 것만 적는다. 예정 선거는 날짜가 바뀔 수 있어 넣지 않는다.
 */
const ONE_OFF: Record<string, MsgKey> = {
  "2020-04-15": "todos.hol.election", // 제21대 국회의원선거
  "2020-08-17": "todos.hol.temporary",
  "2022-03-09": "todos.hol.election", // 제20대 대통령선거
  "2022-06-01": "todos.hol.election", // 제8회 지방선거
  "2023-10-02": "todos.hol.temporary",
  "2024-04-10": "todos.hol.election", // 제22대 국회의원선거
  "2025-01-27": "todos.hol.temporary",
  "2025-06-03": "todos.hol.election", // 제21대 대통령선거
  "2026-06-03": "todos.hol.election", // 제9회 지방선거
};

/** 제헌절이 공휴일로 되돌아온 해 (2025년 개정, 2026-05-11 시행) */
const CONSTITUTION_DAY_FROM = 2026;

/**
 * 대체공휴일 규칙 — 「관공서의 공휴일에 관한 규정」 제3조.
 *   "weekend" 국경일(3·1절·제헌절·광복절·개천절·한글날)·부처님오신날·어린이날·성탄절
 *             → 토요일이나 일요일과 겹치면 대체
 *   "sun"     설날·추석 연휴 → 일요일과 겹칠 때만 대체 (토요일은 제외)
 *   "none"    신정·현충일·선거일·임시공휴일 → 대체 없음
 * 세 경우 모두, 토·일이 아닌 날에 다른 공휴일과 겹쳐도 대체가 생긴다(제3조제1항제3호).
 */
type SubRule = "none" | "sun" | "weekend";

type Base = { date: string; key: MsgKey; sub: SubRule };

const ymd = (year: number, month: number, day: number) =>
  localDateStr(new Date(year, month - 1, day));

/** 그 해의 본래 공휴일(대체공휴일 제외) — 날짜순 */
function baseHolidays(year: number): Base[] {
  const out: Base[] = [];

  out.push({ date: ymd(year, 1, 1), key: "todos.hol.newYear", sub: "none" });
  out.push({ date: ymd(year, 3, 1), key: "todos.hol.independence", sub: "weekend" });
  out.push({ date: ymd(year, 5, 5), key: "todos.hol.children", sub: "weekend" });
  out.push({ date: ymd(year, 6, 6), key: "todos.hol.memorial", sub: "none" });
  if (year >= CONSTITUTION_DAY_FROM)
    out.push({ date: ymd(year, 7, 17), key: "todos.hol.constitution", sub: "weekend" });
  out.push({ date: ymd(year, 8, 15), key: "todos.hol.liberation", sub: "weekend" });
  out.push({ date: ymd(year, 10, 3), key: "todos.hol.foundation", sub: "weekend" });
  out.push({ date: ymd(year, 10, 9), key: "todos.hol.hangeul", sub: "weekend" });
  out.push({ date: ymd(year, 12, 25), key: "todos.hol.christmas", sub: "weekend" });

  const lunar = LUNAR[year];
  if (lunar) {
    const [seollal, buddha, chuseok] = lunar.map((md) => `${year}-${md}`);
    // 설날·추석은 전날·당일·다음날 3일이 통째로 공휴일. 앞뒤는 '연휴' 로 묶어 부른다.
    out.push({ date: shiftDay(seollal, -1), key: "todos.hol.seollalHoliday", sub: "sun" });
    out.push({ date: seollal, key: "todos.hol.seollal", sub: "sun" });
    out.push({ date: shiftDay(seollal, 1), key: "todos.hol.seollalHoliday", sub: "sun" });
    out.push({ date: buddha, key: "todos.hol.buddha", sub: "weekend" });
    out.push({ date: shiftDay(chuseok, -1), key: "todos.hol.chuseokHoliday", sub: "sun" });
    out.push({ date: chuseok, key: "todos.hol.chuseok", sub: "sun" });
    out.push({ date: shiftDay(chuseok, 1), key: "todos.hol.chuseokHoliday", sub: "sun" });
  }

  for (const [date, key] of Object.entries(ONE_OFF))
    if (date.startsWith(`${year}-`)) out.push({ date, key, sub: "none" });

  return out.sort((a, b) => a.date.localeCompare(b.date));
}

const dowOf = (date: string) => parseLocalDate(date).getDay();

/** 그 날짜에 걸린 공휴일들이 대체공휴일을 만드는가 (제3조제1항) */
function needsSubstitute(entries: Base[], date: string): boolean {
  const dow = dowOf(date);
  const weekend = dow === 0 || dow === 6;
  if (weekend)
    return entries.some(
      (e) => e.sub === "weekend" || (e.sub === "sun" && dow === 0),
    );
  // 평일인데 공휴일이 둘 이상 겹친 날 — 대체 대상인 공휴일이 하나라도 있으면 하루가 밀린다.
  // (2028년 추석 당일과 개천절이 10/3 에 겹쳐 10/5 하루만 대체되는 것이 이 규칙이다)
  return entries.length > 1 && entries.some((e) => e.sub !== "none");
}

const cache = new Map<number, Map<string, Holiday>>();

/** 그 해의 공휴일 전부 (대체공휴일 포함) — 'YYYY-MM-DD' → Holiday */
export function holidaysOfYear(year: number): Map<string, Holiday> {
  const hit = cache.get(year);
  if (hit) return hit;

  const byDate = new Map<string, Base[]>();
  for (const b of baseHolidays(year)) {
    const list = byDate.get(b.date);
    if (list) list.push(b);
    else byDate.set(b.date, [b]);
  }

  const out = new Map<string, Holiday>();
  for (const [date, entries] of byDate)
    out.set(date, {
      name: [...new Set(entries.map((e) => t(e.key)))].join("·"),
      substitute: false,
    });

  // 대체공휴일: 본래 공휴일 다음의 첫 '비공휴일'(일요일도, 이미 잡힌 공휴일도 아닌 날).
  // 날짜순으로 처리해 앞의 대체가 뒤의 대체를 밀어낸다(제3조제2항).
  for (const date of [...byDate.keys()].sort()) {
    if (!needsSubstitute(byDate.get(date)!, date)) continue;
    let d = shiftDay(date, 1);
    while (dowOf(d) === 0 || out.has(d)) d = shiftDay(d, 1);
    out.set(d, { name: t("todos.hol.substitute"), substitute: true });
  }

  cache.set(year, out);
  return out;
}

/** 그 날짜가 공휴일이면 이름을, 아니면 undefined. 표 범위 밖 연도는 양력 공휴일만 안다. */
export function holidayOf(date: string): Holiday | undefined {
  const year = Number(date.slice(0, 4));
  if (!Number.isFinite(year)) return undefined;
  return holidaysOfYear(year).get(date);
}

/** 음력 공휴일까지 아는 연도인가 — 표 밖이면 설날·추석·부처님오신날이 빠진다 */
export function hasLunarData(year: number): boolean {
  return year >= LUNAR_FROM && year <= LUNAR_TO;
}
