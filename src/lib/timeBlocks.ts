// 시간 블록(타임테이블) DB 접근 계층. todos.ts 패턴 그대로: getDb 싱글턴, $n 플레이스홀더.
// 스키마는 Rust 마이그레이션(0007_time_blocks.sql)이 정본.
// 시간 좌표는 "로컬 날짜 TEXT + 자정 기준 분" — UTC ms 재해석 금지(.claude/DESIGN.md §10).

import { getDb } from "./db";
import type { TimeBlock } from "../types";

const now = () => Date.now();

/** 특정 날짜의 블록. 시작 시각순 (idx_blocks_day 에 대응) */
export async function listBlocks(date: string): Promise<TimeBlock[]> {
  const db = await getDb();
  return db.select<TimeBlock[]>(
    `SELECT * FROM time_blocks WHERE date = $1 ORDER BY start_min, id`,
    [date],
  );
}

/** 블록 생성. todoId 를 주면 그 할 일의 시간 배치(제목은 할 일 내용을 미러) */
export async function createBlock(
  date: string,
  startMin: number,
  endMin: number,
  title = "",
  todoId: number | null = null,
): Promise<number> {
  const db = await getDb();
  const res = await db.execute(
    `INSERT INTO time_blocks (date, start_min, end_min, title, todo_id)
     VALUES ($1, $2, $3, $4, $5)`,
    [date, startMin, endMin, title, todoId],
  );
  return res.lastInsertId as number;
}

/** 이동/리사이즈 — 시간 범위 갱신 */
export async function updateBlockTime(
  id: number,
  startMin: number,
  endMin: number,
): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE time_blocks SET start_min = $1, end_min = $2, updated_at = $3 WHERE id = $4`,
    [startMin, endMin, now(), id],
  );
}

/** 제목 변경 (연동 블록엔 쓰지 않는다 — 제목은 할 일 내용을 미러) */
export async function renameBlock(id: number, title: string): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE time_blocks SET title = $1, updated_at = $2 WHERE id = $3`,
    [title, now(), id],
  );
}

/** 삭제 (즉시, 확인 모달 없음 — 값싼 대상) */
export async function deleteBlock(id: number): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM time_blocks WHERE id = $1`, [id]);
}

// ---- 순수 헬퍼 (컴포넌트/뷰 공용) ----

/** 지금 시각의 자정 기준 분 (로컬 벽시계) */
export function nowMinute(): number {
  const d = new Date();
  return d.getHours() * 60 + d.getMinutes();
}

/** fromMin 이후로 dur 분이 들어가는 첫 빈 슬롯의 시작 분.
 *  기존 블록 사이 gap 을 순서대로 훑고, 끝까지 없으면 fromMin 그대로(겹침 허용 — lane 이 처리). */
export function findFreeSlot(
  blocks: TimeBlock[],
  fromMin: number,
  dur: number,
): number {
  const latest = 1440 - dur;
  let cur = Math.min(fromMin, latest);
  const sorted = [...blocks].sort((a, b) => a.start_min - b.start_min);
  for (const b of sorted) {
    if (b.end_min <= cur) continue; // 이미 지난 블록
    if (b.start_min >= cur + dur) return cur; // cur 앞에 dur 만큼 gap
    cur = Math.min(b.end_min, latest); // 블록 뒤로 밀기
  }
  return cur;
}
