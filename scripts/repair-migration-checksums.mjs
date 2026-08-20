#!/usr/bin/env node
// 적용된 마이그레이션의 체크섬 복구 — sqlx 가 검증에 실패해 **모든** 마이그레이션이 멈췄을 때.
//
// 왜 필요한가: sqlx 는 `_sqlx_migrations` 에 각 마이그레이션 SQL 의 SHA-384 를 남기고, 매번
// 적용 전에 파일과 비교한다(validate_applied_migrations). 하나라도 다르면 VersionMismatch 로
// **그 뒤 새 마이그레이션까지 통째로 적용을 포기한다.** 체크섬은 주석까지 포함한 파일 전체라,
// 이미 적용된 파일의 주석 한 줄만 고쳐도 이렇게 된다(실제로 6f1a052 가 0011·0012 주석을 고쳤다).
//
// 더 나쁜 건 조용하다는 점이다: tauri-plugin-sql 은 첫 `load` 에서 마이그레이션 목록을
// map 에서 꺼내 쓰는데(commands::load), 적용이 실패해도 목록은 이미 빠져 있다. lib/db.ts 는
// `_db` 를 캐시하지 못한 채 다음 호출에서 다시 load 하고, 그때는 마이그레이션 없이 연결이
// 성공한다 — 앱은 잘 뜨고 새 컬럼만 없다("no such column: …").
//
// 이 스크립트는 **체크섬만** 고친다. 스키마·데이터는 건드리지 않는다. 그래서 안전한 경우는
// 하나뿐이다: 파일 변경이 주석/공백뿐이라 그 DB의 스키마가 이미 최신인 경우.
// DDL 이 바뀌었다면 체크섬을 맞춰선 안 된다 — 그건 새 마이그레이션으로 처리할 일이다.
//
//   node scripts/repair-migration-checksums.mjs          # 진단만 (변경 없음)
//   node scripts/repair-migration-checksums.mjs --apply  # 백업 뜨고 체크섬 갱신
//
// 앱을 먼저 종료한다(트레이 상주 — 창만 닫으면 안 된다). 갱신 후 다시 띄우면 밀린
// 마이그레이션이 적용된다.

import { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/migrations");
const DEFAULT_DB = join(
  homedir(),
  "Library/Application Support/dev.jhzlo.amber/amber.db",
);

const args = process.argv.slice(2);
const apply = args.includes("--apply");
const dbPath = args.find((a) => !a.startsWith("--")) ?? DEFAULT_DB;

if (!existsSync(dbPath)) {
  console.error(`DB 가 없습니다 — ${dbPath}`);
  process.exit(1);
}

// 파일별 기대 체크섬 (sqlx 와 동일: SQL 텍스트 전체의 SHA-384)
const expected = new Map();
for (const f of readdirSync(DIR).filter((f) => f.endsWith(".sql")).sort()) {
  const version = Number(f.slice(0, 4));
  const sql = readFileSync(join(DIR, f));
  expected.set(version, { file: f, sum: createHash("sha384").update(sql).digest() });
}

const db = new DatabaseSync(dbPath);
const applied = db
  .prepare("SELECT version, description, checksum FROM _sqlx_migrations ORDER BY version")
  .all();

const drift = [];
for (const row of applied) {
  const exp = expected.get(Number(row.version));
  if (!exp) {
    // 파일이 없는데 적용돼 있다 — 파일을 지웠거나 번호를 바꿨다. 체크섬으로 못 고친다.
    console.error(`v${row.version} (${row.description}) 은 적용됐는데 파일이 없습니다`);
    process.exitCode = 1;
    continue;
  }
  if (!Buffer.from(row.checksum).equals(exp.sum)) drift.push({ row, exp });
}

if (!drift.length) {
  console.log(`체크섬 일치 — 적용된 ${applied.length}개 모두 파일과 같습니다 (${dbPath})`);
  db.close();
  process.exit(process.exitCode ?? 0);
}

console.log(`체크섬이 어긋난 마이그레이션 ${drift.length}개 — 이 DB 는 새 마이그레이션을 적용하지 못합니다:`);
for (const { row, exp } of drift) console.log(`  v${row.version}  ${exp.file}`);

if (!apply) {
  console.log("\n파일 변경이 주석/공백뿐인 걸 확인했다면 --apply 로 갱신하세요 (DDL 이 바뀐 거면 안 됩니다).");
  db.close();
  process.exit(1);
}

// 백업 — VACUUM INTO 는 일관된 스냅샷을 뜬다(WAL 이 살아 있어도 안전).
const backup = `${dbPath}.bak-${new Date().toISOString().replace(/[:.]/g, "-")}`;
db.exec(`VACUUM INTO '${backup.replaceAll("'", "''")}'`);
const stmt = db.prepare("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?");
for (const { row, exp } of drift) stmt.run(exp.sum, row.version);
db.close();

console.log(`\n백업: ${backup}`);
console.log(`체크섬 ${drift.length}개 갱신 완료 — 앱을 다시 띄우면 밀린 마이그레이션이 적용됩니다.`);
