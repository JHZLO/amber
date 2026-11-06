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
const { compile, toSvg } = await import(join(nm, "@jhzlo/resin/dist/src/index.js"));
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
