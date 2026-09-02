// DB 연동 — 스키마 스냅샷(schemaSnapshot.ts) → mermaid `erDiagram` 원문.
// 형식의 정본은 src-tauri/context/diagram-erd.md(하우스 스타일)이고, 이 모듈은 그 규칙을
// AI 없이 결정적으로 적용하는 생성기다: 같은 스냅샷이면 바이트 단위로 같은 출력이 나온다.
//
// 왜 결정적이어야 하나: 자동 생성 파일은 "DB 가 바뀌었나"를 보는 기준이라, 재생성 결과가
// 흔들리면 스키마 변경과 생성기 노이즈를 구분할 수 없다. 그래서 정렬은 로케일에 좌우되는
// localeCompare 대신 코드 유닛 비교를 쓰고, 테이블 순서는 입력 순서가 아니라 관계선·이름순에서
// 유도한다(컬럼 순서만 스냅샷 순서를 따른다 — 그게 DDL 의 순서다).
//
// 여기서 만드는 표기(타입 뒤 `?` = 널 허용, PK/FK/UK 마커, `#quot;`)는 캔버스(diagramGraph.ts)가
// 그대로 읽으므로 형식을 바꾸면 그쪽도 함께 본다. 코멘트는 번역하지 않고 그대로 옮긴다 —
// 의미를 지어내는 일은 하지 않는다(코멘트가 없으면 설명도 없다).

import type {
  SchemaSnapshot,
  SnapshotColumn,
  SnapshotIndex,
  SnapshotTable,
} from "./schemaSnapshot";

export type ErdLang = "ko" | "en";

export interface ErdGenOptions {
  /** 관계 라벨의 근거 단어·범례 주석의 언어. 테이블/컬럼 코멘트는 번역하지 않는다 */
  lang: ErdLang;
  /** `erDiagram` 바로 다음에 그대로 넣을, 이미 들여쓴 한 줄(formatDbHeader 결과). null/undefined = 없음 */
  header?: string | null;
  /** 그릴 테이블 부분집합. null = 전부. 짝이 되는 `*_aud` 와 `revinfo` 는 있으면 자동으로 딸려온다 */
  tables?: string[] | null;
  /** 감사 테이블(`*_aud`·`revinfo`)을 그리는가. 기본 true — 하우스 스타일은 맨 뒤에 축약해 싣는다.
   *  false 면 도메인 테이블만 남고 `revinfo ||..o{` 관계선도 없다 */
  audit?: boolean | null;
}

export interface RefEdge {
  /** 자식 테이블 — `*_id` 컬럼을 가진 쪽 */
  from: string;
  fromColumn: string;
  /** 부모 테이블 */
  to: string;
  /** 논리 참조는 'id', 물리 FK 는 참조 컬럼 */
  toColumn: string;
  /** true = 스냅샷에 선언된 FOREIGN KEY, false = 이름으로 추론한 논리 참조 */
  physical: boolean;
  /** 자식 컬럼에 단일 컬럼 UNIQUE 인덱스가 있다(또는 key === "UNI") */
  oneToOne: boolean;
  /** 첫 컬럼이 fromColumn 인 인덱스 이름(unique 여부 무관). 없으면 null */
  indexName: string | null;
  /** 물리 FK 의 제약 이름 */
  constraintName: string | null;
}

export interface ErdStats {
  /** 그린 엔티티 블록 수(revinfo·*_aud 포함) */
  tables: number;
  /** 그린 `*_aud` 수 */
  auditTables: number;
  /** 그린 물리 FK 관계선 수 */
  physicalFk: number;
  /** 그린 논리 참조 관계선 수 */
  logicalRefs: number;
  /** 관계를 만들지 못한 `*_id` 컬럼 수(스냅샷 전체 기준) */
  unresolvedRefs: number;
}

export interface ErdGenResult {
  mermaid: string;
  stats: ErdStats;
  unresolved: { table: string; column: string }[];
}

// ---- 언어별 문구 ----
//
// 라벨의 근거 단어와 설명의 참조 문구만 언어를 탄다. 식별자·타입·인덱스 이름은 스키마 그대로.

interface Wording {
  legend: string;
  physical: string;
  physicalUq: string;
  logicalIx: (index: string) => string;
  logicalNoIx: string;
  logicalUq: string;
  /** 설명의 참조 문구 앞머리 — `<refPhysical> -> table.col` */
  refPhysical: string;
  refLogical: string;
  absence: string;
}

const WORDING: Record<ErdLang, Wording> = {
  ko: {
    legend: "    %% 실선(--)=물리 FK(DB 제약) / 점선(..)=논리 참조(앱 레벨, 물리 FK 없음)",
    physical: "물리 FK",
    physicalUq: "uq, 물리 FK",
    logicalIx: (index) => `논리 FK(${index})`,
    logicalNoIx: "논리 FK, DB 제약 없음",
    logicalUq: "uq, 논리",
    refPhysical: "물리 FK",
    refLogical: "논리 FK",
    absence: "DB 제약/인덱스 없음",
  },
  en: {
    legend:
      "    %% solid(--)=physical FK (DB constraint) / dotted(..)=logical reference (app level, no FK constraint)",
    physical: "physical FK",
    physicalUq: "uq, physical FK",
    logicalIx: (index) => `logical FK(${index})`,
    logicalNoIx: "logical FK, no DB constraint",
    logicalUq: "uq, logical",
    refPhysical: "physical FK",
    refLogical: "logical FK",
    absence: "no DB constraint or index",
  },
};

// ---- 이름·문자열 도우미 ----

/** 로케일 무관 정렬 — 출력이 기기·언어 설정에 따라 달라지면 안 된다 */
const cmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

const isAudit = (name: string): boolean => name.toLowerCase().endsWith("_aud");
const isRevinfo = (name: string): boolean => name.toLowerCase() === "revinfo";
/** 도메인 테이블 = 감사 테이블도 revinfo 도 아닌 것. 논리 추론과 접두사 계산은 이들만 본다 */
const isDomain = (name: string): boolean => !isAudit(name) && !isRevinfo(name);

/** 엔티티·속성 이름 — mermaid 가 읽을 수 있는 문자만 남긴다 */
const ent = (name: string): string => name.replace(/[^A-Za-z0-9_-]/g, "_");
/** 타입 — 소문자, 길이·정밀도 없는 물리 타입 한 토큰 */
const typ = (dataType: string): string => dataType.toLowerCase().replace(/[^A-Za-z0-9_]/g, "_");

/** mermaid 는 `\"` 를 모른다 — 따옴표는 `#quot;` 로만 살아남는다 */
const q = (s: string): string => s.replace(/"/g, "#quot;");

/** 코멘트 정리 — 줄바꿈은 공백으로(설명은 한 줄이어야 한다), 양끝 공백 제거 */
const cleanText = (s: string): string => s.replace(/\s*[\r\n]+\s*/g, " ").trim();

const escapeRe = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const isCreatedAt = (c: SnapshotColumn): boolean => c.name.toLowerCase() === "created_at";
const isUpdatedAt = (c: SnapshotColumn): boolean => c.name.toLowerCase() === "updated_at";
const isTimestamp = (c: SnapshotColumn): boolean => isCreatedAt(c) || isUpdatedAt(c);

/** 공통 접두사(e.g. 'ts_')를 뗀 이름. 떼고 나서 빈 문자열이면 원래 이름 */
function stripPrefix(name: string, prefix: string): string {
  if (!prefix || !name.startsWith(prefix) || name.length === prefix.length) return name;
  return name.slice(prefix.length);
}

/**
 * 테이블 이름들의 최장 공통 접두사를 마지막 '_' 직후까지 잘라 돌려준다.
 * ['ts_booking','ts_booking_passenger'] 의 공통 문자열은 'ts_booking' 인데 이건 단어 경계가
 * 아니라서 'ts_' 로 물린다. '_' 가 없거나 이름이 둘 미만이면 ''.
 * 호출자가 도메인 테이블(감사·revinfo 제외)만 넘긴다 — 이 함수는 걸러 주지 않는다.
 */
export function commonTablePrefix(names: string[]): string {
  if (names.length < 2) return "";
  let lcp = names[0];
  for (let i = 1; i < names.length && lcp.length > 0; i++) {
    const n = names[i];
    let k = 0;
    while (k < lcp.length && k < n.length && lcp[k] === n[k]) k++;
    lcp = lcp.slice(0, k);
  }
  const cut = lcp.lastIndexOf("_");
  return cut < 0 ? "" : lcp.slice(0, cut + 1);
}

// ---- 인덱스 조회 ----
//
// PRIMARY 는 마커(PK)로 따로 표기하므로 인덱스 근거로 세지 않는다 — 복합 PK 가 uk(...) 로 둔갑하거나
// 'PK 컬럼에 인덱스가 있다'는 당연한 사실이 설명을 채우는 일을 막는다.

const PRIMARY_RE = /^primary$/i;

function indexesOf(t: SnapshotTable): SnapshotIndex[] {
  return t.indexes.filter((ix) => !PRIMARY_RE.test(ix.name));
}

/** 첫 컬럼이 col 인 인덱스(unique 여부 무관) — 스냅샷 순서에서 첫 번째 */
function leadingIndex(t: SnapshotTable, col: string): SnapshotIndex | null {
  return indexesOf(t).find((ix) => ix.columns[0] === col) ?? null;
}

function leadingNonUniqueIndex(t: SnapshotTable, col: string): SnapshotIndex | null {
  return indexesOf(t).find((ix) => !ix.unique && ix.columns[0] === col) ?? null;
}

function hasSingleUnique(t: SnapshotTable, c: SnapshotColumn): boolean {
  if (c.key === "UNI") return true;
  return indexesOf(t).some((ix) => ix.unique && ix.columns.length === 1 && ix.columns[0] === c.name);
}

/** col 을 포함하는 2컬럼 이상 UNIQUE 인덱스 — 같은 이름은 한 번만 */
function compositeUniques(t: SnapshotTable, col: string): SnapshotIndex[] {
  const seen = new Set<string>();
  const out: SnapshotIndex[] = [];
  for (const ix of indexesOf(t)) {
    if (!ix.unique || ix.columns.length < 2 || !ix.columns.includes(col) || seen.has(ix.name)) continue;
    seen.add(ix.name);
    out.push(ix);
  }
  return out;
}

// ---- 논리 참조 추론 ----

function hasIdPk(t: SnapshotTable | undefined): boolean {
  return !!t && t.columns.some((c) => c.name === "id" && c.key === "PRI");
}

/** `x_id` 의 x 가 가리킬 만한 테이블 이름 후보 — 단수·복수·접두사 유무 */
function candidateNames(x: string, prefix: string): string[] {
  const raw = [x, `${x}s`, `${x}es`, prefix + x, `${prefix}${x}s`, `${prefix}${x}es`];
  if (x.endsWith("y")) {
    const ies = `${x.slice(0, -1)}ies`;
    raw.push(ies, prefix + ies);
  }
  return [...new Set(raw)];
}

/**
 * 스냅샷 전체의 관계 — 선언된 FK(물리) + 이름으로 추론한 참조(논리).
 * 물리 FK 가 걸린 컬럼은 추론하지 않는다(다른 스키마를 가리키는 FK 도 '걸린' 것으로 친다 —
 * 그 컬럼을 이 스키마 안의 테이블로 잇는 건 틀린 추론이다).
 * 후보가 정확히 하나일 때만 잇고, 0 개나 2 개 이상이면 unresolved 로 남긴다 — 추측해서 긋지 않는다.
 */
export function inferReferences(snapshot: SchemaSnapshot): {
  edges: RefEdge[];
  unresolved: { table: string; column: string }[];
} {
  const byName = new Map(snapshot.tables.map((t) => [t.name, t]));
  const prefix = commonTablePrefix(snapshot.tables.map((t) => t.name).filter(isDomain));
  const edges: RefEdge[] = [];
  const unresolved: { table: string; column: string }[] = [];

  for (const t of snapshot.tables) {
    const covered = new Set<string>();
    for (const fk of t.foreign_keys) {
      for (const c of fk.columns) covered.add(c);
      if (fk.columns.length === 0) continue;
      if (fk.ref_schema !== snapshot.schema || !byName.has(fk.ref_table)) continue;
      const fromColumn = fk.columns[0];
      const col = t.columns.find((c) => c.name === fromColumn);
      edges.push({
        from: t.name,
        fromColumn,
        to: fk.ref_table,
        toColumn: fk.ref_columns[0] ?? "id",
        physical: true,
        oneToOne: col ? hasSingleUnique(t, col) : false,
        indexName: leadingIndex(t, fromColumn)?.name ?? null,
        constraintName: fk.name,
      });
    }

    if (!isDomain(t.name)) continue;
    for (const c of t.columns) {
      if (c.key === "PRI" || covered.has(c.name)) continue;
      const m = /^(.+)_id$/.exec(c.name);
      if (!m) continue;
      const cands = candidateNames(m[1], prefix).filter(
        (n) => n !== t.name && isDomain(n) && hasIdPk(byName.get(n)),
      );
      if (cands.length !== 1) {
        unresolved.push({ table: t.name, column: c.name });
        continue;
      }
      edges.push({
        from: t.name,
        fromColumn: c.name,
        to: cands[0],
        toColumn: "id",
        physical: false,
        oneToOne: hasSingleUnique(t, c),
        indexName: leadingIndex(t, c.name)?.name ?? null,
        constraintName: null,
      });
    }
  }

  edges.sort(
    (a, b) =>
      cmp(a.to, b.to) ||
      cmp(a.from, b.from) ||
      cmp(a.fromColumn, b.fromColumn) ||
      cmp(a.constraintName ?? "", b.constraintName ?? ""),
  );
  unresolved.sort((a, b) => cmp(a.table, b.table) || cmp(a.column, b.column));
  return { edges, unresolved };
}

// ---- 생성 ----

interface Ctx {
  w: Wording;
  schema: string;
  prefix: string;
  drawn: Set<string>;
  /** `${from}\u0000${fromColumn}` → 그 컬럼의 관계(정렬 순서상 첫 번째) */
  edgeOf: Map<string, RefEdge>;
}

const edgeKey = (table: string, column: string): string => `${table}\u0000${column}`;

const ENC_RE = /암호화|encrypt|enc\b/i;
const SENSITIVE_NAME_RE = /(name|phone|mobile|email|birth|passport|doc_id|ssn|card_no|address)/i;

/** 암호문을 담을 만큼 넓은 컬럼 — text 또는 varchar(255+) */
function isWideText(c: SnapshotColumn): boolean {
  if (c.data_type.toLowerCase() === "text") return true;
  const m = /^varchar\((\d+)\)/i.exec(c.column_type.trim());
  return !!m && Number(m[1]) >= 255;
}

/** 코멘트가 암호화를 말하거나, 같은 테이블에 그런 컬럼이 있고 이름·폭이 개인정보 컬럼답다 */
function isEncrypted(c: SnapshotColumn, tableHasEncComment: boolean): boolean {
  if (ENC_RE.test(c.comment)) return true;
  return tableHasEncComment && SENSITIVE_NAME_RE.test(c.name) && isWideText(c);
}

/** `'A','B'` 꼴에서 값만 — MySQL 이 CHECK 절에 붙이는 `_utf8mb4'A'` 도 이 방식으로 통과한다 */
function quotedValues(s: string): string[] {
  const out: string[] = [];
  for (const m of s.matchAll(/'((?:[^']|'')*)'/g)) out.push(m[1].replace(/''/g, "'"));
  return out;
}

/** enum 후보 — column_type 의 enum(...) 이 우선, 없으면 같은 테이블 CHECK 의 `col in (...)`. 지어내지 않는다 */
function enumValues(t: SnapshotTable, c: SnapshotColumn): string[] {
  const m = /^enum\((.*)\)$/i.exec(c.column_type.trim());
  if (m) {
    const v = quotedValues(m[1]);
    if (v.length) return v;
  }
  const re = new RegExp(`(?:^|[^A-Za-z0-9_])\`?${escapeRe(c.name)}\`?\\s+in\\s*\\(([^)]*)\\)`, "i");
  for (const k of t.checks) {
    const mm = re.exec(k.clause);
    if (!mm) continue;
    const v = quotedValues(mm[1]);
    if (v.length) return v;
  }
  return [];
}

/** 관계선 라벨의 근거 부분 — 선 모양이 이미 말하는 것(물리/논리·1:1)을 글로 한 번 더 남긴다 */
function evidence(w: Wording, e: RefEdge): string {
  if (e.physical) return e.oneToOne ? w.physicalUq : w.physical;
  if (e.oneToOne) return w.logicalUq;
  return e.indexName ? w.logicalIx(e.indexName) : w.logicalNoIx;
}

function connector(e: RefEdge): string {
  const line = e.physical ? "--" : "..";
  return `||${line}${e.oneToOne ? "o|" : "o{"}`;
}

/** 설명의 참조 사실. 관계가 없어도 선언된 FK 가 있으면(다른 스키마·스냅샷 밖 테이블) 사실은 적는다 */
function referenceFact(ctx: Ctx, t: SnapshotTable, c: SnapshotColumn, edge: RefEdge | null): string | null {
  if (edge) {
    const word = edge.physical ? ctx.w.refPhysical : ctx.w.refLogical;
    return `${word} -> ${ent(stripPrefix(edge.to, ctx.prefix))}.${ent(edge.toColumn)}`;
  }
  const fk = t.foreign_keys.find((f) => f.columns[0] === c.name);
  if (!fk) return null;
  const target =
    fk.ref_schema !== ctx.schema
      ? `${ent(fk.ref_schema)}.${ent(fk.ref_table)}`
      : ent(stripPrefix(fk.ref_table, ctx.prefix));
  return `${ctx.w.refPhysical} -> ${target}.${ent(fk.ref_columns[0] ?? "id")}`;
}

/**
 * 속성 설명 — 하우스 스타일의 순서대로 사실을 모은다.
 * 괄호 항목 `(enc)`·`(index)` 는 앞 사실에 공백으로 붙고("주문자 ID (idx_user_id)"),
 * 나머지 독립된 사실은 `; ` 로 잇는다.
 */
function describe(ctx: Ctx, t: SnapshotTable, c: SnapshotColumn, edge: RefEdge | null, encTable: boolean): string {
  if (isTimestamp(c)) return "";
  const facts: string[] = [];
  const attach = (p: string) => {
    if (facts.length) facts[facts.length - 1] += ` ${p}`;
    else facts.push(p);
  };

  const meaning = cleanText(c.comment);
  if (meaning) facts.push(meaning);
  if (isEncrypted(c, encTable)) attach("(enc)");
  const en = enumValues(t, c);
  if (en.length) facts.push(en.join("/"));
  if (c.key !== "PRI") {
    const ix = leadingNonUniqueIndex(t, c.name);
    if (ix) attach(`(${ix.name})`);
  }
  for (const u of compositeUniques(t, c.name)) facts.push(`uk(${u.columns.map(ent).join(",")})`);
  const ref = referenceFact(ctx, t, c, edge);
  if (ref) facts.push(ref);
  if (edge && !edge.physical && edge.indexName === null && !edge.oneToOne) facts.push(ctx.w.absence);

  return q(facts.join("; "));
}

/** created_at → updated_at 을 맨 뒤로. 나머지는 스냅샷(DDL) 순서 */
function orderColumns(cols: SnapshotColumn[]): SnapshotColumn[] {
  return [
    ...cols.filter((c) => !isTimestamp(c)),
    ...cols.filter(isCreatedAt),
    ...cols.filter(isUpdatedAt),
  ];
}

function entityLines(ctx: Ctx, t: SnapshotTable): string[] {
  const encTable = t.columns.some((c) => ENC_RE.test(c.comment));
  return orderColumns(t.columns).map((c) => {
    const isPk = c.key === "PRI";
    const type = typ(c.data_type) + (c.nullable && !isPk ? "?" : "");
    const edge = ctx.edgeOf.get(edgeKey(t.name, c.name)) ?? null;
    const markers: string[] = [];
    if (isPk) markers.push("PK");
    if (edge && ctx.drawn.has(edge.to)) markers.push("FK");
    if (!isPk && hasSingleUnique(t, c)) markers.push("UK");
    const desc = describe(ctx, t, c, edge, encTable);
    let line = `${type} ${ent(c.name)}`;
    if (markers.length) line += ` ${markers.join(",")}`;
    if (desc) line += ` "${desc}"`;
    return line;
  });
}

/** Envers 감사 테이블은 축약 — 무엇이 버전 관리되는지만. `?`·설명·타임스탬프 없음, id/rev/revtype 이 머리 */
function auditLines(t: SnapshotTable): string[] {
  const head = ["id", "rev", "revtype"];
  const cols = t.columns.filter((c) => !isTimestamp(c));
  const ordered: SnapshotColumn[] = [];
  for (const h of head) {
    const c = cols.find((x) => x.name.toLowerCase() === h);
    if (c) ordered.push(c);
  }
  for (const c of cols) if (!head.includes(c.name.toLowerCase())) ordered.push(c);
  return ordered.map((c) => {
    const markers = c.name.toLowerCase() === "rev" ? "PK,FK" : c.key === "PRI" ? "PK" : "";
    return `${typ(c.data_type)} ${ent(c.name)}${markers ? ` ${markers}` : ""}`;
  });
}

function block(name: string, lines: string[]): string[] {
  return [`    ${ent(name)} {`, ...lines.map((l) => `        ${l}`), "    }"];
}

export function generateErd(snapshot: SchemaSnapshot, opts: ErdGenOptions): ErdGenResult {
  const w = WORDING[opts.lang];
  const byName = new Map(snapshot.tables.map((t) => [t.name, t]));
  const names = snapshot.tables.map((t) => t.name);
  const prefix = commonTablePrefix(names.filter(isDomain));
  const revinfoName = names.find(isRevinfo) ?? null;
  const { edges, unresolved } = inferReferences(snapshot);

  // 그릴 집합 — 부분집합이면 짝 감사 테이블과 revinfo 를 딸려 넣는다
  const drawn = new Set<string>();
  if (opts.tables == null) {
    for (const n of names) drawn.add(n);
  } else {
    for (const n of opts.tables) if (byName.has(n)) drawn.add(n);
    for (const n of [...drawn]) if (byName.has(`${n}_aud`)) drawn.add(`${n}_aud`);
    if (revinfoName && [...drawn].some(isAudit)) drawn.add(revinfoName);
  }
  if (opts.audit === false) {
    for (const n of [...drawn]) if (isAudit(n) || isRevinfo(n)) drawn.delete(n);
  }

  const edgeOf = new Map<string, RefEdge>();
  for (const e of edges) {
    const k = edgeKey(e.from, e.fromColumn);
    if (!edgeOf.has(k)) edgeOf.set(k, e);
  }
  const ctx: Ctx = { w, schema: snapshot.schema, prefix, drawn, edgeOf };

  // 도메인 관계선 — 양 끝이 그려지고, revinfo 가 끼지 않고, 자식이 감사 테이블이 아닌 것
  const domainEdges = edges.filter(
    (e) =>
      drawn.has(e.from) &&
      drawn.has(e.to) &&
      !isRevinfo(e.from) &&
      !isRevinfo(e.to) &&
      !isAudit(e.from),
  );
  const domainLines = domainEdges.map((e) => {
    const child = byName.get(e.from);
    const meaning = cleanText(child?.comment ?? "") || ent(stripPrefix(e.from, prefix));
    return `    ${ent(e.to)} ${connector(e)} ${ent(e.from)} : "${q(`${meaning} · ${evidence(w, e)}`)}"`;
  });

  const drawnAudits = [...drawn].filter(isAudit).sort(cmp);
  const auditRelLines =
    revinfoName && drawn.has(revinfoName)
      ? drawnAudits.map((a) => `    ${ent(revinfoName)} ||..o{ ${ent(a)} : "Envers rev"`)
      : [];

  // 엔티티 순서 — 관계선에 처음 나온 순(부모→자식) → 관계 없는 도메인 테이블 이름순 → revinfo → *_aud
  const order: string[] = [];
  const seen = new Set<string>();
  const add = (n: string) => {
    if (seen.has(n)) return;
    seen.add(n);
    order.push(n);
  };
  for (const e of domainEdges) {
    add(e.to);
    add(e.from);
  }
  for (const n of [...drawn].filter((n) => isDomain(n) && !seen.has(n)).sort(cmp)) add(n);
  if (revinfoName && drawn.has(revinfoName)) add(revinfoName);
  for (const a of drawnAudits) add(a);

  const out: string[] = ["erDiagram"];
  if (opts.header) out.push(opts.header);
  out.push(w.legend, ...domainLines);
  if (auditRelLines.length) out.push("", ...auditRelLines);
  // 블록마다 앞에 빈 줄 하나 — 첫 블록은 관계선과, 그 뒤는 서로 떨어진다
  for (const n of order) {
    const t = byName.get(n);
    if (!t) continue;
    out.push("", ...block(n, isAudit(n) ? auditLines(t) : entityLines(ctx, t)));
  }

  const physicalFk = domainEdges.filter((e) => e.physical).length;
  return {
    mermaid: `${out.join("\n")}\n`,
    stats: {
      tables: order.length,
      auditTables: drawnAudits.length,
      physicalFk,
      logicalRefs: domainEdges.length - physicalFk,
      unresolvedRefs: unresolved.length,
    },
    unresolved,
  };
}
