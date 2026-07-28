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

/** 밀린 할 일: before(보통 오늘) 이전의 미완료. idx_todos_open 부분 인덱스 전용.
 *  형제 순서는 listTodos 와 같은 sort_order 기준 — 스트립도 계층으로 그리므로 어긋나면 안 된다. */
export async function listOverdueOpen(before: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    `SELECT * FROM todos WHERE done = 0 AND due_date < $1
      ORDER BY due_date, sort_order, id`,
    [before],
  );
}

/** 할 일 추가. parentId 를 주면 그 항목의 하위로. sort_order 는 형제 그룹 내 맨 끝 */
export async function createTodo(
  content: string,
  dueDate: string,
  parentId: number | null = null,
): Promise<number> {
  const db = await getDb();
  const res = await db.execute(
    `INSERT INTO todos (content, due_date, parent_id, sort_order)
     VALUES ($1, $2, $3,
       (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM todos WHERE due_date = $4 AND parent_id IS $5))`,
    [content, dueDate, parentId, dueDate, parentId],
  );
  return res.lastInsertId as number;
}

/** 드래그 트리 이동: 부모 변경 (하위로 넣기·상위로 꺼내기·다른 부모로).
 *  순환(자기 서브트리 안으로) 방지는 호출부(UI)가 후보에서 제외하는 방식으로 보장한다. */
export async function reparentTodo(
  id: number,
  parentId: number | null,
): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE todos SET parent_id = $1, updated_at = $2 WHERE id = $3`,
    [parentId, now(), id],
  );
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

/** 어떤 노드 토글 → 그 노드와 모든 자손(재귀)을 같은 상태로 (하향 전파, 다단계).
 *  트리거가 각 행 completed_at 관리. 위쪽(조상) 재계산은 recomputeChainFrom 이 담당. */
export async function setSubtreeDone(id: number, done: 0 | 1): Promise<void> {
  const db = await getDb();
  await db.execute(
    `WITH RECURSIVE sub(id) AS (
       SELECT $2
       UNION ALL
       SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )
     UPDATE todos SET done = $1 WHERE done <> $1 AND id IN (SELECT id FROM sub)`,
    [done, id],
  );
}

/** 자식 상태로 부모 완료 재계산 — 자식이 있고 전부 완료면 부모도 완료, 아니면 미완료 */
export async function recomputeParentDone(parentId: number): Promise<void> {
  const db = await getDb();
  const rows = await db.select<{ open: number; total: number }[]>(
    `SELECT COALESCE(SUM(done = 0), 0) AS open, COUNT(*) AS total
       FROM todos WHERE parent_id = $1`,
    [parentId],
  );
  const r = rows[0] ?? { open: 0, total: 0 };
  const done: 0 | 1 = r.total > 0 && r.open === 0 ? 1 : 0;
  await db.execute(`UPDATE todos SET done = $1 WHERE id = $2 AND done <> $1`, [
    done,
    parentId,
  ]);
}

/** 자손 변경 후, 주어진 부모에서 위로(조상 체인) 완료 상태를 아래→위로 재계산한다.
 *  각 단계는 직속 자식만 보므로(recomputeParentDone), 바닥부터 올라가면 정합성이 유지된다.
 *  parentId=null 이면 최상위라 재계산할 조상이 없다(no-op). */
export async function recomputeChainFrom(
  parentId: number | null,
): Promise<void> {
  const db = await getDb();
  let pid: number | null = parentId;
  while (pid != null) {
    await recomputeParentDone(pid);
    const rows = await db.select<{ parent_id: number | null }[]>(
      `SELECT parent_id FROM todos WHERE id = $1`,
      [pid],
    );
    pid = rows.length ? rows[0].parent_id : null;
  }
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

/** 여러 항목의 날짜를 옮김 (밀린 할 일 이월). done 은 건드리지 않음.
 *  **서브트리째** 옮긴다 — 자손을 남기면 그 자손의 부모가 다른 날에 있어 어느 날에도 안 그려진다
 *  (하루 목록은 parent_id IS NULL 부터 내려가며 그린다). 같은 이유로, 함께 오지 않는 부모 밑에
 *  있던 항목은 도착 날짜에서 최상위로 올린다(parent_id = NULL).
 *  단일 문장 두 개(조회 → UPDATE)이며 실제 변경은 한 UPDATE 안에서 원자적으로 일어난다.
 *  @returns 뒤에 남은 원래 부모 id 들 — 자식이 빠졌으니 호출부가 recomputeChainFrom 로 재계산한다. */
export async function moveTodos(
  ids: number[],
  dueDate: string,
): Promise<number[]> {
  if (!ids.length) return [];
  const db = await getDb();
  // 씨앗 id 끼리 부모-자식일 수 있어(‘모두 오늘로’) UNION 으로 중복을 접는다
  const movedCte = (from: number) =>
    `WITH RECURSIVE moved(id) AS (
       SELECT id FROM todos WHERE id IN (${ids.map((_, i) => `$${i + from}`).join(",")})
       UNION
       SELECT t.id FROM todos t JOIN moved ON t.parent_id = moved.id
     )`;
  const orphaned = await db.select<{ parent_id: number }[]>(
    `${movedCte(1)}
     SELECT DISTINCT parent_id FROM todos
      WHERE id IN (SELECT id FROM moved)
        AND parent_id IS NOT NULL
        AND parent_id NOT IN (SELECT id FROM moved)`,
    ids,
  );
  await db.execute(
    `${movedCte(3)}
     UPDATE todos
        SET due_date = $1,
            updated_at = $2,
            parent_id = CASE WHEN parent_id IN (SELECT id FROM moved)
                             THEN parent_id ELSE NULL END
      WHERE id IN (SELECT id FROM moved)`,
    [dueDate, now(), ...ids],
  );
  return orphaned.map((r) => r.parent_id);
}

/** 삭제. 노드를 지우면 그 서브트리 전체(모든 자손)를 함께 — 되돌릴 수 없으므로
 *  자손이 있을 때만 호출부가 확인 모달을 띄운다(홑 항목은 값싼 대상이라 즉시 삭제, DESIGN §8).
 *  삭제 후 부모 완료 상태 재계산은 호출부(recomputeChainFrom)가 담당. */
export async function deleteTodo(id: number): Promise<void> {
  const db = await getDb();
  await db.execute(
    `WITH RECURSIVE sub(id) AS (
       SELECT $1
       UNION ALL
       SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )
     DELETE FROM todos WHERE id IN (SELECT id FROM sub)`,
    [id],
  );
}
