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

// fromSchema — DB 들여다보기가 주는 꼴의 입력을 손으로 지어, 갈래마다 한 번은 지나게 한다
const { fromSchema } = await import(join(nm, "@jhzlo/resin/dist/src/index.js"));
const { parseType } = await import(join(nm, "@jhzlo/resin/dist/src/sql-parser.js"));
const c = (name, type, o = {}) => ({ name, type, nullable: o.nullable ?? false, ...o });
const schemaCases = [
  ["basic", { tables: [
    { name: "users", comment: "People\nwho sign in", columns: [c("id", "bigint"), c("email", "varchar(255)", { comment: 'say "hi" \\ there' }), c("name", "varchar(64)", { nullable: true, encrypted: true })], primaryKey: ["id"], indexes: [{ name: "uk_email", columns: ["email"], unique: true }] },
    { name: "orders", columns: [c("id", "bigint"), c("user_id", "bigint", { nullable: true }), c("status", "varchar(16)", { values: ["PENDING", "PAID", "two words", "", "a`b", "PAID", "7"] }), c("created_at", "datetime(6)")], primaryKey: ["id"], indexes: [{ name: "idx_user", columns: ["user_id"] }, { columns: ["user_id"] }, { name: "idx_s_c", columns: ["status", "created_at"] }, { name: "idx_s_c2", columns: ["status", "created_at"] }], foreignKeys: [{ name: "fk_user", columns: ["user_id"], table: "users" }] },
    { name: "notes", columns: [c("id", "int"), c("order_id", "bigint"), c("author_id", "bigint")], primaryKey: ["id"], foreignKeys: [{ columns: ["order_id"], table: "orders", references: ["id"], kind: "logical" }, { columns: ["author_id"], table: "users", references: ["email"] }] },
  ] }],
  ["composite", { tables: [
    { name: "a", columns: [c("x", "int"), c("y", "int"), c("z", "int")], primaryKey: ["x", "y"], indexes: [{ name: "u_xy", columns: ["y", "x"], unique: true }, { name: "u_x", columns: ["x"], unique: true }, { name: "u_zx", columns: ["z", "x"], unique: true }, { name: "u_xz", columns: ["x", "z"], unique: true }] },
    { name: "b", columns: [c("id", "int"), c("ax", "int"), c("ay", "int", { nullable: true })], primaryKey: ["id"], indexes: [{ name: "u_id", columns: ["id"], unique: true }], foreignKeys: [{ name: "fk_ab", columns: ["ax", "ay"], table: "a", references: ["x", "y"] }, { columns: ["ax", "ay"], table: "a" }, { columns: ["ax", "ax"], table: "a", references: ["x", "y"] }, { columns: ["ax", "ay"], table: "a", references: ["x"] }, { columns: ["ax", "nope"], table: "a", references: ["x", "y"] }, { columns: [], table: "a" }, { columns: ["ax", "ay"], table: "a", references: ["x", "nope"] }] },
  ] }],
  ["types", { tables: [
    { name: "t", columns: [
      c("a", "bigint unsigned"), c("b", "int(11)"), c("c", "decimal(12,2)"), c("d", "enum('A','B','a b')"), c("e", "set('x','y')"), c("f", "character varying(20)"),
      c("g", "double precision"), c("h", "timestamp with time zone"), c("i", "text[]"), c("j", "tinyint(1)"), c("k", "national char varying(10)"), c("l", ""),
      c("m", "public.my_enum"), c("n", "long varchar"), c("o", "bit varying(3)"), c("p", "interval day to second(3)"), c("q", "numeric(*,0)"), c("r", "varchar(max)"),
      c("s", "serial"), c("t2", "BIGSERIAL"), c("u", "time without time zone"), c("v", "integer ARRAY"), c("w", 'enum("q","r")'), c("x", "geometry(Point, 4326)"), c("y", "2xtype"), c("z", "money-ish"),
    ], primaryKey: ["s"] },
  ] }],
  ["names", { tables: [
    { name: "order-items", columns: [c("id", "int"), c("1st", "int"), c("a`b", "int"), c("a_b", "int"), c("", "int"), c("col with space", "int"), c("주문번호", "varchar(20)", { comment: "한글 설명" }), c("ID", "int")], primaryKey: ["id"], indexes: [{ name: "ix`1", columns: ["1st"] }, { name: "ix_1", columns: ["a_b"] }, { name: "ix_1", columns: ["a`b", "id"] }] },
    { name: "a.b", columns: [c("id", "int")], primaryKey: ["id"] },
    { name: "a`b", columns: [c("id", "int")], primaryKey: ["id"] },
    { name: "a_b", columns: [c("id", "int")], primaryKey: ["id"] },
    { name: "주문", columns: [c("아이디", "bigint")], primaryKey: ["아이디"] },
    { name: "order-items", columns: [c("id", "int")] },
  ] }],
  ["schemas", { tables: [
    { name: "users", schema: "Svc_A", columns: [c("id", "bigint")], primaryKey: ["id"] },
    { name: "users", schema: "svc_b", columns: [c("id", "bigint"), c("a_user", "bigint")], primaryKey: ["id"], foreignKeys: [{ columns: ["a_user"], table: "users", schema: "svc_a" }] },
    { name: "orders", schema: "svc_a", columns: [c("id", "bigint"), c("user_id", "bigint")], primaryKey: ["id"], foreignKeys: [{ columns: ["user_id"], table: "users" }] },
    { name: "loose", columns: [c("id", "int"), c("user_id", "bigint")], primaryKey: ["id"], foreignKeys: [{ columns: ["user_id"], table: "users" }] },
    { name: "svc a", schema: "svc a", columns: [c("id", "int")], primaryKey: ["id"] },
  ] }],
  ["external", { tables: [
    { name: "orders", schema: "shop", columns: [c("id", "bigint"), c("user_id", "bigint"), c("buyer_id", "varchar(36)"), c("seller_id", "int"), c("cx", "int"), c("cy", "int"), c("dx", "int"), c("dy", "int")], primaryKey: ["id"], foreignKeys: [
      { columns: ["user_id"], table: "users", schema: "accounts" },
      { columns: ["buyer_id"], table: "users", schema: "accounts", references: ["uuid"] },
      { columns: ["seller_id"], table: "users", schema: "accounts", references: ["uuid"] },
      { name: "fk_c", columns: ["cx", "cy"], table: "pairs", schema: "accounts", references: ["x", "y"] },
      { columns: ["dx", "dy"], table: "pairs", schema: "accounts" },
      { columns: ["dx", "dy"], table: "other-pairs", references: ["p`q", "p_q"] },
      { columns: ["seller_id"], table: "gone" },
    ] },
  ] }],
  ["envers", { tables: [
    { name: "orders", columns: [c("id", "bigint"), c("status", "varchar(8)"), c("memo", "text", { nullable: true }), c("created_at", "datetime")], primaryKey: ["id"] },
    { name: "orders_aud", columns: [c("id", "bigint"), c("rev", "int"), c("revtype", "tinyint"), c("status", "varchar(8)", { nullable: true }), c("memo", "text", { nullable: true })], primaryKey: ["id", "rev"] },
    { name: "items", columns: [c("id", "bigint"), c("qty", "int"), c("price", "int")], primaryKey: ["id"] },
    { name: "items_aud", columns: [c("id", "bigint"), c("REV", "int"), c("REVTYPE", "tinyint"), c("qty", "int")], primaryKey: ["id", "rev"] },
    { name: "nokey", columns: [c("a", "int")] },
    { name: "nokey_aud", columns: [c("a", "int"), c("rev", "int"), c("revtype", "tinyint")] },
    { name: "orphan_aud", columns: [c("id", "bigint"), c("rev", "int"), c("revtype", "tinyint")] },
    { name: "refs", columns: [c("id", "int"), c("order_aud_id", "bigint"), c("x", "int"), c("y", "int")], primaryKey: ["id"], foreignKeys: [{ columns: ["order_aud_id"], table: "orders_aud" }, { columns: ["x", "y"], table: "orders_aud", references: ["id", "rev"] }] },
    { name: "revinfo", columns: [c("rev", "int"), c("revtstmp", "bigint")], primaryKey: ["rev"] },
  ] }],
  ["noRevinfo", { tables: [
    { name: "orders", columns: [c("id", "bigint")], primaryKey: ["id"] },
    { name: "orders_aud", columns: [c("id", "bigint"), c("rev", "int"), c("revtype", "tinyint")], primaryKey: ["id", "rev"] },
  ] }],
  ["dupes", { tables: [
    { name: "t", columns: [c("id", "int"), c("ID", "bigint"), c("x", "int")], primaryKey: ["id", "nope"], indexes: [{ name: "i", columns: ["x", "nope"] }, { name: "u", columns: [], unique: true }] },
    { name: "T", columns: [c("id", "int")] },
    { name: "s", columns: [c("id", "int"), c("t_id", "int")], primaryKey: ["id"], foreignKeys: [{ columns: ["t_id"], table: "t", references: ["nope"] }, { columns: ["t_id"], table: "T" }] },
    { name: "k", columns: [c("a", "int"), c("b", "int"), c("t_id", "int")], primaryKey: ["a", "b"], foreignKeys: [{ columns: ["t_id"], table: "k" }] },
  ] }],
  ["empty", { tables: [] }],
  // MySQL 이 넘긴 빈 이름 — JS 에선 거짓이라 꼬리표가 붙지 않고, 이름표는 `_` 로 바뀐다
  ["emptyNames", { tables: [
    { name: "p", columns: [c("id", "int"), c("q", "int"), c("r", "int")], primaryKey: ["id"], indexes: [{ name: "", columns: ["nope"] }, { name: "", columns: ["q"] }, { name: "", columns: ["q", "r"], unique: true }], foreignKeys: [{ name: "", columns: ["q", "r"], table: "p", references: ["id", "q"] }] },
  ] }],
];
const fixtures = schemaCases.map(([name, input]) => ({ name, input, output: fromSchema(input) }));
fixtures[0].types = [
  "varchar(255)", "bigint unsigned", "int(11)", "decimal(12,2)", "enum('A','B','it''s')", "set('x','y')", "character varying(20)", "double precision",
  "timestamp with time zone", "timestamp with local time zone", "text[]", "tinyint(1)", "national char varying(10)", "national character(3)", "", "public.my_enum",
  "long varchar", "long raw", "bit varying(3)", "interval day to second(3)", "numeric(*,0)", "varchar(max)", "serial", "BIGSERIAL", "integer ARRAY", 'enum("q","r")',
  "geometry(Point, 4326)", "2xtype", "money-ish", "decimal(10, 2) zerofill", "float(7,4) unsigned", "nchar varying(5)", "char(36)", "json", "datetime(6)",
  "varchar(20) character set utf8mb4", "int[][]", "_text", "number(12)", "enum(E'a\\'b', N'c')", "x(1e3, .5, 7)", "a.b.c(1)", "-- comment\nint", "/* c */ bigint",
  "varchar('a'", "`quoted` type", "[nvarchar](10)", "$$x$$", "time(3) with time zone", "tinyint(1) unsigned zerofill", "year(4)", "bit(1)", "a b c",
].map((text) => ({ text, type: parseType(text) }));
writeFileSync(new URL("../tests/resin_from_schema.json", import.meta.url), JSON.stringify(fixtures, null, 1) + "\n");
console.log(`${fixtures.length} schemas, ${fixtures[0].types.length} types`);
