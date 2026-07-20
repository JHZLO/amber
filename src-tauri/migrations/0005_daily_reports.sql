-- 데일리 리포트: 투두(계획) × 연동 플랫폼 활동(실행)을 AI 가 대조해 정리한 하루 요약.
-- 본문 정본은 vault/reports/<date>.md 파일. 이 테이블은 메타(생성 근거 스냅샷·모델·소요시간)만.
-- 날짜당 1개(report_date UNIQUE) — 재생성은 덮어쓰기.

CREATE TABLE daily_reports (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    report_date   TEXT    NOT NULL UNIQUE
                          CHECK (report_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    file_path     TEXT    NOT NULL,              -- vault 기준 상대경로 'reports/<date>.md'
    sources_json  TEXT    NOT NULL DEFAULT '[]', -- 생성 근거 스냅샷 [{id,rank,ok,items,error}]
    provider      TEXT,                          -- claude|codex|gemini
    model         TEXT,
    duration_ms   INTEGER,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch('subsec')*1000),
    updated_at    INTEGER NOT NULL DEFAULT (unixepoch('subsec')*1000)
) STRICT;
