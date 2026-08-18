// DB 접근 계층. tauri-plugin-sql 로 프론트에서 직접 SQLite 를 다룬다.
// 마이그레이션은 Rust 쪽(add_migrations)에서 Database.load 시 자동 적용됨.

import Database from "@tauri-apps/plugin-sql";
import type {
  Concept,
  ConceptFilter,
  ConceptStatus,
  ConceptWithTags,
  Confidence,
  SourceKind,
} from "../types";

let _db: Database | null = null;

export async function getDb(): Promise<Database> {
  if (!_db) _db = await Database.load("sqlite:amber.db");
  return _db;
}

const now = () => Date.now();

// 내부 조회 로우(태그는 group_concat CSV 로 붙여 옴)
type ConceptRow = Concept & { tags_csv: string | null };

function toWithTags(r: ConceptRow): ConceptWithTags {
  const { tags_csv, ...c } = r;
  return {
    ...c,
    confidence: c.confidence as Confidence,
    tags: tags_csv ? tags_csv.split(",") : [],
  };
}

const TAGS_SUBQUERY =
  "(SELECT group_concat(t.name, ',') FROM concept_tags ct " +
  "JOIN tags t ON t.id = ct.tag_id WHERE ct.concept_id = c.id) AS tags_csv";

/** 관리 창 리스트: 상태/검색/태그 필터 + 정렬 */
export async function listConcepts(
  filter: ConceptFilter,
): Promise<ConceptWithTags[]> {
  const db = await getDb();
  const where: string[] = [];
  const params: unknown[] = [];
  let p = 1;

  if (filter.status !== "all") {
    where.push(`c.status = $${p++}`);
    params.push(filter.status);
  }

  const search = filter.search?.trim();
  if (search) {
    const like = `%${search}%`;
    // 제목 / 요약 / 태그 substring (PRD: MVP 는 FTS 없이 LIKE)
    where.push(
      `(c.title LIKE $${p} OR c.summary LIKE $${p + 1} OR EXISTS(` +
        `SELECT 1 FROM concept_tags ct JOIN tags t ON t.id = ct.tag_id ` +
        `WHERE ct.concept_id = c.id AND t.name LIKE $${p + 2}))`,
    );
    params.push(like, like, like);
    p += 3;
  }

  const tags = filter.tags?.filter(Boolean) ?? [];
  if (tags.length) {
    const placeholders = tags.map(() => `$${p++}`).join(",");
    where.push(
      `c.id IN (SELECT ct.concept_id FROM concept_tags ct ` +
        `JOIN tags t ON t.id = ct.tag_id WHERE t.name IN (${placeholders}) ` +
        `GROUP BY ct.concept_id HAVING COUNT(DISTINCT t.name) = $${p++})`,
    );
    params.push(...tags, tags.length);
  }

  // 정렬. canonical 은 SQLite 의 "ASC 는 NULL 이 먼저" 특성을 이용해
  // (last_seen_at IS NULL) 표현식 없이도 미노출(NULL)을 앞세우며 idx_concepts_widget 을 그대로 탄다.
  const orderBy = {
    canonical: "c.confidence ASC, c.last_seen_at ASC, c.id ASC",
    recent_updated: "c.updated_at DESC, c.id DESC",
    recent_created: "c.created_at DESC, c.id DESC",
    title: "c.title COLLATE NOCASE ASC",
  }[filter.sort ?? "recent_updated"];

  const sql =
    `SELECT c.*, ${TAGS_SUBQUERY} FROM concepts c` +
    (where.length ? ` WHERE ${where.join(" AND ")}` : "") +
    ` ORDER BY ${orderBy}`;

  const rows = await db.select<ConceptRow[]>(sql, params);
  return rows.map(toWithTags);
}

/** 위젯/학습중 큐: canonical 정렬된 학습중 개념 */
export function learningQueue(): Promise<ConceptWithTags[]> {
  return listConcepts({ status: "learning", sort: "canonical" });
}

export async function getConcept(id: number): Promise<ConceptWithTags | null> {
  const db = await getDb();
  const rows = await db.select<ConceptRow[]>(
    `SELECT c.*, ${TAGS_SUBQUERY} FROM concepts c WHERE c.id = $1`,
    [id],
  );
  return rows.length ? toWithTags(rows[0]) : null;
}

/** 주어진 시각 범위 [startMs, endMs) 에 학습완료된 개념 (할 일 탭의 "이날 학습완료" 칩용).
 *  로컬 날짜 → ms 범위 변환은 호출부(lib/date.ts dayRangeMs)가 담당한다. */
export async function conceptsLearnedOn(
  startMs: number,
  endMs: number,
): Promise<{ id: number; title: string }[]> {
  const db = await getDb();
  return db.select<{ id: number; title: string }[]>(
    `SELECT id, title FROM concepts
     WHERE status = 'learned' AND learned_at >= $1 AND learned_at < $2
     ORDER BY learned_at DESC`,
    [startMs, endMs],
  );
}

/** 노트가 이름변경·이동됐을 때 개념의 출처 경로(source JSON 의 noteRel)를 따라 옮긴다.
 *  파일 하나면 정확히 일치하는 행만, 폴더면 그 접두사로 시작하는 모든 행을 재매핑한다
 *  — 폴더 rename 은 사이드카가 폴더째 따라가서 파일 단위 훅이 아예 안 돌기 때문에 여기서만 잡힌다.
 *  실패해도 파일 작업은 이미 끝났으므로 호출부가 삼킨다(링크가 깨질 뿐, 노트는 멀쩡). */
export async function repointConceptSource(
  oldRel: string,
  newRel: string,
  isDir: boolean,
): Promise<void> {
  const db = await getDb();
  if (isDir) {
    await db.execute(
      `UPDATE concepts
          SET source = json_set(source, '$.noteRel',
                $2 || substr(json_extract(source, '$.noteRel'), length($1) + 1))
        WHERE source_kind = 'file'
          AND json_valid(source)
          AND json_extract(source, '$.noteRel') LIKE $1 || '/%'`,
      [oldRel, newRel],
    );
    return;
  }
  await db.execute(
    `UPDATE concepts
        SET source = json_set(source, '$.noteRel', $2)
      WHERE source_kind = 'file'
        AND json_valid(source)
        AND json_extract(source, '$.noteRel') = $1`,
    [oldRel, newRel],
  );
}

export interface CreateConceptInput {
  ulid: string;
  title: string;
  summary: string;
  detailPath: string;
  tags?: string[];
  confidence?: Confidence;
  source?: string | null;
  sourceKind?: SourceKind;
}

/** 개념 생성 (status=learning 기본). 생성된 id 반환 */
export async function createConcept(input: CreateConceptInput): Promise<number> {
  const db = await getDb();
  const res = await db.execute(
    `INSERT INTO concepts (ulid, title, summary, detail_path, source, source_kind, confidence)
     VALUES ($1, $2, $3, $4, $5, $6, $7)`,
    [
      input.ulid,
      input.title,
      input.summary,
      input.detailPath,
      input.source ?? null,
      input.sourceKind ?? "paste",
      input.confidence ?? 1,
    ],
  );
  const id = res.lastInsertId as number;
  if (input.tags?.length) await setConceptTags(id, input.tags);
  return id;
}

/** 제목/요약 등 내용 수정 (updated_at 갱신) */
export async function updateConceptContent(
  id: number,
  fields: { title?: string; summary?: string },
): Promise<void> {
  const db = await getDb();
  const sets: string[] = [];
  const params: unknown[] = [];
  let p = 1;
  if (fields.title !== undefined) {
    sets.push(`title = $${p++}`);
    params.push(fields.title);
  }
  if (fields.summary !== undefined) {
    sets.push(`summary = $${p++}`);
    params.push(fields.summary);
  }
  if (!sets.length) return;
  sets.push(`updated_at = $${p++}`);
  params.push(now());
  params.push(id);
  await db.execute(
    `UPDATE concepts SET ${sets.join(", ")} WHERE id = $${p}`,
    params,
  );
}

/** 상태 전이 (졸업/되돌리기). 이미 해당 상태면 no-op → 멱등. learned_at 은 트리거가 관리 */
export async function setStatus(
  id: number,
  status: ConceptStatus,
): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE concepts SET status = $1, updated_at = $2 WHERE id = $3 AND status <> $1`,
    [status, now(), id],
  );
}

/** 자신감 ±1 (1~3 clamp). updated_at 은 건드리지 않음(내용 수정 아님). 새 값 반환 */
export async function adjustConfidence(
  id: number,
  delta: number,
): Promise<Confidence> {
  const db = await getDb();
  await db.execute(
    `UPDATE concepts SET confidence = MIN(MAX(confidence + $1, 1), 3) WHERE id = $2`,
    [delta, id],
  );
  const rows = await db.select<{ confidence: number }[]>(
    `SELECT confidence FROM concepts WHERE id = $1`,
    [id],
  );
  return (rows[0]?.confidence ?? 1) as Confidence;
}

/** 위젯 노출 기록 (seen_count++, last_seen_at). updated_at 은 절대 건드리지 않음 (PRD §5.3) */
export async function markSeen(id: number): Promise<void> {
  const db = await getDb();
  await db.execute(
    `UPDATE concepts SET seen_count = seen_count + 1, last_seen_at = $1 WHERE id = $2`,
    [now(), id],
  );
}

/** 개념 삭제 (concept_tags 는 FK CASCADE). vault 디렉터리 삭제는 호출부에서 별도 처리 */
export async function deleteConcept(id: number): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM concepts WHERE id = $1`, [id]);
}

// ---- 태그 ----

async function upsertTag(name: string): Promise<number> {
  const db = await getDb();
  await db.execute(
    `INSERT INTO tags (name) VALUES ($1) ON CONFLICT(name) DO NOTHING`,
    [name],
  );
  const rows = await db.select<{ id: number }[]>(
    `SELECT id FROM tags WHERE name = $1`,
    [name],
  );
  return rows[0].id;
}

/** 개념의 태그를 주어진 목록으로 교체 */
export async function setConceptTags(
  conceptId: number,
  names: string[],
): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM concept_tags WHERE concept_id = $1`, [
    conceptId,
  ]);
  const clean = [...new Set(names.map((n) => n.trim()).filter(Boolean))];
  for (const name of clean) {
    const tagId = await upsertTag(name);
    await db.execute(
      `INSERT OR IGNORE INTO concept_tags (concept_id, tag_id) VALUES ($1, $2)`,
      [conceptId, tagId],
    );
  }
}

/** 전체 태그 (사용 횟수 desc) — 태그 필터 UI 용 */
export async function allTags(): Promise<{ name: string; count: number }[]> {
  const db = await getDb();
  return db.select<{ name: string; count: number }[]>(
    `SELECT t.name AS name, COUNT(ct.concept_id) AS count FROM tags t
     LEFT JOIN concept_tags ct ON ct.tag_id = t.id
     GROUP BY t.id ORDER BY count DESC, t.name COLLATE NOCASE ASC`,
  );
}

/** 상태별 개수 (탭 배지용) */
export async function statusCounts(): Promise<{
  learning: number;
  learned: number;
  all: number;
}> {
  const db = await getDb();
  const rows = await db.select<{ status: string; c: number }[]>(
    `SELECT status, COUNT(*) AS c FROM concepts GROUP BY status`,
  );
  const learning = rows.find((r) => r.status === "learning")?.c ?? 0;
  const learned = rows.find((r) => r.status === "learned")?.c ?? 0;
  return { learning, learned, all: learning + learned };
}

// ---- 설정 (settings key/value) ----

export async function getSetting(key: string): Promise<string | null> {
  const db = await getDb();
  const rows = await db.select<{ value: string }[]>(
    `SELECT value FROM settings WHERE key = $1`,
    [key],
  );
  return rows.length ? rows[0].value : null;
}

export async function setSetting(key: string, value: string): Promise<void> {
  const db = await getDb();
  await db.execute(
    `INSERT INTO settings (key, value) VALUES ($1, $2)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
    [key, value],
  );
}

// ---- Claude 호출 로그 ----

export interface InvocationLog {
  conceptId?: number | null;
  model: string;
  sessionId?: string | null;
  inputTokens?: number | null;
  outputTokens?: number | null;
  costUsd?: number | null;
  durationMs?: number | null;
  status: "success" | "error";
  retryCount?: number;
}

export async function logInvocation(log: InvocationLog): Promise<void> {
  const db = await getDb();
  await db.execute(
    `INSERT INTO ai_invocations
       (concept_id, model, session_id, input_tokens, output_tokens, cost_usd, duration_ms, status, retry_count)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)`,
    [
      log.conceptId ?? null,
      log.model,
      log.sessionId ?? null,
      log.inputTokens ?? null,
      log.outputTokens ?? null,
      log.costUsd ?? null,
      log.durationMs ?? null,
      log.status,
      log.retryCount ?? 0,
    ],
  );
}
