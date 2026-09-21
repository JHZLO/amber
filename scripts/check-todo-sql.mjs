#!/usr/bin/env node
// 할 일 SQL 스모크 체크 — 릴리스 게이트(release.mjs)가 태그 전에 돌린다.
//
// 지키는 것 하나: **달력 작업이 서랍 안까지 손을 뻗지 않는다.**
//
// `parkSubtree` 는 `parent_id` 를 지우지 않는다(서랍이 경로를 그리고 꺼낼 때 묶음을 되살리려면
// 필요하다). 그래서 내려놓은 항목은 달력 부모의 서브트리에 그대로 남아 있고, 재귀 CTE 를 쓰는
// 작업이 경계를 안 그으면 서랍 속 항목까지 건드린다. 실제로 v0.21~0.22 에서 묶음을 지우자
// 서랍에 치워 둔 두 건이 **말없이 함께 삭제됐다** — 확인 창은 그 날 목록만 세므로 개수도
// 틀리게 말했고, 지워졌다는 사실은 아무 데도 안 남았다.
//
// 왜 vitest 가 아닌가: SQL 을 실제로 돌려야 하는데 node:sqlite / node:fs 가 필요하고,
// 앱 타입 그래프에는 @types/node 를 일부러 넣지 않는다(브라우저 코드다). check-migrations.mjs 와
// 같은 이유로 릴리스 게이트에 둔다.
//
// SQL 이 두 벌이 되어 어긋나지 않도록, 경계 식은 todos.ts 에서 **읽어다** 쓴다.

import { DatabaseSync } from "node:sqlite";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const MIGRATIONS = join(HERE, "../src-tauri/migrations");
const TODOS = join(HERE, "../src/lib/todos.ts");

const fail = (msg) => {
  console.error(`할 일 SQL 검사 실패: ${msg}`);
  process.exit(1);
};

const src = readFileSync(TODOS, "utf8");

// 1) 경계 식이 아직 있는가 — 지우면 서랍이 다시 달력 작업에 노출된다
const m = src.match(/const parkedBoundary = \(seed: string\) =>\s*`([^`]+)`/);
if (!m) fail("todos.ts 에 parkedBoundary 가 없습니다 — 달력 작업이 서랍까지 내려갑니다.");
const boundary = (seed) => m[1].replace("${seed}", seed);

// 2) 서브트리를 타는 작업마다 parked 를 가리는 조건이 있는가.
//    재귀 단계에 경계를 걸든(parkedBoundary), 바깥 UPDATE 의 WHERE 에서 한쪽만 고르든
//    (parkSubtree/unparkSubtree 가 그렇다) 둘 중 하나는 있어야 한다. 그래서 문(statement)
//    끝까지 훑는다 — 재귀 단계 다음 여섯 줄이면 어느 쪽이든 들어온다.
for (const [, tail] of src.matchAll(
  /JOIN sub ON t\.parent_id = sub\.id((?:[^\n]*\n){6})/g,
)) {
  if (!/parkedBoundary|parked_at/.test(tail)) {
    fail(
      `경계 없는 서브트리 재귀가 있습니다 — "JOIN sub ON t.parent_id = sub.id" 뒤에\n` +
        `  WHERE \${parkedBoundary("$n")} (씨앗과 같은 쪽만) 또는 WHERE t.parked_at IS NULL 을 붙이세요.\n` +
        `  문제 구간: ${tail.trim().slice(0, 80)}`,
    );
  }
}

// 3) 실제로 돌려 본다 — 빈 DB 에 마이그레이션을 얹고 위 경계 식 그대로.
const db = new DatabaseSync(":memory:");
for (const f of readdirSync(MIGRATIONS).filter((f) => f.endsWith(".sql")).sort())
  db.exec(readFileSync(join(MIGRATIONS, f), "utf8"));

const sub = `WITH RECURSIVE sub(id) AS (
   SELECT $seed UNION
   SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id WHERE ${boundary("$seed")})`;
const check = (label, cond, got) => {
  if (!cond) fail(`${label} — 실제: ${JSON.stringify(got)}`);
  console.log(`  OK  ${label}`);
};

// 묶음을 지워도 서랍에 치워 둔 것은 살아남고, 없어진 부모에서 떨어져 나온다
db.exec(`INSERT INTO todos (id, content, due_date, parent_id, parked_at) VALUES
  (1,'묶음','2026-09-21',NULL,NULL),
  (2,'미뤄 둔 것 A','2026-09-21',1,1000),
  (3,'미뤄 둔 것 B','2026-09-21',1,1001),
  (4,'오늘 하위','2026-09-21',1,NULL)`);
db.prepare(`${sub} UPDATE todos SET parent_id = NULL
   WHERE parked_at IS NOT NULL AND parent_id IN (SELECT id FROM sub)`).run({ seed: 1 });
db.prepare(`${sub} DELETE FROM todos WHERE id IN (SELECT id FROM sub)`).run({ seed: 1 });
const left = db.prepare("SELECT id, parent_id FROM todos ORDER BY id").all();
check(
  "묶음 삭제: 내려놓은 것은 살아남고 묶음에서 떨어진다",
  JSON.stringify(left) === JSON.stringify([
    { id: 2, parent_id: null },
    { id: 3, parent_id: null },
  ]),
  left,
);

// 반대 방향: 서랍 카드를 지우면 그 안의 서브트리는 통째로 지워져야 한다(경계는 방향이 아니다)
db.exec(`INSERT INTO todos (id, content, due_date, parent_id, parked_at) VALUES
  (10,'카드','2026-09-21',NULL,2000),
  (11,'하위 A','2026-09-21',10,2001),
  (12,'하위 B','2026-09-21',10,2002)`);
db.prepare(`${sub} DELETE FROM todos WHERE id IN (SELECT id FROM sub)`).run({ seed: 10 });
const n = db.prepare("SELECT count(*) n FROM todos WHERE id IN (10,11,12)").get().n;
check("카드 삭제: 내려놓은 서브트리는 통째로 지워진다", n === 0, n);

// 묶음을 체크해도 미뤄 둔 것은 완료되지 않는다
db.exec(`INSERT INTO todos (id, content, due_date, parent_id, parked_at) VALUES
  (20,'묶음','2026-09-21',NULL,NULL),
  (21,'오늘 것','2026-09-21',20,NULL),
  (22,'미뤄 둔 것','2026-09-21',20,3000)`);
db.prepare(`${sub} UPDATE todos SET done = $done
   WHERE done <> $done AND id IN (SELECT id FROM sub)`).run({ done: 1, seed: 20 });
const d = db.prepare("SELECT id, done FROM todos WHERE id IN (20,21,22) ORDER BY id").all();
check(
  "묶음 체크: 미뤄 둔 것은 완료되지 않는다",
  JSON.stringify(d) === JSON.stringify([
    { id: 20, done: 1 },
    { id: 21, done: 1 },
    { id: 22, done: 0 },
  ]),
  d,
);
db.close();

console.log("할 일 SQL 경계 OK");
