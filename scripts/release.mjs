#!/usr/bin/env node
// 릴리스 도우미 — "지난 태그 이후 커밋"을 읽어 SemVer bump 종류를 스스로 판정한다.
// conventional commits(feat/fix/…)를 지켜온 덕에 커밋 타입만 보면 결정할 수 있다.
//
// 버전 정책
//   0.x  (초기 개발):  **커밋 타입만으로 정하지 않는다.** 실제 태그 이력이 그 규칙과 다르다 —
//                       기존 탭 안에 기능이 붙는 것은 새 테이블·마이그레이션이 있어도 patch 로 나갔고
//                       (feat(todos) 두 건이 v0.17.4/v0.17.5), minor 는 새 탭·섹션이 열리거나
//                       새 AI 축이 생길 때만 쓴다. 그래서 0.x 에서는 `auto` 를 거부하고
//                       사람이 patch/minor 를 골라 넘기게 한다(추천값은 참고용으로만 출력).
//                       ※ 1.0.0 승격도 "이제 안정판" 이라는 의도적 결정이라 자동화하지 않음
//   1.0+ (안정판):      BREAKING → major,  feat → minor,  그 외 → patch
//
// 사용법
//   node scripts/release.mjs           분석만 — 추천 버전 출력, 파일은 그대로 (dry-run)
//   node scripts/release.mjs patch     bump 종류를 명시 (major|minor|patch)
//   node scripts/release.mjs 0.5.0     정확한 버전 강제
//   node scripts/release.mjs auto      추천대로 (1.0+ 에서만 — 0.x 에서는 거부)
//
// 적용 전 안전장치: 태그 이후 커밋 0개 / main 이 아님 / 워킹트리 더러움 / 검증 실패면 멈춘다.
// 이 스크립트가 유일한 게이트다 — .git/hooks 도 CI 도 없고, .claude/ 는 gitignore 된 로컬 전용이라
// 슬래시 커맨드에 검증을 넣어도 다른 기기·다른 에이전트에는 존재하지 않는다.
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
      curMajor === 0
        ? `\n(dry-run) 적용하려면 bump 를 직접 고르세요:  node scripts/release.mjs patch   |   minor   |   x.y.z\n` +
            `  기존 탭 안의 기능 추가 → patch,  새 탭·섹션·AI 축 → minor  (위 추천은 참고용)`
        : `\n(dry-run) 적용하려면:  node scripts/release.mjs auto   또는   node scripts/release.mjs ${recommended}`,
    );
  }
  process.exit(0);
}

// ── 적용 전 안전장치 ─────────────────────────────────────────
const die = (msg) => {
  console.error(`\n${msg}`);
  process.exit(1);
};

const exact = /^\d+\.\d+\.\d+$/.test(arg);

// 커밋 0개인데 버전을 올리면 코드 없는 버전 커밋 + 공개 태그가 생긴다 (되돌리려면 force-push).
// 정확한 x.y.z 는 재태그·정정용 탈출구로 열어 둔다.
if (messages.length === 0 && !exact) {
  die("지난 태그 이후 커밋이 없습니다. 올릴 변경이 없어요 (정정이면 정확한 x.y.z 를 주세요).");
}

if (curMajor === 0 && arg === "auto") {
  die(
    `0.x 에서는 auto 를 쓰지 않습니다 — 커밋 타입만으로는 판정이 어긋납니다.\n` +
      `  참고 추천: ${recommended}\n` +
      `  기존 탭 안의 기능 추가 → patch,  새 탭·섹션·AI 축 → minor\n` +
      `  예)  node scripts/release.mjs patch`,
  );
}

const branch = git(["rev-parse", "--abbrev-ref", "HEAD"]).trim();
if (branch !== "main") die(`현재 브랜치가 ${branch} 입니다. main 에서만 릴리스합니다.`);

// 뒤따르는 `git commit -am` 이 무관한 편집까지 쓸어담지 않게
const dirty = git(["status", "--porcelain"]).trim();
if (dirty) die(`워킹트리에 커밋되지 않은 변경이 있습니다:\n${dirty}`);

// 태그는 공개된다 — 깨진 상태로 나가면 되돌릴 방법이 태그 삭제뿐이다.
// SKIP_VERIFY=1 로 건너뛸 수 있다(이미 방금 돌린 경우).
if (!process.env.SKIP_VERIFY) {
  const checks = [
    ["pnpm", ["exec", "tsc", "--noEmit"]],
    ["pnpm", ["test:ts"]],
    ["cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml"]],
    // SQL 은 어디서도 컴파일되지 않는다 — 태그 전에 한 번은 실제로 실행해 본다
    ["node", ["scripts/check-migrations.mjs"]],
  ];
  for (const [cmd, args] of checks) {
    console.log(`\n검증  ${cmd} ${args.join(" ")}`);
    try {
      execFileSync(cmd, args, { cwd: root, stdio: "inherit" });
    } catch {
      die(`검증 실패: ${cmd} ${args.join(" ")} — 고친 뒤 다시 실행하세요.`);
    }
  }
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

// GitHub Release 는 만들지 않는다 — 배포 단위는 태그뿐이다(AGENTS.md "배포 정책").
console.log(
  `\n다음 단계:\n` +
    `  git commit -am "build: Bump version to v${next}"\n` +
    `  git tag -a v${next} -m "Amber v${next}" && git push origin main --tags`,
);
