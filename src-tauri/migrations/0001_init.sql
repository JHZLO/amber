-- v1: initial schema
-- 타임스탬프 규칙: UTC epoch milliseconds (INTEGER)
-- WAL / foreign_keys 는 sqlx 커넥션 기본값(ON)에 의존하므로 여기서 PRAGMA 를 두지 않는다.

-- 개념(정본): title/summary 는 DB 가 정본, 상세 본문은 detail_path 의 .md 파일이 정본
CREATE TABLE concepts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,          -- 내부 대리키(정렬/FK/creation-order tie-break)
    ulid          TEXT    NOT NULL UNIQUE,                    -- 외부 앵커(파일 디렉터리명/frontmatter)
    title         TEXT    NOT NULL,
    summary       TEXT    NOT NULL,                           -- 위젯용 짧은 요약
    detail_path   TEXT    NOT NULL,                           -- vault 기준 상대경로 'concepts/<ulid>/index.md'
    status        TEXT    NOT NULL DEFAULT 'learning'
                          CHECK (status IN ('learning','learned')),
    confidence    INTEGER NOT NULL DEFAULT 1
                          CHECK (confidence BETWEEN 1 AND 3),
    source        TEXT,                                       -- 원문(붙여넣은 Q&A) 참조/발췌
    source_kind   TEXT    CHECK (source_kind IN ('paste','url','file') OR source_kind IS NULL),
    seen_count    INTEGER NOT NULL DEFAULT 0,                 -- 위젯 노출 누적(표시·통계용, 정렬키 아님)
    last_seen_at  INTEGER,                                    -- 마지막 위젯 노출(UTC ms). NULL=미노출
    created_at    INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at    INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    learned_at    INTEGER                                     -- learned 전이 시각, 아니면 NULL (트리거가 관리)
) STRICT;

CREATE TABLE tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL COLLATE NOCASE UNIQUE
) STRICT;

CREATE TABLE concept_tags (
    concept_id INTEGER NOT NULL REFERENCES concepts(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
    PRIMARY KEY (concept_id, tag_id)
) STRICT;

-- 앱 설정(claude 경로/모델, 위젯 동작 등). 창 위치/크기는 window-state 플러그인이 별도 관리.
CREATE TABLE settings (
    key   TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- Claude 호출 로그
CREATE TABLE claude_invocations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    concept_id    INTEGER REFERENCES concepts(id) ON DELETE SET NULL,   -- 실패/삭제 시 NULL
    model         TEXT    NOT NULL,
    session_id    TEXT,
    input_tokens  INTEGER,
    output_tokens INTEGER,
    cost_usd      REAL,
    duration_ms   INTEGER,
    status        TEXT    NOT NULL,                            -- 'success' | 'error'
    retry_count   INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;

-- canonical 위젯/학습중 정렬(§5.1)에 정확히 대응하는 부분 인덱스
CREATE INDEX idx_concepts_widget
    ON concepts (confidence, last_seen_at, id) WHERE status = 'learning';
CREATE INDEX idx_concepts_status_updated
    ON concepts (status, updated_at DESC);
CREATE INDEX idx_concepts_learned_at
    ON concepts (learned_at DESC) WHERE status = 'learned';
CREATE INDEX idx_concept_tags_tag ON concept_tags (tag_id);

-- learned_at 자동 정합. 내부 UPDATE 는 learned_at 만 건드리므로 'AFTER UPDATE OF status' 를 재발동시키지 않음.
CREATE TRIGGER trg_learned_stamp
AFTER UPDATE OF status ON concepts FOR EACH ROW
WHEN NEW.status = 'learned' AND OLD.status <> 'learned'
BEGIN
    UPDATE concepts SET learned_at = CAST(unixepoch('subsec') * 1000 AS INTEGER) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_relearn_clear
AFTER UPDATE OF status ON concepts FOR EACH ROW
WHEN NEW.status = 'learning' AND OLD.status = 'learned'
BEGIN
    UPDATE concepts SET learned_at = NULL WHERE id = NEW.id;
END;
