// DB 연동 — 스키마 스냅샷의 형태와 그 위의 순수 연산(지문·비교·연동 헤더).
// Rust `dbconn.rs` 의 `db_introspect` 가 돌려주는 JSON 과 1:1 이고, 스키마 폴더의 `schema.json`
// 에 그대로 저장된다. I/O(파일·SQLite·invoke)는 lib/dbconn.ts, mermaid 생성은 lib/erdGen.ts.
//
// 왜 파일로 남기나: 변경 감지의 기준이자, 연결이 끊긴 채로도 스키마 개요를 그릴 재료다.
// 구조만 담는다(테이블·컬럼·인덱스·FK·CHECK) — 행 데이터는 어디에도 없다.

export type ColumnKey = "PRI" | "UNI" | "MUL" | "";

export interface SnapshotColumn {
  name: string;
  /** 길이·정밀도 없는 물리 타입 소문자 — 'bigint' 'varchar' 'datetime' */
  data_type: string;
  /** 선언 그대로 — 'varchar(255)' 'bigint unsigned' "enum('A','B')". enum 후보의 근거 */
  column_type: string;
  nullable: boolean;
  key: ColumnKey;
  default_value: string | null;
  /** 'auto_increment' 등 */
  extra: string;
  comment: string;
}

export interface SnapshotIndex {
  name: string;
  unique: boolean;
  /** 인덱스 순서대로 */
  columns: string[];
}

export interface SnapshotForeignKey {
  name: string;
  columns: string[];
  ref_schema: string;
  ref_table: string;
  ref_columns: string[];
}

export interface SnapshotCheck {
  name: string;
  clause: string;
}

export interface SnapshotTable {
  name: string;
  comment: string;
  /** information_schema.TABLES.TABLE_ROWS — 추정치. 없으면 null */
  rows_estimate: number | null;
  columns: SnapshotColumn[];
  indexes: SnapshotIndex[];
  foreign_keys: SnapshotForeignKey[];
  checks: SnapshotCheck[];
}

export interface SchemaSnapshot {
  amber: 1;
  /** db_connections.ulid */
  connection: string;
  schema: string;
  /** 서버 버전 표기 — 'MySQL 8.0.36' */
  server: string;
  /** UTC ms */
  synced_at: number;
  /** 구조 지문(fingerprintOf). 같으면 ERD 재생성 결과도 같다 */
  fingerprint: string;
  tables: SnapshotTable[];
}

/** 스키마 폴더 안의 스냅샷 파일명. 트리(vaultTree)는 .mmd 만 보이므로 여기 뜨지 않는다.
 *  점으로 시작하지 않는 이유: tauri-plugin-fs 스코프는 `**` 가 dotfile 을 매치하지 않아(requireLiteralLeadingDot)
 *  `.schema.json` 은 exists/write 가 "forbidden path" 로 거부된다 — 실측. */
export const SNAPSHOT_FILE = "schema.json";

/** Rust 가 돌려주는 형태 — 지문은 프론트가 계산해 붙인다 */
export type RawSnapshot = Omit<SchemaSnapshot, "amber" | "fingerprint">;

/** 53-bit 해시(cyrb53). 암호학적 용도가 아니라 '바뀌었나'만 보면 되므로 의존성 없이 간다 */
function hash53(str: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  const n = 4294967296 * (2097151 & h2) + (h1 >>> 0);
  return n.toString(16).padStart(14, "0");
}

/** 구조 지문 — 테이블 순서·컬럼 순서와 무관하게 이름순으로 정렬해 계산한다.
 *  rows_estimate 는 뺀다(행이 늘어난 건 스키마 변경이 아니다). 코멘트는 넣는다(ERD 설명이 바뀐다). */
export function fingerprintOf(tables: SnapshotTable[]): string {
  const canon = [...tables]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((t) => ({
      n: t.name,
      c: t.comment,
      cols: [...t.columns]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((c) => [c.name, c.column_type, c.nullable, c.key, c.default_value, c.extra, c.comment]),
      ix: [...t.indexes]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((i) => [i.name, i.unique, i.columns]),
      fk: [...t.foreign_keys]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((f) => [f.name, f.columns, f.ref_schema, f.ref_table, f.ref_columns]),
      ck: [...t.checks].sort((a, b) => a.name.localeCompare(b.name)).map((k) => [k.name, k.clause]),
    }));
  return hash53(JSON.stringify(canon));
}

export function finalizeSnapshot(raw: RawSnapshot): SchemaSnapshot {
  return { amber: 1, ...raw, fingerprint: fingerprintOf(raw.tables) };
}

/** `schema.json` 본문 → 스냅샷. 형태가 어긋나면 null (깨진 파일은 '없는 것'으로 친다) */
export function parseSnapshot(json: string): SchemaSnapshot | null {
  try {
    const v = JSON.parse(json) as Partial<SchemaSnapshot>;
    if (!v || v.amber !== 1 || typeof v.schema !== "string" || !Array.isArray(v.tables)) return null;
    return v as SchemaSnapshot;
  } catch {
    return null;
  }
}

// ---- 비교 ----

export interface SchemaDiff {
  tablesAdded: string[];
  tablesRemoved: string[];
  /** 'table.column' */
  columnsAdded: string[];
  columnsRemoved: string[];
  /** 타입·널·키·기본값·코멘트가 바뀐 컬럼 'table.column' */
  columnsChanged: string[];
  /** 인덱스·FK·CHECK 가 바뀐 테이블 */
  constraintTables: string[];
}

function colSig(c: SnapshotColumn): string {
  return [c.column_type, c.nullable, c.key, c.default_value ?? " ", c.extra, c.comment].join("|");
}

function constraintSig(t: SnapshotTable): string {
  return JSON.stringify([
    [...t.indexes].sort((a, b) => a.name.localeCompare(b.name)),
    [...t.foreign_keys].sort((a, b) => a.name.localeCompare(b.name)),
    [...t.checks].sort((a, b) => a.name.localeCompare(b.name)),
  ]);
}

export function diffSnapshots(prev: SchemaSnapshot, next: SchemaSnapshot): SchemaDiff {
  const d: SchemaDiff = {
    tablesAdded: [],
    tablesRemoved: [],
    columnsAdded: [],
    columnsRemoved: [],
    columnsChanged: [],
    constraintTables: [],
  };
  const before = new Map(prev.tables.map((t) => [t.name, t]));
  const after = new Map(next.tables.map((t) => [t.name, t]));
  for (const name of after.keys()) if (!before.has(name)) d.tablesAdded.push(name);
  for (const name of before.keys()) if (!after.has(name)) d.tablesRemoved.push(name);
  for (const [name, b] of before) {
    const a = after.get(name);
    if (!a) continue;
    const bc = new Map(b.columns.map((c) => [c.name, c]));
    const ac = new Map(a.columns.map((c) => [c.name, c]));
    for (const cn of ac.keys()) if (!bc.has(cn)) d.columnsAdded.push(`${name}.${cn}`);
    for (const [cn, c] of bc) {
      const n = ac.get(cn);
      if (!n) d.columnsRemoved.push(`${name}.${cn}`);
      else if (colSig(c) !== colSig(n)) d.columnsChanged.push(`${name}.${cn}`);
    }
    if (constraintSig(b) !== constraintSig(a)) d.constraintTables.push(name);
  }
  return d;
}

export function diffIsEmpty(d: SchemaDiff): boolean {
  return (
    !d.tablesAdded.length &&
    !d.tablesRemoved.length &&
    !d.columnsAdded.length &&
    !d.columnsRemoved.length &&
    !d.columnsChanged.length &&
    !d.constraintTables.length
  );
}

// ---- 연동 헤더 ----
//
// 자동 생성 파일의 두 번째 줄(erDiagram 다음)에 남는 출처 표식. 앱은 이 줄로 "DB 연동 파일"을
// 알아보고(동기화 버튼·변경 배너), 사람은 파일만 봐도 어디서 언제 왔는지 안다.
//   %% amber:db dev/svc_booking · 2026-09-02 09:41 · 3f9a1c0b2d4e5f[ · noaud]
// 지문 토막은 생성 당시 스냅샷의 지문 — `schema.json` 의 지문과 다르면 "DB 가 그 뒤 바뀌었다"는
// 뜻이라, 연결이 끊긴 채로도 파일이 낡았는지 판정할 수 있다. `noaud` 는 감사 테이블(*_aud·revinfo)을
// 빼고 그렸다는 표식 — 옵션이 바뀌면 지문이 같아도 파일은 낡은 것이다.
// 연결 이름엔 공백이 올 수 있어 스키마는 뒤에서 잡는다(스키마 이름엔 공백·슬래시가 없다).

const DB_HEADER_RE =
  /^\s*%% amber:db (.+)\/([^\s/]+) · (\d{4}-\d{2}-\d{2} \d{2}:\d{2})(?: · ([0-9a-f]+))?(?: · (noaud))?\s*$/;

export interface DbHeader {
  connection: string;
  schema: string;
  generatedAt: string;
  /** 생성 당시 스냅샷 지문. 구버전 헤더엔 없다 */
  fingerprint: string | null;
  /** 감사 테이블(*_aud·revinfo)을 포함해 그렸는가 */
  audit: boolean;
}

export interface DbHeaderOptions {
  /** false 면 ` · noaud` 표식을 붙인다 */
  audit?: boolean;
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** 로컬 벽시계 'YYYY-MM-DD HH:mm' — 표시용 타임스탬프(달력 좌표가 아니라 date.ts 를 거치지 않는다) */
export function formatHeaderTime(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

export function formatDbHeader(
  connection: string,
  schema: string,
  at: Date,
  fingerprint: string,
  opts: DbHeaderOptions = {},
): string {
  const flag = opts.audit === false ? " · noaud" : "";
  return `    %% amber:db ${connection}/${schema} · ${formatHeaderTime(at)} · ${fingerprint}${flag}`;
}

/** 소스 앞머리(5줄)에서 연동 헤더를 찾는다. 없으면 null = 손으로 만든 다이어그램 */
export function parseDbHeader(source: string): DbHeader | null {
  for (const line of source.split("\n", 5)) {
    const m = DB_HEADER_RE.exec(line);
    if (m)
      return {
        connection: m[1],
        schema: m[2],
        generatedAt: m[3],
        fingerprint: m[4] ?? null,
        audit: m[5] !== "noaud",
      };
  }
  return null;
}
