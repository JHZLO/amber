-- 주간 리포트: 그 주(월~일)의 일간 리포트 본문을 AI 가 다시 묶어 정리한 요약.
-- 본문 정본은 vault/reports/<월요일>-week.md 파일. 이 테이블은 메타만 (daily_reports 와 같은 구조).
--
-- 왜 daily_reports 를 재사용하지 않는가: report_date 가 컬럼 레벨 UNIQUE 라
-- UNIQUE(report_date, period) 로 완화하려면 STRICT 테이블 12단계 재구축이 필요하다.
-- 별도 테이블이 훨씬 싸고 daily_reports 와 그 접근자를 전혀 건드리지 않는다.
--
-- week_start 는 **월요일**이다 (스프린트 규약 월~일). 앱의 weekStartOf 는 일요일 기준이지만
-- 그건 미니 캘린더/타임테이블 주간 뷰용이고, 리포트는 lib/date.ts mondayOf 를 쓴다.

CREATE TABLE weekly_reports (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    week_start    TEXT    NOT NULL UNIQUE
                          CHECK (week_start GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    file_path     TEXT    NOT NULL,              -- vault 기준 상대경로 'reports/<월요일>-week.md'
    -- 재료 스냅샷: 어느 날짜의 일간 리포트를 묶었는지 ["2026-08-17", …]. 빈 날은 안 담긴다.
    sources_json  TEXT    NOT NULL DEFAULT '[]',
    provider      TEXT,                          -- claude|codex|gemini
    model         TEXT,
    duration_ms   INTEGER,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch('subsec')*1000),
    updated_at    INTEGER NOT NULL DEFAULT (unixepoch('subsec')*1000)
) STRICT;
