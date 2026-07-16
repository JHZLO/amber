#!/usr/bin/env node
// 버전 범프: 버전이 박힌 4개 파일(package.json, tauri.conf.json, Cargo.toml, Cargo.lock)을
// 한 번에 동기화한다. 사용법:
//   node scripts/bump-version.mjs 0.2.0
// 이후 릴리스 절차 (소스 배포 — 바이너리 없음):
//   git commit -am "build: Bump version to v0.2.0"
//   git tag -a v0.2.0 -m "Amber v0.2.0" && git push origin main --tags
//   gh release create v0.2.0 --title "Amber v0.2.0" --generate-notes

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const next = process.argv[2];

if (!/^\d+\.\d+\.\d+$/.test(next ?? "")) {
  console.error("사용법: node scripts/bump-version.mjs <major.minor.patch>  (예: 0.2.0)");
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
    // Cargo.lock 의 amber 패키지 엔트리만 갱신 (다음 cargo 실행에서도 일치)
    file: "src-tauri/Cargo.lock",
    replace: (s) =>
      s.replace(/(name = "amber"\nversion = )"[^"]+"/, `$1"${next}"`),
  },
];

for (const t of targets) {
  const p = join(root, t.file);
  const before = readFileSync(p, "utf8");
  const after = t.replace(before);
  if (before === after) {
    console.error(`변경 없음(패턴 미일치?): ${t.file}`);
    process.exit(1);
  }
  writeFileSync(p, after);
  console.log(`✓ ${t.file} → ${next}`);
}

console.log(
  `\n다음 단계:\n` +
    `  git commit -am "build: Bump version to v${next}"\n` +
    `  git tag -a v${next} -m "Amber v${next}" && git push origin main --tags\n` +
    `  gh release create v${next} --title "Amber v${next}" --generate-notes`,
);
