// 동등성 하네스 — 지금 TS 구현의 동작을 픽스처로 박제하고, 옮긴 구현(Rust)이 같은지 잰다.
//
//   pnpm parity                    픽스처 재생 (TS 엔진. pnpm test:ts 에도 들어 있다)
//   pnpm parity:record             다시 녹화 — 동작을 일부러 바꿨을 때만. git diff 로 바뀐 단계를 검토한다
//   AMBER_PARITY_ENGINE="<명령>" pnpm parity    옮긴 엔진으로도 같은 픽스처를 재생 (engine.ts 의 계약)
//
// 실데이터: .claude/parity/real/amber.db(로컬 전용 사본)가 있으면 그걸로 녹화한 픽스처도 재생하고,
// 그 파일의 `_sqlx_migrations` 장부가 지금 마이그레이션과 같은지 — 새 앱이 기존 DB 를 그대로 여는지 — 본다.
// 사본은 이렇게 뜬다. WAL 그대로 두면 읽기만 해도 옆 파일이 생기고 녹화 때의 해시와 어긋난다:
//   sqlite3 ~/Library/Application\ Support/dev.jhzlo.amber/amber.db ".backup '.claude/parity/real/amber.db'"
//   sqlite3 .claude/parity/real/amber.db "PRAGMA journal_mode=DELETE"
// ERD 스냅샷도 있으면(.claude/parity/real/erd) 전부 resin 원문으로 녹화해 견준다:
//   rsync -a --include='*/' --include='schema.json' --exclude='*' ~/Library/Application\ Support/dev.jhzlo.amber/vault/erd/ .claude/parity/real/erd/

import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { ExternalEngine, TsEngine, type Engine, type Outcome } from "./engine";
import { fsRoot } from "./fs";
import { MODULES, NOT_COVERED, OPS, PURE } from "./ops";
import {
  FIXTURES,
  REAL_FIXTURES,
  ledgerOf,
  migrationGolden,
  migrationsBy,
  readFixtures,
  record,
  replay,
  writeFixture,
  type Fixture,
  type MigrationGolden,
} from "./runner";
import { SCENARIOS, realErd, realSweep, realWalks } from "./scenarios";
import { canonical, firstDiff } from "./state";
import { REPO, pluginSql } from "./sqlite";

vi.mock("@tauri-apps/plugin-sql", async () => {
  const { pluginSql } = await import("./sqlite");
  return { default: { load: async () => pluginSql } };
});
vi.mock("ulid", async () => {
  const { nextUlid } = await import("./sqlite");
  return { ulid: () => nextUlid() };
});
vi.mock("@tauri-apps/plugin-fs", async () => (await import("./fs")).pluginFs);
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<object>()),
  invoke: async (cmd: string, args: { relPath: string }) => {
    if (cmd !== "move_to_trash") throw new Error(`parity: ${cmd} 명령은 대역이 없다`);
    (await import("./fs")).moveToTrash(args.relPath);
  },
}));
// 작업 폴더는 기본 보관함(앱 데이터 폴더의 vault/…) — 사용자가 연 폴더는 localStorage 에 있다
vi.mock("../src/lib/workspace", async (original) => {
  const w = await original<typeof import("../src/lib/workspace")>();
  return { ...w, getRoot: (s: keyof typeof w.DEFAULT_ROOTS) => w.DEFAULT_ROOTS[s] };
});

const RECORD = process.env.PARITY_RECORD === "1";
const EXTERNAL = process.env.AMBER_PARITY_ENGINE;
const REAL_DB_REL = ".claude/parity/real/amber.db";
const REAL_DB = join(REPO, REAL_DB_REL);
/** ERD 탭이 동기화한 스냅샷 사본(로컬 전용) — vault/erd 아래 schema.json 만 그대로 떠 온다 */
const REAL_ERD = join(REPO, ".claude/parity/real/erd");
const MIGRATIONS_FILE = join(FIXTURES, "_migrations.json");

beforeAll(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
});
afterAll(() => {
  vi.useRealTimers();
});

/** 같은 시나리오를 두 번 녹화해 같아야 저장한다 — 녹화가 흔들리면 재생 실패가 아무 뜻이 없다 */
async function recordAll(dir: string, scenarios: typeof SCENARIOS) {
  mkdirSync(dir, { recursive: true });
  for (const n of readdirSync(dir)) if (n.endsWith(".json") && !n.startsWith("_")) rmSync(join(dir, n));
  for (const s of scenarios) {
    const a = await record(s, new TsEngine());
    const b = await record(s, new TsEngine());
    expect(firstDiff(a, b), `${s.name}: 녹화가 결정적이지 않다`).toBeNull();
    writeFixture(dir, a);
  }
}

describe.runIf(RECORD)("녹화", () => {
  it("합성 시나리오", async () => {
    await recordAll(FIXTURES, SCENARIOS);
    writeFileSync(MIGRATIONS_FILE, JSON.stringify(migrationGolden(), null, 2) + "\n");
  }, 300_000);

  it.runIf(existsSync(REAL_DB))("실데이터 사본", async () => {
    await recordAll(REAL_FIXTURES, [
      realSweep(REAL_DB_REL),
      ...realWalks(REAL_DB_REL),
      ...(existsSync(REAL_ERD) ? [realErd(REAL_ERD)] : []),
    ]);
  }, 600_000);
});

describe.skipIf(RECORD)("재생", () => {
  const synthetic = readFixtures(FIXTURES);
  const real = readFixtures(REAL_FIXTURES);
  const external = EXTERNAL ? new ExternalEngine(EXTERNAL) : null;
  afterAll(() => external?.dispose());

  it("합성 픽스처가 있다", () => {
    expect(synthetic.length, "pnpm parity:record 로 먼저 녹화한다").toBeGreaterThan(0);
  });

  for (const engine of [new TsEngine(), ...(external ? [external] : [])] as Engine[]) {
    describe(engine.name, () => {
      for (const f of [...synthetic, ...real]) {
        it(f.name, async (ctx) => {
          const known = engine instanceof ExternalEngine ? (await engine.capabilities()).ops : null;
          const missing = known ? [...new Set(f.steps.map((s) => s.op))].filter((op) => !known.has(op)) : [];
          if (missing.length) ctx.skip(`아직 옮기지 않은 op: ${missing.join(", ")}`);
          const miss = await replay(f, engine);
          expect(miss, `[${f.name}] ${miss}`).toBeNull();
        }, 120_000);
      }
    });
  }
});

describe.skipIf(RECORD)("마이그레이션", () => {
  const golden = (): MigrationGolden => JSON.parse(readFileSync(MIGRATIONS_FILE, "utf8"));

  it("지금 마이그레이션이 녹화한 장부, 스키마와 같다", () => {
    expect(firstDiff(golden(), migrationGolden()), "마이그레이션을 더했으면 pnpm parity:record").toBeNull();
  });

  it.runIf(!!EXTERNAL)("옮긴 엔진이 같은 장부와 스키마를 남긴다", async (ctx) => {
    const engine = new ExternalEngine(EXTERNAL!);
    try {
      if (!(await engine.capabilities()).migrate) ctx.skip("엔진이 아직 migrate 를 모른다");
      expect(firstDiff(golden(), await migrationsBy(engine))).toBeNull();
    } finally {
      engine.dispose();
    }
  });

  it.runIf(existsSync(REAL_DB))("실제 DB 사본의 장부가 지금 마이그레이션과 같다 (새 앱이 그대로 연다)", () => {
    expect(firstDiff(golden().ledger, ledgerOf(REAL_DB))).toBeNull();
  });
});

describe.skipIf(RECORD)("하네스 자체", () => {
  const synthetic = readFixtures(FIXTURES);

  it("도메인 파일의 export 는 OPS, NOT_COVERED, PURE 중 꼭 한 곳에 있다", () => {
    const seen = new Set<string>();
    for (const m of MODULES) {
      const src = readFileSync(join(REPO, `src/lib/${m}.ts`), "utf8");
      // 마지막 꼴은 만든 트리의 메서드를 내보내는 별칭(`export const listNoteTree = tree.listTree;`)
      const decl = /^export (async )?function (\w+)|^export const (\w+) = (async )?\(|^export const (\w+) = \w+\.\w+;/gm;
      for (const x of src.matchAll(decl)) {
        const key = `${m}.${x[2] ?? x[3] ?? x[5]}`;
        const homes = [key in OPS, key in NOT_COVERED, PURE.has(key)].filter(Boolean).length;
        expect(homes, `${key}: 재는지(OPS), 왜 안 재는지(NOT_COVERED), 순수한지(PURE) 정한다`).toBe(1);
        if (PURE.has(key)) expect(!!(x[1] ?? x[4]), `${key}: PURE 인데 async 다`).toBe(false);
        seen.add(key);
      }
    }
    for (const k of [...Object.keys(OPS), ...Object.keys(NOT_COVERED), ...PURE]) {
      expect(seen.has(k), `${k}: 그런 export 가 없다`).toBe(true);
    }
  });

  it("OPS 의 함수마다 합성 픽스처에서 성공으로 도는 단계가 있다", () => {
    const ran = new Set(synthetic.flatMap((f) => f.steps.filter((s) => s.ok).map((s) => s.op)));
    expect(Object.keys(OPS).filter((op) => !ran.has(op))).toEqual([]);
  });

  /** 잘 돌다가 한 군데만 몰래 틀리는 엔진 — 하네스가 그걸 잡는지 본다 */
  class Sabotaged extends TsEngine {
    constructor(
      private at: string,
      private harm: (o: Outcome) => Outcome,
    ) {
      super();
    }
    async run(op: string, args: unknown[], now: number) {
      const o = await super.run(op, args, now);
      return op === this.at ? this.harm(o) : o;
    }
  }
  const fixture = (name: string) => synthetic.find((f) => f.name === name)!;

  it("조용히 늘어난 쓰기 한 줄을 잡는다", async () => {
    const miss = await replay(
      fixture("todos/park-boundary"),
      new Sabotaged("todos.deleteTodo", (o) => {
        pluginSql.conn!.exec("UPDATE todos SET content = content || '!' WHERE parked_at IS NOT NULL");
        return o;
      }),
    );
    expect(miss).toMatch(/DB 변경이 다르다/);
  });

  it("이동하며 사이드카를 두고 가면 잡는다 (파일 상태)", async () => {
    const miss = await replay(
      fixture("notes/files"),
      new Sabotaged("notes.moveEntry", (o) => {
        if (o.ok && o.result === "Ops/tcp.md") {
          const dir = join(fsRoot.dir, "vault/notes");
          renameSync(join(dir, "Ops/tcp.comments.json"), join(dir, "CS/네트워크/tcp.comments.json"));
        }
        return o;
      }),
    );
    expect(miss).toMatch(/DB 변경이 다르다 — \$\.__files/);
  });

  it("휴지통 대신 지워 버리면 잡는다", async () => {
    const miss = await replay(
      fixture("vault/concept-notes"),
      new Sabotaged("vault.deleteConceptDir", (o) => {
        rmSync(join(fsRoot.dir, ".trash"), { recursive: true, force: true });
        return o;
      }),
    );
    expect(miss).toMatch(/__files/);
  });

  it("숫자 1 과 문자열 '1' 을 가른다", async () => {
    const miss = await replay(
      fixture("todos/tree-basics"),
      new Sabotaged("todos.createTodo", (o) => (o.ok ? { ok: true, result: String(o.result) } : o)),
    );
    expect(miss).toMatch(/반환값이 다르다/);
  });

  it("반환 행의 순서가 바뀌면 잡는다", async () => {
    const miss = await replay(
      fixture("todos/tree-basics"),
      new Sabotaged("todos.listTodos", (o) => (o.ok && Array.isArray(o.result) ? { ok: true, result: [...o.result].reverse() } : o)),
    );
    expect(miss).toMatch(/반환값이 다르다/);
  });

  it("반환 행의 필드 하나가 빠지면 잡는다", async () => {
    const miss = await replay(
      fixture("todos/carry-ghosts"),
      new Sabotaged("todos.listTodos", (o) =>
        o.ok && Array.isArray(o.result)
          ? { ok: true, result: o.result.map(({ gone: _g, ...r }: Record<string, unknown>) => r) }
          : o,
      ),
    );
    expect(miss).toMatch(/gone/);
  });

  it("외부 엔진 계약: 참조 엔진이 프로토콜대로 재생되고, 틀린 시계는 잡힌다", async () => {
    const scenario = {
      name: "contract/vacations",
      seed: [{ sql: "INSERT INTO vacations (date, kind) VALUES ('2026-11-03', 'weird')" }],
      steps: [
        { op: "vacations.listVacations", args: ["2026-11-01", "2026-11-30"] },
        { op: "vacations.setVacation", args: ["2026-11-05", "half"] },
        { op: "vacations.setVacation", args: ["2026-11-05", "annual"] },
        { op: "vacations.getVacation", args: ["2026-11-05"] },
        { op: "vacations.removeVacation", args: ["2026-11-03"] },
        { op: "vacations.listVacations", args: ["2026-11-01", "2026-11-30"] },
      ],
    };
    const golden = await record(scenario, new TsEngine());
    const engine = new ExternalEngine(`node ${join(REPO, "parity/reference-engine.mjs")}`);
    try {
      expect(await replay(golden, engine)).toBeNull();
      const skewed: Fixture = JSON.parse(canonical(golden));
      for (const s of skewed.steps) s.at += 1000;
      expect(await replay(skewed, engine)).toMatch(/DB 변경이 다르다/);
      expect(firstDiff(JSON.parse(readFileSync(MIGRATIONS_FILE, "utf8")), await migrationsBy(engine))).toBeNull();
    } finally {
      engine.dispose();
    }
  });
});
