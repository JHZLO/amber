#!/usr/bin/env node
// line_diff.rs 가 jsdiff 9 의 diffLines 와 같은지 보는 픽스처 — 진짜 jsdiff 로 만든다.
// 같은 Myers 라도 갈림길 고르기가 다르면 결과가 갈린다. 그래서 겹치는 줄이 많은 작은 어휘로 갈림길을 일부러 만든다.
//
//   node gpui/tools/jsdiff-fixture.mjs [node_modules/diff/libesm/index.js 경로]
//
// 결과는 gpui/tests/jsdiff_lines.json (씨앗이 고정이라 늘 같다)

import { writeFileSync } from "node:fs";

const lib = process.argv[2] ?? new URL("../../node_modules/diff/libesm/index.js", import.meta.url).pathname;
const { diffLines } = await import(lib);

// mulberry32 — 씨앗 고정
let seed = 0x5eed1234;
const rand = () => {
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (a) => a[Math.floor(rand() * a.length)];

const VOCAB = ["a", "b", "c", "", "## 1. 제목", "- 항목", "```", "😀 이모지", "  들여쓰기", "a "];
function doc(n) {
  const lines = Array.from({ length: n }, () => pick(VOCAB));
  const eol = rand() < 0.15 ? "\r\n" : "\n";
  let s = lines.join(eol);
  if (rand() < 0.6) s += eol;
  if (rand() < 0.05) s = s.replace("\n", "\r"); // 홀로 선 \r
  return s;
}
function mutate(s) {
  const lines = s.split("\n");
  const n = 1 + Math.floor(rand() * 4);
  for (let i = 0; i < n; i++) {
    const at = Math.floor(rand() * (lines.length + 1));
    const op = rand();
    if (op < 0.35) lines.splice(at, 0, pick(VOCAB));
    else if (op < 0.7) lines.splice(at, 1);
    else lines.splice(at, 1, pick(VOCAB));
  }
  return lines.join("\n");
}

const cases = [
  ["", ""],
  ["", "a\n"],
  ["a\n", ""],
  ["a\nb\nc\n", "a\nb\nc\n"],
  ["a\nb\nc", "a\nb\nc\n"],
  ["a\r\nb\r\n", "a\nb\n"],
  ["a\nb\na\nb\n", "b\na\nb\na\n"],
  ["x\n".repeat(30), "x\n".repeat(29) + "y\n"],
];
for (let i = 0; i < 400; i++) {
  const old = doc(Math.floor(rand() * 14));
  cases.push([old, rand() < 0.2 ? doc(Math.floor(rand() * 14)) : mutate(old)]);
}
const out = cases.map(([o, n]) => ({
  old: o,
  new: n,
  parts: diffLines(o, n).map((p) => ({ added: !!p.added, removed: !!p.removed, count: p.count, value: p.value })),
}));
const dest = new URL("../tests/jsdiff_lines.json", import.meta.url);
writeFileSync(dest, JSON.stringify(out, null, 1) + "\n");
console.log(`${out.length} cases → ${dest.pathname}`);
