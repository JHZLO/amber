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

/** 생성 규칙 버전. 같은 스냅샷에서 다른 출력이 나오게 규칙을 고칠 때마다 올린다 — 헤더에 실려 있어
 *  예전 규칙으로 만든 파일에 "생성 규칙이 새로워졌어요" 배너가 뜬다(사용자가 다시 생성을 고른다).
 *  2: 코멘트 참조·접미사 후보·타입 호환·관계선 병합·중복 설명 정리·인코딩 복원 (2026-09) */
export const ERD_GEN_VERSION = 2;

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
  /** 어떻게 알았나 — 선언된 FK / 컬럼 코멘트의 `table.column` / 컬럼 이름 규칙 */
  source: "fk" | "comment" | "name";
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
  /** 코멘트가 대상 테이블을 적어 둔 덕에 찾은 참조 — 인덱스도 제약도 없을 때의 근거 */
  logicalComment: string;
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
    logicalComment: "논리 FK(코멘트)",
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
    logicalComment: "logical FK(comment)",
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

/** 잘못된 문자셋으로 저장된 코멘트("ì—¬í–‰")를 되살린다 — UTF-8 바이트를 latin1 로 읽은 꼴이라, 모든 글자가
 *  0xFF 이하이고 그 바이트열이 온전한 UTF-8 이면 되돌릴 수 있다. 한 글자라도 그 밖이면 진짜 텍스트로 두고 손대지 않는다. */
function fixMojibake(s: string): string {
  // UTF-8 선두 바이트(C2~F4)가 라틴 글자로 보이지 않으면 깨진 게 아니다
  if (!/[\u00C2-\u00F4]/.test(s)) return s;
  const bytes = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) {
    let code = s.charCodeAt(i);
    if (code > 0xff) {
      // windows-1252 로 읽힌 0x80~0x9F 구간(€ ‚ ƒ … ˆ ‰ ™ 등)은 원래 바이트로 되돌린다
      const b = CP1252_REVERSE.get(code);
      if (b === undefined) return s;
      code = b;
    }
    bytes[i] = code;
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return s;
  }
}

/** windows-1252 가 0x80~0x9F 에 배정한 글자 → 바이트. 정의되지 않은 자리(0x81·0x8D·0x8F·0x90·0x9D)는 제어문자로 남아 그대로 통과한다 */
const CP1252_REVERSE = new Map<number, number>([
  [0x20ac, 0x80], [0x201a, 0x82], [0x0192, 0x83], [0x201e, 0x84], [0x2026, 0x85], [0x2020, 0x86],
  [0x2021, 0x87], [0x02c6, 0x88], [0x2030, 0x89], [0x0160, 0x8a], [0x2039, 0x8b], [0x0152, 0x8c],
  [0x017d, 0x8e], [0x2018, 0x91], [0x2019, 0x92], [0x201c, 0x93], [0x201d, 0x94], [0x2022, 0x95],
  [0x2013, 0x96], [0x2014, 0x97], [0x02dc, 0x98], [0x2122, 0x99], [0x0161, 0x9a], [0x203a, 0x9b],
  [0x0153, 0x9c], [0x017e, 0x9e], [0x0178, 0x9f],
]);

/** 코멘트 정리 — 깨진 인코딩 복원, 줄바꿈은 공백으로(설명은 한 줄이어야 한다), 양끝 공백 제거 */
const cleanText = (s: string): string => fixMojibake(s).replace(/\s*[\r\n]+\s*/g, " ").trim();

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
  if (cut >= 0) return lcp.slice(0, cut + 1);
  // 공통 문자열이 없어도 대다수가 같은 첫 토큰을 쓰면 그게 접두사다 — `editing_logs` 하나가 끼어 있다고
  // `ts_` 마흔 개가 접두사를 잃으면 `device_id` 가 `ts_devices` 를 못 찾는다(실측).
  const counts = new Map<string, number>();
  for (const n of names) {
    const i = n.indexOf("_");
    if (i > 0) counts.set(n.slice(0, i + 1), (counts.get(n.slice(0, i + 1)) ?? 0) + 1);
  }
  let best = "";
  let bestN = 0;
  for (const [tok, c] of counts) if (c > bestN || (c === bestN && cmp(tok, best) < 0)) [best, bestN] = [tok, c];
  return bestN >= 2 && bestN * 5 >= names.length * 3 ? best : "";
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

/** col 이 첫 컬럼인 2컬럼 이상 UNIQUE 인덱스 — 복합 uk 는 첫 컬럼에만 적는다(컬럼마다 반복하면 설명이 uk 로 도배된다) */
function compositeUniques(t: SnapshotTable, col: string): SnapshotIndex[] {
  const seen = new Set<string>();
  const out: SnapshotIndex[] = [];
  for (const ix of indexesOf(t)) {
    if (!ix.unique || ix.columns.length < 2 || ix.columns[0] !== col || seen.has(ix.name)) continue;
    seen.add(ix.name);
    out.push(ix);
  }
  return out;
}

// ---- 타입 호환 ----
//
// `travel_id varchar` 가 `ts_travels.id bigint` 를 가리킬 수는 없다 — 그때는 같은 이름의 UNIQUE 컬럼(travel_id)이 대상이다.

const INT_TYPES = new Set(["tinyint", "smallint", "mediumint", "int", "integer", "bigint"]);
const CHAR_TYPES = new Set(["char", "varchar", "text", "tinytext", "mediumtext", "longtext"]);

function typesCompatible(a: string, b: string): boolean {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  if (x === y) return true;
  if (INT_TYPES.has(x) && INT_TYPES.has(y)) return true;
  return CHAR_TYPES.has(x) && CHAR_TYPES.has(y);
}

/** 후보 테이블 T 에서 자식 컬럼 c 가 가리킬 컬럼 — 타입이 맞는 `id` PK, 아니면 같은 이름의 UNIQUE/PK 컬럼 */
function targetColumn(T: SnapshotTable, c: SnapshotColumn): string | null {
  const id = T.columns.find((col) => col.name === "id" && col.key === "PRI");
  if (id && typesCompatible(id.data_type, c.data_type)) return "id";
  const same = T.columns.find(
    (col) => col.name === c.name && (col.key === "PRI" || col.key === "UNI" || hasSingleUnique(T, col)),
  );
  if (same && typesCompatible(same.data_type, c.data_type)) return same.name;
  return null;
}

// ---- 코멘트 속 참조 ----
//
// "ts_customers.id - 유저 ID", "FK to ts_organizations.id" 처럼 작성자가 대상을 적어 둔 코멘트가 많다.
// 앞뒤에 점이 없는 `table.column` 한 쌍만 읽는다 — `svc_inventory.ts_packages.id` 같은 다른 스키마 참조는
// 세 토막이라 걸리지 않는다(그건 설명으로만 남는다).
const COMMENT_REF_RE = /(?<![\w.])([A-Za-z]\w*)\.([A-Za-z_]\w*)(?![\w.])/g;

function commentRefs(
  comment: string,
  byName: Map<string, SnapshotTable>,
  self: string,
): { table: string; column: string }[] {
  const out = new Map<string, { table: string; column: string }>();
  for (const m of comment.matchAll(COMMENT_REF_RE)) {
    const T = byName.get(m[1]);
    if (!T || m[1] === self || !isDomain(m[1])) continue;
    if (!T.columns.some((c) => c.name === m[2])) continue;
    out.set(`${m[1]}.${m[2]}`, { table: m[1], column: m[2] });
  }
  return [...out.values()];
}

// ---- 논리 참조 추론 ----

/** `x_id` 의 x 가 가리킬 만한 테이블 이름 후보 — 단수·복수·접두사 유무 */
function candidateNames(x: string, prefix: string): string[] {
  const raw = [x, `${x}s`, `${x}es`, prefix + x, `${prefix}${x}s`, `${prefix}${x}es`];
  if (x.endsWith("y")) {
    const ies = `${x.slice(0, -1)}ies`;
    raw.push(ies, prefix + ies);
  }
  return [...new Set(raw)];
}

/** 이름이 `_x`·`_xs`… 로 끝나는 테이블 — `profile_id` → `ts_user_profiles`, `policy_set_id` → `ts_authorization_policy_sets`.
 *  정확한 이름이 안 맞을 때의 2차 후보. 둘 이상 걸리면 추측하지 않는다(`share_id` 가 like_share 둘에 걸리듯). */
function suffixCandidates(x: string, names: string[]): string[] {
  const forms = [x, `${x}s`, `${x}es`];
  if (x.endsWith("y")) forms.push(`${x.slice(0, -1)}ies`);
  return names.filter((n) => forms.some((f) => n.endsWith(`_${f}`)));
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
        source: "fk",
      });
    }

    if (!isDomain(t.name)) continue;
    const domainNames = snapshot.tables.map((x) => x.name).filter((n) => n !== t.name && isDomain(n));
    for (const c of t.columns) {
      if (c.key === "PRI" || covered.has(c.name)) continue;
      const logical = (to: string, toColumn: string, source: "comment" | "name"): RefEdge => ({
        from: t.name,
        fromColumn: c.name,
        to,
        toColumn,
        physical: false,
        oneToOne: hasSingleUnique(t, c),
        indexName: leadingIndex(t, c.name)?.name ?? null,
        constraintName: null,
        source,
      });

      // 1) 코멘트가 대상을 적어 뒀으면 그게 가장 정확하다 — 이름 규칙으로는 못 잇는 user_id → ts_customers 도 풀린다
      const refs = commentRefs(c.comment, byName, t.name).filter((r) => {
        const target = byName.get(r.table)!.columns.find((col) => col.name === r.column)!;
        return typesCompatible(target.data_type, c.data_type);
      });
      if (refs.length === 1) {
        edges.push(logical(refs[0].table, refs[0].column, "comment"));
        continue;
      }

      // 2) `x_id` 이름 규칙 — 정확한 이름(단복수·접두사) 후보가 하나면 그것, 아니면 `_x` 로 끝나는 테이블이 하나일 때
      const m = /^(.+)_id$/.exec(c.name);
      if (!m) continue;
      const resolve = (names: string[]) =>
        names
          .map((n) => ({ n, col: targetColumn(byName.get(n)!, c) }))
          .filter((h): h is { n: string; col: string } => h.col !== null);
      const exact = resolve(candidateNames(m[1], prefix).filter((n) => domainNames.includes(n)));
      const hits = exact.length === 1 ? exact : resolve(suffixCandidates(m[1], domainNames));
      if (hits.length !== 1) {
        unresolved.push({ table: t.name, column: c.name });
        continue;
      }
      edges.push(logical(hits[0].n, hits[0].col, "name"));
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
const TRIVIAL_ID_RE = /^(id|pk|primary key|고유\s*식별자|식별자|아이디)$/i;
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
  if (e.indexName) return w.logicalIx(e.indexName);
  return e.source === "comment" ? w.logicalComment : w.logicalNoIx;
}

function connector(e: RefEdge): string {
  const line = e.physical ? "--" : "..";
  return `||${line}${e.oneToOne ? "o|" : "o{"}`;
}

/** 설명의 참조 사실. 관계가 없어도 선언된 FK 가 있으면(다른 스키마·스냅샷 밖 테이블) 사실은 적는다 */
function referenceFact(ctx: Ctx, t: SnapshotTable, c: SnapshotColumn, edge: RefEdge | null): string | null {
  // 코멘트가 이미 대상 테이블을 말하면("FK to ts_customers.id") 같은 말을 되풀이하지 않는다
  const said = (table: string) => cleanText(c.comment).includes(table);
  if (edge) {
    if (said(edge.to)) return null;
    const word = edge.physical ? ctx.w.refPhysical : ctx.w.refLogical;
    return `${word} -> ${ent(stripPrefix(edge.to, ctx.prefix))}.${ent(edge.toColumn)}`;
  }
  const fk = t.foreign_keys.find((f) => f.columns[0] === c.name);
  if (!fk || said(fk.ref_table)) return null;
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

  let meaning = cleanText(c.comment);
  // `id PK "ID"` — 이름이 이미 말하는 걸 코멘트가 되풀이한 것. 하우스 규칙(id 엔 설명 없음)대로 비운다
  if (c.key === "PRI" && c.name.toLowerCase() === "id" && TRIVIAL_ID_RE.test(meaning)) meaning = "";
  if (meaning) facts.push(meaning);
  if (isEncrypted(c, encTable)) attach("(enc)");
  const en = enumValues(t, c);
  if (en.length) facts.push(en.join("/"));
  if (c.key !== "PRI") {
    const ix = leadingNonUniqueIndex(t, c.name);
    // MySQL 이 FK 마다 자동으로 만드는 같은 이름의 인덱스는 FK 사실에 이미 들어 있다
    if (ix && ix.name !== edge?.constraintName) attach(`(${ix.name})`);
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
  // 같은 두 테이블 사이의 관계선은 하나 — FK 컬럼이 여럿이면(정·부 담당자 등) ×N 으로 적는다.
  // 컬럼별 사실은 각 속성 설명에 그대로 있다. (edges 는 to·from·column 순으로 이미 정렬돼 있다)
  const pairs = new Map<string, { edge: RefEdge; n: number }>();
  for (const e of domainEdges) {
    const k = `${e.to}\u0000${e.from}`;
    const g = pairs.get(k);
    if (g) g.n++;
    else pairs.set(k, { edge: e, n: 1 });
  }
  // 자식 테이블이 많은 부모(허브)부터 — ts_customers 같은 중심 테이블이 다이어그램 머리에 온다.
  // 컬럼 수가 아니라 이어진 테이블 수로 잰다(한 테이블에 FK 셋인 건 허브가 아니다).
  const childCount = new Map<string, number>();
  for (const { edge } of pairs.values()) childCount.set(edge.to, (childCount.get(edge.to) ?? 0) + 1);
  const ordered = [...pairs.values()].sort(
    (a, b) =>
      (childCount.get(b.edge.to) ?? 0) - (childCount.get(a.edge.to) ?? 0) ||
      cmp(a.edge.to, b.edge.to) ||
      cmp(a.edge.from, b.edge.from),
  );
  const domainLines = ordered.map(({ edge: e, n }) => {
    const child = byName.get(e.from);
    const meaning = cleanText(child?.comment ?? "") || ent(stripPrefix(e.from, prefix));
    const ev = n > 1 ? `${evidence(w, e)} ×${n}` : evidence(w, e);
    return `    ${ent(e.to)} ${connector(e)} ${ent(e.from)} : "${q(`${meaning} · ${ev}`)}"`;
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
  for (const { edge: e } of ordered) {
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
