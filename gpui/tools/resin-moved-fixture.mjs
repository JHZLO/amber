#!/usr/bin/env node
// 끌어 옮긴 카드의 기준 — resin 의 moveCard(캔버스가 카드를 끌 때 부른다)로 카드를 옮겨, 옮긴 뒤의 카드 자리, 연결선, 서비스 영역,
// 그림 크기를 남긴다. svg.rs 는 SvgOptions.moved 로 같은 자리를 주고 그린 그림이 이것과 같은지 본다.
// moveCard 는 아직 npm 판에 없어 resin 저장소의 원본을 읽는다(Node 가 .ts 를 바로 돈다).
//
//   node gpui/tools/resin-moved-fixture.mjs <resin 저장소>
//
// 결과는 gpui/tests/resin_moved.json

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const resin = process.argv[2] ?? new URL("../../../resin", import.meta.url).pathname;
const { compile, moveCard, toSvg } = await import(join(resin, "src/index.ts"));
const ELK = (await import(join(resin, "node_modules/elkjs/lib/elk.bundled.js"))).default;
const elk = new ELK();
const example = (name) => readFileSync(join(resin, "examples", name), "utf8");

const selfRef = "table categories {\n  id  bigint  pk\n  parent_id  bigint?  -> categories  index\n}\ntable items {\n  id  bigint  pk\n  category_id  bigint  -> categories  index\n}\n";
const hub = ["table users {\n  id  bigint  pk\n}\n"]
  .concat(Array.from({ length: 34 }, (_, i) => `table t${String(i).padStart(2, "0")} {\n  id  bigint  pk\n  user_id  bigint  -> users.id\n}\n`))
  .join("\n");

// 옮기기는 차례로: [카드 순번, dx, dy] — 순번은 처음 배치의 boxes 자리
const cases = [
  ["order.erd", example("order.erd"), { direction: "right", look: "aurora-dark", stage: false, idPrefix: "erd-" }, [[1, 0, 420]]],
  ["order.erd", example("order.erd"), { direction: "right", look: "aurora-light", stage: false, edges: "curved", idPrefix: "erd-" }, [[0, 900, 300]]],
  ["order.erd", example("order.erd"), { direction: "down", look: "silk-light", stage: true, idPrefix: "erd-" }, [[2, -400, -600]]],
  ["order.erd", example("order.erd"), { direction: "right", look: "caustic-dark", stage: true, idPrefix: "erd-" }, [[1, 300, 0], [2, -200, 260]]],
  ["shop.erd", example("shop.erd"), { direction: "right", look: "aurora-dark", stage: false, idPrefix: "erd-" }, [[3, 500, 700]]],
  ["shop.erd", example("shop.erd"), { direction: "down", look: "aurora-light", stage: false, edges: "curved", idPrefix: "erd-" }, [[0, 40, 900]]],
  ["composite.erd", example("composite.erd"), { direction: "right", look: "graphite", columns: "keys" }, [[0, 600, 200], [0, -100, 0]]],
  ["self.erd", selfRef, { direction: "down", look: "aurora-dark", stage: false, idPrefix: "erd-" }, [[0, 300, 200]]],
  ["self.erd", selfRef, { direction: "right", look: "aurora-dark", stage: false, idPrefix: "erd-" }, [[1, -300, 300]]],
  ["hub.erd", hub, { direction: "right", look: "aurora-dark", stage: false, idPrefix: "erd-", columns: "none" }, [[0, 2000, 400]]],
];

const out = [];
for (const [file, source, options, moves] of cases) {
  const { model } = compile(source);
  const r = await toSvg(model, elk, options);
  const links = [...r.svg.matchAll(/<g class="rz-r"[^>]*>(.*?)<\/g>/g)].map((m) => m[1]);
  let areas = null;
  let size = [r.width, r.height];
  for (const [i, dx, dy] of moves) {
    const b = r.boxes[i];
    const m = moveCard(r.scene, r.boxes, b.table, b.x + dx, b.y + dy);
    for (const [n, svg] of m.links) links[n] = svg;
    if (m.areas !== null) areas = m.areas;
    size = [m.width, m.height];
  }
  const moved = Object.fromEntries(r.boxes.filter((b, i) => r.scene.boxes[i].x !== b.x || r.scene.boxes[i].y !== b.y).map((b) => [b.table, [b.x, b.y]]));
  out.push({ name: `${file}#${out.length}`, source, options: { ...options, moved }, boxes: r.boxes, links, areas, width: size[0], height: size[1] });
}
writeFileSync(new URL("../tests/resin_moved.json", import.meta.url), JSON.stringify(out) + "\n");
console.log(`${out.length} cases`);
