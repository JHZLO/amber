// 녹화와 재생. 시나리오를 TS 엔진으로 돌려 픽스처(정답)를 만들고, 어떤 엔진이든 같은 픽스처를
// 재생해 단계마다 ① 성공/실패 ② 반환값 ③ DB 변경 을 정확히 대조한다. 첫 어긋남에서 멈춘다 —
// 그 뒤 단계는 이미 다른 상태 위에서 돌아 의미가 없다.

import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { Engine, Outcome } from "./engine";
import { canonical, diff, dump, firstDiff, hashDump, type Changes } from "./state";
import { checksum, clock, migrate, openSqlite, REPO, registeredMigrations } from "./sqlite";

export interface SeedSql {
  sql: string;
  params?: (string | number | null)[];
}

/** 앱 데이터 폴더에 깔 파일. path 가 '/' 로 끝나면 빈 폴더 */
export interface SeedFile {
  path: string;
  text?: string;
}

export type SeedItem = SeedSql | SeedFile;

/** 시드 파일은 저장소 기준 상대경로 — 실데이터 사본은 .claude/ 아래(로컬 전용)에 둔다 */
export type Seed = SeedItem[] | { file: string };

export interface StepCtx {
  /** 하네스 연결로 현재 상태를 읽는다 — 다음 단계의 인자를 고를 때만 쓴다 */
  peek: <T = Record<string, unknown>>(sql: string, params?: (string | number | null)[]) => T[];
  rng: () => number;
  last: { op: string; outcome: Outcome } | null;
  index: number;
}

export interface Step {
  op: string;
  args: unknown[] | ((ctx: StepCtx) => unknown[]);
  /** 절대 시각(ms). 없으면 앞 단계 + 1초 — 1초 단위라 unixepoch('subsec') * 1000 이 정확히 떨어진다 */
  at?: number;
}

export interface Scenario {
  name: string;
  tz?: string;
  start?: number;
  seed?: Seed;
  steps?: Step[];
  /** 시작 상태를 보고 단계 목록을 한 번에 짠다 — 실데이터 사본의 날짜, id 를 훑을 때 */
  plan?: (ctx: StepCtx) => Step[];
  /** 상태를 보며 단계를 고르는 무작위 걸음. 한 번에 여러 단계(화면의 한 동작)를 낼 수 있다 */
  walk?: { seed: number; length: number; next: () => (ctx: StepCtx) => Step[] };
}

export interface RecordedStep {
  op: string;
  args: unknown[];
  at: number;
  ok: boolean;
  result?: unknown;
  error?: string;
  changes: Changes;
}

export interface Fixture {
  format: 1;
  name: string;
  tz: string;
  start: number;
  seed: SeedItem[] | { file: string; sha256: string };
  steps: RecordedStep[];
  /** 마지막 전체 상태의 해시 — 단계별 변경이 다 맞았는데 시작 상태가 달랐던 경우까지 잡는다 */
  final: string;
}

export const DEFAULT_TZ = "Asia/Seoul";
/** 2026-11-05 09:00 KST */
export const DEFAULT_START = Date.UTC(2026, 10, 5, 0, 0, 0);

export const FIXTURES = join(REPO, "parity/fixtures");
export const REAL_FIXTURES = join(REPO, ".claude/parity/real");

const sha256File = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");

/** 임시 폴더에 픽스처의 시작 상태를 깐다. 엔진과 하네스는 이 파일 하나를 함께 본다 */
function prepare(seed: Seed | Fixture["seed"], start: number): { path: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), "amber-parity-"));
  const path = join(dir, "amber.db");
  if (Array.isArray(seed)) {
    clock.now = start;
    const db = openSqlite(path);
    migrate(db);
    for (const s of seed) {
      if ("sql" in s) db.prepare(s.sql).run(...(s.params ?? []));
      else if (s.path.endsWith("/")) mkdirSync(join(dir, s.path), { recursive: true });
      else {
        mkdirSync(dirname(join(dir, s.path)), { recursive: true });
        writeFileSync(join(dir, s.path), s.text ?? "");
      }
    }
    db.close();
  } else {
    copyFileSync(join(REPO, seed.file), path);
  }
  return { path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

function mulberry32(a: number) {
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export async function record(s: Scenario, engine: Engine): Promise<Fixture> {
  const tz = s.tz ?? DEFAULT_TZ;
  const start = s.start ?? DEFAULT_START;
  const seed = s.seed ?? [];
  const { path, cleanup } = prepare(seed, start);
  const observer = new DatabaseSync(path);
  const rng = mulberry32(s.walk?.seed ?? 1);
  // 걸음이 품은 상태(대기 중인 후속 단계, 번호)는 녹화마다 새로 — 두 번 녹화해 같은지 보는 데 필요하다
  const next = s.walk?.next();
  const steps: RecordedStep[] = [];
  let opened = false;
  try {
    await engine.open(path, tz);
    opened = true;
    let before = dump(observer, dirname(path));
    let at = start - 1000;
    let last: StepCtx["last"] = null;
    const ctx = (): StepCtx => ({
      peek: (sql, params = []) => observer.prepare(sql).all(...params) as never[],
      rng,
      last,
      index: steps.length,
    });
    const queue: Step[] = [...(s.steps ?? []), ...(s.plan?.(ctx()) ?? [])];
    while (queue.length || (s.walk && steps.length < s.walk.length)) {
      if (!queue.length) queue.push(...next!(ctx()));
      const step = queue.shift()!;
      at = step.at ?? at + 1000;
      const args = typeof step.args === "function" ? step.args(ctx()) : step.args;
      const outcome = await engine.run(step.op, args, at);
      const after = dump(observer, dirname(path));
      steps.push({
        op: step.op,
        args,
        at,
        ok: outcome.ok,
        ...(outcome.ok ? { result: outcome.result } : { error: outcome.error }),
        changes: diff(before, after),
      });
      before = after;
      last = { op: step.op, outcome };
    }
    return {
      format: 1,
      name: s.name,
      tz,
      start,
      seed: Array.isArray(seed) ? seed : { file: seed.file, sha256: sha256File(join(REPO, seed.file)) },
      steps,
      final: hashDump(dump(observer, dirname(path))),
    };
  } finally {
    if (opened) await engine.close();
    observer.close();
    cleanup();
  }
}

/** 어긋남 한 줄. 없으면 null */
export async function replay(f: Fixture, engine: Engine): Promise<string | null> {
  if (!Array.isArray(f.seed)) {
    const now = sha256File(join(REPO, f.seed.file));
    if (now !== f.seed.sha256) return `시드 파일이 녹화 때와 다르다(${f.seed.file}) — 다시 녹화한다`;
  }
  const { path, cleanup } = prepare(f.seed, f.start);
  const observer = new DatabaseSync(path);
  let opened = false;
  try {
    await engine.open(path, f.tz);
    // 어긋나서 중간에 나가도 닫는다 — 외부 엔진은 다음 픽스처에서 같은 프로세스를 다시 연다
    opened = true;
    let before = dump(observer, dirname(path));
    for (const [i, s] of f.steps.entries()) {
      const outcome = await engine.run(s.op, s.args, s.at);
      const after = dump(observer, dirname(path));
      const where = `${i + 1}번째 단계 ${s.op}(${short(s.args)})`;
      if (outcome.ok !== s.ok) {
        return `${where}: 기대 ${s.ok ? "성공" : `실패(${s.error})`}, 실제 ${outcome.ok ? "성공" : `실패(${outcome.error})`}`;
      }
      if (outcome.ok) {
        const d = firstDiff(s.result ?? null, outcome.result ?? null);
        if (d) return `${where}: 반환값이 다르다 — ${d}`;
      }
      const d = firstDiff(s.changes, diff(before, after));
      if (d) return `${where}: DB 변경이 다르다 — ${d}`;
      before = after;
    }
    const final = hashDump(dump(observer, dirname(path)));
    return final === f.final ? null : `마지막 상태 해시가 다르다 (${f.final.slice(0, 12)} ≠ ${final.slice(0, 12)})`;
  } finally {
    if (opened) await engine.close();
    observer.close();
    cleanup();
  }
}

const short = (v: unknown) => {
  const s = canonical(v).slice(1, -1);
  return s.length > 80 ? s.slice(0, 77) + "..." : s;
};

// ---- 픽스처 파일 ----

/** 단계는 한 줄에 하나 — 다시 녹화했을 때 git diff 가 '어느 단계의 무엇이 바뀌었나'로 읽힌다 */
export function writeFixture(dir: string, f: Fixture) {
  const { steps, ...head } = f;
  const body = JSON.stringify(head, null, 2).replace(
    /\n}$/,
    `,\n  "steps": [\n${steps.map((s) => "    " + JSON.stringify(s)).join(",\n")}\n  ]\n}\n`,
  );
  writeFileSync(join(dir, `${f.name.replace(/\//g, "__")}.json`), body);
}

export function readFixtures(dir: string): Fixture[] {
  let names: string[];
  try {
    names = readdirSync(dir).filter((n) => n.endsWith(".json") && !n.startsWith("_"));
  } catch {
    return [];
  }
  return names.sort().map((n) => JSON.parse(readFileSync(join(dir, n), "utf8")) as Fixture);
}

// ---- 마이그레이션 ----

export interface MigrationGolden {
  /** sqlx 가 `_sqlx_migrations` 에 남길 (version, checksum). 기존 amber.db 를 새 앱이 열 수 있는 조건 */
  ledger: { version: number; checksum: string }[];
  schema: { type: string; name: string; sql: string }[];
}

export function schemaOf(db: DatabaseSync): MigrationGolden["schema"] {
  return db
    .prepare(
      `SELECT type, name, sql FROM sqlite_master
        WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'
        ORDER BY type, name`,
    )
    .all()
    .map((r) => ({ type: String(r.type), name: String(r.name), sql: String(r.sql).replace(/\s+/g, " ").trim() }));
}

export function migrationGolden(): MigrationGolden {
  clock.now = DEFAULT_START;
  const db = openSqlite(":memory:");
  migrate(db);
  const schema = schemaOf(db);
  db.close();
  return { ledger: registeredMigrations().map((m) => ({ version: m.version, checksum: checksum(m.sql) })), schema };
}

/** 다른 엔진이 빈 파일에 깐 결과 — sqlx 가 남긴 장부와 스키마 */
export async function migrationsBy(engine: Engine): Promise<MigrationGolden> {
  const dir = mkdtempSync(join(tmpdir(), "amber-parity-mig-"));
  const path = join(dir, "fresh.db");
  try {
    await engine.migrate!(path);
    const db = new DatabaseSync(path);
    const ledger = db
      .prepare("SELECT version, hex(checksum) AS checksum FROM _sqlx_migrations ORDER BY version")
      .all()
      .map((r) => ({ version: Number(r.version), checksum: String(r.checksum).toLowerCase() }));
    const schema = schemaOf(db);
    db.close();
    return { ledger, schema };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** 실제 amber.db 사본의 장부 — 새 앱이 그대로 열려면 golden 장부와 같아야 한다 */
export function ledgerOf(file: string): MigrationGolden["ledger"] {
  const db = new DatabaseSync(file, { readOnly: true });
  try {
    return db
      .prepare("SELECT version, hex(checksum) AS checksum FROM _sqlx_migrations ORDER BY version")
      .all()
      .map((r) => ({ version: Number(r.version), checksum: String(r.checksum).toLowerCase() }));
  } finally {
    db.close();
  }
}
