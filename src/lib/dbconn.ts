// DB 연동 — 연결 프로필(SQLite `db_connections`) · Rust 커맨드 래퍼 · 스냅샷 파일 · 동기화.
//
// 분담(DESIGN §11): 프로필은 SQLite, 스키마 구조는 스키마 폴더의 `.schema.json`, 비밀번호는 키체인(Rust).
// 프론트는 비밀번호를 저장 순간 한 번 Rust 로 넘기고 그 뒤로는 모른다 — 여기 어떤 함수도 비밀번호를
// 돌려주지 않는다. 순수 계산(지문·비교·헤더)은 lib/schemaSnapshot.ts, mermaid 생성은 lib/erdGen.ts.

import { invoke } from "@tauri-apps/api/core";
import { ulid as newUlid } from "ulid";
import { getDb } from "./db";
import { ensureDiagramDir, readDiagramFileIfExists, writeDiagramFile } from "./diagrams";
import { isCodedError } from "./errors";
import { t } from "./i18n";
import {
  SNAPSHOT_FILE,
  diffSnapshots,
  finalizeSnapshot,
  parseSnapshot,
  type RawSnapshot,
  type SchemaDiff,
  type SchemaSnapshot,
} from "./schemaSnapshot";

export type DbKind = "mysql" | "postgres";
export type DbEnv = "dev" | "staging" | "prod";
export type DbTls = "disabled" | "preferred" | "required";

/** 연동 대상 스키마 하나 — 물리 이름 + 사용자가 붙인 표시명. 체크 해제하면 enabled=false 로 남겨 둔다 */
export interface DbSchemaPref {
  name: string;
  label: string;
  enabled: boolean;
}

/** db_connections 한 행 (schemas_json 은 풀어서 담는다) */
export interface DbConnection {
  id: number;
  ulid: string;
  name: string;
  kind: DbKind;
  env: DbEnv;
  host: string;
  port: number;
  username: string;
  tls: DbTls;
  /** 다이어그램 루트 기준 상대경로 — 연결 폴더. 스키마 폴더는 `${folder_path}/${schema}` */
  folder_path: string;
  schemas: DbSchemaPref[];
  last_sync_at: number | null; // UTC ms
  /** 마지막 실패의 에러 코드('DB_REFUSED' 등). 성공하면 null */
  last_error: string | null;
  created_at: number;
  updated_at: number;
}

/** Rust 커맨드가 접속에 필요로 하는 것만 — 비밀번호는 없다(키체인) */
export interface DbProfile {
  ulid: string;
  kind: DbKind;
  host: string;
  port: number;
  username: string;
  tls: DbTls;
}

export interface DbTestResult {
  server: string;
  latency_ms: number;
  schemas: { name: string; tables: number }[];
}

export const profileOf = (c: DbProfile): DbProfile => ({
  ulid: c.ulid,
  kind: c.kind,
  host: c.host,
  port: c.port,
  username: c.username,
  tls: c.tls,
});

/** 연결 목록이 바뀌었다(추가·편집·삭제·동기화) — 트리·설정이 서로 다시 읽게 하는 창 안 이벤트 */
export const DB_CONNECTIONS_EVENT = "amber-db-connections-change";

export function notifyConnectionsChanged(): void {
  window.dispatchEvent(new CustomEvent(DB_CONNECTIONS_EVENT));
}

// ---- SQLite ----

type Row = Omit<DbConnection, "schemas"> & { schemas_json: string };

function parseSchemas(json: string): DbSchemaPref[] {
  try {
    const arr = JSON.parse(json);
    if (!Array.isArray(arr)) return [];
    return arr
      .filter((s): s is DbSchemaPref => s && typeof s.name === "string")
      .map((s) => ({ name: s.name, label: String(s.label ?? ""), enabled: s.enabled !== false }));
  } catch {
    return [];
  }
}

function fromRow(r: Row): DbConnection {
  const { schemas_json, ...rest } = r;
  return { ...rest, schemas: parseSchemas(schemas_json) };
}

const COLS =
  "id, ulid, name, kind, env, host, port, username, tls, folder_path, schemas_json, " +
  "last_sync_at, last_error, created_at, updated_at";

export async function listConnections(): Promise<DbConnection[]> {
  const db = await getDb();
  const rows = await db.select<Row[]>(`SELECT ${COLS} FROM db_connections ORDER BY name COLLATE NOCASE, id`);
  return rows.map(fromRow);
}

export async function getConnection(id: number): Promise<DbConnection | null> {
  const db = await getDb();
  const rows = await db.select<Row[]>(`SELECT ${COLS} FROM db_connections WHERE id = $1`, [id]);
  return rows.length ? fromRow(rows[0]) : null;
}

export interface ConnectionInput {
  name: string;
  kind: DbKind;
  env: DbEnv;
  host: string;
  port: number;
  username: string;
  tls: DbTls;
  folder_path: string;
  schemas: DbSchemaPref[];
}

/** 프로필 저장. 비밀번호는 여기 오지 않는다 — 호출부가 dbSecretSet 으로 따로 넘긴다 */
export async function createConnection(input: ConnectionInput): Promise<DbConnection> {
  const db = await getDb();
  const ulid = newUlid();
  const res = await db.execute(
    `INSERT INTO db_connections (ulid, name, kind, env, host, port, username, tls, folder_path, schemas_json)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)`,
    [
      ulid,
      input.name.trim(),
      input.kind,
      input.env,
      input.host.trim(),
      input.port,
      input.username.trim(),
      input.tls,
      input.folder_path,
      JSON.stringify(input.schemas),
    ],
  );
  const c = await getConnection(Number(res.lastInsertId));
  if (!c) throw new Error("db_connections insert failed");
  return c;
}

export async function updateConnection(id: number, patch: Partial<ConnectionInput>): Promise<void> {
  const db = await getDb();
  const sets: string[] = [];
  const args: unknown[] = [];
  const put = (col: string, v: unknown) => {
    args.push(v);
    sets.push(`${col} = $${args.length}`);
  };
  if (patch.name !== undefined) put("name", patch.name.trim());
  if (patch.kind !== undefined) put("kind", patch.kind);
  if (patch.env !== undefined) put("env", patch.env);
  if (patch.host !== undefined) put("host", patch.host.trim());
  if (patch.port !== undefined) put("port", patch.port);
  if (patch.username !== undefined) put("username", patch.username.trim());
  if (patch.tls !== undefined) put("tls", patch.tls);
  if (patch.folder_path !== undefined) put("folder_path", patch.folder_path);
  if (patch.schemas !== undefined) put("schemas_json", JSON.stringify(patch.schemas));
  if (!sets.length) return;
  args.push(Date.now());
  sets.push(`updated_at = $${args.length}`);
  args.push(id);
  await db.execute(`UPDATE db_connections SET ${sets.join(", ")} WHERE id = $${args.length}`, args);
}

/** 연결 삭제 = 프로필 + 키체인 항목. 파일(폴더·ERD·스냅샷)은 남긴다 — 폴더는 일반 폴더로 돌아간다 */
export async function deleteConnection(c: DbConnection): Promise<void> {
  await dbSecretDelete(c.ulid);
  const db = await getDb();
  await db.execute("DELETE FROM db_connections WHERE id = $1", [c.id]);
}

/** 동기화 결과 기록. 성공이면 시각 갱신 + 에러 지움, 실패면 코드만 남긴다(문장은 프론트가 만든다) */
export async function markSync(id: number, errorCode: string | null): Promise<void> {
  const db = await getDb();
  if (errorCode) {
    await db.execute("UPDATE db_connections SET last_error = $1, updated_at = $2 WHERE id = $3", [
      errorCode,
      Date.now(),
      id,
    ]);
  } else {
    await db.execute(
      "UPDATE db_connections SET last_sync_at = $1, last_error = NULL, updated_at = $1 WHERE id = $2",
      [Date.now(), id],
    );
  }
}

/** 트리에서 폴더 이름이 바뀌거나 옮겨졌을 때 — 그 폴더거나 그 하위였던 연결 폴더 경로를 따라 옮긴다.
 *  vaultTree.remapPath 와 같은 규칙(구분자까지 봐야 'dev' 와 'dev2' 가 섞이지 않는다). */
export async function remapConnectionFolders(oldPrefix: string, newPrefix: string): Promise<boolean> {
  const list = await listConnections();
  const db = await getDb();
  let touched = false;
  for (const c of list) {
    let next: string | null = null;
    if (c.folder_path === oldPrefix) next = newPrefix;
    else if (c.folder_path.startsWith(`${oldPrefix}/`)) next = newPrefix + c.folder_path.slice(oldPrefix.length);
    if (next === null) continue;
    await db.execute("UPDATE db_connections SET folder_path = $1, updated_at = $2 WHERE id = $3", [
      next,
      Date.now(),
      c.id,
    ]);
    touched = true;
  }
  return touched;
}

// ---- Rust 커맨드 ----

/** 접속 확인. password 를 안 주면 Rust 가 키체인에서 읽는다(편집·재검사) */
export function dbTest(profile: DbProfile, password?: string | null): Promise<DbTestResult> {
  return invoke<DbTestResult>("db_test", { profile: profileOf(profile), password: password ?? null });
}

export function dbIntrospect(profile: DbProfile, schema: string): Promise<RawSnapshot> {
  return invoke<RawSnapshot>("db_introspect", { profile: profileOf(profile), schema });
}

export function dbSecretSet(ulid: string, password: string): Promise<void> {
  return invoke("db_secret_set", { ulid, password });
}

export function dbSecretDelete(ulid: string): Promise<void> {
  return invoke("db_secret_delete", { ulid });
}

export function dbSecretExists(ulid: string): Promise<boolean> {
  return invoke<boolean>("db_secret_exists", { ulid });
}

// ---- 폴더 · 스냅샷 파일 ----

export const schemaFolder = (c: DbConnection, schema: string) => `${c.folder_path}/${schema}`;
export const snapshotPath = (c: DbConnection, schema: string) =>
  `${schemaFolder(c, schema)}/${SNAPSHOT_FILE}`;

export async function readSnapshot(c: DbConnection, schema: string): Promise<SchemaSnapshot | null> {
  const body = await readDiagramFileIfExists(snapshotPath(c, schema));
  return body === null ? null : parseSnapshot(body);
}

export async function writeSnapshot(c: DbConnection, schema: string, snap: SchemaSnapshot): Promise<void> {
  await ensureDiagramDir(schemaFolder(c, schema));
  await writeDiagramFile(snapshotPath(c, schema), JSON.stringify(snap, null, 2) + "\n");
}

/** 열 때 자동 동기화하는 최소 간격 — 터널이 자주 끊기는 환경에서 폴링 대신 '열 때 한 번'으로 족하다 */
export const SYNC_THROTTLE_MS = 10 * 60 * 1000;

export function isStale(snap: SchemaSnapshot | null, now = Date.now()): boolean {
  return !snap || now - snap.synced_at > SYNC_THROTTLE_MS;
}

export interface SyncResult {
  snapshot: SchemaSnapshot;
  previous: SchemaSnapshot | null;
  /** 이전 스냅샷이 있을 때만. 구조 변화가 없으면 빈 diff */
  diff: SchemaDiff | null;
}

/** 스키마 하나를 DB 에서 다시 읽어 `.schema.json` 을 갱신한다. 파일(ERD)은 건드리지 않는다 —
 *  ERD 는 사용자가 [다시 생성] 으로 초안을 받아 ⌘S 할 때만 바뀐다. */
export async function syncSchema(c: DbConnection, schema: string): Promise<SyncResult> {
  const previous = await readSnapshot(c, schema);
  try {
    const raw = await dbIntrospect(c, schema);
    const snapshot = finalizeSnapshot(raw);
    await writeSnapshot(c, schema, snapshot);
    await markSync(c.id, null);
    return { snapshot, previous, diff: previous ? diffSnapshots(previous, snapshot) : null };
  } catch (e) {
    await markSync(c.id, isCodedError(e) ? e.code : "DB_QUERY");
    throw e;
  }
}

export function enabledSchemas(c: DbConnection): DbSchemaPref[] {
  return c.schemas.filter((s) => s.enabled);
}

/** 트리가 폴더 경로로 연결·스키마를 알아보기 위한 색인 */
export interface ConnectionIndex {
  byFolder: Map<string, DbConnection>;
  schemaByFolder: Map<string, { conn: DbConnection; pref: DbSchemaPref }>;
}

export function indexConnections(list: DbConnection[]): ConnectionIndex {
  const byFolder = new Map<string, DbConnection>();
  const schemaByFolder = new Map<string, { conn: DbConnection; pref: DbSchemaPref }>();
  for (const c of list) {
    byFolder.set(c.folder_path, c);
    for (const pref of enabledSchemas(c)) schemaByFolder.set(schemaFolder(c, pref.name), { conn: c, pref });
  }
  return { byFolder, schemaByFolder };
}

export const ENVS: DbEnv[] = ["dev", "staging", "prod"];

export function envLabel(env: DbEnv): string {
  return env === "prod"
    ? t("diagrams.db.env.prod")
    : env === "staging"
      ? t("diagrams.db.env.staging")
      : t("diagrams.db.env.dev");
}

/** 연결 이름 → 폴더 이름. 경로 구분자·선행 점만 걸러낸다(폴더 규칙은 vaultTree.invalidNameReason 이 정본) */
export function folderNameFor(name: string): string {
  return name.trim().replace(/[/\\:]/g, "-").replace(/^\.+/, "").slice(0, 80) || "db";
}
