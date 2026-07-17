// 할 일(todos) DB 접근 계층. db.ts 패턴 그대로: getDb 싱글턴, $n 플레이스홀더,
// db.select/db.execute, lastInsertId, 멱등 UPDATE(WHERE done <> $1).
// 스키마·트리거는 Rust 마이그레이션(0002_todos.sql)이 정본. 완료 시각(completed_at)은 트리거가 관리.

import { getDb } from "./db";
import type { DayTodoCount, Todo } from "../types";

const now = () => Date.now();

/** 특정 날짜의 할 일. 사용자가 정한 순서(sort_order) → 생성순 (idx_todos_sort 에 대응) */
export async function listTodos(date: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    `SELECT * FROM todos WHERE due_date = $1 ORDER BY sort_order, id`,
    [date],
  );
}

/** 날짜 범위(캘린더 그리드) 날짜별 개수 — 점·월 요약용 */
export async function listMonthCounts(
  from: string,
  to: string,
): Promise<DayTodoCount[]> {
  const db = await getDb();
  return db.select<DayTodoCount[]>(
    `SELECT due_date, COUNT(*) AS total, COALESCE(SUM(done), 0) AS done
       FROM todos WHERE due_date BETWEEN $1 AND $2 GROUP BY due_date`,
    [from, to],
  );
}

/** 밀린 할 일: before(보통 오늘) 이전의 미완료. idx_todos_open 부분 인덱스 전용 */
export async function listOverdueOpen(before: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    `SELECT * FROM todos WHERE done = 0 AND due_date < $1 ORDER BY due_date, id`,
    [before],
  );
}

/** 할 일 추가. 생성된 id 반환 (createConcept 방식) */
export async function createTodo(
  content: string,
  dueDate: string,
): Promise<number> {
  const db = await getDb();
  const res = await db.execute(
    `INSERT INTO todos (content, due_date, sort_order)
     VALUES ($1, $2, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM todos WHERE due_date = $3))`,
    [content, dueDate, dueDate],
  );
  return res.lastInsertId as number;
}

/** 드래그로 정한 순서를 저장 — orderedIds 의 위치(0..n)를 각 항목의 sort_order 로 */
export async function reorderTodos(orderedIds: number[]): Promise<void> {
  if (!orderedIds.length) return;
  const db = await getDb();
  for (let i = 0; i < orderedIds.length; i++) {
    await db.execute(`UPDATE todos SET sort_order = $1 WHERE id = $2`, [
      i,
      orderedIds[i],
    ]);
  }
}

/** 완료 토글. 이미 해당 상태면 no-op → 멱등(setStatus 패턴). completed_at 은 트리거가 스탬프 */
export async function toggleTodo(id: number, done: 0 | 1): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE todos SET done = $1 WHERE id = $2 AND done <> $1`,
    [done, id],
  );
}

/** 내용 수정 (updated_at 갱신) */
export async function updateTodoContent(
  id: number,
  content: string,
): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE todos SET content = $1, updated_at = $2 WHERE id = $3`,
    [content, now(), id],
  );
}

/** 여러 항목의 날짜를 옮김 (밀린 할 일 이월). done 은 건드리지 않음 */
export async function moveTodos(ids: number[], dueDate: string): Promise<void> {
  if (!ids.length) return;
  const db = await getDb();
  const placeholders = ids.map((_, i) => `$${i + 3}`).join(",");
  await db.execute(
    `UPDATE todos SET due_date = $1, updated_at = $2 WHERE id IN (${placeholders})`,
    [dueDate, now(), ...ids],
  );
}

/** 삭제 (즉시, 확인 모달 없음 — 값싼 대상) */
export async function deleteTodo(id: number): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM todos WHERE id = $1`, [id]);
}
