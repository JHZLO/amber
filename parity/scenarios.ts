// 시나리오 — 손으로 쓴 것은 깨지기 쉬운 규칙을 하나씩 짚고(이월 고스트, 서랍 경계, 연동 블록…),
// 무작위 걸음은 사람이 생각 못 한 순서를 덮는다. 무작위라도 시드가 고정이라 다시 녹화해도 같다.
// 걸음은 화면이 실제로 부르는 순서를 흉내 낸다(체크 뒤 조상 재계산, 순환을 만드는 부모 이동은 하지 않음).

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parityUlid } from "./sqlite";
import type { Scenario, Step, StepCtx } from "./runner";

const T = "2026-11-05"; // 하네스 시계의 '오늘' (KST)
const Y = "2026-11-04";
const Y2 = "2026-11-03";
const N = "2026-11-06";
const N2 = "2026-11-08";
const WEEK = "2026-11-01"; // 일요일 시작 주(migrations/0013)
const PREV_WEEK = "2026-10-25";

const s = (op: string, ...args: unknown[]): Step => ({ op, args });
/** 앞 단계의 결과로 인자를 만든다 — 화면이 받은 값을 그대로 다시 넘기는 흐름 */
const after = (op: string, f: (last: unknown) => unknown[]): Step => ({
  op,
  args: (ctx: StepCtx) => f(ctx.last?.outcome.ok ? ctx.last.outcome.result : null),
});

const todosTreeBasics: Scenario = {
  name: "todos/tree-basics",
  steps: [
    s("todos.createTodo", "백엔드 배포", T),
    s("todos.createTodo", "API 테스트", T, 1),
    s("todos.createTodo", "로그 확인", T, 1),
    s("todos.createTodo", "회고 쓰기", T),
    s("todos.createTodo", "주간 목표", WEEK, null, "week"),
    s("todos.createTodo", "주간 하위", WEEK, 5, "week"),
    s("todos.createTodo", "같은 날짜의 일 항목", WEEK),
    s("todos.listTodos", T),
    s("todos.listTodos", WEEK),
    s("todos.listWeekTodos", WEEK),
    s("todos.listWeekCounts", PREV_WEEK, WEEK),
    s("todos.updateTodoContent", 4, "회고 쓰기 (금)"),
    s("todos.reorderTodos", [4, 1]),
    s("todos.reparentTodo", 4, 1),
    s("todos.reorderTodos", []),
    s("todos.createTodo", `100% 완료_테스트 'quote' "dq" 😀`, T),
    s("todos.listTodos", T),
    s("todos.listRangeRows", "2026-11-01", "2026-11-30"),
    s("todos.listOverdueOpen", N),
  ],
};

const todosDoneChain: Scenario = {
  name: "todos/done-chain",
  steps: [
    s("todos.createTodo", "묶음", T),
    s("todos.createTodo", "A", T, 1),
    s("todos.createTodo", "B", T, 1),
    s("todos.createTodo", "B-1", T, 3),
    s("todos.setSubtreeDone", 1, 1),
    s("todos.listTodos", T),
    s("todos.setSubtreeDone", 4, 0),
    s("todos.recomputeChainFrom", 3),
    s("todos.listTodos", T),
    s("todos.setSubtreeDone", 4, 1),
    s("todos.recomputeChainFrom", 3),
    // 잎에 부르면 자식이 없으니 미완료가 된다 — 화면은 부모에만 부르지만 동작은 이렇다
    s("todos.recomputeParentDone", 2),
    s("todos.setSubtreeDone", 1, 1),
    s("todos.setSubtreeDone", 1, 1),
    s("todos.listTodos", T),
  ],
};

const todosCarry: Scenario = {
  name: "todos/carry-ghosts",
  steps: [
    s("todos.createTodo", "항공권 정리", Y),
    s("todos.createTodo", "1", Y, 1),
    s("todos.createTodo", "2", Y, 1),
    s("todos.setSubtreeDone", 2, 1),
    s("todos.moveTodos", [1], T),
    s("todos.listTodos", Y),
    s("todos.listTodos", T),
    s("todos.listRangeRows", Y, T),
    s("todos.moveTodos", [1], T),
    s("todos.setSubtreeDone", 3, 1),
    s("todos.recomputeChainFrom", 1),
    s("todos.listTodos", Y),
    s("todos.updateTodoContent", 1, "항공권 정리 (이름 바꿈)"),
    s("todos.listTodos", Y),
    s("todos.moveTodos", [1], N),
    s("todos.listTodos", T),
    s("todos.deleteTodo", 3),
    s("todos.listTodos", Y),
    s("todos.removeCarry", 3, Y),
    s("todos.listTodos", Y),
    s("report.buildTodosDigest", Y),
    s("report.buildTodosDigest", T),
    s("todos.createTodo", "밀린 일", Y2),
    s("report.buildTodosDigest", T),
    s("todos.listOverdueOpen", T),
  ],
};

const todosParkBoundary: Scenario = {
  name: "todos/park-boundary",
  steps: [
    s("todos.createTodo", "묶음", T),
    s("todos.createTodo", "A", T, 1),
    s("todos.createTodo", "B", T, 1),
    s("todos.createTodo", "A-1", T, 2),
    s("todos.parkSubtree", 2),
    s("todos.recomputeChainFrom", 1),
    s("todos.listParked"),
    s("todos.listParkedAncestors"),
    // 묶음을 체크해도 서랍의 항목은 완료되지 않는다
    s("todos.setSubtreeDone", 1, 1),
    s("todos.listTodos", T),
    // 묶음을 지워도 서랍의 항목은 남고 부모에서만 떨어진다
    s("todos.deleteTodo", 1),
    s("todos.listParked"),
    s("todos.listParkedAncestors"),
    s("todos.unparkSubtree", 2, N),
    s("todos.listTodos", N),
    s("todos.parkSubtree", 4),
    s("todos.deleteTodo", 4),
    s("todos.listParked"),
  ],
};

const todosUnparkGroup: Scenario = {
  name: "todos/unpark-restores-group",
  steps: [
    s("todos.createTodo", "DEVOPS", Y),
    s("todos.createTodo", "kafka connect", Y, 1),
    s("todos.createTodo", "argo 정리", Y, 1),
    s("todos.parkSubtree", 2),
    s("todos.parkSubtree", 3),
    s("todos.listParkedAncestors"),
    s("todos.unparkSubtree", 2, T),
    after("todos.recomputeChainFrom", (p) => [p]),
    s("todos.unparkSubtree", 3, T),
    after("todos.recomputeChainFrom", (p) => [p]),
    s("todos.listTodos", T),
    s("todos.listTodos", Y),
    s("todos.parkSubtree", 4),
    s("todos.unparkSubtree", 4, Y),
    s("todos.listTodos", Y),
    s("todos.listParked"),
  ],
};

const todosDeleteLinks: Scenario = {
  name: "todos/delete-links",
  steps: [
    s("todos.createTodo", "A", Y),
    s("todos.createTodo", "B", Y),
    s("timeBlocks.createBlock", Y, 540, 600, "", 1),
    s("timeBlocks.createBlock", Y, 600, 660, "", 2),
    s("timeBlocks.createBlock", T, 600, 660, "", 2),
    s("todos.moveTodos", [2], T),
    s("todos.deleteTodo", 1),
    s("todos.deleteTodo", 2),
    s("timeBlocks.listBlocks", Y, T),
    s("todos.listTodos", Y),
  ],
};

const todosMissing: Scenario = {
  name: "todos/missing-and-empty",
  steps: [
    s("todos.deleteTodo", 999),
    s("todos.parkSubtree", 999),
    s("todos.unparkSubtree", 999, T),
    s("todos.recomputeChainFrom", 999),
    s("todos.recomputeParentDone", 999),
    s("todos.reparentTodo", 999, null),
    s("todos.moveTodos", [], T),
    s("todos.moveTodos", [999], T),
    s("todos.reorderTodos", [999]),
    s("todos.updateTodoContent", 999, "x"),
    s("todos.removeCarry", 999, T),
    s("todos.setSubtreeDone", 999, 1),
    s("todos.listTodos", "0000-00-00"),
    s("todos.listParked"),
    s("todos.listParkedAncestors"),
    s("report.buildTodosDigest", T),
  ],
};

// 화면은 순환을 못 만들지만 손상된 DB 에서도 서브트리 재귀는 끝나야 한다(UNION)
const todosCycle: Scenario = {
  name: "todos/cycle-safety",
  seed: [
    { sql: "INSERT INTO todos (id, content, due_date) VALUES (1, 'a', ?1), (2, 'b', ?1)", params: [T] },
    { sql: "UPDATE todos SET parent_id = 2 WHERE id = 1" },
    { sql: "UPDATE todos SET parent_id = 1 WHERE id = 2" },
  ],
  steps: [
    s("todos.setSubtreeDone", 1, 1),
    s("todos.listTodos", T),
    s("todos.moveTodos", [1], N),
    s("todos.deleteTodo", 1),
    s("todos.listTodos", T),
  ],
};

const concepts: Scenario = {
  name: "db/concepts",
  steps: [
    s("db.createConcept", {
      ulid: parityUlid(101),
      title: "Kafka 파티션",
      summary: "리더 편중",
      detailPath: `concepts/${parityUlid(101)}/detail.md`,
      tags: ["kafka", " infra ", "kafka", ""],
      confidence: 2,
    }),
    s("db.createConcept", {
      ulid: parityUlid(102),
      title: "b-tree",
      summary: "인덱스",
      detailPath: `concepts/${parityUlid(102)}/detail.md`,
      source: JSON.stringify({ noteRel: "Database/index.md", heading: "B-Tree" }),
      sourceKind: "file",
    }),
    s("db.createConcept", {
      ulid: parityUlid(103),
      title: "Async Rust",
      summary: "100% pin",
      detailPath: `concepts/${parityUlid(103)}/detail.md`,
      tags: ["rust"],
    }),
    s("db.listConcepts", { status: "all" }),
    s("db.listConcepts", { status: "learning", sort: "canonical" }),
    s("db.listConcepts", { status: "all", search: "kaf" }),
    s("db.listConcepts", { status: "all", search: "infra" }),
    s("db.listConcepts", { status: "all", search: "%" }),
    s("db.listConcepts", { status: "all", search: "   " }),
    s("db.listConcepts", { status: "all", tags: ["kafka", "infra"] }),
    s("db.listConcepts", { status: "all", tags: ["kafka", "rust"] }),
    s("db.listConcepts", { status: "all", sort: "title" }),
    s("db.listConcepts", { status: "all", sort: "recent_created", limit: 2 }),
    s("db.getConcept", 2),
    s("db.getConcept", 99),
    s("db.setStatus", 1, "learned"),
    s("db.setStatus", 1, "learned"),
    s("db.conceptsLearnedOn", Date.UTC(2026, 10, 4, 15), Date.UTC(2026, 10, 5, 15)),
    s("db.statusCounts"),
    s("db.adjustConfidence", 1, 5),
    s("db.adjustConfidence", 1, -9),
    s("db.adjustConfidence", 99, 1),
    s("db.markSeen", 3),
    s("db.markSeen", 3),
    s("db.listConcepts", { status: "learning", sort: "canonical" }),
    s("db.updateConceptContent", 3, { title: "Async Rust 정리" }),
    s("db.updateConceptContent", 3, {}),
    s("db.updateConceptContent", 3, { summary: "Pin, Unpin" }),
    s("db.setConceptTags", 3, ["rust", "async", "rust "]),
    s("db.allTags"),
    s("db.repointConceptSource", "Database/index.md", "DB/index.md", false),
    s("db.repointConceptSource", "DB", "Storage", true),
    s("db.getConcept", 2),
    s("db.deleteConcept", 1),
    s("db.allTags"),
    s("db.learningQueue"),
    s("db.setStatus", 3, "learned"),
    s("db.setStatus", 3, "learning"),
    s("db.getConcept", 3),
  ],
};

const settings: Scenario = {
  name: "db/settings-invocations",
  steps: [
    s("db.getSetting", "x"),
    s("db.setSetting", "x", "1"),
    s("db.setSetting", "x", "2"),
    s("db.getSetting", "x"),
    s("db.logInvocation", { model: "opus", status: "success" }),
    s("db.logInvocation", {
      conceptId: null,
      model: "gpt",
      sessionId: "s-1",
      inputTokens: 10,
      outputTokens: 20,
      costUsd: 0.0123,
      durationMs: 1500,
      status: "error",
      retryCount: 2,
    }),
  ],
};

const reports: Scenario = {
  name: "report/meta-and-config",
  steps: [
    s("report.getReport", T),
    s("report.upsertReport", {
      date: T,
      filePath: `reports/${T}.md`,
      sourcesJson: "[]",
      provider: "claude",
      model: null,
      durationMs: 1200,
    }),
    s("report.upsertReport", {
      date: T,
      filePath: `reports/${T}.md`,
      sourcesJson: '[{"id":"github"}]',
      provider: "claude",
      model: "opus",
      durationMs: 900,
    }),
    s("report.upsertReport", { date: Y, filePath: `reports/${Y}.md`, sourcesJson: "[]", provider: null, model: null, durationMs: null }),
    s("report.getReport", T),
    s("report.recentReportDates", T, 5),
    s("report.recentReportDates", "2026-12-01", 1),
    s("report.getWeeklyReport", WEEK),
    s("report.upsertWeeklyReport", {
      weekStart: WEEK,
      filePath: `reports/${WEEK}-week.md`,
      sourcesJson: "[]",
      provider: "codex",
      model: null,
      durationMs: 3000,
    }),
    s("report.upsertWeeklyReport", {
      weekStart: WEEK,
      filePath: `reports/${WEEK}-week.md`,
      sourcesJson: "[1]",
      provider: "codex",
      model: "gpt",
      durationMs: 2500,
    }),
    s("report.getWeeklyReport", WEEK),
    s("report.loadReportConfig"),
    after("report.saveReportConfig", (c) => [
      {
        ...(c as object),
        onboarded: true,
        githubRepos: ["a/b", " c/d "],
        githubAccount: " me ",
        sessionsCodex: false,
        mcpPicked: ["slack", " notion "],
        displayName: " 홍길동 ",
        context: "  백엔드  ",
      },
    ]),
    s("report.loadReportConfig"),
    s("db.setSetting", "report_sources", "not json"),
    s("report.loadReportConfig"),
    s("report.loadMcpCache"),
    s("report.saveMcpCache", []),
    s("report.saveMcpCache", [
      { name: "slack", connected: true, status: "ok", transport: "http" },
      { bad: 1 },
    ]),
    s("report.loadMcpCache"),
    s("db.setSetting", "report_mcp_cache", "{"),
    s("report.loadMcpCache"),
    s("db.setSetting", "report_mcp_servers", ""),
    s("db.setSetting", "report_slack_server", "slack-legacy"),
    s("report.loadReportConfig"),
  ],
};

const blocksAndVacations: Scenario = {
  name: "timeBlocks/blocks-and-vacations",
  seed: [{ sql: "INSERT INTO vacations (date, kind) VALUES ('2026-11-03', 'weird')" }],
  steps: [
    s("timeBlocks.createBlock", T, 540, 600, "집중"),
    s("timeBlocks.createBlock", T, 600, 630),
    s("timeBlocks.createBlock", N, 0, 1440, "종일"),
    s("timeBlocks.listBlocks", T, T),
    s("timeBlocks.updateBlockTime", 1, N, 60, 120),
    s("timeBlocks.renameBlock", 2, "회의"),
    s("timeBlocks.deleteBlock", 3),
    s("timeBlocks.deleteBlock", 99),
    s("timeBlocks.listBlocks", T, "2026-11-30"),
    s("vacations.listVacations", "2026-11-01", "2026-11-30"),
    s("vacations.getVacation", "2026-11-03"),
    s("vacations.getVacation", T),
    s("vacations.setVacation", T, "half"),
    s("vacations.setVacation", T, "annual"),
    s("vacations.listVacations", "2026-11-01", "2026-11-30"),
    s("vacations.removeVacation", T),
    s("vacations.removeVacation", T),
    s("vacations.getVacation", T),
  ],
};

const connInput = (name: string, extra: object = {}) => ({
  name,
  kind: "mysql",
  env: "prod",
  host: " 127.0.0.1 ",
  port: 13307,
  username: " ro ",
  tls: "preferred",
  credential_command: "  ~/bin/db-credential {schema} ",
  folder_path: name.trim(),
  schemas: [
    { name: "svc_a", label: "", enabled: true },
    { name: "svc_b", label: "B", enabled: false, audit: false },
  ],
  ...extra,
});

const dbconn: Scenario = {
  name: "dbconn/profiles",
  seed: [
    {
      sql: `INSERT INTO db_connections (ulid, name, kind, env, host, port, username, tls, folder_path, schemas_json)
            VALUES ('01SEEDBROKENJSON000000000A', 'broken', 'mysql', 'dev', 'h', 3306, 'u', 'disabled', 'broken', 'not json'),
                   ('01SEEDODDSCHEMAS000000000B', 'odd', 'postgres', 'staging', 'h', 5432, 'u', 'required', 'odd',
                    '[{"name":1},{"name":"x","enabled":false,"starred":true,"audit":true,"label":null}]')`,
    },
  ],
  steps: [
    s("dbconn.listConnections"),
    s("dbconn.createConnection", connInput("  MySQL ")),
    s("dbconn.createConnection", connInput("dev", { env: "dev", credential_command: "" })),
    s("dbconn.listConnections"),
    s("dbconn.getConnection", 3),
    s("dbconn.getConnection", 99),
    s("dbconn.updateConnection", 3, {}),
    s("dbconn.updateConnection", 3, { name: " MySQL2 ", port: 3306, schemas: [] }),
    s("dbconn.markSync", 3, "DB_CREDENTIAL"),
    s("dbconn.markSync", 3, null),
    s("dbconn.getConnection", 4),
    after("dbconn.setSchemaStarred", (c) => [c, "svc_a", true]),
    s("dbconn.getConnection", 4),
    after("dbconn.setSchemaStarred", (c) => [c, "svc_a", false]),
    s("dbconn.listConnections"),
  ],
};

// ---- 무작위 걸음 ----

const WORDS = [
  "배포",
  "API 테스트",
  "로그 확인",
  "회고",
  "DEVOPS",
  "kafka connect",
  "100%",
  "a_b",
  "'따옴표'",
  '"쌍따옴표"',
  "😀 이모지",
  "  앞뒤 공백  ",
  "",
  "줄\n바꿈",
  "Mixed Case",
  "mixed case",
];
const TAGS = ["kafka", "rust", " infra ", "db", "DB", "spring", ""];
const NOTES = ["Database/index.md", "Database/mvcc.md", "Rust/async.md", "Rust", "Database"];

const pick = <T>(rng: () => number, xs: T[]): T => xs[Math.floor(rng() * xs.length)];
const some = <T>(rng: () => number, xs: T[], max: number): T[] =>
  xs.filter(() => rng() < max / Math.max(xs.length, 1)).slice(0, max);
function shuffle<T>(rng: () => number, xs: T[]): T[] {
  const a = [...xs];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

interface TodoRow {
  id: number;
  due_date: string;
  scope: string;
  done: number;
  parent_id: number | null;
  parked_at: number | null;
}

/** 할 일 탭 전체를 걷는다 — 만들기, 체크, 이월, 내려놓기와 꺼내기, 지우기, 부모 이동, 순서, 연동 블록 */
export function todoWalk(seed: number, length: number, extraDates: string[] = []): Scenario["walk"] {
  return {
    seed,
    length,
    next: () => {
      let pending: ((ctx: StepCtx) => Step[]) | null = null;
      return (ctx) => {
        if (pending) {
          const p = pending;
          pending = null;
          return p(ctx);
        }
        const { rng } = ctx;
        const dates = [Y2, Y, T, N, N2, ...extraDates];
        const all = ctx.peek<TodoRow>("SELECT id, due_date, scope, done, parent_id, parked_at FROM todos ORDER BY id");
        const onCal = all.filter((t) => t.parked_at == null);
        const parked = all.filter((t) => t.parked_at != null);
        const day = onCal.filter((t) => t.scope === "day");
        const r = rng();

        if (r < 0.22 || !all.length) {
          const week = rng() < 0.12;
          const date = week ? pick(rng, [WEEK, PREV_WEEK]) : pick(rng, dates);
          const scope = week ? "week" : "day";
          const cands = onCal.filter((t) => t.due_date === date && t.scope === scope);
          const parent = cands.length && rng() < 0.45 ? pick(rng, cands).id : null;
          return [s("todos.createTodo", pick(rng, WORDS), date, parent, scope)];
        }
        if (r < 0.36 && onCal.length) {
          const t = pick(rng, onCal);
          return [s("todos.setSubtreeDone", t.id, t.done ? 0 : 1), s("todos.recomputeChainFrom", t.parent_id)];
        }
        if (r < 0.44 && day.length) {
          return [s("todos.moveTodos", some(rng, day, 3).map((t) => t.id), pick(rng, dates))];
        }
        if (r < 0.5 && onCal.length) {
          const t = pick(rng, onCal);
          return [s("todos.parkSubtree", t.id), s("todos.recomputeChainFrom", t.parent_id)];
        }
        if (r < 0.56 && parked.length) {
          pending = (c) => [s("todos.recomputeChainFrom", c.last?.outcome.ok ? c.last.outcome.result : null)];
          return [s("todos.unparkSubtree", pick(rng, parked).id, pick(rng, dates))];
        }
        if (r < 0.62) {
          const t = pick(rng, all);
          return [s("todos.deleteTodo", t.id), s("todos.recomputeChainFrom", t.parent_id)];
        }
        if (r < 0.67 && day.length) {
          const t = pick(rng, day);
          const sub = new Set(
            ctx
              .peek<{ id: number }>(
                `WITH RECURSIVE sub(id) AS (SELECT ?1 UNION SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id)
                 SELECT id FROM sub`,
                [t.id],
              )
              .map((x) => x.id),
          );
          const cands = day.filter((x) => x.due_date === t.due_date && !sub.has(x.id));
          const p = rng() < 0.3 || !cands.length ? null : pick(rng, cands).id;
          return [
            s("todos.reparentTodo", t.id, p),
            s("todos.recomputeChainFrom", t.parent_id),
            s("todos.recomputeChainFrom", p),
          ];
        }
        if (r < 0.72 && onCal.length) {
          const t = pick(rng, onCal);
          const sibs = onCal.filter((x) => x.due_date === t.due_date && x.scope === t.scope && x.parent_id === t.parent_id);
          return [s("todos.reorderTodos", shuffle(rng, sibs.map((x) => x.id)))];
        }
        if (r < 0.77) return [s("todos.updateTodoContent", pick(rng, all).id, pick(rng, WORDS))];
        if (r < 0.8) {
          const carries = ctx.peek<{ todo_id: number; date: string }>("SELECT todo_id, date FROM todo_carries");
          if (carries.length) {
            const c = pick(rng, carries);
            return [s("todos.removeCarry", c.todo_id, c.date)];
          }
        }
        if (r < 0.85) {
          const blocks = ctx.peek<{ id: number }>("SELECT id FROM time_blocks");
          const b = rng();
          const start = 30 * Math.floor(rng() * 40);
          const end = start + 30 * (1 + Math.floor(rng() * 4));
          if (b < 0.45 && day.length) {
            const t = pick(rng, day);
            return [s("timeBlocks.createBlock", t.due_date, start, end, "", t.id)];
          }
          if (b < 0.65 || !blocks.length) return [s("timeBlocks.createBlock", pick(rng, dates), start, end, pick(rng, WORDS))];
          if (b < 0.8) return [s("timeBlocks.updateBlockTime", pick(rng, blocks).id, pick(rng, dates), start, end)];
          if (b < 0.9) return [s("timeBlocks.renameBlock", pick(rng, blocks).id, pick(rng, WORDS))];
          return [s("timeBlocks.deleteBlock", pick(rng, blocks).id)];
        }
        if (r < 0.98) {
          return [
            pick(rng, [
              s("todos.listTodos", pick(rng, dates)),
              s("todos.listTodos", pick(rng, dates)),
              s("todos.listRangeRows", Y2, N2),
              s("todos.listOverdueOpen", T),
              s("todos.listParked"),
              s("todos.listParkedAncestors"),
              s("todos.listWeekTodos", pick(rng, [WEEK, PREV_WEEK])),
              s("todos.listWeekCounts", PREV_WEEK, WEEK),
              s("report.buildTodosDigest", pick(rng, dates)),
              s("timeBlocks.listBlocks", Y2, N2),
            ]),
          ];
        }
        return [
          pick(rng, [
            s("todos.deleteTodo", 9999),
            s("todos.setSubtreeDone", 9999, 1),
            s("todos.unparkSubtree", 9999, T),
            s("todos.recomputeParentDone", all.length ? pick(rng, all).id : 9999),
          ]),
        ];
      };
    },
  };
}

interface ConceptRow {
  id: number;
}

/** 개념 카드와 태그, 설정을 걷는다 */
export function conceptWalk(seed: number, length: number): Scenario["walk"] {
  return {
    seed,
    length,
    next: () => (ctx) => {
      const { rng } = ctx;
      const ids = ctx.peek<ConceptRow>("SELECT id FROM concepts ORDER BY id").map((c) => c.id);
      const id = () => (ids.length && rng() < 0.92 ? pick(rng, ids) : 9999);
      const r = rng();
      if (r < 0.2 || !ids.length) {
        const ulid = parityUlid(seed * 1000 + ctx.index);
        const file = rng() < 0.4;
        return [
          s("db.createConcept", {
            ulid,
            title: pick(rng, WORDS),
            summary: pick(rng, WORDS),
            detailPath: `concepts/${ulid}/detail.md`,
            ...(rng() < 0.6 ? { tags: some(rng, TAGS, 3) } : {}),
            ...(rng() < 0.5 ? { confidence: pick(rng, [1, 2, 3]) } : {}),
            ...(file
              ? { source: JSON.stringify({ noteRel: pick(rng, NOTES.filter((n) => n.endsWith(".md"))) }), sourceKind: "file" }
              : rng() < 0.5
                ? { source: "붙여 넣은 원문", sourceKind: "paste" }
                : {}),
          }),
        ];
      }
      if (r < 0.32) return [s("db.setStatus", id(), pick(rng, ["learned", "learning"]))];
      if (r < 0.42) return [s("db.adjustConfidence", id(), pick(rng, [-2, -1, 1, 2]))];
      if (r < 0.5) return [s("db.markSeen", id())];
      if (r < 0.58) return [s("db.setConceptTags", id(), some(rng, TAGS, 4))];
      if (r < 0.66) {
        const f = rng();
        return [
          s(
            "db.updateConceptContent",
            id(),
            f < 0.4 ? { title: pick(rng, WORDS) } : f < 0.8 ? { summary: pick(rng, WORDS) } : {},
          ),
        ];
      }
      if (r < 0.7) return [s("db.deleteConcept", id())];
      if (r < 0.75) {
        const dir = rng() < 0.4;
        const from = pick(rng, NOTES.filter((n) => (dir ? !n.endsWith(".md") : n.endsWith(".md"))));
        return [s("db.repointConceptSource", from, dir ? "Moved" : "Moved/x.md", dir)];
      }
      if (r < 0.95) {
        return [
          pick(rng, [
            s("db.listConcepts", {
              status: pick(rng, ["all", "learning", "learned"]),
              sort: pick(rng, ["canonical", "recent_updated", "recent_created", "title"]),
              ...(rng() < 0.4 ? { search: pick(rng, ["a", "rust", "%", "_", "DB", "로그"]) } : {}),
              ...(rng() < 0.3 ? { tags: some(rng, TAGS, 2) } : {}),
              ...(rng() < 0.2 ? { limit: 2 } : {}),
            }),
            s("db.getConcept", id()),
            s("db.allTags"),
            s("db.statusCounts"),
            s("db.learningQueue"),
            s("db.conceptsLearnedOn", DEFAULT_DAY_START, DEFAULT_DAY_START + 86_400_000),
          ]),
        ];
      }
      const key = pick(rng, ["a", "b", "report_context"]);
      return [rng() < 0.5 ? s("db.setSetting", key, pick(rng, WORDS)) : s("db.getSetting", key)];
    },
  };
}

/** 2026-11-05 00:00 KST */
const DEFAULT_DAY_START = Date.UTC(2026, 10, 4, 15, 0, 0);

// ---- vault 파일 ----
// 정렬(ICU 한국어: 한글, 한자가 라틴보다 앞), 대소문자만 바꾸는 이름 변경(macOS 는 대소문자를 안 가린다),
// 이동할 때 따라가는 사이드카(질문, 개념 링크)와 그림, 휴지통에 들어간 내용까지 상태로 잰다.

const LONG_LINE = `${"앞부분 ".repeat(25)}needle 이 가운데 있다 ${"뒷부분 ".repeat(25)}`;

const notesFiles: Scenario = {
  name: "notes/files",
  seed: [
    { path: "vault/notes/CS/네트워크/TCP.md", text: "# TCP\n\n악수는 세 번 한다.\n\n![그림](_assets/a.png)\n![없는 그림](./_assets/none.png)\n![밖](../x.png)\n" },
    { path: "vault/notes/CS/네트워크/_assets/a.png", text: "PNG-a" },
    { path: "vault/notes/CS/네트워크/TCP.comments.json", text: '{"version":1,"threads":[]}' },
    { path: "vault/notes/CS/네트워크/TCP.concepts.json", text: '{"links":[]}' },
    { path: "vault/notes/CS수업.md", text: "# CS수업\n" },
    { path: "vault/notes/apple.md", text: "사과. TCP 를 언급한다\n" },
    { path: "vault/notes/Banana.md", text: "" },
    { path: "vault/notes/10.md", text: "" },
    { path: "vault/notes/2.md", text: "" },
    { path: "vault/notes/한글.md", text: "" },
    { path: "vault/notes/漢字.md", text: "" },
    { path: "vault/notes/éclair.md", text: "" },
    { path: "vault/notes/eclair2.md", text: "" },
    { path: "vault/notes/_x.md", text: "" },
    { path: "vault/notes/😀.md", text: "" },
    { path: "vault/notes/a b.md", text: "" },
    { path: "vault/notes/Upper.MD", text: "대문자 확장자" },
    { path: "vault/notes/긴줄.md", text: `${LONG_LINE}\n` },
    { path: "vault/notes/x.txt", text: "트리에 안 보이는 파일" },
    { path: "vault/notes/.DS_Store", text: "" },
    { path: "vault/notes/.숨김/안.md", text: "" },
    { path: "vault/notes/빈 폴더/" },
    {
      sql: `INSERT INTO concepts (ulid, title, summary, detail_path, source, source_kind) VALUES
            ('01SEEDNOTECONCEPT00000000A', 'TCP', 's', 'concepts/a/index.md', '{"noteRel":"CS/네트워크/TCP.md"}', 'file'),
            ('01SEEDNOTECONCEPT00000000B', '다른', 's', 'concepts/b/index.md', '{"noteRel":"CS/네트워크/다른.md"}', 'file'),
            ('01SEEDNOTECONCEPT00000000C', '깨짐', 's', 'concepts/c/index.md', 'not json', 'file')`,
    },
  ],
  steps: [
    s("notes.listNoteTree"),
    s("notes.createNote", "", "새 노트"),
    s("notes.createNote", "", "새 노트"),
    s("notes.createNote", "CS", " 깊은 / 경로 / 노트 "),
    s("notes.createNote", "없는폴더", "안"),
    s("notes.createFolder", "", "Ops/k8s"),
    s("notes.createFolder", "", "Ops"),
    s("notes.writeNoteFile", "새 노트.md", "본문 한 줄\n"),
    s("notes.readNoteFile", "새 노트.md"),
    s("notes.readNoteFile", "없는.md"),
    s("notes.renameEntry", "CS/네트워크/TCP.md", "tcp", false),
    s("notes.renameEntry", "apple.md", "Banana", false),
    s("notes.renameEntry", "apple.md", "apple", false),
    s("notes.moveEntry", "CS/네트워크/tcp.md", "Ops"),
    s("notes.moveEntry", "CS", "CS/네트워크"),
    s("notes.moveEntry", "Ops", ""),
    s("notes.moveEntry", "Banana.md", " Ops / k8s / "),
    s("notes.moveEntry", "Banana.md", "Ops"),
    s("notes.renameEntry", "Ops", "Infra", true),
    s("notes.renameEntry", "CS", "CS수업", true),
    s("notes.deleteEntry", "Infra/tcp.md"),
    s("notes.deleteEntry", "  "),
    s("notes.deleteEntry", "없는.md"),
    s("notes.deleteEntry", "빈 폴더"),
    s("notes.listNoteTree"),
    s("vaultTree.searchFiles", "notes", "tcp", 20),
    s("vaultTree.searchFiles", "notes", "NEEDLE", 20),
    s("vaultTree.searchFiles", "notes", "노트", 1),
    s("vaultTree.searchFiles", "notes", "  ", 20),
    s("vaultTree.writeAtomic", "vault/notes/직접.md", "원자적으로 쓴다"),
    s("vaultTree.writeAtomic", "vault/notes/없는폴더2/x.md", "부모가 없다"),
    s("notes.listNoteTree"),
  ],
};

const diagramsFiles: Scenario = {
  name: "diagrams/files",
  seed: [
    { path: "vault/diagrams/주문.mmd", text: "erDiagram\n    USER ||--o{ ORDER : places\n" },
    { path: "vault/diagrams/flow.mermaid", text: "flowchart LR\n  a --> b\n" },
    { path: "vault/diagrams/note.md", text: "트리에 안 보인다" },
  ],
  steps: [
    s("diagrams.listDiagramTree"),
    s("diagrams.createDiagram", "", "결제"),
    s("diagrams.createFolder", "", "도메인"),
    s("diagrams.renameEntry", "flow.mermaid", "Flow", false),
    s("diagrams.moveEntry", "Flow.mmd", "도메인"),
    s("diagrams.moveEntry", "flow.mermaid", "도메인"),
    s("diagrams.writeDiagramFile", "결제.mmd", "erDiagram\n    PAY {\n        bigint id PK\n    }\n"),
    s("diagrams.readDiagramFile", "결제.mmd"),
    s("diagrams.deleteEntry", "주문.mmd"),
    s("vaultTree.searchFiles", "diagrams", "pay", 20),
    s("diagrams.listDiagramTree"),
  ],
};

const conceptNotes: Scenario = {
  name: "vault/concept-notes",
  seed: [
    { path: "vault/concepts/OLD/index.md", text: "---\nid: OLD\n---\n\n\n본문만\n" },
    { path: "vault/concepts/RAW/index.md", text: "frontmatter 없음" },
    { path: "vault/concepts/OPEN/index.md", text: "---\nid: OPEN\n끝이 없다" },
    { path: "vault/concepts/ENDONLY/index.md", text: "---\nid: E\n---" },
  ],
  steps: [
    s("vault.writeNote", "01NEW", "  \n\n새 개념 본문\n"),
    s("vault.readNote", "concepts/01NEW/index.md"),
    s("vault.readNote", "concepts/OLD/index.md"),
    s("vault.readNote", "concepts/RAW/index.md"),
    s("vault.readNote", "concepts/OPEN/index.md"),
    s("vault.readNote", "concepts/ENDONLY/index.md"),
    s("vault.readNote", "concepts/NONE/index.md"),
    s("vault.noteExists", "concepts/01NEW/index.md"),
    s("vault.noteExists", "concepts/NONE/index.md"),
    s("vault.writeNote", "01NEW", "덮어쓴다"),
    s("vault.deleteConceptDir", "01NEW"),
    s("vault.deleteConceptDir", "NONE"),
  ],
};

const erdFiles: Scenario = {
  name: "erd/files",
  steps: [
    s("erdStore.ensureErdDir", ""),
    s("erdStore.ensureErdDir", " conn / svc_a "),
    s("erdStore.ensureErdDir", "conn/svc_a"),
    s("erdStore.writeErdFile", "conn/svc_a/schema.json", '{"tables":[]}\n'),
    s("erdStore.erdFileExists", "conn/svc_a/schema.json"),
    s("erdStore.erdFileExists", "conn/svc_b/schema.json"),
    s("erdStore.readErdFileIfExists", "conn/svc_a/schema.json"),
    s("erdStore.readErdFileIfExists", "conn/svc_b/schema.json"),
    s("erdStore.deleteErdEntry", "conn"),
    s("erdStore.deleteErdEntry", "conn"),
  ],
};

/** 다이어그램 탭 시절 ERD 옮기기 — 스냅샷 복사(이미 있으면 그대로), 남은 자동 생성물 찾기(자동 생성물뿐이면 폴더째,
 *  손으로 둔 파일이 섞이면 파일 하나씩), 휴지통으로. 점 파일은 세지 않는다 */
const erdMigrateFiles: Scenario = {
  name: "erd/migrate",
  seed: (() => {
    const snap = (schema: string) => JSON.stringify({ amber: 1, connection: "c", schema, server: "MySQL 8", synced_at: 1, fingerprint: "f", tables: [] }, null, 2) + "\n";
    const gen = (who: string) => `%% amber:db ${who} 2026-01-01\nerDiagram\n  A ||--o{ B : has\n`;
    const D = "vault/diagrams";
    return [
      { path: `${D}/Shop DB/svc_a/schema.json`, text: snap("svc_a") },
      { path: `${D}/Shop DB/svc_a/ERD.mmd`, text: gen("Shop DB/svc_a") },
      { path: `${D}/Shop DB/.DS_Store`, text: "x" },
      { path: `${D}/Shop DB/svc_b/schema.json`, text: "{}" },
      { path: `${D}/Mixed/svc_c/schema.json`, text: snap("svc_c") },
      { path: `${D}/Mixed/svc_c/ERD.MERMAID`, text: "\n\n  %% amber:db Mixed/svc_c x\nerDiagram\n" },
      { path: `${D}/Mixed/svc_c/hand.mmd`, text: "erDiagram\n  A ||--o{ B : x\n" },
      { path: `${D}/Mixed/notes.png`, text: "PNG" },
      { path: `${D}/Clean/svc_d/schema.json`, text: snap("svc_d") },
      { path: `${D}/Clean/svc_d/ERD.mmd`, text: gen("Clean/svc_d") },
      { path: "vault/erd/Clean/svc_d/schema.json", text: snap("already") },
      { path: `${D}/Off/svc_e/schema.json`, text: snap("svc_e") },
    ];
  })(),
  plan() {
    const conn = (id: number, folder: string, schemas: [string, boolean][]) => ({
      id, ulid: `01J${id}`, name: folder, kind: "mysql", env: "dev", host: "h", port: 3306, username: "u", tls: "preferred",
      credential_command: "", folder_path: folder, schemas: schemas.map(([name, enabled]) => ({ name, label: "", enabled })),
      last_sync_at: null, last_error: null, created_at: 0, updated_at: 0,
    });
    const list = [
      conn(1, "Shop DB", [["svc_a", true], ["svc_b", true]]),
      conn(2, "Mixed", [["svc_c", true]]),
      conn(3, "Clean", [["svc_d", true]]),
      conn(4, "Off", [["svc_e", false]]),
      conn(5, "Missing", [["svc_x", true]]),
    ];
    return [
      ...["%% amber:db a/b x", "  %% amber:db a b/c x", "%% amber:db a/b", "%% amber:db /b x", "x\ny\nz\nw\n%% amber:db a/b x", "x\ny\nz\nw\nv\n%% amber:db a/b x", "%%amber:db a/b x", "\t%% amber:db a/b/c x"]
        .map((t) => s("erdMigrate.isGeneratedErd", t)),
      s("erdMigrate.copyLegacySnapshots", list),
      s("erdMigrate.findLegacyErds", list),
      s("erdMigrate.trashLegacyErds", { folders: ["Shop DB", "Clean"], files: ["Mixed/svc_c/schema.json", "Mixed/svc_c/ERD.MERMAID"], erds: 3 }),
      s("erdMigrate.findLegacyErds", list),
    ];
  },
};

/** 날짜 계산 훑기 — 월 경계, 윤년, 연 경계, 주 시작, 달력 격자, 공휴일(대체공휴일, 음력 표 밖 연도) */
const dates: Scenario = {
  name: "dates/sweep",
  plan() {
    const steps: Step[] = [s("date.todayStr"), s("date.monthsShort"), s("date.weekdaysShort"), s("date.weekdaysShort", 1)];
    for (let y = 2023; y <= 2029; y++)
      for (let m = 1; m <= 12; m++) {
        steps.push(s("date.monthGridDates", y, m), s("date.formatMonthTitle", y, m));
        if (m === 1) steps.push(s("date.monthGridDates", y, m, 1), s("date.formatYearTitle", y));
      }
    const edges = ["2024-02-28", "2024-02-29", "2025-02-28", "2026-01-01", "2026-12-31", "2027-03-31", "2028-02-29", "2100-02-28"];
    for (const d of edges)
      for (const delta of [-400, -31, -7, -1, 0, 1, 7, 31, 365])
        steps.push(s("date.shiftDay", d, delta));
    for (const [y, m] of [[2026, 1], [2026, 11], [2026, 12], [2024, 2], [2030, 6]])
      for (const delta of [-25, -13, -12, -1, 0, 1, 11, 12, 13])
        steps.push(s("date.addMonths", { year: y, month: m }, delta));
    for (let i = 0; i < 400; i++) {
      const d = new Date(Date.UTC(2025, 11, 1 + i)).toISOString().slice(0, 10);
      steps.push(s("date.weekStartOf", d), s("date.formatDayLong", d), s("date.weekdayShort", d), s("date.formatDayShort", d));
      if (i % 7 === 0) steps.push(s("date.weekDays", d), s("date.monthOf", d), s("date.dayRangeMs", d));
    }
    for (let y = 2015; y <= 2055; y++) steps.push(s("holidays.holidaysOfYear", y), s("holidays.hasLunarData", y));
    for (const d of ["2026-01-01", "2026-02-17", "2026-02-18", "2026-03-02", "2026-05-25", "2026-10-05", "2026-11-05", "bad", "1999-12-25"])
      steps.push(s("holidays.holidayOf", d));
    return steps;
  },
};

/** 할 일 트리 계산 — 무작위 숲(없는 부모, 순환, 지워진 기록, 체크 섞임)마다 함수 전부를 부른다 */
const todoTrees: Scenario = {
  name: "todoTree/forests",
  plan(ctx) {
    const steps: Step[] = [
      s("todoTree.clampDropDepth", 3, { id: 1, parent_id: null, depth: 0 }, null),
      s("todoTree.clampDropDepth", 0, { id: 1, parent_id: null, depth: 0 }, { id: 2, parent_id: 1, depth: 1 }),
      s("todoTree.clampDropDepth", 2, null, null),
      s("todoTree.dayProgress", []),
      s("todoTree.flattenTree", []),
    ];
    for (let f = 0; f < 40; f++) {
      const n = 2 + Math.floor(ctx.rng() * 14);
      const nodes = Array.from({ length: n }, (_, i) => {
        const id = i + 1 + (f % 3) * 100;
        const r = ctx.rng();
        const parent_id =
          r < 0.35 ? null : r < 0.85 && i > 0 ? id - 1 - Math.floor(ctx.rng() * i) : r < 0.93 ? 999 : id + 1;
        return { id, parent_id, content: `n${id}`, done: ctx.rng() < 0.4 ? 1 : 0, gone: ctx.rng() < 0.1 ? 1 : 0 };
      });
      const ids = nodes.map((x) => x.id);
      const pickId = () => ids[Math.floor(ctx.rng() * ids.length)];
      const rows = (() => {
        // 화면이 쓰는 그대로 — 루트에서 펼친 순서에 깊이
        const out: { id: number; parent_id: number | null; depth: number }[] = [];
        const walk = (p: number | null, d: number) => {
          for (const x of nodes.filter((y) => (y.parent_id ?? null) === p)) {
            out.push({ id: x.id, parent_id: x.parent_id, depth: d });
            walk(x.id, d + 1);
          }
        };
        walk(null, 0);
        return out;
      })();
      const dragged = pickId();
      const sub = new Set<number>([dragged]);
      for (let grew = true; grew; ) {
        grew = false;
        for (const x of nodes) if (x.parent_id != null && sub.has(x.parent_id) && !sub.has(x.id)) (sub.add(x.id), (grew = true));
      }
      const candidates = rows.filter((r) => !sub.has(r.id));
      steps.push(
        s("todoTree.childrenOf", nodes, null),
        s("todoTree.childrenOf", nodes, pickId()),
        s("todoTree.ancestorPath", nodes, pickId()),
        s("todoTree.flattenTree", nodes),
        s("todoTree.visibleRoots", nodes),
        s("todoTree.flattenSubset", nodes),
        s("todoTree.subtreeIds", nodes, pickId()),
        s("todoTree.descendantCount", nodes, pickId()),
        s("todoTree.dayProgress", nodes),
      );
      for (let k = 0; k < 4; k++) {
        const idx = Math.floor(ctx.rng() * (candidates.length + 1));
        const depth = Math.floor(ctx.rng() * 4);
        const invalid = ctx.rng() < 0.3 ? [pickId()] : undefined;
        steps.push(s("todoTree.resolveDrop", nodes, candidates, dragged, { idx, depth }, ...(invalid ? [invalid] : [])));
        steps.push(s("todoTree.clampDropDepth", depth, candidates[idx - 1] ?? null, candidates[idx] ?? null));
      }
    }
    return steps;
  },
};

/** 타임테이블 배치 — 포함(들여쓰기), 교차(칸 나누기), 같은 범위, 칸이 모자라 얹히는 경우, 만드는 중 블록 */
const timetable: Scenario = {
  name: "timetable/layout",
  plan(ctx) {
    const block = (id: number, start: number, end: number) => ({
      id, date: "2026-11-05", start_min: start, end_min: end, title: `b${id}`, todo_id: null, created_at: 0, updated_at: 0,
    });
    const root = { pctL: 0, pxL: 0, pctW: 100, pxW: -12 };
    const steps: Step[] = [
      s("timeBlocks.nowMinute"),
      s("ttLayout.boxLeft", { pctL: 33.333333333, pxL: -0, pctW: 0, pxW: 0 }),
      s("ttLayout.boxLeft", { pctL: 12.5, pxL: -14.00004, pctW: 0, pxW: 0 }),
      s("ttLayout.boxWidth", { pctL: 0, pxL: 0, pctW: 33.33335, pxW: 2 }),
      s("ttLayout.boxWidth", { pctL: 0, pxL: 0, pctW: 50, pxW: -12 }, 0),
      s("ttLayout.layoutColumn", [], root, 300),
      s("ttLayout.layoutColumn", [block(1, 540, 1080), block(2, 600, 660), block(3, 630, 720), block(4, 600, 660)], root, 320),
      s("ttLayout.withGhost", [block(1, 540, 600)], null),
      s("ttLayout.withGhost", [block(1, 540, 600)], { id: null, date: "2026-11-05", start: 550, end: 610 }),
      s("ttLayout.withGhost", [block(1, 540, 600), block(2, 700, 760)], { id: 2, date: "2026-11-06", start: 500, end: 520 }),
    ];
    for (let f = 0; f < 60; f++) {
      const n = 1 + Math.floor(ctx.rng() * 12);
      const blocks = Array.from({ length: n }, (_, i) => {
        const start = Math.floor(ctx.rng() * 48) * 15 + 480;
        const len = (1 + Math.floor(ctx.rng() * (ctx.rng() < 0.2 ? 40 : 8))) * 15;
        return block(i + 1 + (f % 2) * 50, start, Math.min(1440, start + len));
      });
      if (ctx.rng() < 0.3 && n > 1) blocks.push({ ...blocks[0], id: 900 + f }); // 같은 범위
      const gridW = [120, 180, 260, 340, 520, 900][Math.floor(ctx.rng() * 6)];
      const box = ctx.rng() < 0.5 ? root : { pctL: 14.285714, pxL: 3, pctW: 14.285714, pxW: -15 };
      steps.push(s("ttLayout.layoutColumn", blocks, box, gridW));
      const ghost =
        ctx.rng() < 0.5
          ? { id: null, date: "2026-11-05", start: 600, end: 690 }
          : { id: blocks[0].id, date: "2026-11-05", start: 660, end: 780 };
      steps.push(s("ttLayout.withGhost", blocks, ghost));
      steps.push(after("ttLayout.layoutColumn", (bl) => [bl, root, gridW]));
      steps.push(s("timeBlocks.findFreeSlot", blocks, Math.floor(ctx.rng() * 96) * 15, (1 + Math.floor(ctx.rng() * 8)) * 15));
      steps.push(s("timeBlocks.findFreeSlot", blocks, 1430, 60));
    }
    return steps;
  },
};

/** 노트 트리에서 아무 일이나 한다 — 없는 경로, 겹치는 이름, 대소문자만 다른 이름이 섞인다 */
function notesWalk(seed: number, length: number): Scenario["walk"] {
  const NAMES = ["노트", "Note", "note", "가/나", "CS", "테스트 노트", "a b"];
  const DIRS = ["", "CS", "가", "Ops", "CS/깊이"];
  const FILES = ["노트.md", "Note.md", "note.md", "가/나.md", "CS/노트.md", "CS/깊이/x.md", "Ops/note.md", "테스트 노트.md"];
  return {
    seed,
    length,
    next: () => (ctx) => {
      const r = ctx.rng();
      const name = pick(ctx.rng, NAMES);
      const dir = pick(ctx.rng, DIRS);
      const file = pick(ctx.rng, FILES);
      if (r < 0.2) return [s("notes.createNote", dir, name)];
      if (r < 0.3) return [s("notes.createFolder", dir, name)];
      if (r < 0.42) return [s("notes.writeNoteFile", file, `${name} @ ${ctx.index}\n![](_assets/p.png)\n`)];
      if (r < 0.52) return [s("notes.renameEntry", pick(ctx.rng, [file, dir || "CS"]), name, ctx.rng() < 0.3)];
      if (r < 0.64) return [s("notes.moveEntry", pick(ctx.rng, [file, dir || "CS"]), pick(ctx.rng, DIRS))];
      if (r < 0.72) return [s("notes.deleteEntry", pick(ctx.rng, [file, dir || "Ops"]))];
      if (r < 0.8) return [s("notes.readNoteFile", file)];
      if (r < 0.9) return [s("vaultTree.searchFiles", "notes", name.slice(0, 2), 5)];
      return [s("notes.listNoteTree")];
    },
  };
}

/** 트리 화면의 순수 계산 — 이름 규칙(빈 이름, 금지 글자, 점, 80자 UTF-16), 경로 정리, 접두사 다시 잇기 */
const vaultTreePure: Scenario = {
  name: "vaultTree/pure",
  plan() {
    const names = [
      "", "  ", "ok", " ok ", "\u00a0ok\u00a0", "\ufeffbom", "a/b", "a\\b", "a:b", ".hidden", " .x",
      "x".repeat(80), "x".repeat(81), "한".repeat(80), "한".repeat(81), "😀".repeat(40), "😀".repeat(41), "노트 1",
    ];
    const paths = ["", "/", "CS/네트워크", "a//b", "a/.b", "a/b:c", " a / b ", "a/" + "x".repeat(81), "///"];
    const steps: Step[] = [];
    for (const n of names) steps.push(s("vaultTree.invalidNameReason", n));
    for (const p of paths) steps.push(s("vaultTree.invalidPathReason", p), s("vaultTree.normalizePath", p));
    for (const p of ["", "a", "a/b", "a/b/c.md", "/a//b/"]) steps.push(s("vaultTree.parentOf", p), s("vaultTree.ancestorPaths", p));
    for (const [p, o, n] of [
      ["CS", "CS", "X"], ["CS/a.md", "CS", "X"], ["CS수업.md", "CS", "X"], ["a", "CS", "X"],
      ["CS/깊이/x.md", "CS/깊이", "CS/얕게"], ["CS/", "CS", "X/Y"], ["", "", "X"],
    ])
      steps.push(s("vaultTree.remapPath", p, o, n));
    steps.push(
      s("vaultTree.remapPaths", ["CS", "CS/a", "CS수업", "X"], "CS", "X"),
      s("vaultTree.remapPaths", [], "CS", "X"),
      s("vaultTree.flattenDirs", []),
      s("vaultTree.flattenDirs", [
        { name: "CS", path: "CS", isDir: true, children: [
          { name: "깊이", path: "CS/깊이", isDir: true, children: [{ name: "x", path: "CS/깊이/x.md", isDir: false }] },
          { name: "노트", path: "CS/노트.md", isDir: false },
        ] },
        { name: "빈", path: "빈", isDir: true },
        { name: "a", path: "a.md", isDir: false },
      ]),
    );
    return steps;
  },
};

/** 트리에서 찾기 — 대소문자, 한글, 이모지(UTF-16 짝), 폴더가 맞으면 아래 통째, 조상 펼침 순서 */
const noteAiPure: Scenario = {
  name: "noteAi/pure",
  plan() {
    const steps: Step[] = [];
    const docs = [
      "",
      "서문만 있다\n",
      "intro\n# 1. A\nbody\n## 1-2. B\n### x\n#### 넷째는 아니다\n",
      "# 1. 설치\n```bash\n# 주석은 제목이 아니다\n```\n## 2. 사용\n",
      "# A\n````md\n```\n# 안\n```\n````\n# B\n",
      "# A\n```bash\n# 안\n```bash\n# 아직 안\n```\n# B\n",
      "# A\r\n텍스트\r\n# B\rC\n## 😀 이모지 절\n",
      "#제목 아님\n#  두 칸\n ## 앞 공백은 제목 아님\n# \u00a0nbsp\n",
      "~~~\n# 물결 안\n~~~\n# 1-2-. 꼬리\n# 3 . 공백\n",
    ];
    for (const d of docs) steps.push(s("mdSections.splitSections", d));
    steps.push(
      s("mdSections.mergeRuns", []),
      s("mdSections.mergeRuns", [
        { start: 30, end: 40, k: "c" },
        { start: 0, end: 10, k: "a" },
        { start: 10, end: 20, k: "b" },
        { start: 50, end: 60, k: "d" },
        { start: 55, end: 70, k: "e" },
        { start: 0, end: 5, k: "f" },
      ]),
    );
    const md = "앞\n\n## 1. 가\n옛 글 😀\n\n## 2. 나\n둘\n";
    for (const [st, en, rep] of [
      [3, 15, "## 1. 가\n새 글"],
      [3, 15, "  \n\n  "],
      [0, 0, "x"],
      [1, 3, "   "],
      [md.length - 3, md.length, "\u00a0끝\ufeff"],
    ] as const)
      steps.push(s("mdSections.spliceSpan", md, st, en, rep));
    steps.push(
      s("noteAiRun.spliceSpans", md, [{ kind: "section", start: 3, end: 15, label: "1" }, { kind: "section", start: 15, end: md.length, label: "2" }], ["## 1. 가\nA", "## 2. 나\nB"]),
      s("noteAiRun.spliceSpans", md, [{ kind: "selection", start: 3, end: 8, label: "s" }], []),
    );
    steps.push(
      s("aiInstruction.composeInstruction", "", []),
      s("aiInstruction.composeInstruction", "  내가 친 말 ", ["", "  저장 프롬프트\n여러 줄 ", "빠른 지시"]),
      s("aiInstruction.tailSpan", "짧다"),
      s("aiInstruction.tailSpan", "a".repeat(800) + "\n마지막 줄", 20),
      s("aiInstruction.tailSpan", "\n" + "b".repeat(30), 30),
      s("aiInstruction.tailSpan", "c".repeat(40), 10),
      s("aiInstruction.tailSpan", "😀".repeat(400) + "\n끝", 700),
    );
    for (const [t, k] of [
      ["", 2], ["index.ts", 2], ["x".repeat(70), 2], ["/Users/me/src/lib/index.ts", 2],
      ["src/lib/", 2], ["/a", 2], ["a/b/c", 1], ["  /tmp//x/y/  ", 2], ["가".repeat(61), 2],
    ] as const)
      steps.push(s("aiActivity.shortTarget", t, k));
    for (const a of [
      { tool: "Read" }, { tool: "Read", target: "/Users/me/notes/CS/네트워크.md" }, { tool: "Glob", target: "**/*.md" },
      { tool: "Grep", target: "TODO" }, { tool: "search", target: "rust gpui" }, { tool: "LS", target: "/a/b/c" },
      { tool: "Write", target: "/tmp/draft/01.md" }, { tool: "Edit" }, { tool: "command", target: "ls -la" },
      { tool: "Bash", target: "rg x" }, { tool: "mcp__notion", target: "page" }, { tool: "mcp__notion" },
    ])
      steps.push(s("aiActivity.describeActivity", a));
    const t0 = 1_700_000_000_000;
    for (const [act, at, now, ref] of [
      [null, 0, t0, false], [null, 0, t0 + 4_000, false], [null, 0, t0 + 9_000, false],
      [{ tool: "thinking" }, t0 + 1_000, t0 + 13_500, false], [{ tool: "thought" }, t0 + 1_000, t0 + 2_000, false],
      [{ tool: "writing" }, t0, t0 + 30_000, false], [{ tool: "Read", target: "/a/b/c.md" }, t0, t0 + 61_000, false],
      [{ tool: "thinking" }, t0, t0 + 180_000, true], [{ tool: "thinking" }, t0, t0 + 180_000, false],
      [{ tool: "unknown-phase" }, t0, t0 + 5_000, false], [null, 0, t0 - 5_000, false],
    ] as const)
      steps.push(s("aiWait.waitLine", { startedAt: t0, activity: act, activityAt: at, hasRefDirs: ref }, now));
    return steps;
  },
};

const diagramGraphPure: Scenario = {
  name: "diagramGraph/pure",
  plan() {
    const steps: Step[] = [];
    for (const t of ["string", "string?", "?", "", "varchar(20)?", "a??"])
      steps.push(s("diagramGraph.splitOptionalType", t));
    for (const c of ["", "[NULL] 메모", "설명 [ not  null ]", "[NOTNULL]", "[nullable]", "x [NULL] y [NOTNULL]"])
      steps.push(s("diagramGraph.nullFlagOf", c));
    for (const [t, c, o] of [
      ["string?", "", false], ["string", "", true], ["string", "[NULL]", false], ["string", "", false],
      ["int?", "[NOTNULL]", true],
    ] as const)
      steps.push(s("diagramGraph.nullabilityFlag", t, c, o));
    const ids = ["entity-a-0", "entity-a_b-1", "entity-b-2", "entity-booking_air_tickets-3"];
    for (const d of [
      "", "id_entity-a-0_entity-b-2_0", "id_entity-a_b-1_entity-a-0_4", "id_entity-booking_air_tickets-3_entity-a_b-1_1",
      "id_entity-a-0_entity-a-0_2", "entity-b-2-cyclic-special-1", "entity-b-2-cyclic-special-mid",
      "nope-cyclic-special-2", "L_A_B_0", "id_unknown_entity-a-0_0", "id_entity-a-0_",
    ])
      steps.push(s("diagramGraph.parseEdgeEndpoints", d, ids));
    const edges = [
      { source: "a", target: "b" }, { source: "c", target: "a" }, { source: "a", target: "a" },
      { source: "b", target: "c" }, { source: "a", target: "b" },
    ];
    for (const n of ["a", "b", "c", "z"]) steps.push(s("diagramGraph.neighborsOf", n, edges));
    steps.push(s("diagramGraph.neighborsOf", "a", []));
    return steps;
  },
};

// DB 연결의 순수 판정 — 설정 목록과 ERD 트리가 같이 쓴다
const dbconnPure: Scenario = {
  name: "dbconn/pure",
  plan() {
    const conn = (o: Record<string, unknown> = {}) => ({
      id: 1, ulid: "01JCONN", name: "운영 DB", kind: "mysql", env: "prod", host: "127.0.0.1", port: 3306, username: "u", tls: "preferred",
      credential_command: "", folder_path: "운영 DB", schemas: [
        { name: "svc_a", label: "", enabled: true }, { name: "svc_b", label: "B", enabled: false, starred: true },
        { name: "svc_c", label: "", enabled: true, audit: false, starred: true },
      ], last_sync_at: null, last_error: null, created_at: 0, updated_at: 0, ...o,
    });
    const steps: Step[] = [];
    for (const p of [{ name: "a", label: "", enabled: true }, { name: "a", label: "", enabled: true, audit: false }, { name: "a", label: "", enabled: true, audit: true }])
      steps.push(s("dbconn.prefAudit", p));
    steps.push(s("dbconn.schemaFolder", conn(), "svc_a"), s("dbconn.enabledSchemas", conn()), s("dbconn.profileOf", conn({ credential_command: "aws x" })));
    for (const [snap, now] of [[null, 0], [{ synced_at: 1000 }, 1000 + 600000], [{ synced_at: 1000 }, 1001 + 600000]] as const)
      steps.push(s("dbconn.isStale", snap, now));
    for (const e of ["dev", "staging", "prod", "x"]) steps.push(s("dbconn.envLabel", e));
    for (const c of [conn(), conn({ last_sync_at: 5 }), conn({ last_sync_at: 0 }), conn({ last_error: "KEYCHAIN_MISSING", last_sync_at: 5 }), conn({ last_error: "DB_REFUSED" }), conn({ last_error: "" })])
      steps.push(s("dbconn.connStatus", c));
    for (const st of ["connected", "failed", "needsPassword", "never"]) steps.push(s("dbconn.connStatusLabel", st), s("dbconn.connStatusDot", st));
    for (const n of ["  운영 DB  ", "a/b\\c:d", "...hidden", "", "   ", "x".repeat(100), "😀".repeat(50), "\u0085a\u0085", "\ufeffb"])
      steps.push(s("dbconn.folderNameFor", n));
    return steps;
  },
};

// ERD 생성 — 스냅샷(information_schema 꼴)을 손으로 지어 추론 규칙마다 한 번은 지나게 한다(erdGen.test.ts 의 갈래)
/** 스키마 스냅샷의 순수 연산 — 지문(cyrb53 을 UTF-16 단위로, 이름순 localeCompare, rows_estimate 는 빼고 코멘트는 넣는다),
 *  마무리, 읽기(깨진 파일은 null), 비교 */
const schemaSnapshotPure: Scenario = {
  name: "schemaSnapshot/pure",
  plan() {
    const col = (name: string, o: Record<string, unknown> = {}) => ({
      name, data_type: "bigint", column_type: "bigint", nullable: false, key: "", default_value: null, extra: "", comment: "", ...o,
    });
    const tbl = (name: string, columns: ReturnType<typeof col>[], o: Record<string, unknown> = {}) =>
      ({ name, comment: "", rows_estimate: null as number | null, columns, indexes: [] as unknown[], foreign_keys: [] as unknown[], checks: [] as unknown[], ...o });
    const base = [
      tbl("users", [col("id", { key: "PRI", extra: "auto_increment" }), col("email", { data_type: "varchar", column_type: "varchar(255)", key: "UNI", comment: "이메일" })],
        { comment: "회원", rows_estimate: 1200, indexes: [{ name: "PRIMARY", unique: true, columns: ["id"] }, { name: "uq_email", unique: true, columns: ["email"] }] }),
      tbl("orders", [col("id", { key: "PRI" }), col("user_id", { key: "MUL" }), col("memo", { data_type: "text", column_type: "text", nullable: true, default_value: "''", comment: 'a "q"\r\n\t\\ 😀 끝' })],
        { indexes: [{ name: "ix_user", unique: false, columns: ["user_id"] }], foreign_keys: [{ name: "fk_user", columns: ["user_id"], ref_schema: "shop", ref_table: "users", ref_columns: ["id"] }],
          checks: [{ name: "chk_b", clause: "(`memo` <> '')" }, { name: "chk_a", clause: "(`id` > 0)" }] }),
      tbl("Order_items", [col("order_id"), col("ID", { key: "PRI" }), col("qty", { data_type: "int", column_type: "int unsigned", default_value: "1" })]),
      tbl("_audit", [col("rev", { data_type: "int", column_type: "int", key: "PRI" })]),
      tbl("주문", [col("번호", { key: "PRI" })], { comment: "한글 표" }),
    ];
    const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
    const reordered = clone(base).reverse().map((t) => ({ ...t, columns: [...t.columns].reverse(), rows_estimate: 9 }));
    const commented = clone(base);
    commented[0].comment = "회원 (바뀜)";
    const raw = { connection: "01JCONN", schema: "shop", server: "MySQL 8.0.36", synced_at: 1793000000000, tables: base };
    const snap = (tables: unknown[]) => ({ amber: 1, ...raw, fingerprint: "x", tables });
    // 비교 — 표 추가/삭제, 컬럼 추가/삭제/변경(타입, 널, 기본값 null 과 " "), 제약 변경, 순서만 바뀐 제약
    const next = clone(base);
    next.splice(3, 1);
    next.push(tbl("coupons", [col("id", { key: "PRI" })]));
    next[0].columns.push(col("name", { data_type: "varchar", column_type: "varchar(64)" }));
    next[1].columns = next[1].columns.filter((c) => c.name !== "user_id");
    next[1].columns[1].nullable = false;
    next[2].columns[2].default_value = " ";
    next[0].indexes = [...next[0].indexes].reverse();
    next[1].checks = [{ name: "chk_a", clause: "(`id` >= 0)" }];
    return [
      s("schemaSnapshot.fingerprintOf", base),
      s("schemaSnapshot.fingerprintOf", reordered),
      s("schemaSnapshot.fingerprintOf", commented),
      s("schemaSnapshot.fingerprintOf", []),
      s("schemaSnapshot.finalizeSnapshot", raw),
      s("schemaSnapshot.finalizeSnapshot", { ...raw, tables: [] }),
      ...[
        JSON.stringify(snap(base)),
        JSON.stringify({ ...snap(base), amber: 2 }),
        JSON.stringify({ ...snap(base), schema: 3 }),
        JSON.stringify({ ...snap(base), tables: {} }),
        JSON.stringify({ amber: 1, schema: "s", tables: [] }),
        "{ broken",
        "null",
        "[]",
        "",
      ].map((text) => s("schemaSnapshot.parseSnapshot", text)),
      s("schemaSnapshot.diffSnapshots", snap(base), snap(next)),
      s("schemaSnapshot.diffSnapshots", snap(base), snap(clone(base))),
      s("schemaSnapshot.diffSnapshots", snap(next), snap(base)),
      s("schemaSnapshot.diffIsEmpty", { tablesAdded: [], tablesRemoved: [], columnsAdded: [], columnsRemoved: [], columnsChanged: [], constraintTables: [] }),
      s("schemaSnapshot.diffIsEmpty", { tablesAdded: [], tablesRemoved: [], columnsAdded: [], columnsRemoved: [], columnsChanged: [], constraintTables: ["t"] }),
    ];
  },
};

const erdGenPure: Scenario = {
  name: "erdGen/pure",
  plan() {
    type Col = { name: string; data_type: string; column_type: string; nullable: boolean; key: string; default_value: string | null; extra: string; comment: string };
    const col = (name: string, data_type: string, o: Partial<Col> = {}): Col => ({
      name, data_type, column_type: o.column_type ?? data_type, nullable: o.nullable ?? false, key: o.key ?? "",
      default_value: o.default_value ?? null, extra: o.extra ?? "", comment: o.comment ?? "",
    });
    const pk = (name = "id", type = "bigint") => col(name, type, { key: "PRI", extra: "auto_increment" });
    const ix = (name: string, columns: string[], unique = false) => ({ name, unique, columns });
    const fk = (name: string, columns: string[], ref_table: string, ref_columns: string[] = ["id"], ref_schema = "svc_booking") =>
      ({ name, columns, ref_schema, ref_table, ref_columns });
    const tbl = (name: string, columns: Col[], o: { comment?: string; indexes?: ReturnType<typeof ix>[]; fks?: ReturnType<typeof fk>[]; checks?: { name: string; clause: string }[] } = {}) =>
      ({ name, comment: o.comment ?? "", rows_estimate: null, columns, indexes: o.indexes ?? [], foreign_keys: o.fks ?? [], checks: o.checks ?? [] });
    const snap = (tables: ReturnType<typeof tbl>[], schema = "svc_booking") =>
      ({ amber: 1, connection: "01JCONN", schema, server: "MySQL 8.0.36", synced_at: 0, fingerprint: "test", tables });

    const fixture = [
      tbl("ts_booking", [pk(), col("user_id", "bigint", { key: "MUL", comment: "회원 ID" }), col("booking_no", "varchar", { column_type: "varchar(32)", key: "UNI", comment: "예약번호" }),
        col("status", "varchar", { column_type: "enum('PENDING','CONFIRMED','CANCELED')", key: "MUL" }), col("cancel_reason", "varchar", { column_type: "varchar(255)", nullable: true, comment: "취소 사유" }),
        col("created_at", "datetime"), col("updated_at", "datetime")], { comment: "예약", indexes: [ix("idx_user_id", ["user_id"]), ix("idx_status", ["status"])] }),
      tbl("ts_booking_passenger", [pk(), col("booking_id", "bigint", { key: "MUL", comment: "예약 ID" }), col("name", "varchar", { column_type: "varchar(255)", nullable: true, comment: "이름 (암호화)" }),
        col("passport_no", "varchar", { column_type: "varchar(255)", nullable: true, comment: "여권번호" }), col("phone", "varchar", { column_type: "varchar(64)", nullable: true }), col("created_at", "datetime")],
        { comment: "예약 승객", indexes: [ix("ix_booking_id", ["booking_id"])] }),
      tbl("ts_booking_payment", [pk(), col("booking_id", "bigint", { key: "UNI" }), col("method", "varchar", { column_type: "varchar(20)" }), col("amount", "decimal", { column_type: "decimal(12,2)" }),
        col("created_at", "datetime"), col("updated_at", "datetime", { nullable: true })],
        { comment: "결제", indexes: [ix("uq_booking_id", ["booking_id"], true)], checks: [{ name: "chk_payment_method", clause: "(`method` in (_utf8mb4'CARD',_utf8mb4'BANK_TRANSFER'))" }] }),
      tbl("ts_booking_refund", [pk(), col("payment_id", "bigint", { key: "MUL", comment: "결제 ID" }), col("created_at", "datetime")],
        { comment: "환불", indexes: [ix("ix_payment_id", ["payment_id"])], fks: [fk("fk_refund_payment", ["payment_id"], "ts_booking_payment")] }),
      tbl("ts_booking_memo", [pk(), col("booking_id", "bigint"), col("body", "text", { nullable: true, comment: 'a "quoted" memo\r\n  second   line' }), col("created_at", "datetime")]),
      tbl("ts_booking_coupon_use", [pk(), col("booking_id", "bigint", { key: "MUL" }), col("coupon_code", "varchar", { column_type: "varchar(64)" }), col("created_at", "datetime")],
        { comment: "쿠폰 사용", indexes: [ix("uk_booking_coupon", ["booking_id", "coupon_code"], true)] }),
      tbl("revinfo", [pk("rev", "int"), col("revtstmp", "bigint")]),
      tbl("ts_booking_aud", [col("id", "bigint", { key: "PRI" }), col("rev", "int", { key: "PRI" }), col("revtype", "tinyint", { nullable: true }),
        col("status", "varchar", { column_type: "varchar(20)", nullable: true }), col("created_at", "datetime", { nullable: true })]),
    ];
    // 실제 accounts 스키마의 구멍들 — 코멘트 참조, 대다수 접두사, `_x` 꼬리, 타입이 다르면 같은 이름 UNIQUE, 여러 FK, 깨진 인코딩
    const accounts = [
      tbl("ts_customers", [pk(), col("email", "varchar", { column_type: "varchar(255)", comment: "이메일 (enc)" })]),
      tbl("ts_devices", [pk(), col("model", "varchar", { column_type: "varchar(32)" })]),
      tbl("editing_logs", [pk(), col("user_id", "bigint", { comment: "ts_customers.id - 유저 ID" }), col("device_id", "bigint"), col("note", "varchar", { column_type: "varchar(10)", comment: "ì—¬í–‰ ë…¸íŠ¸" })]),
      tbl("ts_user_profiles", [pk(), col("nick", "varchar", { column_type: "varchar(20)" })]),
      tbl("ts_like_share", [pk()]),
      tbl("ts_post_share", [pk()]),
      tbl("ts_posts", [pk(), col("profile_id", "bigint"), col("share_id", "bigint"), col("travel_id", "varchar", { column_type: "varchar(36)" }), col("category_id", "int")]),
      tbl("ts_travels", [pk(), col("travel_id", "varchar", { column_type: "varchar(36)", key: "UNI" })]),
      tbl("ts_categories", [pk("id", "int"), col("name", "varchar", { column_type: "varchar(30)" })]),
      tbl("ts_transfers", [pk(), col("from_id", "bigint"), col("to_id", "bigint"), col("customer_id", "bigint")],
        { fks: [fk("fk_from", ["from_id"], "ts_customers"), fk("fk_to", ["to_id"], "ts_customers"), fk("fk_none", [], "ts_customers")] }),
      tbl("ts_id_only", [col("id", "bigint", { key: "PRI", comment: "ID" }), col("parent_id", "bigint"), col("ts_id_only_id", "bigint")]),
    ];
    // 컬럼 세부 — 순서, 따옴표, 널 PK, enc 형제 규칙, enum 우선, 다른 스키마 FK, 없는 표, 1:1, 이상한 이름
    const details = [
      tbl("orders", [col("updated_at", "datetime"), col("created_at", "datetime"), pk(), col("code", "varchar", { column_type: "varchar(8)", key: "UNI" }),
        col("state", "varchar", { column_type: "enum('A','B')" }), col("grade", "char", { column_type: "char(1)" }), col("kind", "varchar", { column_type: "varchar(4)" })],
        { checks: [{ name: "c1", clause: "`state` in ('X')" }, { name: "c2", clause: "grade IN ('S','M', 'it''s')" }, { name: "c3", clause: "kind in ()" }] }),
      tbl("ts_pay", [pk("id", "bigint"), col("buyer_id", "bigint"), col("card_no", "varchar", { column_type: "varchar(200)" }), col("email", "varchar", { column_type: "VARCHAR(512)" }),
        col("address", "text"), col("secret", "varchar", { column_type: "varchar(10)", comment: "AES 암호화" }), col("encoding", "varchar", { column_type: "varchar(300)", comment: "encode only" })],
        { fks: [fk("fk_buyer", ["buyer_id"], "users", ["id"], "svc_accounts"), fk("fk_gone", ["id"], "ts_gone")] }),
      tbl("ts_pay_detail", [pk(), col("pay_id", "bigint", { key: "UNI" })], { fks: [fk("fk_pay", ["pay_id"], "ts_pay")] }),
      tbl("order items", [col("id", "bigint", { key: "PRI", nullable: true, comment: "고유 식별자" }), col("weird col", "character varying", { column_type: "character varying(20)" }),
        col("geo", "geometry", { column_type: "point" })], { comment: "줄바꿈\n있는 표" }),
      tbl("ts_mixed", [pk(), col("x", "int"), col("y", "int")], { indexes: [ix("PRIMARY", ["id"], true), ix("ix_xy", ["x", "y"]), ix("uq_x", ["x"], true)] }),
    ];
    const steps: Step[] = [];
    for (const [tables, schema] of [[fixture, "svc_booking"], [accounts, "svc_accounts"], [details, "svc_booking"]] as const) {
      const sn = snap([...tables], schema);
      steps.push(s("erdGen.generateErd", sn, {}), s("erdGen.generateErd", sn, { audit: false }), s("erdGen.inferReferences", sn));
      // 순서를 섞어도 같은 원문
      steps.push(s("erdGen.generateErd", snap([...tables].reverse(), schema), {}));
    }
    // revinfo 가 없으면 *_aud 는 평범한 표
    steps.push(s("erdGen.generateErd", snap([fixture[0], fixture[7]]), {}));
    for (const names of [[], ["a"], ["ts_booking", "ts_booking_passenger"], ["ts_a", "ts_b", "editing_logs"], ["ts_a", "x_b", "y_c"], ["abc", "abd"], ["a_x", "b_x", "a_y", "b_y"]])
      steps.push(s("erdGen.commonTablePrefix", names));
    return steps;
  },
};

const treeFindSweep: Scenario = {
  name: "treeFind/sweep",
  plan() {
    const tree = [
      { name: "CS", path: "CS", isDir: true, children: [
        { name: "Network", path: "CS/Network", isDir: true, children: [
          { name: "TCP 혼잡 제어", path: "CS/Network/TCP 혼잡 제어.md", isDir: false },
          { name: "tcp-handshake", path: "CS/Network/tcp-handshake.md", isDir: false },
        ] },
        { name: "운영체제", path: "CS/운영체제", isDir: true, children: [
          { name: "스케줄링", path: "CS/운영체제/스케줄링.md", isDir: false },
        ] },
        { name: "빈 폴더", path: "CS/빈 폴더", isDir: true },
      ] },
      { name: "Inbox", path: "Inbox.md", isDir: false },
      { name: "😀 emoji note", path: "😀 emoji note.md", isDir: false },
      { name: "ΣΊΣΥΦΟΣ", path: "Sisyphus.md", isDir: false },
    ];
    const queries = ["", "  ", "tcp", "TCP", " tcp ", "net", "운영", "스케", "o", "😀", "emoji", "σ", "ς", "zzz", "CS"];
    const steps: Step[] = [];
    for (const q of queries) steps.push(s("treeFind.filterTree", tree, q));
    for (const [name, q] of [
      ["TCP 혼잡 제어", "tcp"], ["tcp-handshake", "TCP"], ["aaaa", "aa"], ["😀 emoji note", "e"], ["😀😀", "😀"],
      ["ΣΊΣΥΦΟΣ", "σ"], ["스케줄링", "케줄"], ["Inbox", ""], ["Inbox", "x"], ["İstanbul", "i"],
    ])
      steps.push(s("treeFind.splitHit", name, q), s("treeFind.hits", name, q));
    return steps;
  },
};

/** 노트 질문 사이드카 — 정상, 깨진 JSON, comments 가 배열이 아님, 후속 문답 형태 검사, 비우면 파일을 지운다 */
const commentFiles: Scenario = {
  name: "comments/files",
  seed: [
    { path: "vault/notes/A.md", text: "# A\n" },
    {
      path: "vault/notes/A.comments.json",
      text: JSON.stringify({
        version: 1,
        comments: [
          { id: "1", anchor: "문장", occurrence: 0, question: "왜?", answer: "그래서", createdAt: 1, model: "m",
            followUps: [{ question: "또", answer: "응", createdAt: 2 }, { question: 3 }, "x"] },
          { id: "2", anchor: "b", occurrence: 1, question: "q", answer: "a", createdAt: 3, followUps: "bad" },
          { id: "3", anchor: 5, question: "q" },
          "junk",
          { id: "4", anchor: "c", occurrence: 0, question: "q2", answer: "", createdAt: 4, extra: { keep: true } },
        ],
      }),
    },
    { path: "vault/notes/B.md", text: "# B\n" },
    { path: "vault/notes/B.comments.json", text: "{ broken" },
    { path: "vault/notes/C.md", text: "# C\n" },
    { path: "vault/notes/C.comments.json", text: '{"comments": {}}' },
    { path: "vault/notes/D/E.md", text: "# E\n" },
  ],
  plan() {
    const one = [{ id: "n", anchor: "새", occurrence: 0, question: "질문", answer: "답", createdAt: 9 }];
    return [
      s("comments.commentsPathFor", "a.md"),
      s("comments.commentsPathFor", "a.MD"),
      s("comments.commentsPathFor", "x/y.md"),
      s("comments.commentsPathFor", "noext"),
      s("comments.readComments", "A.md"),
      s("comments.readComments", "B.md"),
      s("comments.readComments", "C.md"),
      s("comments.readComments", "D/E.md"),
      s("comments.loadComments", "B.md"),
      s("comments.loadComments", "A.md"),
      s("comments.mutateComments", "B.md", one),
      s("comments.mutateComments", "C.md", one),
      s("comments.mutateComments", "D/E.md", one),
      s("comments.mutateComments", "A.md", one),
      s("comments.readComments", "A.md"),
      s("comments.saveComments", "A.md", []),
      s("comments.readComments", "A.md"),
      s("comments.saveComments", "Z.md", []),
      s("noteAnchor.nthIndex", "ab ab ab", "ab", 0),
      s("noteAnchor.nthIndex", "ab ab ab", "ab", 2),
      s("noteAnchor.nthIndex", "ab ab ab", "ab", 3),
      s("noteAnchor.nthIndex", "aaa", "aa", 1),
      s("noteAnchor.nthIndex", "😀 ab 😀 ab", "ab", 1),
      s("noteAnchor.nthIndex", "x", "", 0),
    ];
  },
};

/** AI 연결 설정 — 빈 판, 옛 claude_path 이전, gemini 는 미연결, 연결/건너뛰기/저장 */
const aiConfig: Scenario = {
  name: "config/ai",
  seed: [
    { sql: "INSERT INTO settings (key, value) VALUES ('claude_path', '/usr/local/bin/claude'), ('claude_model', 'claude-opus-5')" },
  ],
  plan() {
    return [
      s("config.loadConfig"),
      s("config.loadConfig"),
      s("config.saveConfig", { provider: "claude", onboarded: true, cliPath: "  /opt/claude  ", model: "", aiLang: "en" }),
      s("config.loadConfig"),
      s("config.connectProvider", "codex", " /usr/bin/codex ", "gpt-5.5"),
      s("config.connectProvider", "claude", "/x/claude"),
      s("config.saveConfig", { provider: null, onboarded: true, cliPath: "", model: "", aiLang: "ko" }),
      s("config.skipAiOnboarding"),
    ];
  },
};

const aiConfigGemini: Scenario = {
  name: "config/gemini",
  seed: [
    { sql: "INSERT INTO settings (key, value) VALUES ('ai_provider', 'gemini'), ('ai_onboarded', '1'), ('ai_lang', 'xx')" },
  ],
  plan() {
    return [s("config.loadConfig"), s("config.skipAiOnboarding"), s("config.connectProvider", "codex", "")];
  },
};

export const SCENARIOS: Scenario[] = [
  todosTreeBasics,
  todosDoneChain,
  todosCarry,
  todosParkBoundary,
  todosUnparkGroup,
  todosDeleteLinks,
  todosMissing,
  todosCycle,
  concepts,
  settings,
  reports,
  blocksAndVacations,
  dbconn,
  ...Array.from({ length: 16 }, (_, i) => ({ name: `walk/todos-${String(i + 1).padStart(2, "0")}`, walk: todoWalk(i + 1, 60) })),
  ...Array.from({ length: 6 }, (_, i) => ({
    name: `walk/concepts-${String(i + 1).padStart(2, "0")}`,
    walk: conceptWalk(i + 1, 50),
  })),
  notesFiles,
  diagramsFiles,
  dates,
  todoTrees,
  timetable,
  vaultTreePure,
  treeFindSweep,
  noteAiPure,
  diagramGraphPure,
  erdGenPure,
  schemaSnapshotPure,
  erdMigrateFiles,
  dbconnPure,
  commentFiles,
  aiConfig,
  aiConfigGemini,
  conceptNotes,
  erdFiles,
  ...Array.from({ length: 4 }, (_, i) => ({
    name: `walk/notes-${String(i + 1).padStart(2, "0")}`,
    seed: [
      { path: "vault/notes/CS/노트.md", text: "# 노트\n![](_assets/p.png)\n" },
      { path: "vault/notes/CS/_assets/p.png", text: "PNG" },
      { path: "vault/notes/CS/노트.comments.json", text: "{}" },
    ],
    walk: notesWalk(i + 1, 60),
  })),
];

// ---- 실데이터(로컬 전용) ----

/** 실제 amber.db 사본을 훑는다: 있는 날짜, 주, 개념, 리포트, 연결을 전부 읽는다 */
export function realSweep(file: string): Scenario {
  return {
    name: "real/sweep",
    seed: { file },
    plan(ctx) {
      const col = <T>(sql: string) => ctx.peek<Record<string, T>>(sql).map((r) => Object.values(r)[0] as T);
      const days = col<string>("SELECT DISTINCT due_date FROM todos WHERE scope = 'day' UNION SELECT DISTINCT date FROM todo_carries ORDER BY 1");
      const weeks = col<string>("SELECT DISTINCT due_date FROM todos WHERE scope = 'week' ORDER BY 1");
      const months = [...new Set(days.map((d) => d.slice(0, 7)))];
      const steps: Step[] = [
        ...days.map((d) => s("todos.listTodos", d)),
        // 화면이 받은 목록 그대로 트리 계산에 넘긴다(앞 단계의 결과가 다음 단계의 인자)
        ...days.flatMap((d) =>
          ["todoTree.flattenSubset", "todoTree.visibleRoots", "todoTree.dayProgress"].flatMap((op) => [
            s("todos.listTodos", d),
            after(op, (rows) => [rows]),
          ]),
        ),
        ...days.map((d) => s("report.buildTodosDigest", d)),
        ...months.map((m) => s("todos.listRangeRows", `${m}-01`, `${m}-31`)),
        ...months.map((m) => s("timeBlocks.listBlocks", `${m}-01`, `${m}-31`)),
        ...months.map((m) => s("vacations.listVacations", `${m}-01`, `${m}-31`)),
        ...weeks.map((w) => s("todos.listWeekTodos", w)),
        s("todos.listWeekCounts", "2000-01-01", "2100-01-01"),
        s("todos.listOverdueOpen", T),
        s("todos.listParked"),
        s("todos.listParkedAncestors"),
        ...["canonical", "recent_updated", "recent_created", "title"].flatMap((sort) =>
          ["all", "learning", "learned"].map((status) => s("db.listConcepts", { status, sort })),
        ),
        ...col<number>("SELECT id FROM concepts ORDER BY id").map((id) => s("db.getConcept", id)),
        s("db.allTags"),
        s("db.statusCounts"),
        ...col<string>("SELECT report_date FROM daily_reports ORDER BY 1").map((d) => s("report.getReport", d)),
        ...col<string>("SELECT week_start FROM weekly_reports ORDER BY 1").map((w) => s("report.getWeeklyReport", w)),
        s("report.recentReportDates", "2100-01-01", 1000),
        s("report.loadReportConfig"),
        s("report.loadMcpCache"),
        s("dbconn.listConnections"),
      ];
      return steps;
    },
  };
}

/** 실제 스키마 스냅샷(ERD 탭이 동기화해 둔 schema.json 사본)을 전부 resin 원문으로 — 감사 표를 싣고, 빼고 */
export function realErd(dir: string): Scenario {
  const files: string[] = [];
  const walk = (d: string) => {
    for (const n of readdirSync(d, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
      if (n.isDirectory()) walk(join(d, n.name));
      else if (n.name === "schema.json") files.push(join(d, n.name));
    }
  };
  walk(dir);
  return {
    name: "real/erd",
    plan() {
      return files.flatMap((f) => {
        const text = readFileSync(f, "utf8");
        const snapshot = JSON.parse(text);
        // 앱이 남긴 지문을 다시 계산한다 — 파일을 읽어 마무리하면 같은 지문이 나와야 한다
        const raw = { ...snapshot };
        delete raw.amber;
        delete raw.fingerprint;
        return [
          s("erdGen.generateErd", snapshot, {}),
          s("erdGen.generateErd", snapshot, { audit: false }),
          s("schemaSnapshot.parseSnapshot", text),
          s("schemaSnapshot.finalizeSnapshot", raw),
        ];
      });
    },
  };
}

export function realWalks(file: string): Scenario[] {
  return [
    ...Array.from({ length: 6 }, (_, i) => ({
      name: `real/walk-todos-${i + 1}`,
      seed: { file },
      walk: todoWalk(100 + i, 50, ["2026-10-28", "2026-10-31"]),
    })),
    ...Array.from({ length: 2 }, (_, i) => ({ name: `real/walk-concepts-${i + 1}`, seed: { file }, walk: conceptWalk(200 + i, 40) })),
  ];
}
