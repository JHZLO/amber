// DB 상태를 엔진과 무관한 모양으로 떠서 비교한다. 반환값만 보면 '말없이 함께 지워진 행' 같은
// 부수효과를 놓친다 — 실제로 묶음을 지울 때 서랍의 항목이 같이 사라진 적이 있다(todos.ts parkedBoundary).
//
// 행은 선언된 PK 로 짝짓는다(rowid 아님 — 같은 내용을 다른 순서로 넣어도 같은 상태다). PK 가 없는
// 테이블은 행 전체가 열쇠다. REAL 은 {"$real": n} 으로 남긴다: JS 숫자로는 2 와 2.0 이 같아서
// INTEGER 로 넣을 것을 REAL 로 넣는 차이가 안 보인다.

import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { listFiles } from "./fs";

export type Row = Record<string, unknown>;
export type Dump = Record<string, Record<string, Row>>;

export interface TableChanges {
  added: Row[];
  removed: Row[];
  changed: { before: Row; after: Row }[];
}
export type Changes = Record<string, TableChanges>;

const SKIP = new Set(["_sqlx_migrations"]);

const quote = (id: string) => `"${id.replace(/"/g, '""')}"`;

/** dir 를 주면 그 폴더(앱 데이터 폴더)의 파일도 `__files` 표로 뜬다. 파일이 하나도 없으면 표를 넣지 않는다 —
 *  DB 만 쓰는 픽스처의 해시가 그대로 남는다 */
export function dump(db: DatabaseSync, dir?: string): Dump {
  const out: Dump = {};
  const files = dir ? listFiles(dir) : [];
  if (files.length) out.__files = Object.fromEntries(files.map((f) => [canonical([f.path]), f]));
  const tables = db
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .all()
    .map((r) => String(r.name))
    .filter((n) => !SKIP.has(n));
  for (const t of tables) {
    const cols = db.prepare(`PRAGMA table_info(${quote(t)})`).all() as { name: string; pk: number }[];
    const pk = cols.filter((c) => c.pk > 0).sort((a, b) => a.pk - b.pk).map((c) => c.name);
    const sel = cols.map((c) => `${quote(c.name)}, typeof(${quote(c.name)}) AS ${quote("parity_t:" + c.name)}`);
    const rows: Record<string, Row> = {};
    for (const raw of db.prepare(`SELECT ${sel.join(", ")} FROM ${quote(t)}`).all()) {
      const row: Row = {};
      for (const c of cols) {
        const v = raw[c.name];
        row[c.name] =
          raw["parity_t:" + c.name] === "real"
            ? { $real: v }
            : v instanceof Uint8Array
              ? { $blob: Buffer.from(v).toString("hex") }
              : v;
      }
      rows[canonical(pk.length ? pk.map((k) => row[k]) : row)] = row;
    }
    out[t] = rows;
  }
  return out;
}

export function diff(before: Dump, after: Dump): Changes {
  const out: Changes = {};
  for (const t of [...new Set([...Object.keys(before), ...Object.keys(after)])].sort()) {
    const a = before[t] ?? {};
    const b = after[t] ?? {};
    const c: TableChanges = { added: [], removed: [], changed: [] };
    for (const k of Object.keys(b).sort()) {
      if (!(k in a)) c.added.push(b[k]);
      else if (canonical(a[k]) !== canonical(b[k])) c.changed.push({ before: a[k], after: b[k] });
    }
    for (const k of Object.keys(a).sort()) if (!(k in b)) c.removed.push(a[k]);
    if (c.added.length || c.removed.length || c.changed.length) out[t] = c;
  }
  return out;
}

/** 키 순서를 정렬한 JSON — 같은 값이면 같은 문자열. 배열 순서는 의미가 있으므로 그대로 둔다 */
export function canonical(v: unknown): string {
  return JSON.stringify(sortKeys(v));
}

function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === "object") {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(v).sort()) out[k] = sortKeys((v as Record<string, unknown>)[k]);
    return out;
  }
  return Object.is(v, -0) ? 0 : v;
}

export const hashDump = (d: Dump) => createHash("sha256").update(canonical(d)).digest("hex");

/** 두 값이 처음 갈리는 경로와 양쪽 값 — 틀렸을 때 수천 줄 JSON 대신 이 한 줄을 보여 준다 */
export function firstDiff(a: unknown, b: unknown, path = "$"): string | null {
  if (canonical(a) === canonical(b)) return null;
  if (Array.isArray(a) && Array.isArray(b)) {
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      if (i >= a.length) return `${path}[${i}]: 기대에 없음, 실제 ${short(b[i])}`;
      if (i >= b.length) return `${path}[${i}]: 기대 ${short(a[i])}, 실제에 없음`;
      const d = firstDiff(a[i], b[i], `${path}[${i}]`);
      if (d) return d;
    }
  }
  if (a && b && typeof a === "object" && typeof b === "object" && !Array.isArray(a) && !Array.isArray(b)) {
    const ka = a as Record<string, unknown>;
    const kb = b as Record<string, unknown>;
    for (const k of [...new Set([...Object.keys(ka), ...Object.keys(kb)])].sort()) {
      if (!(k in kb)) return `${path}.${k}: 기대 ${short(ka[k])}, 실제에 없음`;
      if (!(k in ka)) return `${path}.${k}: 기대에 없음, 실제 ${short(kb[k])}`;
      const d = firstDiff(ka[k], kb[k], `${path}.${k}`);
      if (d) return d;
    }
  }
  return `${path}: 기대 ${short(a)}, 실제 ${short(b)}`;
}

const short = (v: unknown) => {
  const s = canonical(v) ?? String(v);
  return s.length > 160 ? s.slice(0, 157) + "..." : s;
};
