// 휴가(연차·반차 등) DB 접근 계층. todos.ts 패턴 그대로: getDb 싱글턴, $n 플레이스홀더.
// 스키마는 Rust 마이그레이션(0010_vacations.sql)이 정본 — 날짜가 PK라 하루에 하나뿐이다.
//
// 공휴일(holidays.ts)과 짝이지만 성격이 정반대다: 공휴일은 규칙으로 계산되는 사실이라 저장하지
// 않고, 휴가는 그 사람만 아는 정보라 저장이 정본이다. 캘린더는 둘을 같은 자리(날짜 밑 라벨)에
// 그리되 휴가를 먼저 보여준다 — 사용자가 직접 표시한 것이 계산된 것보다 앞선다.

import { getDb } from "./db";
import { t, type MsgKey } from "./i18n";

/** 휴가 종류 코드. DB엔 이 코드가 들어가고 화면 문구는 i18n 사전이 정한다 */
export const VACATION_KINDS = [
  "annual", // 연차
  "half", // 반차
  "sick", // 병가
  "public", // 공가
  "special", // 특별휴가
] as const;

export type VacationKind = (typeof VACATION_KINDS)[number];

/** 기본 종류 — '휴가' 버튼 한 번에 잡히는 값. 회사 휴가의 대다수가 연차다 */
export const DEFAULT_KIND: VacationKind = "annual";

export function isVacationKind(s: string): s is VacationKind {
  return (VACATION_KINDS as readonly string[]).includes(s);
}

/** 종류 코드 → 사전 키. 문자열을 조립하지 않고 표로 두어 번역 누락이 컴파일에 걸리게 한다 */
const KIND_MSG: Record<VacationKind, MsgKey> = {
  annual: "todos.vac.kind.annual",
  half: "todos.vac.kind.half",
  sick: "todos.vac.kind.sick",
  public: "todos.vac.kind.public",
  special: "todos.vac.kind.special",
};

/** 종류 코드 → 표시 이름 (달력 라벨·헤더 칩 공용) */
export function vacationLabel(kind: VacationKind): string {
  return t(KIND_MSG[kind]);
}

/** 날짜 범위(캘린더 그리드)의 휴가 — 'YYYY-MM-DD' → 종류 */
export async function listVacations(
  from: string,
  to: string,
): Promise<Record<string, VacationKind>> {
  const db = await getDb();
  const rows = await db.select<{ date: string; kind: string }[]>(
    `SELECT date, kind FROM vacations WHERE date BETWEEN $1 AND $2`,
    [from, to],
  );
  const out: Record<string, VacationKind> = {};
  // 모르는 코드(예전 버전이 넣었거나 손으로 고친 값)는 기본값으로 읽는다 — 날짜가 통째로
  // 사라지는 것보다 종류 하나 틀리는 편이 낫다
  for (const r of rows)
    out[r.date] = isVacationKind(r.kind) ? r.kind : DEFAULT_KIND;
  return out;
}

/** 하루의 휴가 종류 (없으면 null) */
export async function getVacation(date: string): Promise<VacationKind | null> {
  const db = await getDb();
  const rows = await db.select<{ kind: string }[]>(
    `SELECT kind FROM vacations WHERE date = $1`,
    [date],
  );
  if (!rows.length) return null;
  return isVacationKind(rows[0].kind) ? rows[0].kind : DEFAULT_KIND;
}

/** 휴가 지정·종류 변경 (하루에 하나라 UPSERT) */
export async function setVacation(
  date: string,
  kind: VacationKind,
): Promise<void> {
  const db = await getDb();
  await db.execute(
    `INSERT INTO vacations (date, kind) VALUES ($1, $2)
     ON CONFLICT(date) DO UPDATE SET kind = $3, updated_at = $4`,
    [date, kind, kind, Date.now()],
  );
}

/** 휴가 해제 */
export async function removeVacation(date: string): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM vacations WHERE date = $1`, [date]);
}
