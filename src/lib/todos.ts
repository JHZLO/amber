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

/** 여러 항목의 날짜를 옮김 (밀린 할 일 이월). 체크 상태 자체는 바꾸지 않는다.
 *
 *  **미완료 가지만 따라간다** — 이미 체크한 자식은 끝낸 날짜의 기록이라 그대로 둔다.
 *  그런데 부모만 옮겨버리면 남은 완료 자식이 헤딩을 잃는다("누아 항공권" 밑에 1·2 를 체크해
 *  두고 부모를 가져오면 어제에서 그 부모가 사라진다). 그래서 **뒤에 남는 자식이 있는 노드는
 *  분할**한다 — 원본은 완료분의 부모로 원래 날짜에 남고, 같은 내용의 사본이 미완료 자식을
 *  데리고 옮겨간다. 어제 = `누아 항공권 › 1✓ 2✓`, 오늘 = `누아 항공권 › 3 4`.
 *
 *  분할하고 남은 원본은 자식이 전부 완료이므로 완료로 재계산한다 — 그래야 그 날의 기록으로
 *  닫히고 내일 밀린 목록에 사본과 함께 두 번 뜨지 않는다.
 *  함께 오지 않는 부모 밑에 있던 항목은 도착 날짜에서 최상위로 올린다(parent_id = NULL). */
export async function moveTodos(ids: number[], dueDate: string): Promise<void> {
  if (!ids.length) return;
  const db = await getDb();
  const ph = (n: number, from: number) =>
    Array.from({ length: n }, (_, i) => `$${i + from}`).join(",");

  // 씨앗과 그 자손 전부(완료 포함) — 어느 가지가 남는지 알아야 분할 여부를 판단한다
  const all = await db.select<Todo[]>(
    `WITH RECURSIVE sub(id) AS (
       SELECT id FROM todos WHERE id IN (${ph(ids.length, 1)})
       UNION SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )
     SELECT * FROM todos WHERE id IN (SELECT id FROM sub) ORDER BY sort_order, id`,
    ids,
  );
  if (!all.length) return;
  const byId = new Map(all.map((t) => [t.id, t]));
  const kids = (pid: number) => all.filter((t) => t.parent_id === pid);

  // 옮길 대상 = 씨앗 + 미완료 자식만 따라간 자손 (완료된 가지는 그 아래까지 통째로 남는다)
  const moving = new Set<number>();
  const walk = (id: number) => {
    if (moving.has(id)) return;
    moving.add(id);
    for (const c of kids(id)) if (c.done === 0) walk(c.id);
  };
  for (const id of ids) if (byId.has(id)) walk(id);

  // 분할 대상 = 옮기는데 남는 자식이 있는 노드
  const splitting = [...moving].filter((id) =>
    kids(id).some((c) => !moving.has(c.id)),
  );
  const copyOf = new Map<number, number>(); // 원본 id → 도착 날짜 사본 id

  /** 도착 날짜에서 이 노드를 가리키는 id — 분할했으면 사본, 아니면 원본(그대로 이동) */
  const targetId = (id: number) => copyOf.get(id) ?? id;
  /** 도착 날짜에서의 부모 — 부모가 함께 오지 않으면 최상위 */
  const targetParent = (t: Todo) =>
    t.parent_id != null && moving.has(t.parent_id) ? targetId(t.parent_id) : null;

  // 부모부터 만들어야 자식이 붙을 사본 id 가 존재한다 (얕은 순서로 정렬)
  const depthOf = (t: Todo): number => {
    let d = 0;
    for (let p = t.parent_id; p != null && byId.has(p); p = byId.get(p)!.parent_id)
      d++;
    return d;
  };
  for (const id of splitting.sort((a, b) => depthOf(byId.get(a)!) - depthOf(byId.get(b)!))) {
    const t = byId.get(id)!;
    const res = await db.execute(
      `INSERT INTO todos (content, due_date, parent_id, sort_order) VALUES ($1, $2, $3, $4)`,
      [t.content, dueDate, targetParent(t), t.sort_order],
    );
    copyOf.set(id, res.lastInsertId as number);
  }

  // 분할하지 않은 것만 실제로 이동 (분할한 원본은 제자리에 남는다)
  for (const id of moving) {
    if (copyOf.has(id)) continue;
    const t = byId.get(id)!;
    await db.execute(
      `UPDATE todos SET due_date = $1, updated_at = $2, parent_id = $3 WHERE id = $4`,
      [dueDate, now(), targetParent(t), id],
    );
  }

  // 남은 원본은 이제 완료분만 거느린다 → 그 날짜의 기록으로 닫는다
  for (const id of splitting) await recomputeChainFrom(id);
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
