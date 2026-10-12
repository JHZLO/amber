// DB 연동 — 스키마 스냅샷(schemaSnapshot.ts) → resin 원문. ERD 탭이 이 원문을 resin 으로 그린다.
//
// 나누어 맡는다: 여기는 **사내 스키마를 읽는 규칙**(이름과 코멘트로 논리 참조 추론, 암호화 컬럼 판정, 깨진
// 인코딩 복원, CHECK 의 값 목록)만 갖고, resin 원문을 쓰는 일(타입 정규화, 키, 인덱스, FK, Envers 감사 테이블을
// `audit envers` 로 접기, 다른 스키마를 가리키는 FK 의 external table, 정렬)은 resin 의 `fromSchema` 가 한다 —
// 플레이그라운드의 SQL 가져오기와 같은 규칙이라 같은 스키마는 어디서든 같은 resin 이 된다.
//
// 결정적이어야 한다: 같은 스냅샷이면 바이트 단위로 같은 원문이 나온다. 정렬은 로케일에 좌우되는
// localeCompare 대신 코드 유닛 비교를 쓰고, 컬럼 순서는 스냅샷(DDL) 순서를 따른다(타임스탬프만 맨 뒤).
// 코멘트는 번역하지 않고 그대로 옮긴다 — 의미를 지어내는 일은 하지 않는다(코멘트가 없으면 설명도 없다).

import { fromSchema, type SchemaForeignKey, type SchemaInput } from "resin";
import type {
  SchemaSnapshot,
  SnapshotColumn,
  SnapshotIndex,
  SnapshotTable,
} from "./schemaSnapshot";

export interface ErdGenOptions {
  /** 감사 테이블(`*_aud`, `revinfo`)을 싣는가. 기본 true — resin 이 원본 테이블의 `audit envers` 로 접는다 */
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
  /** true = 스냅샷에 선언된 FOREIGN KEY, false = 이름이나 코멘트로 추론한 논리 참조 */
  physical: boolean;
}

export interface ErdGenResult {
  /** resin 원문 */
  source: string;
  /** resin 이 그대로 옮기지 못한 것(이름을 바꿔 쓴 것, 풀지 못한 FK) — 사람이 읽을 문장 */
  notes: string[];
}

// ---- 이름·문자열 도우미 ----

/** 로케일 무관 정렬 — 출력이 기기·언어 설정에 따라 달라지면 안 된다 */
const cmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

const isAudit = (name: string): boolean => name.toLowerCase().endsWith("_aud");
const isRevinfo = (name: string): boolean => name.toLowerCase() === "revinfo";
/** 도메인 테이블 = 감사 테이블도 revinfo 도 아닌 것. 논리 추론과 접두사 계산은 이들만 본다 */
const isDomain = (name: string): boolean => !isAudit(name) && !isRevinfo(name);

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
// PRIMARY 는 기본 키로 따로 넘기므로 인덱스로 세지 않는다 — 복합 PK 가 unique(...) 로 둔갑하지 않게.

const PRIMARY_RE = /^primary$/i;

function indexesOf(t: SnapshotTable): SnapshotIndex[] {
  return t.indexes.filter((ix) => !PRIMARY_RE.test(ix.name));
}

function hasSingleUnique(t: SnapshotTable, c: SnapshotColumn): boolean {
  if (c.key === "UNI") return true;
  return indexesOf(t).some((ix) => ix.unique && ix.columns.length === 1 && ix.columns[0] === c.name);
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
      edges.push({
        from: t.name,
        fromColumn: fk.columns[0],
        to: fk.ref_table,
        toColumn: fk.ref_columns[0] ?? "id",
        physical: true,
      });
    }

    if (!isDomain(t.name)) continue;
    const domainNames = snapshot.tables.map((x) => x.name).filter((n) => n !== t.name && isDomain(n));
    for (const c of t.columns) {
      if (c.key === "PRI" || covered.has(c.name)) continue;
      const logical = (to: string, toColumn: string): RefEdge => ({
        from: t.name,
        fromColumn: c.name,
        to,
        toColumn,
        physical: false,
      });

      // 1) 코멘트가 대상을 적어 뒀으면 그게 가장 정확하다 — 이름 규칙으로는 못 잇는 user_id → ts_customers 도 풀린다
      const refs = commentRefs(c.comment, byName, t.name).filter((r) => {
        const target = byName.get(r.table)!.columns.find((col) => col.name === r.column)!;
        return typesCompatible(target.data_type, c.data_type);
      });
      if (refs.length === 1) {
        edges.push(logical(refs[0].table, refs[0].column));
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
      edges.push(logical(hits[0].n, hits[0].col));
    }
  }

  edges.sort((a, b) => cmp(a.to, b.to) || cmp(a.from, b.from) || cmp(a.fromColumn, b.fromColumn));
  unresolved.sort((a, b) => cmp(a.table, b.table) || cmp(a.column, b.column));
  return { edges, unresolved };
}

// ---- 생성 ----

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

/** 같은 테이블 CHECK 의 `col in (...)` 값 목록. `enum(...)` 타입의 값은 resin 이 타입에서 읽는다. 지어내지 않는다 */
function checkValues(t: SnapshotTable, c: SnapshotColumn): string[] | null {
  const re = new RegExp(`(?:^|[^A-Za-z0-9_])\`?${escapeRe(c.name)}\`?\\s+in\\s*\\(([^)]*)\\)`, "i");
  for (const k of t.checks) {
    const m = re.exec(k.clause);
    if (!m) continue;
    const v = quotedValues(m[1]);
    if (v.length) return v;
  }
  return null;
}

/** 컬럼 설명 — 코멘트의 뜻만. `id` PK 의 'ID' 같은 되풀이는 이름이 이미 말하므로 비운다 */
function meaningOf(c: SnapshotColumn): string {
  const meaning = cleanText(c.comment);
  if (c.key === "PRI" && c.name.toLowerCase() === "id" && TRIVIAL_ID_RE.test(meaning)) return "";
  return meaning;
}

/** created_at → updated_at 을 맨 뒤로. 나머지는 스냅샷(DDL) 순서 */
function orderColumns(cols: SnapshotColumn[]): SnapshotColumn[] {
  return [
    ...cols.filter((c) => !isTimestamp(c)),
    ...cols.filter(isCreatedAt),
    ...cols.filter(isUpdatedAt),
  ];
}

/** 스냅샷 → resin 원문. 선언된 FK 는 `->`, 추론한 논리 참조는 `~>` 로 싣는다 */
export function generateErd(snapshot: SchemaSnapshot, opts: ErdGenOptions = {}): ErdGenResult {
  const { edges } = inferReferences(snapshot);
  const logical = new Map<string, RefEdge[]>();
  for (const e of edges) if (!e.physical) logical.set(e.from, [...(logical.get(e.from) ?? []), e]);
  // 테이블은 이름순 — 스냅샷이 어떤 순서로 오든 같은 원문이 나오게(컬럼만 DDL 순서를 따른다)
  const tables = (opts.audit === false ? snapshot.tables.filter((t) => isDomain(t.name)) : [...snapshot.tables]).sort(
    (a, b) => cmp(a.name, b.name),
  );

  const input: SchemaInput = {
    tables: tables.map((t) => {
      const encTable = t.columns.some((c) => ENC_RE.test(c.comment));
      const primary = t.indexes.find((ix) => PRIMARY_RE.test(ix.name));
      const foreignKeys: SchemaForeignKey[] = [
        ...t.foreign_keys
          .filter((fk) => fk.columns.length > 0)
          .map((fk) => ({
            name: fk.name,
            columns: fk.columns,
            table: fk.ref_table,
            // 같은 스키마면 이름만 — 스냅샷에 없는 테이블(지워진 대상)이 `svc_x.t` 같은 긴 이름이 되지 않게
            schema: fk.ref_schema !== snapshot.schema ? fk.ref_schema : null,
            references: fk.ref_columns.length ? fk.ref_columns : null,
          })),
        ...(logical.get(t.name) ?? []).map((e) => ({
          columns: [e.fromColumn],
          table: e.to,
          schema: snapshot.schema,
          references: [e.toColumn],
          kind: "logical" as const,
        })),
      ];
      return {
        name: t.name,
        schema: snapshot.schema,
        comment: cleanText(t.comment) || null,
        columns: orderColumns(t.columns).map((c) => ({
          name: c.name,
          type: c.column_type,
          nullable: c.nullable,
          comment: meaningOf(c) || null,
          values: checkValues(t, c),
          encrypted: isEncrypted(c, encTable),
        })),
        primaryKey: primary?.columns ?? t.columns.filter((c) => c.key === "PRI").map((c) => c.name),
        indexes: [
          ...indexesOf(t).map((ix) => ({ name: ix.name, columns: ix.columns, unique: ix.unique })),
          // key 는 UNI 인데 인덱스 목록에 그 UNIQUE 가 없으면(스냅샷이 덜 읽힌 경우) 이름 없이라도 싣는다
          ...t.columns
            .filter((c) => c.key === "UNI" && !indexesOf(t).some((ix) => ix.unique && ix.columns.length === 1 && ix.columns[0] === c.name))
            .map((c) => ({ columns: [c.name], unique: true })),
        ],
        foreignKeys,
      };
    }),
  };
  const { source, notes } = fromSchema(input);
  return { source, notes: notes.map((n) => n.message) };
}
