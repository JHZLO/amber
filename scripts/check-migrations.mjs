#!/usr/bin/env node
// 마이그레이션 스모크 체크 — 릴리스 게이트(release.mjs)가 태그 전에 돌린다.
//
// 왜 필요한가: src-tauri/migrations/*.sql 은 lib.rs 가 include_str! 로 문자열로만 가져가므로
// 컴파일 시 파싱되지 않는다. **처음 실행되는 곳이 사용자의 실제 amber.db** 다 — 문법 오류나
// 이미 있는 컬럼을 다시 추가하는 실수는 새 설치에선 안 나고 기존 데이터가 있는 사용자만 터진다.
//
// 왜 vitest 가 아닌가: 이 검사는 node:sqlite / node:fs 가 필요한데, 앱 타입 그래프에는
// @types/node 를 일부러 넣지 않는다(브라우저 코드다 — vite.config.ts 의 @ts-expect-error 참고).
// 타입 체크와 싸우는 대신 릴리스 게이트에 둔다.

import { DatabaseSync } from "node:sqlite";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const DIR = join(HERE, "../src-tauri/migrations");
const LIB = join(HERE, "../src-tauri/src/lib.rs");

const files = readdirSync(DIR)
  .filter((f) => f.endsWith(".sql"))
  .sort(); // 0001_, 0002_ … 접두어가 곧 적용 순서

const fail = (msg) => {
  console.error(`마이그레이션 검사 실패: ${msg}`);
  process.exit(1);
};

// 번호가 1부터 빠짐없이 이어지는지 (건너뛴 번호 = 적용 안 되는 파일)
files.forEach((f, i) => {
  const n = Number(f.slice(0, 4));
  if (n !== i + 1) fail(`번호가 이어지지 않습니다 — ${f} (기대: ${String(i + 1).padStart(4, "0")})`);
});

// **파일을 두는 것만으로는 아무도 실행하지 않는다.** lib.rs 의 `migrations` vec 이
// Migration { version, description, sql: include_str!(...) } 로 하나씩 등록해야 돌아간다.
// 빠뜨리면 증상이 체크섬 드리프트와 똑같다 — 앱은 잘 뜨고 새 컬럼만 없다("no such column: …").
// 실제로 0016 이 등록 없이 태그까지 나갔다(v0.21.0).
const REGISTERED = new Map(
  [
    ...readFileSync(LIB, "utf8").matchAll(
      /version:\s*(\d+)\s*,\s*description:\s*"[^"]*"\s*,\s*sql:\s*include_str!\(\s*"\.\.\/migrations\/([^"]+)"\s*\)/g,
    ),
  ].map((m) => [m[2], Number(m[1])]),
);
for (const f of files) {
  const version = REGISTERED.get(f);
  if (version === undefined) {
    fail(
      `lib.rs 에 등록되지 않았습니다 — ${f}\n` +
        `  src-tauri/src/lib.rs 의 migrations vec 에 Migration { version, description, ` +
        `sql: include_str!("../migrations/${f}"), kind: MigrationKind::Up } 를 더하세요.`,
    );
  }
  const n = Number(f.slice(0, 4));
  if (version !== n) fail(`lib.rs 의 version 이 파일 번호와 다릅니다 — ${f} (version: ${version}, 기대: ${n})`);
}
for (const f of REGISTERED.keys()) {
  if (!files.includes(f)) fail(`lib.rs 가 없는 파일을 등록하고 있습니다 — ${f}`);
}

// **이미 커밋된 마이그레이션 파일은 고치지 않는다 — 주석 한 줄도.**
// sqlx 는 `_sqlx_migrations` 에 파일 전체(주석 포함)의 SHA-384 를 남기고 적용 전마다 비교하는데,
// 하나라도 다르면 VersionMismatch 로 **그 뒤 새 마이그레이션까지 전부 적용을 포기한다.** 게다가
// 조용하다: tauri-plugin-sql 은 첫 load 에서 목록을 map 에서 꺼내므로(commands::load), 실패한
// 뒤 재연결은 마이그레이션 없이 성공한다 — 앱은 잘 뜨고 새 컬럼만 없다("no such column: …").
// 실제로 6f1a052 가 0011·0012 의 주석을 고쳐 그 뒤 0014 가 기존 DB 에 적용되지 않았다.
// 고칠 게 있으면 새 마이그레이션으로, 어긋난 DB 는 scripts/repair-migration-checksums.mjs 로.
const GRANDFATHERED = new Set([
  // 6f1a052 의 주석 수정 — 이미 나간 드리프트라 여기서 막으면 릴리스가 영구히 멈춘다.
  // 기존 DB 는 repair-migration-checksums.mjs 로 맞춘다(스키마는 이미 최신).
  "0011_weekly_reports.sql",
  "0012_todo_scope.sql",
]);
const git = (...args) =>
  execFileSync("git", args, { cwd: DIR, encoding: "utf8" }).trim();
let tracked = true;
try {
  git("rev-parse", "--git-dir");
} catch {
  tracked = false; // 배포 tarball 등 git 밖 — 이 검사만 건너뛴다
}
if (tracked) {
  for (const f of files) {
    if (GRANDFATHERED.has(f)) continue;
    const added = git("log", "--diff-filter=A", "--format=%H", "--", f).split("\n")[0];
    if (!added) continue; // 아직 커밋 안 된 새 파일
    const later = git("log", "--format=%h %s", `${added}..HEAD`, "--", f);
    if (later) fail(`이미 커밋된 마이그레이션이 나중에 수정됐습니다 — ${f}\n  ${later.split("\n").join("\n  ")}`);
  }
}

// 빈 DB 에 순서대로 적용
const db = new DatabaseSync(":memory:");
db.exec("PRAGMA foreign_keys = ON");
for (const f of files) {
  try {
    db.exec(readFileSync(join(DIR, f), "utf8"));
  } catch (e) {
    fail(`${f} — ${e.message}`);
  }
}

// 앱이 실제로 쓰는 테이블이 다 생겼는지
const have = new Set(
  db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all().map((r) => r.name),
);
for (const t of [
  "concepts",
  "tags",
  "concept_tags",
  "settings",
  "ai_invocations",
  "todos",
  "todo_carries",
  "time_blocks",
  "daily_reports",
  "vacations",
]) {
  if (!have.has(t)) fail(`적용 후에도 ${t} 테이블이 없습니다`);
}
db.close();

console.log(`마이그레이션 ${files.length}개 적용 OK`);
