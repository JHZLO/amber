// DB 연동 — 연결 프로필(SQLite `db_connections`) · Rust 커맨드 래퍼 · 스냅샷 파일 · 동기화.
//
// 분담(DESIGN §11): 프로필은 SQLite, 스키마 구조는 ERD 저장소(lib/erdStore.ts)의 `schema.json`, 비밀번호는
// 키체인(Rust). 프론트는 비밀번호를 저장 순간 한 번 Rust 로 넘기고 그 뒤로는 모른다 — 여기 어떤 함수도
// 비밀번호를 돌려주지 않는다. 순수 계산(지문, 비교)은 lib/schemaSnapshot.ts, resin 생성은 lib/erdGen.ts.

import { invoke } from "@tauri-apps/api/core";
import { ulid as newUlid } from "ulid";
import { getDb } from "./db";
import { deleteErdEntry, ensureErdDir, readErdFileIfExists, writeErdFile } from "./erdStore";
import { isCodedError, type CodedError } from "./errors";
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
  /** ERD 에 감사 테이블(*_aud·revinfo)을 그리는가. 없으면 true (하우스 스타일 기본) */
  audit?: boolean;
  /** 즐겨찾기 — 트리에서 연결 맨 위로 올라온다. 없으면 false */
  starred?: boolean;
}

export const prefAudit = (p: DbSchemaPref): boolean => p.audit !== false;

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
  /** 계정 명령. 비어 있으면 username + 키체인, 있으면 스키마마다 이 명령으로 계정을 받는다 */
  credential_command: string;
  /** ERD 저장소 기준 상대경로 — 연결 폴더. 스키마 폴더는 `${folder_path}/${schema}` */
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
  credential_command: string;
}

export interface DbTestResult {
  server: string;
  latency_ms: number;
  /** error 는 계정 명령 연결에서 그 스키마만 붙지 못했을 때 */
  schemas: { name: string; tables: number; error?: CodedError }[];
}

export const profileOf = (c: DbProfile): DbProfile => ({
  ulid: c.ulid,
  kind: c.kind,
  host: c.host,
  port: c.port,
  username: c.username,
  tls: c.tls,
  credential_command: c.credential_command,
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
      .map((s) => ({
        name: s.name,
        label: String(s.label ?? ""),
        enabled: s.enabled !== false,
        ...(s.audit === false ? { audit: false } : {}),
        ...(s.starred === true ? { starred: true } : {}),
      }));
  } catch {
    return [];
  }
}

function fromRow(r: Row): DbConnection {
  const { schemas_json, ...rest } = r;
  return { ...rest, schemas: parseSchemas(schemas_json) };
}

const COLS =
  "id, ulid, name, kind, env, host, port, username, tls, credential_command, folder_path, schemas_json, " +
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
  credential_command: string;
  folder_path: string;
  schemas: DbSchemaPref[];
}

/** 프로필 저장. 비밀번호는 여기 오지 않는다 — 호출부가 dbSecretSet 으로 따로 넘긴다 */
export async function createConnection(input: ConnectionInput): Promise<DbConnection> {
  const db = await getDb();
  const ulid = newUlid();
  const res = await db.execute(
    `INSERT INTO db_connections (ulid, name, kind, env, host, port, username, tls, credential_command, folder_path, schemas_json)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`,
    [
      ulid,
      input.name.trim(),
      input.kind,
      input.env,
      input.host.trim(),
      input.port,
      input.username.trim(),
      input.tls,
      input.credential_command.trim(),
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
  if (patch.credential_command !== undefined) put("credential_command", patch.credential_command.trim());
  if (patch.folder_path !== undefined) put("folder_path", patch.folder_path);
  if (patch.schemas !== undefined) put("schemas_json", JSON.stringify(patch.schemas));
  if (!sets.length) return;
  args.push(Date.now());
  sets.push(`updated_at = $${args.length}`);
  args.push(id);
  await db.execute(`UPDATE db_connections SET ${sets.join(", ")} WHERE id = $${args.length}`, args);
}

/** 연결 삭제 = 프로필 + 키체인 항목 + 스냅샷 폴더(휴지통). 스냅샷은 그 연결로만 그려지는 사본이라 연결이
 *  없으면 볼 길이 없다 — 남겨 두면 보이지 않는 짐이 된다. 다시 연결하면 처음부터 다시 읽는다. */
export async function deleteConnection(c: DbConnection): Promise<void> {
  await dbSecretDelete(c.ulid);
  const db = await getDb();
  await db.execute("DELETE FROM db_connections WHERE id = $1", [c.id]);
  await deleteErdEntry(c.folder_path).catch(() => {}); // 한 번도 동기화하지 않은 연결은 폴더가 없다
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

// ---- Rust 커맨드 ----

/** 접속 확인. password 를 안 주면 Rust 가 키체인에서 읽는다(편집·재검사).
 *  계정 명령 연결은 계정마다 자기 스키마만 보이므로 확인할 스키마를 schemas 로 준다 */
export function dbTest(
  profile: DbProfile,
  password?: string | null,
  schemas?: string[],
): Promise<DbTestResult> {
  return invoke<DbTestResult>("db_test", {
    profile: profileOf(profile),
    password: password ?? null,
    schemas: schemas ?? null,
  });
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

// ---- 폴더 · 스냅샷 파일 ----

export const schemaFolder = (c: DbConnection, schema: string) => `${c.folder_path}/${schema}`;
export const snapshotPath = (c: DbConnection, schema: string) =>
  `${schemaFolder(c, schema)}/${SNAPSHOT_FILE}`;

export async function readSnapshot(c: DbConnection, schema: string): Promise<SchemaSnapshot | null> {
  const body = await readErdFileIfExists(snapshotPath(c, schema));
  return body === null ? null : parseSnapshot(body);
}

export async function writeSnapshot(c: DbConnection, schema: string, snap: SchemaSnapshot): Promise<void> {
  await ensureErdDir(schemaFolder(c, schema));
  await writeErdFile(snapshotPath(c, schema), JSON.stringify(snap, null, 2) + "\n");
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

/** 스키마 하나를 DB 에서 다시 읽어 `schema.json` 을 갱신한다. ERD 탭은 이 스냅샷을 바로 그린다 */
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

export async function setSchemaStarred(c: DbConnection, schema: string, starred: boolean): Promise<void> {
  const schemas = c.schemas.map((p) => {
    if (p.name !== schema) return p;
    const { starred: _drop, ...rest } = p;
    return starred ? { ...rest, starred: true } : rest;
  });
  await updateConnection(c.id, { schemas });
}

export const ENVS: DbEnv[] = ["dev", "staging", "prod"];

export function envLabel(env: DbEnv): string {
  return env === "prod"
    ? t("erd.db.env.prod")
    : env === "staging"
      ? t("erd.db.env.staging")
      : t("erd.db.env.dev");
}

// ---- 연결 상태 ----
//
// 설정 목록과 ERD 탭 트리가 같은 판정과 같은 색을 쓰도록 여기 한 곳에 둔다(둘이 갈리면 같은 연결이
// 화면마다 다른 상태로 보인다). 키체인을 읽어 비밀번호 존재를 확인하지 않는다 — 읽기 자체가 macOS
// 확인창을 띄운다. 동기화가 KEYCHAIN_MISSING 으로 실패한 흔적(last_error)으로 "비밀번호 필요"를 안다.

export type DbConnStatus = "connected" | "failed" | "needsPassword" | "never";

export function connStatus(c: DbConnection): DbConnStatus {
  if (c.last_error === "KEYCHAIN_MISSING") return "needsPassword";
  if (c.last_error) return "failed";
  if (c.last_sync_at) return "connected";
  return "never";
}

const STATUS_KEY = {
  connected: "erd.db.status.connected",
  failed: "erd.db.status.failed",
  needsPassword: "erd.db.status.needsPassword",
  never: "erd.db.status.never",
} as const;

export function connStatusLabel(st: DbConnStatus): string {
  return t(STATUS_KEY[st]);
}

/** 상태 점(`.db-dot`)의 수식 클래스 — 채움=연결됨, 색=안 됐다면 왜. 동기화 전은 무채색 기본형이라 빈 문자열 */
export function connStatusDot(st: DbConnStatus): string {
  if (st === "connected") return "ok";
  if (st === "needsPassword") return "warn";
  if (st === "failed") return "bad";
  return "";
}

/** 연결 이름 → 폴더 이름. 경로 구분자·선행 점만 걸러낸다(폴더 규칙은 vaultTree.invalidNameReason 이 정본) */
export function folderNameFor(name: string): string {
  return name.trim().replace(/[/\\:]/g, "-").replace(/^\.+/, "").slice(0, 80) || "db";
}
