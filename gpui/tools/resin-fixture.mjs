#!/usr/bin/env node
// resin 이식(D4)의 기준 — amber 가 쓰는 @jhzlo/resin 과 elkjs 로 resin 예제를 그려, ELK 에 넘긴 그래프와 받은 배치,
// 그리고 최종 SVG 를 남긴다. elk.rs 는 같은 그래프로 같은 배치가 나오는지, svg 이식은 같은 SVG 가 나오는지 본다.
//
//   node gpui/tools/resin-fixture.mjs <amber 의 node_modules> <resin/examples>
//
// 결과는 gpui/tests/resin_elk.json

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const nm = process.argv[2] ?? new URL("../../node_modules", import.meta.url).pathname;
const examples = process.argv[3] ?? new URL("../../../resin/examples", import.meta.url).pathname;
const { compile, neighbors, toSvg } = await import(join(nm, "@jhzlo/resin/dist/src/index.js"));
const ELK = (await import(join(nm, "elkjs/lib/elk.bundled.js"))).default;
const elk = new ELK();

// 예제엔 없는 길 — 한 표에 선이 32개 넘게 모이면 ELK 의 highDegreeNodes 처리를 켠다
const hub = ["table users {\n  id  bigint  pk\n}\n"]
  .concat(Array.from({ length: 34 }, (_, i) => `table t${String(i).padStart(2, "0")} {\n  id  bigint  pk\n  user_id  bigint  -> users.id\n}\n`))
  .join("\n");
const sources = readdirSync(examples)
  .filter((f) => f.endsWith(".erd"))
  .sort()
  .map((f) => [f, readFileSync(join(examples, f), "utf8")])
  .concat([["hub.erd", hub]]);

const cases = [];
for (const [file, source] of sources) {
  const { model, diagnostics } = compile(source);
  if (!model) throw new Error(`${file}: ${JSON.stringify(diagnostics)}`);
  for (const direction of ["down", "right"]) {
    const layouts = [];
    const recording = {
      async layout(graph, args) {
        const input = structuredClone(graph);
        const out = await elk.layout(graph, args);
        layouts.push({ graph: input, out: structuredClone(out) });
        return out;
      },
    };
    const options = { direction, stage: false };
    const result = await toSvg(model, recording, options);
    cases.push({ name: `${file}:${direction}`, source, options, layouts, svg: result.svg });
  }
}
writeFileSync(new URL("../tests/resin_elk.json", import.meta.url), JSON.stringify(cases, null, 1) + "\n");
console.log(`${cases.length} cases, ${cases.reduce((n, c) => n + c.layouts.length, 0)} layouts`);

// 컴파일 — resin 자체 시험 파일의 문자열(맞는 것도 틀린 것도)을 전부 넣어 진단과 모델을 남긴다. 어떤 입력이든 같아야 한다.
// 문자열은 TypeScript 로 파일을 읽어 구문 나무에서 꺼낸다 — 정규식이나 토큰만 훑으면 주석 속 백틱, 정규식 리터럴에서 어긋난다
const ts = (await import(join(nm, "typescript/lib/typescript.js"))).default;
const srcDir = join(examples, "../src");
const literals = new Set(sources.map(([, s]) => s));
for (const f of ["lexer", "parser", "checker", "model", "resolution"].map((n) => join(srcDir, `${n}.test.ts`))) {
  const file = ts.createSourceFile(f, readFileSync(f, "utf8"), ts.ScriptTarget.Latest, false);
  const walk = (node) => {
    if ((ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) && node.text.length > 0) literals.add(node.text);
    ts.forEachChild(node, walk);
  };
  walk(file);
}
// 시험 파일에 없는 길 — 복합 FK, 남은 파서 갈래, 감사 목록의 기본 키, UTF-16 자리(이모지 뒤 열)
for (const extra of [
  "table a {\n  x int pk\n  y int pk\n}\ntable b {\n  id int pk\n  ax int\n  ay bigint\n  foreign(ax, ay) -> a(x, y) as fk_ab\n}\n",
  "table a {\n  x int pk\n  y int pk\n}\ntable b {\n  ax int\n  foreign(ax) -> a(x)\n  foreign(ax, ax) -> a(x, y)\n  foreign(ax, ay) -> a(x)\n}\n",
  "table b {\n  ax int\n  foreign(ax, ay) => a(x, y)\n}\n",
  "external table e {\n  id int pk\n  foreign(a, b) -> e(id, id)\n}\n",
  "service s1 {\n  table a {\n    x int pk\n    y int pk\n  }\n}\nservice s2 {\n  table b {\n    ax int\n    ay int\n    foreign(ax, ay) -> s1.a(x, y)\n    foreign(ax, ay) ~> s1.a(x, y)\n    unique(ax, ay)\n  }\n}\n",
  "table a {\n  id int pk\n  name varchar\n} audit envers(id, name)\n",
  "table a {\n  id int pk as nope\n  e varchar enum()\n  t `quoted` \n  u int uk as\n}\ntable b {\n  unique()\n}\n",
  "table a {\n  id int pk \"🙂 emoji\" junk\n  `ok` int\n  name varchar(32, x)\n  ref int -> a.id.x.y\n}\n",
  "table a { id int pk }\n",
  "service s { }\ntable s.a {\n  id int pk\n}\n",
]) literals.add(extra);
const compiled = [...literals].map((source) => {
  const { model, diagnostics } = compile(source);
  // 이웃 — 첫 표에서 한 걸음, 두 걸음
  const near = model && model.tables.length > 1 ? [1, 2].map((steps) => ({ table: model.tables[0].name, steps, model: neighbors(model, model.tables[0].name, steps) })) : [];
  return { source, model, diagnostics, near };
});
writeFileSync(new URL("../tests/resin_compile.json", import.meta.url), JSON.stringify(compiled, null, 1) + "\n");
console.log(`${compiled.length} sources, ${compiled.filter((c) => c.model).length} compiled`);
