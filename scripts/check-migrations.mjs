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
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/migrations");

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
