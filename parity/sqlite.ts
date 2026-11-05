// 동등성 하네스의 바닥: node:sqlite 위에 앱과 같은 DB 를 깐다.
//
// 지금 앱은 tauri-plugin-sql 이 sqlx 로 SQLite 를 연다. 그 동작 중 결과를 바꾸는 것만 그대로 흉내 낸다:
//  - 마이그레이션은 lib.rs 에 등록된 순서대로 (파일만 있고 등록이 없으면 앱도 안 돌린다)
//  - PRAGMA foreign_keys = ON (sqlx 기본값 — CASCADE / SET NULL 이 실제로 돈다)
//  - 바인딩: 숫자는 전부 REAL, 문자열은 TEXT, null 은 NULL, 그 밖(불리언, 배열, 객체)은 JSON 문자열
//    (tauri-plugin-sql 2.4.0 wrapper.rs — `number.as_f64()`, `query.bind(value)`)
//  - BLOB 은 바이트 배열로 돌려준다 (decode/sqlite.rs)

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";

export const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
const MIGRATIONS = join(REPO, "src-tauri/migrations");

export interface MigrationFile {
  version: number;
  description: string;
  file: string;
  sql: string;
}

export function registeredMigrations(): MigrationFile[] {
  const lib = readFileSync(join(REPO, "src-tauri/src/lib.rs"), "utf8");
  const re =
    /version:\s*(\d+)\s*,\s*description:\s*"([^"]*)"\s*,\s*sql:\s*include_str!\(\s*"\.\.\/migrations\/([^"]+)"\s*\)/g;
  return [...lib.matchAll(re)]
    .map((m) => ({
      version: Number(m[1]),
      description: m[2],
      file: m[3],
      sql: readFileSync(join(MIGRATIONS, m[3]), "utf8"),
    }))
    .sort((a, b) => a.version - b.version);
}

/** sqlx 가 `_sqlx_migrations.checksum` 에 남기는 값 — 파일 전체(주석 포함)의 SHA-384 */
export const checksum = (sql: string) => createHash("sha384").update(sql).digest("hex");

/** 엔진의 '지금'. Date.now() 와 SQLite 의 unixepoch('subsec') 이 같은 값을 본다 */
export const clock = { now: 0 };

export function openSqlite(path: string): DatabaseSync {
  const db = new DatabaseSync(path);
  db.exec("PRAGMA foreign_keys = ON");
  // 기본값과 트리거의 시계(unixepoch('subsec'))를 하네스 시계에 묶는다. 앱 정의 함수가 내장 함수보다
  // 먼저 잡힌다. 다른 꼴의 호출은 새 쓰임새라 조용히 진짜 시계를 쓰지 않고 멈춰서 알린다
  db.function("unixepoch", { varargs: true, deterministic: false }, (...a: unknown[]) => {
    if (a.length === 1 && a[0] === "subsec") return clock.now / 1000;
    throw new Error(`parity: unixepoch(${a.map(String).join(", ")}) 는 하네스 시계에 묶이지 않았다`);
  });
  return db;
}

export function migrate(db: DatabaseSync): void {
  for (const m of registeredMigrations()) db.exec(m.sql);
}

const bind = (values: unknown[] = []): SQLInputValue[] =>
  values.map((v) =>
    v === null || v === undefined
      ? null
      : typeof v === "string"
        ? v
        : typeof v === "number"
          ? v // node:sqlite 는 숫자를 REAL 로 묶는다 — plugin-sql 의 as_f64 와 같다
          : JSON.stringify(v),
  );

// plugin-sql 은 sqlx 의 `$1` 을 쓴다. node:sqlite 에선 같은 뜻이 `?1`
const positional = (sql: string) => sql.replace(/\$(\d+)/g, "?$1");

const plain = (row: Record<string, unknown>) => {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(row)) out[k] = v instanceof Uint8Array ? [...v] : v;
  return out;
};

/** 도메인 함수가 getDb() 로 받는 객체. 바닥 연결은 하네스가 픽스처마다 갈아 끼운다 */
export const pluginSql = {
  conn: null as DatabaseSync | null,
  async select<T>(query: string, values?: unknown[]): Promise<T> {
    const rows = live().prepare(positional(query)).all(...bind(values));
    return rows.map((r) => plain(r as Record<string, unknown>)) as T;
  },
  async execute(query: string, values?: unknown[]) {
    const r = live().prepare(positional(query)).run(...bind(values));
    return { rowsAffected: Number(r.changes), lastInsertId: Number(r.lastInsertRowid) };
  },
  async close() {
    return true;
  },
};

function live(): DatabaseSync {
  if (!pluginSql.conn) throw new Error("parity: 열린 DB 가 없다");
  return pluginSql.conn;
}

/** TsEngine 이 open 마다 0 으로 되돌린다 — parity.test.ts 의 ulid 대역이 여기서 번호를 받는다 */
export const ulids = { k: 0 };
export const nextUlid = () => parityUlid(++ulids.k);

/** 엔진이 만드는 k 번째 ULID. Rust 엔진도 같은 줄을 내야 한다(엔진 계약) */
export function parityUlid(k: number): string {
  const ALPHA = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let s = "";
  for (let n = k; s.length < 24; n = Math.floor(n / 32)) s = ALPHA[n % 32] + s;
  return "01" + s;
}
