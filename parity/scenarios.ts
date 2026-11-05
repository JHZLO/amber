// 시나리오 — 손으로 쓴 것은 깨지기 쉬운 규칙을 하나씩 짚고(이월 고스트, 서랍 경계, 연동 블록…),
// 무작위 걸음은 사람이 생각 못 한 순서를 덮는다. 무작위라도 시드가 고정이라 다시 녹화해도 같다.
// 걸음은 화면이 실제로 부르는 순서를 흉내 낸다(체크 뒤 조상 재계산, 순환을 만드는 부모 이동은 하지 않음).

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
