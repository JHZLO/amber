#!/usr/bin/env node
// 릴리스 도우미 — "지난 태그 이후 커밋"을 읽어 SemVer bump 종류를 스스로 판정한다.
// conventional commits(feat/fix/…)를 지켜온 덕에 커밋 타입만 보면 결정할 수 있다.
//
// 버전 정책
//   0.x  (초기 개발):  feat 또는 BREAKING → minor (0.1.4 → 0.2.0)
//                       그 외(fix/perf/…)  → patch (0.1.4 → 0.1.5)
//                       ※ 1.0.0 승격은 "이제 안정판" 이라는 의도적 결정이라 자동화하지 않음
//   1.0+ (안정판):      BREAKING → major,  feat → minor,  그 외 → patch
//
// 사용법
//   node scripts/release.mjs           분석만 — 추천 버전 출력, 파일은 그대로 (dry-run)
//   node scripts/release.mjs auto      추천대로 4개 파일 버전 갱신
//   node scripts/release.mjs minor     특정 bump 강제 (major|minor|patch)
//   node scripts/release.mjs 0.5.0     정확한 버전 강제
//
// 버전이 박힌 4개 파일을 함께 갱신: package.json · tauri.conf.json · Cargo.toml · Cargo.lock

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const git = (args) => execFileSync("git", args, { cwd: root, encoding: "utf8" });

// ── 현재 버전 ────────────────────────────────────────────────
const current = JSON.parse(
  readFileSync(join(root, "package.json"), "utf8"),
).version;
const [curMajor] = current.split(".").map(Number);

// ── 지난 태그 이후 커밋 수집 ──────────────────────────────────
let lastTag = "";
try {
  lastTag = git(["describe", "--tags", "--abbrev=0"]).trim();
} catch {
  lastTag = ""; // 태그가 하나도 없으면 전체 히스토리 대상
}
const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
// %B = 제목+본문 원문. NUL 로 커밋 구분 (본문에 개행이 있어도 안전)
const messages = git(["log", range, "--format=%B%x00"])
  .split("\0")
  .map((m) => m.trim())
  .filter(Boolean);

// ── 커밋 파싱 → bump 종류 결정 ────────────────────────────────
const HEADER = /^(\w+)(\([^)]*\))?(!)?:/;
const typeCounts = {};
let hasFeat = false;
let hasBreaking = false;
for (const msg of messages) {
  const header = msg.split("\n", 1)[0];
  const m = header.match(HEADER);
  const type = m ? m[1] : "(non-conventional)";
  typeCounts[type] = (typeCounts[type] ?? 0) + 1;
  if (type === "feat") hasFeat = true;
  if ((m && m[3] === "!") || /^BREAKING CHANGE:/m.test(msg)) hasBreaking = true;
}

function recommendBump() {
  if (curMajor === 0) return hasBreaking || hasFeat ? "minor" : "patch";
  if (hasBreaking) return "major";
  if (hasFeat) return "minor";
  return "patch";
}

function bumpTo(kind, base) {
  if (/^\d+\.\d+\.\d+$/.test(kind)) return kind; // 이미 정확한 버전
  const [a, b, c] = base.split(".").map(Number);
  if (kind === "major") return `${a + 1}.0.0`;
  if (kind === "minor") return `${a}.${b + 1}.0`;
  if (kind === "patch") return `${a}.${b}.${c + 1}`;
  throw new Error(`알 수 없는 bump 인자: ${kind} (major|minor|patch|x.y.z 중 하나)`);
}

// ── 분석 출력 ─────────────────────────────────────────────────
const recommended = recommendBump();
const dist =
  Object.entries(typeCounts)
    .sort((x, y) => y[1] - x[1])
    .map(([t, n]) => `${t}:${n}`)
    .join(", ") || "(없음)";

console.log(`현재 버전   ${current}`);
console.log(
  `지난 태그   ${lastTag || "(없음 — 전체 히스토리)"} → 이후 커밋 ${messages.length}개`,
);
console.log(`커밋 타입   ${dist}`);

const reason = hasBreaking
  ? "BREAKING 포함"
  : hasFeat
    ? "feat 포함"
    : "fix/기타만";
console.log(
  `추천 bump   ${recommended} → ${bumpTo(recommended, current)}  (${reason})`,
);

// ── 인자에 따라 dry-run / 적용 ────────────────────────────────
const arg = process.argv[2];

if (!arg) {
  if (messages.length === 0) {
    console.log("\n지난 태그 이후 커밋이 없습니다. 릴리스할 변경이 없어요.");
  } else {
    console.log(
      `\n(dry-run) 적용하려면:  node scripts/release.mjs auto   또는   node scripts/release.mjs ${recommended}`,
    );
  }
  process.exit(0);
}

const next = bumpTo(arg === "auto" ? recommended : arg, current);
if (next === current) {
  console.error(`\n다음 버전이 현재와 같습니다 (${next}). 중단.`);
  process.exit(1);
}

const targets = [
  {
    file: "package.json",
    replace: (s) => s.replace(/"version": "[^"]+"/, `"version": "${next}"`),
  },
  {
    file: "src-tauri/tauri.conf.json",
    replace: (s) => s.replace(/"version": "[^"]+"/, `"version": "${next}"`),
  },
  {
    file: "src-tauri/Cargo.toml",
    replace: (s) => s.replace(/^version = "[^"]+"/m, `version = "${next}"`),
  },
  {
    // Cargo.lock 의 amber 패키지 엔트리만 (다음 cargo 실행에서도 일치하도록)
    file: "src-tauri/Cargo.lock",
    replace: (s) =>
      s.replace(/(name = "amber"\nversion = )"[^"]+"/, `$1"${next}"`),
  },
];

console.log(`\n${current} → ${next} 적용:`);
for (const t of targets) {
  const p = join(root, t.file);
  const before = readFileSync(p, "utf8");
  const after = t.replace(before);
  if (before === after) {
    console.error(`  ✗ 패턴 미일치: ${t.file}`);
    process.exit(1);
  }
  writeFileSync(p, after);
  console.log(`  ✓ ${t.file}`);
}

console.log(
  `\n다음 단계:\n` +
    `  git commit -am "build: Bump version to v${next}"\n` +
    `  git tag -a v${next} -m "Amber v${next}" && git push origin main --tags\n` +
    `  gh release create v${next} --title "Amber v${next}" --generate-notes`,
);
