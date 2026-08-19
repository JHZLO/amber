// 할 일(todos) DB 접근 계층. db.ts 패턴 그대로: getDb 싱글턴, $n 플레이스홀더,
// db.select/db.execute, lastInsertId, 멱등 UPDATE(WHERE done <> $1).
// 스키마·트리거는 Rust 마이그레이션(0002_todos.sql)이 정본. 완료 시각(completed_at)은 트리거가 관리.

import { getDb } from "./db";
import type { DayTodoCount, Todo, TodoScope } from "../types";

const now = () => Date.now();

/** 특정 날짜의 할 일 = 그 날짜가 마감인 행 + **그 날짜에서 이월해 나간 행(고스트)**.
 *  지워진 행(deleted_at)은 사는 날짜에서만 빠지고 거쳐온 날짜엔 고스트로 남는다 —
 *  오늘 지운 일이 어제의 기록을 소급해 고치면 안 되기 때문(migrations/0009).
 *  사용자가 정한 순서(sort_order) → 생성순 (idx_todos_sort 에 대응).
 *
 *  고스트는 사본이 아니라 같은 행이다(migrations/0008) — 가져오기로 오늘 옮겨도 어제 목록에서
 *  사라지지 않게 하되, done 은 끝까지 하나라 어디서 체크하든 같은 항목이 완료된다.
 *  `carried=1` 표식으로 UI 가 읽기전용 회색 행 + '→ 옮긴 날짜' 뱃지로 그린다.
 *
 *  이월은 항상 **오늘로만** 하므로(TodoView.moveToday) 고스트는 과거 날짜에만 나타난다 —
 *  오늘 목록에는 carried 행이 없다. 정렬에서 고스트를 뒤로 미는 이유도 그래서 과거 한정이다. */
export async function listTodos(date: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    // scope='day' 필수 — 주 항목의 due_date 는 실재하는 월요일이라,
    // 안 걸면 매주 월요일 목록에 '이번 주 할 일'이 통째로 섞인다(migrations/0012)
    `SELECT t.*, 0 AS carried FROM todos t
      WHERE t.due_date = $1 AND t.scope = 'day' AND t.deleted_at IS NULL
     UNION ALL
     SELECT t.*, 1 AS carried FROM todos t
       JOIN todo_carries c ON c.todo_id = t.id
      WHERE c.date = $2 AND t.due_date <> $3 AND t.scope = 'day'
     ORDER BY carried, sort_order, id`,
    [date, date, date],
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
       FROM todos
      WHERE due_date BETWEEN $1 AND $2 AND scope = 'day' AND deleted_at IS NULL
      GROUP BY due_date`,
    [from, to],
  );
}

/** 밀린 할 일: before(보통 오늘) 이전의 미완료. idx_todos_open 부분 인덱스 전용.
 *  형제 순서는 listTodos 와 같은 sort_order 기준 — 스트립도 계층으로 그리므로 어긋나면 안 된다. */
export async function listOverdueOpen(before: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    `SELECT * FROM todos
      WHERE done = 0 AND due_date < $1 AND scope = 'day' AND deleted_at IS NULL
      ORDER BY due_date, sort_order, id`,
    [before],
  );
}

/** 할 일 추가. parentId 를 주면 그 항목의 하위로. sort_order 는 형제 그룹 내 맨 끝 */
export async function createTodo(
  content: string,
  dueDate: string,
  parentId: number | null = null,
  scope: TodoScope = "day",
): Promise<number> {
  const db = await getDb();
  const res = await db.execute(
    // sort_order 는 (scope, due_date, parent) 그룹 안에서 매긴다 — 주 목록과 그 날 목록이
    // 같은 due_date 를 쓸 수 있으므로 scope 를 안 넣으면 순서가 서로 밀린다
    `INSERT INTO todos (content, due_date, parent_id, sort_order, scope)
     VALUES ($1, $2, $3,
       (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM todos
         WHERE due_date = $4 AND parent_id IS $5 AND scope = $6), $6)`,
    [content, dueDate, parentId, dueDate, parentId, scope],
  );
  return res.lastInsertId as number;
}

/** 주(월~일) 할 일 목록. due_date 는 그 주 월요일이고 이월·고스트 개념이 없다 —
 *  주는 '오늘'처럼 지나가는 좌표가 아니라 사용자가 직접 고르는 구간이다. */
export async function listWeekTodos(monday: string): Promise<Todo[]> {
  const db = await getDb();
  return db.select<Todo[]>(
    `SELECT *, 0 AS carried FROM todos
      WHERE due_date = $1 AND scope = 'week' AND deleted_at IS NULL
      ORDER BY sort_order, id`,
    [monday],
  );
}

/** 주 범위의 '주 할 일' 개수 — 캘린더 주 행 표식용 (일별 점과 섞지 않는다) */
export async function listWeekCounts(
  fromMonday: string,
  toMonday: string,
): Promise<DayTodoCount[]> {
  const db = await getDb();
  return db.select<DayTodoCount[]>(
    `SELECT due_date, COUNT(*) AS total, COALESCE(SUM(done), 0) AS done
       FROM todos
      WHERE due_date BETWEEN $1 AND $2 AND scope = 'week' AND deleted_at IS NULL
      GROUP BY due_date`,
    [fromMonday, toMonday],
  );
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
       -- UNION (ALL 아님): parent_id 순환이 생겨도 방문한 id 를 다시 넣지 않아 종료된다.
       -- flattenRows 는 이미 seen set 으로 같은 방어를 한다(todoTree.ts) — SQL 만 빠져 있었다.
       UNION
       SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )
     UPDATE todos SET done = $1
      WHERE done <> $1 AND deleted_at IS NULL AND id IN (SELECT id FROM sub)`,
    [done, id],
  );
}

/** 자식 상태로 부모 완료 재계산 — 자식이 있고 전부 완료면 부모도 완료, 아니면 미완료 */
export async function recomputeParentDone(parentId: number): Promise<void> {
  const db = await getDb();
  const rows = await db.select<{ open: number; total: number }[]>(
    `SELECT COALESCE(SUM(done = 0), 0) AS open, COUNT(*) AS total
       FROM todos WHERE parent_id = $1 AND deleted_at IS NULL`,
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
 *  **행을 복제하지 않는다** — 옮기는 건 due_date UPDATE 하나뿐이고, 떠나온 날짜는
 *  `todo_carries` 에 기록만 남긴다. 그 날짜를 열면 같은 행이 고스트로 다시 보이므로
 *  (listTodos 의 UNION) 어제 기록이 지워지지 않으면서, done 은 끝까지 하나다 — 어제에서
 *  체크하든 오늘에서 체크하든 같은 항목이 완료된다. 사본을 만들면 한 할 일에 done 이 둘
 *  생겨 상태가 어긋나고(한쪽만 체크) 매일 같은 내용이 밀린 목록에 한 줄씩 불어난다.
 *
 *  **미완료 가지만 따라간다** — 이미 체크한 자식은 끝낸 날짜의 기록이라 그대로 둔다.
 *  parent_id 는 건드리지 않는다: 뒤에 남은 완료 자식은 부모가 그 날짜에 고스트로 계속
 *  보이므로 헤딩을 잃지 않고("누아 항공권 › 1✓ 2✓ 3→ 4→"), 도착 날짜에서는 부모가 없는
 *  자식을 visibleRoots/flattenSubset 이 최상위로 올려 그린다. */
export async function moveTodos(ids: number[], dueDate: string): Promise<void> {
  if (!ids.length) return;
  const db = await getDb();
  const ph = (n: number, from: number) =>
    Array.from({ length: n }, (_, i) => `$${i + from}`).join(",");

  // 씨앗과 그 자손 전부(완료 포함) — 어느 가지를 따라갈지 판단하려면 완료 자식도 봐야 한다
  const all = await db.select<Todo[]>(
    `WITH RECURSIVE sub(id) AS (
       SELECT id FROM todos WHERE id IN (${ph(ids.length, 1)})
       UNION SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )
     SELECT * FROM todos
      WHERE id IN (SELECT id FROM sub) AND deleted_at IS NULL
      ORDER BY sort_order, id`,
    ids,
  );
  if (!all.length) return;
  const byId = new Map(all.map((t) => [t.id, t]));
  // parent_id 로 미리 묶는다 — 매번 filter 를 돌면 서브트리 크기에 O(n²) 이 된다
  const byParent = new Map<number, Todo[]>();
  for (const t of all) {
    if (t.parent_id == null) continue;
    const list = byParent.get(t.parent_id);
    if (list) list.push(t);
    else byParent.set(t.parent_id, [t]);
  }
  const kids = (pid: number) => byParent.get(pid) ?? [];

  // 옮길 대상 = 씨앗 + 미완료 자식만 따라간 자손 (완료된 가지는 그 아래까지 통째로 남는다)
  const moving = new Set<number>();
  const walk = (id: number) => {
    if (moving.has(id)) return;
    moving.add(id);
    for (const c of kids(id)) if (c.done === 0) walk(c.id);
  };
  for (const id of ids) if (byId.has(id)) walk(id);

  // 이미 그 날짜인 것은 옮길 것도 남길 흔적도 없다
  const targets = [...moving].filter((id) => byId.get(id)!.due_date !== dueDate);
  if (!targets.length) return;

  // 행마다 2회 왕복하던 것을 집합 기반 2문으로. "밀린 할 일 전부 오늘로"가 한 번에 수십 건이다.
  // 도착 날짜에는 기록을 남기지 않는다 — 남기면 그 날 목록에 실물+고스트로 두 번 나온다.
  await db.execute(
    `INSERT OR IGNORE INTO todo_carries (todo_id, date)
     SELECT id, due_date FROM todos WHERE id IN (${ph(targets.length, 1)})`,
    targets,
  );
  await db.execute(
    `UPDATE todos SET due_date = $1, updated_at = $2
      WHERE id IN (${ph(targets.length, 3)})`,
    [dueDate, now(), ...targets],
  );
}

/** 삭제. 노드를 지우면 그 서브트리 전체(모든 자손)를 함께 — 되돌릴 수 없으므로
 *  자손이 있을 때만 호출부가 확인 모달을 띄운다(홑 항목은 값싼 대상이라 즉시 삭제, DESIGN §8).
 *  삭제 후 부모 완료 상태 재계산은 호출부(recomputeChainFrom)가 담당.
 *
 *  **이월 이력이 있으면 행을 지우지 않고 deleted_at 을 찍는다**(migrations/0009). 이월은 행을
 *  복제하지 않으므로(0008) 행을 없애면 CASCADE 로 todo_carries 까지 날아가 *떠나온 날짜의
 *  기록*까지 사라진다 — 어제 목록은 어제 뭐가 있었는지의 기록이라 오늘 지운 일이 소급해
 *  고쳐선 안 된다. 이력이 없으면 남길 게 없으니 예전처럼 행째로 지운다(행이 쌓이지 않게). */
export async function deleteTodo(id: number): Promise<void> {
  const db = await getDb();
  const sub = `WITH RECURSIVE sub(id) AS (
       SELECT $1
       UNION -- ALL 아님: parent_id 순환에서 무한 재귀가 되지 않게 (setSubtreeDone 과 동일)
       SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
     )`;
  const carried = await db.select<{ n: number }[]>(
    `${sub}
     SELECT COUNT(*) AS n FROM todo_carries WHERE todo_id IN (SELECT id FROM sub)`,
    [id],
  );
  if ((carried[0]?.n ?? 0) > 0) {
    // 서브트리를 통째로 소프트 삭제 — 부분만 남기면 고스트가 부모(헤딩)를 잃는다
    await db.execute(
      `${sub}
       UPDATE todos SET deleted_at = $2, updated_at = $2
        WHERE deleted_at IS NULL AND id IN (SELECT id FROM sub)`,
      [id, now()],
    );
    // 소프트 삭제는 행을 남기므로 CASCADE 가 안 돈다 — 타임테이블에 유령 블록이 남는다.
    // 단 '떠나온 날짜'(todo_carries)의 블록은 그 날 뭘 했는지의 기록이라 보존한다(0009 의 취지).
    await db.execute(
      `${sub}
       DELETE FROM time_blocks
        WHERE todo_id IN (SELECT id FROM sub)
          AND date NOT IN (
            SELECT date FROM todo_carries WHERE todo_id IN (SELECT id FROM sub)
          )`,
      [id],
    );
    return;
  }
  await db.execute(`${sub} DELETE FROM todos WHERE id IN (SELECT id FROM sub)`, [
    id,
  ]);
}
