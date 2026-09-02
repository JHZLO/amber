-- v15: DB 스키마 연동 — 연결 프로필 (다이어그램 탭)
-- 비밀번호 컬럼은 없다. 의도적으로 — 비밀은 macOS 키체인(service dev.jhzlo.amber, account db/<ulid>)에만 둔다.
-- 스키마 구조 스냅샷은 파일(<folder_path>/<schema>/.schema.json), 생성된 ERD 는 .mmd 파일 — DESIGN §11 분담.
CREATE TABLE db_connections (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    ulid          TEXT    NOT NULL UNIQUE,                 -- 키체인 account 키 · Rust 커맨드 식별자
    name          TEXT    NOT NULL,
    kind          TEXT    NOT NULL CHECK (kind IN ('mysql','postgres')),
    env           TEXT    NOT NULL DEFAULT 'dev' CHECK (env IN ('dev','staging','prod')),
    host          TEXT    NOT NULL,
    port          INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username      TEXT    NOT NULL,
    tls           TEXT    NOT NULL DEFAULT 'preferred' CHECK (tls IN ('disabled','preferred','required')),
    folder_path   TEXT    NOT NULL UNIQUE,                 -- 다이어그램 루트 기준 상대경로 ('dev'). 이름 변경/이동 시 갱신
    schemas_json  TEXT    NOT NULL DEFAULT '[]',           -- [{name, label, enabled}]
    last_sync_at  INTEGER,                                 -- UTC ms. NULL = 아직 동기화 안 함
    last_error    TEXT,                                    -- 마지막 실패의 에러 코드만 ('DB_REFUSED'). 문장 아님
    created_at    INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at    INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;
