#!/usr/bin/env node
// vault 의 mermaid 를 전부 모은다 — 노트의 ```mermaid 펜스, 언어 없는 펜스 중 mermaid 선언으로 시작하는 것
// (Markdown.tsx 의 looksLikeMermaid 와 같은 규칙), 다이어그램 탭의 .mmd 파일.
// 실제 노트 내용이라 결과는 로컬 전용 폴더에만 쓴다.
//
//   node gpui/tools/mermaid-corpus.mjs <vault> <out.json>

import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

const [vault, out] = process.argv.slice(2);
if (!vault || !out) {
  console.error("usage: mermaid-corpus.mjs <vault> <out.json>");
  process.exit(1);
}

// src/lib/mdMermaid.ts 의 DECLARATIONS 를 그대로 읽는다 — 두 벌로 갈라지지 않게
const mdMermaid = readFileSync(new URL("../../src/lib/mdMermaid.ts", import.meta.url), "utf8");
const DECLARATIONS = [...mdMermaid.match(/const DECLARATIONS = \[([\s\S]*?)\];/)[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);

function looksLikeMermaid(code) {
  const first = code
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith("%%"));
  if (!first) return false;
  return DECLARATIONS.some((d) => first === d || (first.startsWith(d) && /^[\s:]/.test(first.slice(d.length))));
}

const kind = (src) =>
  src
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith("%%"))
    ?.split(/[\s:]/)[0] ?? "";

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name.startsWith(".")) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else yield p;
  }
}

const corpus = [];
for (const file of walk(vault)) {
  const rel = relative(vault, file);
  if (file.endsWith(".mmd")) {
    const source = readFileSync(file, "utf8");
    corpus.push({ id: `${rel}#0`, file: rel, kind: kind(source), source });
    continue;
  }
  if (!file.endsWith(".md")) continue;
  const lines = readFileSync(file, "utf8").split("\n");
  let n = 0;
  for (let i = 0; i < lines.length; i++) {
    const open = lines[i].match(/^\s*```\s*([\w-]*)\s*$/);
    if (!open) continue;
    let j = i + 1;
    while (j < lines.length && !/^\s*```\s*$/.test(lines[j])) j++;
    const body = lines.slice(i + 1, j).join("\n");
    const lang = open[1];
    if (lang === "mermaid" || (!lang && looksLikeMermaid(body))) {
      corpus.push({ id: `${rel}#${n++}`, file: rel, kind: kind(body), source: body });
    }
    i = j;
  }
}

writeFileSync(out, JSON.stringify(corpus, null, 1));
const byKind = {};
for (const c of corpus) byKind[c.kind] = (byKind[c.kind] ?? 0) + 1;
console.log(`${corpus.length}개`, byKind);
